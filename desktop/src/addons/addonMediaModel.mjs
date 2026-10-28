/**
 * The media player an addon may ask for beside its window, as facts
 * (18 — Addons).
 *
 * An addon granted `media` with `youtube` asks `media.embed` with a box
 * inside its window; the shell opens a child view on the node's player page
 * and places it where that box falls on the main page (`addonMediaHost.ts`).
 * Nothing here touches a view: the host reads these and performs them. The
 * player page speaks through the shell's one door as `bisa:media`, and
 * `stateOf` is the one reader of what it says.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

import { MAX_TITLE_CHARS, MIN_PLAYER_SIDE, cut } from "./addonBridgeModel.mjs";

/** The states the player page says, as the addon hears them in its `media` event. */
export const MEDIA_STATES = Object.freeze(["ready", "unstarted", "playing", "paused", "buffering", "ended", "cued", "error"]);

/** The key the shell holds an addon's player under — never a browser tab's, which are the sessions' ids. @param {string} addonId */
export function mediaKey(addonId) {
  return `addon-media:${addonId}`;
}

/**
 * The opens and closes of one key run in order, whoever queued them — a
 * window's new instance (put away and shown, its grants changed, its addon
 * switched off and on) must not open the key before the old instance's
 * close has landed: the shell refuses a key still open. One map of chains
 * keyed by the shell's key; a step that throws does not stop the next (a
 * step says its own failure where it runs — the host logs every open and
 * close it queues — and the chain keeps it from being an unhandled
 * rejection nobody waits on); a chain that settled and is still the tail is
 * forgotten.
 * @param {Map<string, Promise<void>>} chains
 * @param {string} key
 * @param {() => Promise<void> | void} step
 * @returns {Promise<void>} settles when `step` has
 */
export function enqueue(chains, key, step) {
  const run = () => Promise.resolve().then(step).catch(() => {});
  const last = chains.get(key) ?? Promise.resolve();
  const next = last.then(run, run);
  chains.set(key, next);
  const forget = () => {
    if (chains.get(key) === next) chains.delete(key);
  };
  next.then(forget, forget);
  return next;
}

/** How many times an open the shell refused is tried again, and the wait before each: a closing view can still be seen for a beat after its close answered. */
export const OPEN_RETRIES = 2;
export const OPEN_RETRY_MS = 100;

/** The node's page for a video, as a path. @param {string} addonId @param {string} video */
export function playerPath(addonId, video) {
  return `/addons/${encodeURIComponent(addonId)}/player/youtube/${encodeURIComponent(video)}`;
}

/**
 * Where the player stands on the main page: the addon's box from the frame's
 * top-left, clamped inside the frame — a box that reaches past the frame is
 * cut to it, never let over the window's bar or its neighbours — in whole
 * pixels. Null when what is left is under the policy's square, or the frame
 * has no size.
 * @param {{left: number, top: number, width: number, height: number}} frame the frame's box on the page
 * @param {{x: number, y: number, width: number, height: number}} box the addon's box, from the frame's top-left
 * @returns {{left: number, top: number, width: number, height: number} | null}
 */
export function pageRect(frame, box) {
  if (!(frame.width > 0) || !(frame.height > 0)) return null;
  const x = Math.max(0, Math.min(box.x, frame.width));
  const y = Math.max(0, Math.min(box.y, frame.height));
  const width = Math.min(box.width, frame.width - x);
  const height = Math.min(box.height, frame.height - y);
  if (width < MIN_PLAYER_SIDE || height < MIN_PLAYER_SIDE) return null;
  return { left: Math.round(frame.left + x), top: Math.round(frame.top + y), width: Math.round(width), height: Math.round(height) };
}

/**
 * Whether the player shows right now: a video is embedded, its window is on
 * screen, no drag or resize runs in the layer (a native view would not
 * follow the pointer), no surface stands over the layer, and the box has a
 * place. A hidden player keeps its page — and its sound — until it is removed.
 * @param {{embedded: boolean, visible: boolean, shielded: boolean, mayShow: boolean, rect: object | null}} facts
 */
export function playerShown({ embedded, visible, shielded, mayShow, rect }) {
  return embedded && visible && !shielded && mayShow && rect !== null;
}

/**
 * What the page said, as the `media` event's payload — `bisa:media` and
 * nothing else; a state the list does not know is `unstarted`; the title cut
 * as a window title is; an error carries YouTube's code and the page's words.
 * @param {unknown} message
 * @returns {{state: string, title: string, code?: string, words?: string} | null}
 */
export function stateOf(message) {
  if (!message || typeof message !== "object") return null;
  const m = /** @type {Record<string, unknown>} */ (message);
  if (m.type !== "bisa:media") return null;
  const state = typeof m.state === "string" && MEDIA_STATES.includes(m.state) ? m.state : "unstarted";
  /** @type {{state: string, title: string, code?: string, words?: string}} */
  const out = { state, title: cut(typeof m.title === "string" ? m.title : "", MAX_TITLE_CHARS) };
  if (state === "error") {
    out.code = cut(typeof m.code === "string" ? m.code : "", 20);
    out.words = cut(typeof m.words === "string" ? m.words : "", 200);
  }
  return out;
}
