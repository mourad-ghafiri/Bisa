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
      surface: role("--color-surface"),
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
  const crumb = `all:initial;cursor:pointer;padding:0 6px;border-radius:${t.radius.control};background:${c.surface2};${mono(t.font.small)};color:${c.textDim};${ease("background", "color")}`;
  const close = `all:initial;cursor:pointer;flex:0 0 auto;width:20px;height:20px;border-radius:${t.radius.control};text-align:center;font:14px/20px ${t.font.ui};color:${c.textDim};${ease("background", "color")}`;
  const input = `all:initial;box-sizing:border-box;flex:1 1 auto;min-width:0;padding:6px 8px;border:1px solid ${c.border};border-radius:${t.radius.control};background:${c.surface};${ui("", t.font.size)};color:${c.text};${ease("border-color", "box-shadow")}`;
  const add = `all:initial;cursor:pointer;flex:0 0 auto;padding:5px 10px;border-radius:${t.radius.control};background:${c.accent};color:${c.accentContrast};${ui(600, t.font.size)};${ease("filter")}`;
  return {
    outline: `all:initial;position:fixed;pointer-events:none;${TOP};display:none;box-sizing:border-box;border:2px solid ${c.accent};background:${c.accentSoft};border-radius:${t.radius.control};${ease("top", "left", "width", "height")}`,
    label: `all:initial;position:fixed;pointer-events:none;${TOP};display:none;box-sizing:border-box;max-width:60vw;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;padding:1px 6px;${mono(t.font.small)};color:${c.accentContrast};background:${c.accent};border-radius:${t.radius.control};box-shadow:${t.shadow.raised}`,
    badges: `all:initial;position:fixed;top:0;left:0;pointer-events:none;${TOP}`,
    badge: `all:initial;position:fixed;box-sizing:border-box;min-width:18px;height:18px;padding:0 5px;text-align:center;border-radius:999px;background:${c.accent};color:${c.accentContrast};font:600 ${t.font.small}/18px ${t.font.ui};box-shadow:0 0 0 2px ${c.surface},${t.shadow.raised}`,
    box: `all:initial;position:fixed;${TOP};display:none;box-sizing:border-box;width:${BOX_WIDTH}px;max-width:calc(100vw - 16px);padding:10px 12px;border:1px solid ${c.border};border-radius:${t.radius.card};background:${c.surface};color:${c.text};color-scheme:${t.scheme};${ui("", t.font.size)};box-shadow:${t.shadow.floating};pointer-events:auto`,
    "box-head": "all:initial;display:flex;align-items:center;gap:6px;font:inherit;color:inherit",
    crumbs: `all:initial;display:flex;flex:1 1 auto;min-width:0;flex-wrap:wrap;align-items:center;gap:3px;${mono(t.font.small)};color:${c.textDim}`,
    crumb,
    "crumb:hover": `${crumb};background:${c.border};color:${c.text}`,
    "crumb-sep": `all:initial;font:inherit;color:${c.textDim};opacity:0.7`,
    "crumb-current": `all:initial;padding:0 6px;border-radius:${t.radius.control};background:${c.accentSoft};${mono(t.font.small)};font-weight:600;color:${c.accentInk}`,
    close,
    "close:hover": `${close};background:${c.surface2};color:${c.text}`,
    "box-text": `all:initial;display:block;margin:8px 0 0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;${ui("", t.font.size)};color:${c.textDim}`,
    "box-row": "all:initial;display:flex;align-items:center;gap:6px;margin-top:8px;font:inherit",
    input,
    "input:focus": `${input};border-color:${c.accent};box-shadow:0 0 0 1px ${c.accent}`,
    add,
    "add:hover": `${add};filter:brightness(0.92)`,
  };
}
