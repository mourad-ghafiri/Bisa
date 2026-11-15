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
 * A 409 on that save means somebody else drew meanwhile. Offscreen, the
 * store's scene is re-read, the agent's shapes are merged into **that**
 * scene, drawn again and saved at its hash — so neither an erase the agent
 * made a moment ago nor a peer's change is written over; a second 409 is
 * answered as a refusal that tells the agent to read and draw again. On the
 * **open** canvas the bridge gives way at once (`conflictStep`): its shapes
 * are taken back off, the agent is told the drawing moved, and the person's
 * canvas is left to its own editor — nothing they drew is replaced by an
 * agent's retry. An offscreen save moves no open canvas's record
 * (`liveScene.noteSaved`, by canvas): a drawing the person opened meanwhile
 * hears the engine's frame as somebody else's and adopts it.
 *
 * **Every request is answered.** Work that throws is a refusal; work that
 * outlasts `REQUEST_DEADLINE_MS` is answered as too slow while it is left to
 * finish — the queue still waits for it, since two requests on the one
 * offscreen canvas would save into each other's drawings — and no wait in it
 * is open-ended: a window that is hidden paints no frame, so the layout wait
 * ends on a clock, and a canvas that never mounts fails its request. An
 * answer the node did not take is sent again; a request read off the list
 * after the engine stopped waiting for it is not performed at all.
 */

import { ApiError, api } from "../api";
import { t } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";
import { Lru } from "../shell/lru.mjs";
import type { DrawResult, DrawingDetail, DrawingPending } from "../types";
import type { ExcalidrawImperativeAPI, OrderedExcalidrawElement } from "../ui/excalidraw";
import { loadExcalidraw, loadMermaidToExcalidraw } from "../ui/excalidraw";
import { beginDrawWork, endDrawWork } from "./drawActivityStore";
import { canvasAppState, drawnElements, persistedAppState } from "./drawModel.mjs";
import { drawPrefs } from "./drawPrefsStore";
import { CANVAS_FAILED, CANVAS_SLOW, DRAWING_GONE, DRAWING_MOVED, HANDLED_KEPT, MERMAID_AS_PICTURE, MERMAID_FONT_SIZE, MERMAID_REFUSED, REQUEST_DEADLINE_MS, SNAPSHOT_FAILED, answerRetries, conflictStep, drawnResult, mergeElements, mermaidDrawable, planDrawRequest, refusedResult, requestExpired, saveBase, snapshotName, snapshotResult } from "./drawRequestModel.mjs";
import type { DrawPlan } from "./drawRequestModel.mjs";
import { lastSaved, liveSceneFor, noteSaved } from "./liveScene";
import { offscreenApi, offscreenIdle } from "./offscreen";
import { prefersReducedMotion } from "../shell/motion";

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
  // Read off the list after the engine stopped waiting — a window that was
  // hidden, a node that was away: its agent was already told the canvas was
  // silent, and drawing it now would draw what was said not to be drawn.
  if (requestExpired(pending.asked_at, Date.now())) {
    log.info("draw", "a drawing request was left alone: the engine no longer waits for it", { id: pending.id, action: pending.request.action });
    return;
  }
  const drawing = pending.request.drawing ?? null;
  const plan = planDrawRequest(pending.request);
  // The work, which always ends in a result: a throw anywhere in it — the
  // canvas's code that would not load, a canvas that never mounted, a scene
  // it could not take — is a refusal like any other, answered.
  const work: Promise<DrawResult> =
    plan.kind === "refuse"
      ? Promise.resolve(refusedResult(plan.error, drawing))
      : working(plan.drawing, async () => {
          try {
            return await perform(plan);
          } catch (e) {
            log.warn("draw", "a drawing request could not be performed", { id: pending.id, action: pending.request.action, ...errorFields(e) });
            return refusedResult(`${CANVAS_FAILED}: ${e instanceof Error ? e.message : String(e)}`, plan.drawing);
          }
        });
  // Answered within the engine's wait, whatever the work does: past the
  // deadline the agent is told the canvas was too slow — and to read before
  // it draws again, since the work goes on and may still land.
  let late: ReturnType<typeof setTimeout> | undefined;
  const result = await Promise.race([
    work,
    new Promise<DrawResult>((resolve) => {
      late = setTimeout(() => resolve(refusedResult(CANVAS_SLOW, drawing)), REQUEST_DEADLINE_MS);
    }),
  ]);
  clearTimeout(late);
  // A refusal is data to the agent and a line here: the id and the act, never the scene.
  if (!result.ok) log.warn("draw", "a drawing request was refused", { id: pending.id, action: pending.request.action, drawing, error: result.error });
  for (let attempt = 0; ; attempt += 1) {
    try {
      await api.answerDrawingRequest(pending.id, result);
      break;
    } catch (e) {
      const status = e instanceof ApiError ? e.status : 0;
      if (answerRetries(status, attempt)) {
        await new Promise((resolve) => setTimeout(resolve, 400 * (attempt + 1)));
        continue;
      }
      // Answered already, or the engine gave up waiting and told the agent the
      // canvas was silent: nothing left to say to the agent. The save landed
      // all the same, and the engine announces it off this late answer
      // (`drawings::answer`) — worth a line, since the agent was told otherwise.
      log.warn("draw", "an answer to a drawing request was not taken; the save landed and the engine announces it", { id: pending.id, ok: result.ok, ...errorFields(e) });
      break;
    }
  }
  // The turn ends when the work does, not when it was answered: the next
  // request must not find this one's drawing still on the offscreen canvas.
  await work;
  offscreenIdle();
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

/** How long the layout wait lasts when no frame comes: a hidden or occluded window paints none. */
const SETTLE_FRAMES_MS = 400;
/** How long the fonts are waited for. */
const SETTLE_FONTS_MS = 2000;

/**
 * Two frames and the fonts: what the canvas needs to have laid out what it
 * was handed. Each wait also ends on a clock — a window hidden to the tray
 * or behind another paints no frame, and a wait on frames alone held every
 * agent's request until the window came back.
 */
async function settle(): Promise<void> {
  const clock = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
  await Promise.race([new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))), clock(SETTLE_FRAMES_MS)]);
  await Promise.race([Promise.resolve(document.fonts?.ready).then(() => undefined), clock(SETTLE_FONTS_MS)]);
}

/** The canvas to perform in: the open editor's for a drawing on screen, else the offscreen one loaded with the drawing. */
async function canvasFor(detail: DrawingDetail): Promise<{ api: ExcalidrawImperativeAPI; live: boolean }> {
  const live = liveSceneFor(detail.id);
  if (live) return { api: live, live: true };
  const mod = await loadExcalidraw();
  const api = await offscreenApi();
  api.updateScene({
    elements: mod.restoreElements(detail.scene.elements as Parameters<typeof mod.restoreElements>[0], null, { repairBindings: true }),
    // The drawing's own ground: without it the offscreen canvas's white and no grid were saved over the person's choice, and a snapshot was taken on them.
    appState: canvasAppState(detail.scene.app_state),
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
          // A kind the converter can only hand back as a picture: refused here, before a canvas is touched — put on the person's canvas, the picture stayed there while the store refused it.
          if (!mermaidDrawable(parsed.elements)) return refusedResult(MERMAID_AS_PICTURE, plan.drawing);
          incoming = mod.convertToExcalidrawElements(parsed.elements, { regenerateIds: true }) as OrderedExcalidrawElement[];
        }
      } catch (e) {
        const why = e instanceof Error ? e.message : String(e);
        return refusedResult(plan.kind === "mermaid" ? `${MERMAID_REFUSED}: ${why}` : t("draw-bridge-skeleton-could-not-be-laid-out", { why }), plan.drawing);
      }
      const canvas = await canvasFor(detail);
      const saved = await save(detail, incoming, plan.replace, canvas, mod);
      if (!saved.ok) return saved.result;
      if (canvas.live && incoming.length > 0) canvas.api.scrollToContent(incoming, { fitToContent: true, animate: !prefersReducedMotion() });
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
 * (`saveBase`). On a 409 (`conflictStep`): offscreen, the store's scene is
 * **re-read**, the agent's shapes merged into that scene, drawn again and
 * saved at its hash, once — so neither an erase the agent made a moment ago
 * nor a peer's change is written over; on the open canvas the agent's shapes
 * are taken back off and the agent is told the drawing moved — the person's
 * strokes are never replaced by a retry. Any save that does not land on the
 * open canvas takes its shapes back off it, so what a person sees is what
 * was saved or what they drew. The record moves only for the canvas that
 * saved (`noteSaved` by canvas): an offscreen save leaves an open canvas's
 * record alone.
 */
async function save(
  detail: DrawingDetail,
  incoming: OrderedExcalidrawElement[],
  replace: boolean,
  canvas: { api: ExcalidrawImperativeAPI; live: boolean },
  mod: Awaited<ReturnType<typeof loadExcalidraw>>,
): Promise<{ ok: true; hash: string; count: number } | { ok: false; result: DrawResult }> {
  let read = detail;
  // What the canvas held before the agent's shapes: put back when the save does not land on the open canvas.
  const before = drawnElements(canvas.api.getSceneElements());
  let current = before;
  /** The open canvas as it was, when the agent's shapes did not reach the store: what is on a person's canvas is what was saved, or what they drew. */
  const takeBack = () => {
    if (canvas.live) canvas.api.updateScene({ elements: before, captureUpdate: mod.CaptureUpdateAction.NEVER });
  };
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
      if (e instanceof ApiError && e.status === 409 && conflictStep(canvas.live, attempt) === "merge") {
        try {
          read = (await api.drawing(detail.id)).drawing;
        } catch {
          return { ok: false, result: refusedResult(DRAWING_GONE, detail.id) };
        }
        // Somebody drew meanwhile: what the store holds is the scene the agent's shapes join.
        current = drawnElements(mod.restoreElements(read.scene.elements as Parameters<typeof mod.restoreElements>[0], null, { repairBindings: true }));
        continue;
      }
      takeBack();
      if (e instanceof ApiError && e.status === 409) return { ok: false, result: refusedResult(DRAWING_MOVED, detail.id) };
      return { ok: false, result: refusedResult(e instanceof ApiError ? e.message : String(e), detail.id) };
    }
  }
  return { ok: false, result: refusedResult(DRAWING_MOVED, detail.id) };
}
