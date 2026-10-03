/**
 * The identicon's colour: the same pubkey is the same colour in every
 * surface, so a principal is recognisable before you read a name (`Avatar`).
 *
 * The ground sits under white initials, so its lightness is set by them:
 * at 0.62 the letters read at 3.3:1 in the greenest hue, under the 4.5:1
 * the app holds its text to; at `AVATAR_LIGHTNESS` the worst hue reads
 * 4.8:1 (`avatarModel.test.mjs` measures every one). Plain `.mjs`, so
 * `node --test` reads it.
 */

/** Identicon hues, deliberately skipping 270–330: the app has no purple in it and a stray violet avatar would be the only one on screen. */
export const AVATAR_HUES = [12, 40, 68, 95, 130, 155, 185, 205, 235, 258];
/** The ground's lightness and chroma, in OKLCH: dark enough for white initials at 4.5:1 in every hue. */
export const AVATAR_LIGHTNESS = 0.52;
export const AVATAR_CHROMA = 0.13;

/** FNV-1a over the id: stable across sessions and machines. */
function hash(s) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h);
}

/** The hue an id wears. @param {string} id */
export function principalHue(id) {
  return AVATAR_HUES[hash(id) % AVATAR_HUES.length];
}

/** The ground an id's identicon is drawn on. @param {string} id */
export function principalColor(id) {
  return `oklch(${AVATAR_LIGHTNESS} ${AVATAR_CHROMA} ${principalHue(id)})`;
}
