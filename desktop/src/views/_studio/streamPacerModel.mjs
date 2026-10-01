/**
 * The pace a streamed reply is revealed at (13 — Conversations §The reply
 * streams). The engine sends one frame per `FRAME_MS`, many characters at a
 * time; shown as it lands, a reply moves in jumps. The pacer spreads what is
 * left of a frame's backlog over what is left of the interval since the frame
 * landed (`sinceMs`), so the words appear at a steady rate and are caught up
 * exactly when the next frame is due — never more than one interval behind
 * the engine. A backlog past `JUMP_CHARS` (a reader primed mid-turn), or a
 * turn that ended, is shown at once. Plain `.mjs`, so `node --test` reads it.
 */

/** The engine's flush interval, in ms — what a backlog is spread over. */
export const FRAME_MS = 120;
/** Never slower than this, chars per ms (125 chars a second), so a trickle still moves. */
export const MIN_RATE = 1 / 8;
/** A backlog this long is not typed out — it is shown. */
export const JUMP_CHARS = 2000;

/**
 * The next number of characters to show. `sinceMs` is how long ago the
 * frame that set `target` landed: the backlog left is spread over the
 * interval left, so the reveal lands on the target as the next frame is due.
 * @param {{ shown: number, target: number, dtMs: number, sinceMs?: number, frameMs?: number, done?: boolean }} step
 * @returns {number}
 */
export function pace({ shown, target, dtMs, sinceMs = 0, frameMs = FRAME_MS, done = false }) {
  const have = Math.max(0, Number(shown) || 0);
  const want = Math.max(0, Number(target) || 0);
  const backlog = want - have;
  if (backlog <= 0) return want;
  if (done || backlog > JUMP_CHARS) return want;
  const remaining = Math.max(1, frameMs - Math.max(0, Number(sinceMs) || 0));
  const rate = Math.max(MIN_RATE, backlog / remaining);
  const step = Math.max(1, Math.ceil(rate * Math.max(0, Number(dtMs) || 0)));
  return Math.min(want, have + step);
}

/**
 * The first `n` characters of `text`, never splitting a surrogate pair — a
 * cut inside an emoji would draw a broken glyph for one frame.
 * @param {string} text
 * @param {number} n
 */
export function reveal(text, n) {
  const s = String(text ?? "");
  let end = Math.max(0, Math.min(s.length, Math.floor(Number(n) || 0)));
  if (end > 0 && end < s.length) {
    const code = s.charCodeAt(end - 1);
    if (code >= 0xd800 && code <= 0xdbff) end -= 1;
  }
  return s.slice(0, end);
}
