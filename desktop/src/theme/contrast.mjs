/**
 * Colour arithmetic for the contrast promise, with no DOM in it.
 *
 * A theme's values are `oklch()` strings. To hold them to a legibility rule
 * they have to become the numbers WCAG 2 speaks in — relative luminance and
 * the ratio between two of them — so this file parses an `oklch()` token,
 * walks OKLab → LMS → linear sRGB with the standard matrices, clamps to the
 * gamut the way a browser would, and returns the ratio. A glass family's
 * surfaces carry alpha, so `composite` lays one over another in linear light
 * first — the colour a reader actually sees. `themes.test.mjs` is the only
 * caller; nothing at runtime needs it, because the promise is kept where the
 * values are written.
 */

/**
 * Parse `oklch(L C H)` or `oklch(L C H / A)`; `L` may be a percentage. Null
 * for anything else — a hex or an `rgb()` is not a theme value here.
 * @param {string} text
 * @returns {{l: number, c: number, h: number, alpha: number} | null}
 */
export function parseOklch(text) {
  const m = /^\s*oklch\(\s*([\d.]+)(%?)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)(%?)\s*)?\)\s*$/i.exec(String(text ?? ""));
  if (!m) return null;
  const l = Number(m[1]) / (m[2] ? 100 : 1);
  const alpha = m[5] === undefined ? 1 : Number(m[5]) / (m[6] ? 100 : 1);
  return { l, c: Number(m[3]), h: Number(m[4]), alpha };
}

/** OKLCH → linear sRGB, each channel clamped to 0..1 (the gamut a display has). */
export function oklchToLinearSrgb({ l, c, h }) {
  const rad = (h * Math.PI) / 180;
  const a = c * Math.cos(rad);
  const b = c * Math.sin(rad);
  const l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = l - 0.0894841775 * a - 1.291485548 * b;
  const L = l_ ** 3;
  const M = m_ ** 3;
  const S = s_ ** 3;
  const clamp = (v) => Math.min(1, Math.max(0, v));
  return {
    r: clamp(4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S),
    g: clamp(-1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S),
    b: clamp(-0.0041960863 * L - 0.7034186147 * M + 1.707614701 * S),
  };
}

/**
 * The colour a translucent `over` shows on an opaque `under`: source-over in
 * linear light, which is how a webview paints one `oklch(… / a)` surface on
 * another. Returned as an `oklch()` string so every other function here reads
 * it like a theme value. `under`'s own alpha is ignored — it is the ground.
 * An opaque `over` composites to itself; a value that is not a colour is
 * returned as written.
 * @param {string} over
 * @param {string} under
 * @returns {string}
 */
export function composite(over, under) {
  const top = parseOklch(over);
  const ground = parseOklch(under);
  if (!top || !ground) return over;
  if (top.alpha >= 1) return over;
  const a = oklchToLinearSrgb(top);
  const b = oklchToLinearSrgb(ground);
  const mix = (x, y) => top.alpha * x + (1 - top.alpha) * y;
  return linearSrgbToOklch({ r: mix(a.r, b.r), g: mix(a.g, b.g), b: mix(a.b, b.b) });
}

/** Linear sRGB → an `oklch()` string, the inverse of `oklchToLinearSrgb`. */
function linearSrgbToOklch({ r, g, b }) {
  const l_ = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m_ = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s_ = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l_ + 0.793617785 * m_ - 0.0040720468 * s_;
  const A = 1.9779984951 * l_ - 2.428592205 * m_ + 0.4505937099 * s_;
  const B = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.808675766 * s_;
  const c = Math.hypot(A, B);
  const h = c < 1e-6 ? 0 : ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360;
  const n = (v) => Number(v.toFixed(5));
  return `oklch(${n(L)} ${n(c)} ${n(h)})`;
}

/** WCAG relative luminance of an `oklch()` string; 0 for a value that is not one. */
export function luminance(text) {
  const c = parseOklch(text);
  if (!c) return 0;
  const { r, g, b } = oklchToLinearSrgb(c);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG 2 contrast ratio between two `oklch()` strings, ≥ 1. */
export function contrast(a, b) {
  const la = luminance(a);
  const lb = luminance(b);
  const [hi, lo] = la >= lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

// Read by `themes.test.mjs`: holds every theme's surfaces and borders to a visible step; the app never measures it.
/** The lightness step between two `oklch()` strings, 0..1. */
export function deltaL(a, b) {
  const ca = parseOklch(a);
  const cb = parseOklch(b);
  if (!ca || !cb) return 0;
  return Math.abs(ca.l - cb.l);
}

// Read by `themes.test.mjs`: holds every theme's accent apart from its danger and warn hues; the app never measures it.
/** The shortest hue distance between two `oklch()` strings, 0..180 degrees. */
export function hueDistance(a, b) {
  const ca = parseOklch(a);
  const cb = parseOklch(b);
  if (!ca || !cb) return 0;
  const d = Math.abs(ca.h - cb.h) % 360;
  return d > 180 ? 360 - d : d;
}

/** The linear sRGB of an `oklch()` string as `#rrggbb`, for a fixture or a message. */
export function toHex(text) {
  const c = parseOklch(text);
  if (!c) return null;
  const { r, g, b } = oklchToLinearSrgb(c);
  const gamma = (v) => (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055);
  const byte = (v) => Math.round(gamma(v) * 255).toString(16).padStart(2, "0");
  return `#${byte(r)}${byte(g)}${byte(b)}`;
}
