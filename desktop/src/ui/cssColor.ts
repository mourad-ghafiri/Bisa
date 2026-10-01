/**
 * CSS colours as `#rrggbb`, through the one parser every webview ships.
 *
 * A custom property is not resolved to a colour: `--color-bg` computes to the
 * literal string `oklch(0.975 0.006 240)`, because CSS keeps custom properties
 * as tokens until something substitutes them into a real property. Monaco and
 * Mermaid each parse a short list of colour syntaxes by hand, and `oklch()` is
 * on neither list. A canvas' `fillStyle` accepts anything the page could
 * paint and reads back as hex — the conversion that is guaranteed to agree
 * with what the rest of the app is drawing, because it is the same parser.
 */

let probe: CanvasRenderingContext2D | null | undefined;

function context(): CanvasRenderingContext2D | null {
  if (probe === undefined) {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    probe = canvas.getContext("2d", { willReadFrequently: true });
  }
  return probe;
}

/** One CSS colour → `#rrggbb` (or `#rrggbbaa` when translucent), or `null` when the browser would not take it. */
export function colorToHex(value: string): string | null {
  const css = value.trim();
  if (!css) return null;
  const ctx = context();
  if (!ctx) return null;
  try {
    // Assigning an invalid value is a no-op, so the getter would hand back
    // what was there before. Trying from both ends tells "parsed to black"
    // from "did not parse".
    ctx.fillStyle = "#000000";
    ctx.fillStyle = css;
    const fromBlack = ctx.fillStyle;
    ctx.fillStyle = "#ffffff";
    ctx.fillStyle = css;
    if (typeof fromBlack !== "string" || fromBlack !== ctx.fillStyle) return null;
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
    const hex = (n: number | undefined) => (n ?? 0).toString(16).padStart(2, "0");
    return a === 255 ? `#${hex(r)}${hex(g)}${hex(b)}` : `#${hex(r)}${hex(g)}${hex(b)}${hex(a)}`;
  } catch {
    return null;
  }
}

/** The named roles as `<html>` currently resolves them, in hex; a role that did not resolve is absent. */
export function resolvedRoles(roles: readonly string[]): Record<string, string> {
  const cs = getComputedStyle(document.documentElement);
  const out: Record<string, string> = {};
  for (const role of roles) {
    const hex = colorToHex(cs.getPropertyValue(role));
    if (hex) out[role] = hex;
  }
  return out;
}
