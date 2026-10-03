/**
 * The role contract, translated for the page inspector's overlay.
 *
 * The outline, the tag label, the note box and the badges are drawn *inside*
 * a page — a sandboxed frame or a browser tab's native webview — where none of
 * the app's CSS exists. So the theme goes in as values: this module maps the
 * role tokens read off `<html>` to one `InspectorTheme`, and the theme to the
 * `cssText` each part of the overlay wears (`inspectorStyles`). The core
 * (`pageInspector.mjs`) never names a colour, a radius or a font; it applies
 * `STYLES[part]` — baked into its script as the page opens, and said again as
 * `bisa:theme` when the theme, the accent, the scheme or the type scale moves.
 *
 * Plain `.mjs` with a `.d.mts` beside it, like `terminal/xtermTheme.mjs`, so
 * `node --test` imports the real mapping and not a transcription of it.
 *
 * The overlay paints by `style.cssText` and not by a stylesheet: CSSOM is
 * blocked by no Content-Security-Policy — neither the frame's own nor a web
 * page's `style-src` — and the fake document the tests run the core against
 * has no stylesheet either. Hover and focus are therefore states the core
 * switches between (`part` and `part:hover` / `part:focus`), not selectors.
 */


/** The width the note box asks for; the page's viewport may give it less. */
export const BOX_WIDTH = 320;

/** The tokens the overlay reads off `<html>`, by their names in `theme/tokens.css`. */
export const INSPECTOR_ROLES = Object.freeze([
  "--color-surface",
  "--color-surface-2",
  "--color-border",
  "--color-text",
  "--color-text-dim",
  "--color-accent",
  "--color-accent-ink",
  "--color-accent-soft",
  "--color-accent-contrast",
  "--radius-control",
  "--radius-card",
  "--shadow-raised",
  "--shadow-floating",
  "--font-sans",
  "--font-mono",
  "--motion-fast",
  "--motion-ease",
]);

/**
 * What a role resolves to when the theme answers nothing — a neutral overlay
 * on the side the app landed on, so the box is never invisible. Only the
 * tests and a broken theme ever see these.
 */
const FALLBACK = Object.freeze({
  light: Object.freeze({
    "--color-surface": "#ffffff",
    "--color-surface-2": "#f4f4f5",
    "--color-border": "#d4d4d8",
    "--color-text": "#18181b",
    "--color-text-dim": "#71717a",
    "--color-accent": "#2563eb",
    "--color-accent-ink": "#1d4ed8",
    "--color-accent-soft": "rgba(37, 99, 235, 0.12)",
    "--color-accent-contrast": "#ffffff",
  }),
  dark: Object.freeze({
    "--color-surface": "#18181b",
    "--color-surface-2": "#27272a",
    "--color-border": "#3f3f46",
    "--color-text": "#fafafa",
    "--color-text-dim": "#a1a1aa",
    "--color-accent": "#60a5fa",
    "--color-accent-ink": "#93c5fd",
    "--color-accent-soft": "rgba(96, 165, 250, 0.18)",
    "--color-accent-contrast": "#0b1220",
  }),
  any: Object.freeze({
    "--radius-control": "8px",
    "--radius-card": "12px",
    "--shadow-raised": "0 1px 2px 0 rgba(0, 0, 0, 0.08)",
    "--shadow-floating": "0 10px 15px -3px rgba(0, 0, 0, 0.16), 0 4px 6px -4px rgba(0, 0, 0, 0.12)",
    "--font-sans": "system-ui, -apple-system, sans-serif",
    "--font-mono": "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
    "--motion-fast": "120ms",
    "--motion-ease": "cubic-bezier(0.2, 0, 0, 1)",
  }),
});

/** The type sizes at scale 1, in px — what the probe answers when there is no layout. */
export const FALLBACK_SIZES = Object.freeze({ size: 13, small: 12 });

/** Longer than any theme value — a runaway string is not a colour. */
const MAX_VALUE_CHARS = 200;

/**
 * A theme value the page may be handed, or null. The value lands in a
 * script and in `cssText` inside the page: nothing that could close a
 * declaration, a block or a tag, load a resource, or break the script's
 * template gets through. A theme file is first-party CSS; this is the wall
 * that lets the wire trust it without reading it.
 * @param {unknown} value
 */
export function safeCssValue(value) {
  if (typeof value !== "string") return null;
  const v = value.trim();
  if (!v || v.length > MAX_VALUE_CHARS) return null;
  if (/[;{}<>`\\]/.test(v)) return null;
  if (/url\s*\(|expression\s*\(|@import/i.test(v)) return null;
  return v;
}

/**
 * A colour with its alpha taken off. The overlay is drawn inside a page that
 * has no frost behind it, so glass's translucent `surface` would let the
 * page's own words read through the note box: its ground is the same colour,
 * solid. An opaque family's value, a keyword or anything nested
 * (`color-mix(…)`, `var(…)`) passes as it is.
 * @param {string} value a colour as the theme spells it
 */
export function opaque(value) {
  const m = /^(oklch|oklab|lch|lab|rgba?|hsla?|hwb|color)\((.*)\)$/i.exec(value.trim());
  if (!m || m[2].includes("(")) return value;
  const fn = m[1].toLowerCase();
  let body = m[2].trim();
  if (body.includes("/")) body = body.slice(0, body.lastIndexOf("/")).trim();
  else if (fn === "rgba" || fn === "hsla") body = body.split(",").slice(0, 3).join(",").trim();
  return `${fn === "rgba" || fn === "hsla" ? fn.slice(0, 3) : fn}(${body})`;
}

/** A px length from a probe's number, or the fallback at scale 1. */
function px(n, fallback) {
  return `${Number.isFinite(n) && n > 0 ? Math.round(n) : fallback}px`;
}

/**
 * The overlay's theme from the resolved roles.
 * @param {Readonly<Record<string, string>>} resolved each role's value as `getComputedStyle` gives it; `""` or absent for one no theme set
 * @param {"light" | "dark"} scheme the side the mounted theme landed on (`data-scheme`)
 * @param {{ size?: number, small?: number }} [sizes] `--text-xs` and `--text-2xs` as computed px — a `calc()` token resolves only on a live element
 */
export function inspectorTheme(resolved, scheme, sizes = {}) {
  const side = scheme === "dark" ? "dark" : "light";
  const role = (name) => safeCssValue(resolved?.[name]) ?? FALLBACK[side][name] ?? FALLBACK.any[name];
  return Object.freeze({
    scheme: side,
    color: Object.freeze({
      // Solid: nothing frosts behind a box drawn inside a page.
      surface: opaque(role("--color-surface")),
      surface2: role("--color-surface-2"),
      border: role("--color-border"),
      text: role("--color-text"),
      textDim: role("--color-text-dim"),
      accent: role("--color-accent"),
      accentInk: role("--color-accent-ink"),
      accentSoft: role("--color-accent-soft"),
      accentContrast: role("--color-accent-contrast"),
    }),
    radius: Object.freeze({ control: role("--radius-control"), card: role("--radius-card") }),
    shadow: Object.freeze({ raised: role("--shadow-raised"), floating: role("--shadow-floating") }),
    font: Object.freeze({
      ui: role("--font-sans"),
      mono: role("--font-mono"),
      size: px(sizes.size, FALLBACK_SIZES.size),
      small: px(sizes.small, FALLBACK_SIZES.small),
    }),
    motion: Object.freeze({ fast: role("--motion-fast"), ease: role("--motion-ease") }),
  });
}

/**
 * Every part of the overlay the core dresses, by the `data-bisa-inspector`
 * value it carries; `part:hover` and `part:focus` are the states the core
 * switches a part into and out of.
 */
export const STYLE_PARTS = Object.freeze([
  "outline",
  "label",
  "badges",
  "badge",
  "box",
  "box-head",
  "crumbs",
  "crumb",
  "crumb:hover",
  "crumb-sep",
  "crumb-current",
  "close",
  "close:hover",
  "box-text",
  "box-row",
  "input",
  "input:focus",
  "box-foot",
  "box-hint",
  "add",
  "add:hover",
]);

/** Above everything a page draws. */
const TOP = "z-index:2147483647";

/**
 * The `cssText` of every part, from the theme — the static half; the core
 * sets `display`, `top`, `left`, `width` and `height` on the parts it moves.
 * Each part starts from `all:initial` so a page's own `div {…}` reaches
 * nothing of ours, and restates its font and colour after it.
 * @param {ReturnType<typeof inspectorTheme>} t
 * @returns {Record<string, string>}
 */
export function inspectorStyles(t) {
  const c = t.color;
  const ease = (...props) => `transition:${props.map((p) => `${p} ${t.motion.fast} ${t.motion.ease}`).join(",")}`;
  const ui = (weight, size) => `font:${weight ? `${weight} ` : ""}${size}/1.4 ${t.font.ui}`;
  const mono = (size) => `font:${size}/1.5 ${t.font.mono}`;
  // The box speaks the kit's language: a floating surface (card radius, edge, floating shadow), a
  // quiet path of where the element sits, a ghost close drawn as two strokes in the text's own
  // colour, a field as the kit draws one, and the one primary button at the foot.
  const crumb = `all:initial;cursor:pointer;flex:0 4 auto;min-width:1.5em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;padding:1px 4px;border-radius:${t.radius.control};${mono(t.font.small)};color:${c.textDim};${ease("background-color", "color")}`;
  const stroke = (deg) => `linear-gradient(${deg}deg,transparent calc(50% - 0.75px),currentColor calc(50% - 0.75px),currentColor calc(50% + 0.75px),transparent calc(50% + 0.75px))`;
  const close = `all:initial;cursor:pointer;flex:0 0 auto;width:24px;height:24px;border-radius:${t.radius.control};color:${c.textDim};background-image:${stroke(45)},${stroke(-45)};background-size:10px 10px;background-position:center;background-repeat:no-repeat;${ease("background-color", "color")}`;
  const input = `all:initial;box-sizing:border-box;display:block;width:100%;margin-top:10px;padding:6px 10px;border:1px solid ${c.border};border-radius:${t.radius.control};background:${c.surface};${ui("", t.font.size)};color:${c.text};${ease("border-color", "box-shadow")}`;
  const add = `all:initial;box-sizing:border-box;cursor:pointer;flex:0 0 auto;height:28px;padding:0 12px;border-radius:${t.radius.control};background:${c.accent};color:${c.accentContrast};${ui(500, t.font.size)};line-height:28px;box-shadow:${t.shadow.raised};${ease("filter")}`;
  return {
    // An outline, never a fill: what you point at must stay readable under it. The thin ring
    // in the surface colour keeps the accent edge visible on a page of the same colour.
    outline: `all:initial;position:fixed;pointer-events:none;${TOP};display:none;box-sizing:border-box;border:2px solid ${c.accent};background:transparent;box-shadow:0 0 0 1px ${c.surface};border-radius:${t.radius.control};${ease("top", "left", "width", "height")}`,
    label: `all:initial;position:fixed;pointer-events:none;${TOP};display:none;box-sizing:border-box;max-width:60vw;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;padding:1px 6px;${mono(t.font.small)};color:${c.accentContrast};background:${c.accent};border-radius:${t.radius.control};box-shadow:${t.shadow.raised}`,
    badges: `all:initial;position:fixed;top:0;left:0;pointer-events:none;${TOP}`,
    // `all:initial` resets pointer-events to auto, so a badge says it again: it marks, never catches.
    badge: `all:initial;position:fixed;pointer-events:none;box-sizing:border-box;min-width:18px;height:18px;padding:0 5px;text-align:center;border-radius:999px;background:${c.accent};color:${c.accentContrast};font:600 ${t.font.small}/18px ${t.font.ui};box-shadow:0 0 0 2px ${c.surface},${t.shadow.raised}`,
    // The ground is the surface made solid (`opaque`): nothing frosts behind a box drawn in a page.
    box: `all:initial;position:fixed;${TOP};display:none;box-sizing:border-box;width:${BOX_WIDTH}px;max-width:calc(100vw - 16px);padding:12px 14px;border:1px solid ${c.border};border-radius:${t.radius.card};background:${c.surface};color:${c.text};color-scheme:${t.scheme};${ui("", t.font.size)};box-shadow:${t.shadow.floating};pointer-events:auto`,
    "box-head": "all:initial;display:flex;align-items:center;gap:6px;margin:-4px -6px 0 0;font:inherit;color:inherit",
    // One line: the nearest of the element's parents, the element last; a parent's crumb gives way (an ellipsis) before the element's does.
    crumbs: `all:initial;display:flex;flex:1 1 auto;min-width:0;flex-wrap:nowrap;overflow:hidden;white-space:nowrap;align-items:center;gap:1px;${mono(t.font.small)};color:${c.textDim}`,
    crumb,
    "crumb:hover": `${crumb};background-color:${c.surface2};color:${c.text}`,
    "crumb-sep": `all:initial;flex:0 0 auto;font:inherit;color:${c.textDim};opacity:0.6`,
    // Where you stand is neutral, never the accent: the element in full ink on the pressed step.
    "crumb-current": `all:initial;flex:0 1 auto;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;padding:1px 6px;border-radius:${t.radius.control};background:${c.surface2};${mono(t.font.small)};font-weight:600;color:${c.text}`,
    close,
    "close:hover": `${close};background-color:${c.surface2};color:${c.text}`,
    "box-text": `all:initial;display:block;margin:8px 0 0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;${ui("", t.font.small)};color:${c.textDim}`,
    "box-row": "all:initial;display:block;font:inherit",
    input,
    "input:focus": `${input};border-color:${c.accent};box-shadow:0 0 0 1px ${c.accent}`,
    "box-foot": "all:initial;display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:10px;font:inherit",
    "box-hint": `all:initial;flex:1 1 auto;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;${ui("", t.font.small)};color:${c.textDim}`,
    add,
    "add:hover": `${add};filter:brightness(0.92)`,
  };
}
