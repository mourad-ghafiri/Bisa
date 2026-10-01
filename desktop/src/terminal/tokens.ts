/**
 * Reading the theme out of the page, in a form xterm will accept.
 *
 * The DOM half of `xtermTheme.mjs` — kept apart from it so the mapping stays a
 * pure function with tests, and so the one genuinely browser-shaped problem
 * here has somewhere to be explained.
 *
 * That problem: a custom property is **not** resolved to a colour. `--color-bg`
 * computes to the literal string `oklch(0.98 0 0)`, because CSS keeps custom
 * properties as tokens until something substitutes them into a real property.
 * xterm parses a short list of colour syntaxes by hand and throws on the rest,
 * and `oklch()` is on the rest. Handing it a token value is how a terminal
 * fails to construct at all.
 *
 * So the browser converts. `canvas`' `fillStyle` accepts any colour the page
 * could use — every colour space, every function, every keyword — and reads
 * back as `#rrggbb`. It is the one conversion that is guaranteed to agree with
 * what the rest of the app is painting, because it is the same parser.
 */

import { TERMINAL_ROLES } from "./xtermTheme.mjs";
import { t } from "../i18n/l10n.mjs";

let probe: CanvasRenderingContext2D | null | undefined;

function context(): CanvasRenderingContext2D | null {
  if (probe === undefined) probe = document.createElement("canvas").getContext("2d");
  return probe;
}

/**
 * One CSS colour → `#rrggbb`, or `""` when the browser would not take it.
 *
 * The two sentinels are how a rejection is detected: assigning an invalid
 * value to `fillStyle` is a no-op, so the getter would hand back whatever was
 * there before and a broken token would silently become black. Trying twice
 * from opposite ends and comparing is the only way to tell "it parsed to
 * black" from "it did not parse".
 */
export function toHex(value: string): string {
  const css = value.trim();
  if (!css) return "";
  const ctx = context();
  if (!ctx) return "";
  try {
    ctx.fillStyle = "#000000";
    ctx.fillStyle = css;
    const fromBlack = ctx.fillStyle;
    ctx.fillStyle = "#ffffff";
    ctx.fillStyle = css;
    const fromWhite = ctx.fillStyle;
    if (typeof fromBlack !== "string" || fromBlack !== fromWhite) return "";
    return fromBlack;
  } catch {
    return "";
  }
}

/** Every role the terminal reads, resolved off `<html>`. */
export function readTerminalTokens(): Record<string, string> {
  const style = getComputedStyle(document.documentElement);
  const out: Record<string, string> = {};
  for (const role of TERMINAL_ROLES) out[role] = toHex(style.getPropertyValue(role));
  return out;
}

/**
 * The side the mounted theme actually landed on.
 *
 * `data-scheme` is the authority — `shell/theme.ts` stamps it, and a dark
 * theme chosen on a light machine is still dark. The media query is the
 * fallback for the frame before any script has run.
 */
export function currentScheme(): "light" | "dark" {
  const stamped = document.documentElement.getAttribute("data-scheme");
  if (stamped === "dark" || stamped === "light") return stamped;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/**
 * The terminal's font size in px, taken from whatever the element inherited.
 *
 * Read off the host rather than from `--text-sm`, which is a `calc()` against
 * a virtual rem and would come back as unresolved token text. Computed
 * `font-size` is always px, which means the appearance panel's type-scale dial
 * moves the terminal along with everything else, for free.
 */
export function fontSizeOf(el: Element): number {
  const size = Number.parseFloat(getComputedStyle(el).fontSize);
  return Number.isFinite(size) && size > 0 ? size : 13;
}

/** The mono stack the rest of the app uses, or a stack that always exists. */
export function monoFamily(): string {
  const family = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
  return family || t("terminal-tokens-ui-monospace-sfmono-regular-menlo-consolas");
}
