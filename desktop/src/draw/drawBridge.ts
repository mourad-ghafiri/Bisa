/**
 * The desktop's half of the drawing bridge (19 — Drawings): an agent's
 * drawing tool call the engine parked — a skeleton to lay out, a Mermaid
 * text to draw, a picture to render — heard here as a `drawing_request`
 * frame or read from the parked list, performed in a canvas and answered
 * over the node. The decisions are `drawRequestModel.mjs`'s; the canvas is
 * the open editor's when the drawing is on screen (the person watches the
 * shapes land) and the offscreen one otherwise; the save goes through the
 * same guarded `PATCH` the editor uses, with the hash the store last gave.
 *
 * A 409 on that save means somebody else drew meanwhile: the store's scene
 * is re-read, the agent's shapes are merged into **that** scene, drawn again
 * and saved at its hash — so neither an erase the agent made a moment ago nor
 * a peer's change is written over; a second 409 is answered as a refusal that
 * tells the agent to read and draw again. An offscreen save moves no open
 * canvas's record (`liveScene.noteSaved`, by canvas): a drawing the person
 * opened meanwhile hears the engine's frame as somebody else's and adopts it.
 */

import { ApiError, api } from "../api";
import { t } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";
import { Lru } from "../shell/lru.mjs";
import type { DrawResult, DrawingDetail, DrawingPending } from "../types";
import type { ExcalidrawImperativeAPI, OrderedExcalidrawElement } from "../ui/excalidraw";
import { loadExcalidraw, loadMermaidToExcalidraw } from "../ui/excalidraw";
import { beginDrawWork, endDrawWork } from "./drawActivityStore";
import { drawnElements, persistedAppState } from "./drawModel.mjs";
import { drawPrefs } from "./drawPrefsStore";
import { DRAWING_GONE, DRAWING_MOVED, HANDLED_KEPT, MERMAID_FONT_SIZE, MERMAID_REFUSED, SNAPSHOT_FAILED, drawnResult, mergeElements, planDrawRequest, refusedResult, saveBase, snapshotName, snapshotResult } from "./drawRequestModel.mjs";
import type { DrawPlan } from "./drawRequestModel.mjs";
import { lastSaved, liveSceneFor, noteSaved } from "./liveScene";
import { offscreenApi } from "./offscreen";

/** How long an upload of a snapshot may take. */
const UPLOAD_MS = 30_000;

/**
 * The requests taken up so far, so one heard as a frame and read again from
 * the parked list is performed once. Only the latest are kept: a request
 * answered leaves the engine's desk, and one this old is on no list any more.
 */
const handled = new Lru<true>(HANDLED_KEPT);

/**
 * One request at a time. The offscreen canvas is one canvas: two requests
 * performed together would each load their drawing into it, and the first to
 * read the scene back would save the other's shapes into its own drawing.
 * So every request waits for the one before it, in the order they were
 * taken up — a request that throws ends its own turn and nobody else's.
 */
let turn: Promise<void> = Promise.resolve();

/** Perform one parked request and answer it; a request already taken up is left alone. */
export function performDrawRequest(pending: DrawingPending): Promise<void> {
  if (handled.has(pending.id)) return Promise.resolve();
  handled.set(pending.id, true);
  const mine = turn.then(() => performOne(pending));
  turn = mine.catch((e: unknown) => log.warn("draw", "a drawing request ended without an answer", { id: pending.id, ...errorFields(e) }));
  return turn;
}

async function performOne(pending: DrawingPending): Promise<void> {
  const plan = planDrawRequest(pending.request);
  let result: DrawResult;
  if (plan.kind === "refuse") result = refusedResult(plan.error, pending.request.drawing ?? null);
  else result = await working(plan.drawing, () => perform(plan));
  // A refusal is data to the agent and a line here: the id and the act, never the scene.
  if (!result.ok) log.warn("draw", "a drawing request was refused", { id: pending.id, action: pending.request.action, drawing: pending.request.drawing ?? null, error: result.error });
  try {
    await api.answerDrawingRequest(pending.id, result);
  } catch (e) {
    // Answered already, or the engine gave up waiting and told the agent the
    // canvas was silent: nothing left to say to the agent. The save landed
    // all the same, and the engine announces it off this late answer
    // (`drawings::answer`) — worth a line, since the agent was told otherwise.
    log.warn("draw", "an answer to a drawing request was not taken; the save landed and the engine announces it", { id: pending.id, ok: result.ok, ...errorFields(e) });
  }
}

/** A request acting on a drawing: the drawing is busy from before the act until after it, whatever the act answers. */
async function working<T>(drawing: string, act: () => Promise<T>): Promise<T> {
  beginDrawWork(drawing);
  try {
    return await act();
  } finally {
    endDrawWork(drawing);
  }
}

/** Two frames and the fonts: what the canvas needs to have laid out what it was handed. */
async function settle(): Promise<void> {
  await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  await document.fonts?.ready;
}

/** The canvas to perform in: the open editor's for a drawing on screen, else the offscreen one loaded with the drawing. */
async function canvasFor(detail: DrawingDetail): Promise<{ api: ExcalidrawImperativeAPI; live: boolean }> {
  const live = liveSceneFor(detail.id);
  if (live) return { api: live, live: true };
  const mod = await loadExcalidraw();
  const api = await offscreenApi();
  api.updateScene({
    elements: mod.restoreElements(detail.scene.elements as Parameters<typeof mod.restoreElements>[0], null, { repairBindings: true }),
    captureUpdate: mod.CaptureUpdateAction.NEVER,
  });
  await settle();
  return { api, live: false };
}

async function perform(plan: Exclude<DrawPlan, { kind: "refuse" }>): Promise<DrawResult> {
  let detail: DrawingDetail;
  try {
    detail = (await api.drawing(plan.drawing)).drawing;
  } catch (e) {
    log.debug("draw", "the drawing a request names could not be read", { drawing: plan.drawing, ...errorFields(e) });
    return refusedResult(DRAWING_GONE, plan.drawing);
  }
  const mod = await loadExcalidraw();
  switch (plan.kind) {
    case "draw":
    case "mermaid": {
      // What the agent asked for is laid out before a canvas is chosen: the
      // choice is made as late as it can be, so a drawing the person opens
      // meanwhile is drawn on where they watch rather than offscreen.
      let incoming: OrderedExcalidrawElement[];
      try {
        if (plan.kind === "draw") {
          incoming = mod.convertToExcalidrawElements(plan.elements as Parameters<typeof mod.convertToExcalidrawElements>[0], { regenerateIds: false }) as OrderedExcalidrawElement[];
        } else {
          const { parseMermaidToExcalidraw } = await loadMermaidToExcalidraw();
          const parsed = await parseMermaidToExcalidraw(plan.text, { themeVariables: { fontSize: `${MERMAID_FONT_SIZE}px` } });
          incoming = mod.convertToExcalidrawElements(parsed.elements, { regenerateIds: true }) as OrderedExcalidrawElement[];
        }
      } catch (e) {
        const why = e instanceof Error ? e.message : String(e);
        return refusedResult(plan.kind === "mermaid" ? `${MERMAID_REFUSED}: ${why}` : t("draw-bridge-skeleton-could-not-be-laid-out", { why }), plan.drawing);
      }
      const canvas = await canvasFor(detail);
      const saved = await save(detail, incoming, plan.replace, canvas, mod);
      if (!saved.ok) return saved.result;
      if (canvas.live && incoming.length > 0) canvas.api.scrollToContent(incoming, { fitToContent: true, animate: true });
      return drawnResult(plan.drawing, saved.hash, saved.count);
    }
    case "snapshot": {
      const canvas = await canvasFor(detail);
      try {
        const width = drawPrefs().snapshotWidth;
        let size = { width, height: width };
        const blob = await mod.exportToBlob({
          elements: canvas.api.getSceneElements(),
          appState: { ...canvas.api.getAppState(), exportBackground: true, exportWithDarkMode: false },
          files: canvas.api.getFiles(),
          mimeType: "image/png",
          getDimensions: (w: number, h: number) => {
            const scale = w > 0 ? width / w : 1;
            size = { width, height: Math.max(1, Math.round(h * scale)) };
            return { width: size.width, height: size.height, scale };
          },
        });
        const file = new File([blob], snapshotName(plan.drawing), { type: "image/png" });
        const uploaded = await api.uploadAttachment(file, AbortSignal.timeout(UPLOAD_MS));
        return snapshotResult(plan.drawing, uploaded, size);
      } catch (e) {
        return refusedResult(`${SNAPSHOT_FAILED}: ${e instanceof Error ? e.message : String(e)}`, plan.drawing);
      }
    }
  }
}

/**
 * Draw the incoming elements into the canvas and save what it then holds
 * against the hash the store last gave — the open canvas's record when the
 * shapes landed on it (two writers save through it), else the hash just read
 * (`saveBase`) — trying once more on a 409: the store's scene is **re-read**,
 * the agent's shapes merged into that scene, drawn again and saved at its
 * hash, so neither an erase the agent made a moment ago nor a peer's change
 * is written over. On the open canvas that re-read replaces strokes the
 * person had not saved — the price of a conflict, as *Take theirs* is. A
 * second 409 is a refusal that tells the agent to read and draw again. The
 * record moves only for the canvas that saved (`noteSaved` by canvas): an
 * offscreen save leaves an open canvas's record alone.
 */
async function save(
  detail: DrawingDetail,
  incoming: OrderedExcalidrawElement[],
  replace: boolean,
  canvas: { api: ExcalidrawImperativeAPI; live: boolean },
  mod: Awaited<ReturnType<typeof loadExcalidraw>>,
): Promise<{ ok: true; hash: string; count: number } | { ok: false; result: DrawResult }> {
  let read = detail;
  let current = drawnElements(canvas.api.getSceneElements());
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const merged = mergeElements(current, incoming, replace);
    canvas.api.updateScene({ elements: merged, captureUpdate: canvas.live ? mod.CaptureUpdateAction.IMMEDIATELY : mod.CaptureUpdateAction.NEVER });
    await settle();
    const elements = drawnElements(canvas.api.getSceneElements());
    const app_state = persistedAppState(canvas.api.getAppState());
    const base = saveBase(canvas.live, lastSaved(detail.id, canvas.api)?.hash ?? null, read.hash);
    try {
      const { drawing } = await api.patchDrawing(detail.id, { scene: { elements, app_state }, base_hash: base });
      noteSaved(detail.id, { hash: drawing.hash, elements }, canvas.api);
      return { ok: true, hash: drawing.hash, count: elements.length };
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && attempt === 0) {
        try {
          read = (await api.drawing(detail.id)).drawing;
        } catch {
          return { ok: false, result: refusedResult(DRAWING_GONE, detail.id) };
        }
        // Somebody drew meanwhile: what the store holds is the scene the agent's shapes join.
        current = drawnElements(mod.restoreElements(read.scene.elements as Parameters<typeof mod.restoreElements>[0], null, { repairBindings: true }));
        if (canvas.live) noteSaved(detail.id, { hash: read.hash, elements: current }, canvas.api);
        continue;
      }
      if (e instanceof ApiError && e.status === 409) return { ok: false, result: refusedResult(DRAWING_MOVED, detail.id) };
      return { ok: false, result: refusedResult(e instanceof ApiError ? e.message : String(e), detail.id) };
    }
  }
  return { ok: false, result: refusedResult(DRAWING_MOVED, detail.id) };
}
