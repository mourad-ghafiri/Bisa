/**
 * The desktop's half of the drawing bridge (19 — Drawings), as facts: what a
 * parked request asks the canvas to do, how new elements join a scene, and
 * the answers the desktop gives — the shapes `POST /drawings/requests/{id}`
 * takes. The bridge (`drawBridge.ts`) only drives the canvas and the wire.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/** How often an open desktop reads the parked list — under the engine's 45 s presence window. */
export const DRAW_PRESENCE_MS = 20_000;
/** How many taken-up requests the bridge remembers, so one heard twice is performed once — the latest, never every one since the window opened. */
export const HANDLED_KEPT = 256;
/**
 * How long the desktop gives one request before it answers that the canvas
 * was too slow — under the engine's sixty seconds, so the agent hears the
 * desktop's own word (and what to do next) rather than a silence.
 */
export const REQUEST_DEADLINE_MS = 50_000;
/** How long the engine waits for an answer: a parked request older than this has already been answered as silent. */
const ENGINE_WAIT_MS = 60_000;
/** How many times an answer is sent before the desktop gives up on it. */
export const ANSWER_TRIES = 3;
/** The font size a Mermaid text is laid out at. */
export const MERMAID_FONT_SIZE = 20;

/** The refusals the desktop answers, for the agent to read — said in the window's language, as the browser bridge's are. */
export const DRAWING_GONE = t("draw-request-drawing-gone");
export const DRAWING_MOVED = t("draw-request-drawing-moved");
export const NOT_A_SKELETON = t("draw-request-not-a-skeleton");
export const MERMAID_REFUSED = t("draw-request-mermaid-refused");
export const SNAPSHOT_FAILED = t("draw-request-snapshot-failed");
export const UNKNOWN_ACTION = t("draw-request-unknown-action");
export const MERMAID_AS_PICTURE = t("draw-request-mermaid-as-picture");
export const CANVAS_FAILED = t("draw-request-canvas-failed");
export const CANVAS_SLOW = t("draw-request-canvas-slow");

/**
 * Whether a parked request is past the engine's wait: read off the list
 * after a window that was hidden or a node that was away, it has already
 * been answered to its agent as silent, and performing it now would draw
 * what the agent was told was not drawn. A frame carries no time
 * (`asked_at: 0`): it was asked just now.
 * @param {number} askedAt unix seconds, or 0 @param {number} now milliseconds
 */
export function requestExpired(askedAt, now) {
  return askedAt > 0 && now - askedAt * 1000 > ENGINE_WAIT_MS;
}

/**
 * Whether an answer the node did not take is sent again: when the node did
 * not answer at all or failed (`0`, a `5xx`), up to [`ANSWER_TRIES`]. A
 * `404` is the engine saying nothing waits under that id — it has answered
 * the agent already — and any other `4xx` would be refused the same again.
 * @param {number} status the refusal's status, 0 for none @param {number} attempt the try that failed, from 0
 */
export function answerRetries(status, attempt) {
  return attempt < ANSWER_TRIES - 1 && (status === 0 || status >= 500);
}

/**
 * Whether what a Mermaid text converted to can be drawn as shapes. The
 * converter draws flowcharts, sequence, class, entity-relationship and state
 * diagrams as elements; every other kind comes back as **one image** of the
 * rendered diagram, which a drawing never keeps — and nothing at all is
 * nothing to draw.
 * @param {readonly {type?: string}[]} elements
 */
export function mermaidDrawable(elements) {
  return elements.length > 0 && !elements.some((e) => e.type === "image");
}

/**
 * What a bridge save that lost to another writer does next. On the **open**
 * canvas it gives way: the agent is told the drawing moved, and the person's
 * canvas is left to its own editor — an agent's retry never replaces what
 * they are drawing. Offscreen, the store's scene is re-read and the agent's
 * shapes merged into it, once.
 * @param {boolean} live the shapes landed on the open canvas @param {number} attempt from 0
 * @returns {"merge" | "moved"}
 */
export function conflictStep(live, attempt) {
  return !live && attempt === 0 ? "merge" : "moved";
}

/**
 * What one parked request asks: laid-out skeleton elements, a Mermaid text,
 * or a picture — each with the drawing it is about. Anything else is refused
 * in a sentence, since the engine answers the data actions itself.
 * @param {{action: string, drawing?: string | null, elements?: unknown[] | null, text?: string | null, replace?: boolean}} request
 * @returns {{kind: "refuse", error: string} | {kind: "draw", drawing: string, elements: unknown[], replace: boolean} | {kind: "mermaid", drawing: string, text: string, replace: boolean} | {kind: "snapshot", drawing: string}}
 */
export function planDrawRequest(request) {
  const drawing = typeof request?.drawing === "string" && request.drawing !== "" ? request.drawing : null;
  if (!drawing) return { kind: "refuse", error: DRAWING_GONE };
  switch (request.action) {
    case "draw":
      if (!Array.isArray(request.elements) || !request.elements.every((e) => e !== null && typeof e === "object")) return { kind: "refuse", error: NOT_A_SKELETON };
      return { kind: "draw", drawing, elements: request.elements, replace: request.replace === true };
    case "mermaid": {
      const text = typeof request.text === "string" ? request.text.trim() : "";
      if (text === "") return { kind: "refuse", error: MERMAID_REFUSED };
      return { kind: "mermaid", drawing, text, replace: request.replace === true };
    }
    case "snapshot":
      return { kind: "snapshot", drawing };
    default:
      return { kind: "refuse", error: UNKNOWN_ACTION };
  }
}

/**
 * New elements joining a scene. `replace` draws the incoming alone; else an
 * incoming element with a known id takes that element's place and the rest
 * are appended — nothing the person drew is dropped.
 * @template {{id: string}} E
 * @param {readonly E[]} current
 * @param {readonly E[]} incoming
 * @param {boolean} replace
 * @returns {E[]}
 */
export function mergeElements(current, incoming, replace) {
  if (replace) return [...incoming];
  const byId = new Map(incoming.map((e) => [e.id, e]));
  const kept = current.map((e) => byId.get(e.id) ?? e);
  const known = new Set(current.map((e) => e.id));
  return [...kept, ...incoming.filter((e) => !known.has(e.id))];
}

/**
 * The hash a bridge save states as the scene it drew on: the open canvas's
 * record when the agent's shapes landed on it — two writers save through
 * that canvas, and the record is where its hash stands — else the hash the
 * bridge read from the store a moment ago, since an offscreen canvas has no
 * record and must never borrow an open one's.
 * @param {boolean} live the shapes landed on the open canvas
 * @param {string | null} recordHash the open canvas's record, when it has one
 * @param {string} readHash the detail the bridge read
 */
export function saveBase(live, recordHash, readHash) {
  return live && recordHash ? recordHash : readHash;
}

/** The answer after a draw or a Mermaid: the drawing as it now stands. @param {string} drawing @param {string} hash @param {number} elementCount */
export function drawnResult(drawing, hash, elementCount) {
  return { ok: true, drawing, hash, element_count: elementCount };
}

/** The answer for a snapshot: the PNG the desktop uploaded, by reference, with its size. */
export function snapshotResult(drawing, snapshot, size) {
  return { ok: true, drawing, snapshot, width: size.width, height: size.height };
}

/** A refusal, in the agent's words. @param {string} error @param {string | null} [drawing] */
export function refusedResult(error, drawing = null) {
  return drawing ? { ok: false, error, drawing } : { ok: false, error };
}

/** The file name a snapshot of a drawing is kept under: `drawing-<id>-<stamp>.png`. @param {string} drawing @param {Date} [at] */
export function snapshotName(drawing, at = new Date()) {
  const id = String(drawing ?? "").replace(/[^a-zA-Z0-9]/g, "").slice(0, 26) || "drawing";
  const stamp = at.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  return `drawing-${id}-${stamp}.png`;
}

/** The words a working dot wears while an agent draws. */
export function drawingWords(count) {
  return count === 1 ? t("draw-activity-agent-drawing") : t("draw-activity-agents-drawing", { count });
}
