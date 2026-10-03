/**
 * The overlay's theme: every role it reads is one the theme contract
 * declares, a role no theme answers falls back to the app's side, nothing a
 * page could be hurt by gets through, and every part is dressed from the
 * theme alone. Run with `node --test desktop/src/ui/artifact/inspectorTheme.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { BOX_WIDTH, FALLBACK_SIZES, INSPECTOR_ROLES, STYLE_PARTS, inspectorStyles, inspectorTheme, opaque, safeCssValue } from "./inspectorTheme.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const TOKENS = readFileSync(join(HERE, "..", "..", "theme", "tokens.css"), "utf8");

/** The role names between the markers in `theme/tokens.css` — the contract every theme answers. */
function contract() {
  const region = TOKENS.match(/@roles:start\s*\*\/([\s\S]*?)\/\*\s*@roles:end/);
  assert.ok(region, "tokens.css lost its @roles:start / @roles:end markers");
  return new Set([...region[1].matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
}

test("every role the overlay reads is a token the theme declares", () => {
  const roles = contract();
  for (const role of INSPECTOR_ROLES) {
    const declared = roles.has(role) || new RegExp(`${role}\\s*:`).test(TOKENS);
    assert.ok(declared, `${role} is declared in tokens.css`);
  }
  assert.equal(new Set(INSPECTOR_ROLES).size, INSPECTOR_ROLES.length, "no role twice");
  for (const token of ["--text-xs", "--text-2xs"]) assert.ok(new RegExp(`${token}\\s*:`).test(TOKENS), `${token} exists for the probe to measure`);
});

test("a role no theme answers falls back to the app's side, and every leaf is a non-empty string", () => {
  const light = inspectorTheme({}, "light");
  const dark = inspectorTheme({}, "dark");
  const leaves = (t) => [t.scheme, ...Object.values(t.color), ...Object.values(t.radius), ...Object.values(t.shadow), ...Object.values(t.font), ...Object.values(t.motion)];
  for (const leaf of [...leaves(light), ...leaves(dark)]) assert.ok(typeof leaf === "string" && leaf.length > 0);
  assert.notEqual(light.color.surface, dark.color.surface, "the fallbacks know the side");
  assert.equal(light.radius.control, dark.radius.control, "a shape is not a side");
  assert.equal(inspectorTheme({}, "sideways").scheme, "light", "an unknown side reads as light");
  assert.equal(light.font.size, `${FALLBACK_SIZES.size}px`);
  assert.equal(inspectorTheme({}, "light", { size: 15.6, small: 0 }).font.size, "16px", "a probe's px is rounded");
  assert.equal(inspectorTheme({}, "light", { size: 15.6, small: 0 }).font.small, `${FALLBACK_SIZES.small}px`, "a probe with no layout falls back");
  const answered = inspectorTheme({ "--color-accent": "oklch(0.6 0.2 250)", "--radius-card": " 14px " }, "light");
  assert.equal(answered.color.accent, "oklch(0.6 0.2 250)", "a value passes as the theme spells it");
  assert.equal(answered.radius.card, "14px", "trimmed");
  assert.equal(answered.color.surface, light.color.surface, "only the missing role falls back");
  assert.ok(Object.isFrozen(answered) && Object.isFrozen(answered.color));
});

test("the box's ground is the surface made solid — nothing frosts behind a box drawn inside a page", () => {
  assert.equal(opaque("oklch(0.995 0.003 235 / 0.72)"), "oklch(0.995 0.003 235)", "glass's translucent surface, solid");
  assert.equal(opaque("oklch(0.25 0.014 235 / 0.7)"), "oklch(0.25 0.014 235)");
  assert.equal(opaque("rgba(1, 2, 3, 0.5)"), "rgb(1, 2, 3)");
  assert.equal(opaque("rgb(0 0 0 / 0.5)"), "rgb(0 0 0)");
  for (const kept of ["oklch(0.6 0.2 250)", "#fff", "white", "color-mix(in oklch, red 50%, blue)"]) assert.equal(opaque(kept), kept, `${kept} passes as it is`);
  assert.equal(inspectorTheme({ "--color-surface": "oklch(0.99 0 0 / 0.72)" }, "light").color.surface, "oklch(0.99 0 0)", "the overlay's surface is the solid one");
});

test("nothing that could break the script, a declaration or a tag gets through as a value", () => {
  for (const bad of ["", "   ", "red; background:url(x)", "a}", "{b", "<style>", "url(http://x)", "URL (x)", "expression(1)", "a`b", "back\\slash", "@import 'x'", "x".repeat(201)]) {
    assert.equal(safeCssValue(bad), null, `${JSON.stringify(bad)} is refused`);
  }
  for (const good of ["oklch(1 0 0)", "rgb(0 0 0 / 0.5)", "#fff", "8px", '"Inter", system-ui, sans-serif', "0 1px 2px 0 rgb(0 0 0 / 0.05), 0 0 0 1px red", "cubic-bezier(0.2, 0, 0, 1)", "120ms"]) {
    assert.equal(safeCssValue(good), good.trim(), `${good} passes`);
  }
  assert.equal(safeCssValue(12), null, "a number is not a value");
  assert.equal(inspectorTheme({ "--color-accent": "red;x" }, "light").color.accent, inspectorTheme({}, "light").color.accent, "a refused role falls back");
});

test("every part is dressed from the theme alone, from all:initial, and the box follows the app's side", () => {
  const sentinel = { scheme: "dark", color: {}, radius: {}, shadow: {}, font: {}, motion: {} };
  const t = inspectorTheme({}, "light");
  for (const group of ["color", "radius", "shadow", "font", "motion"]) for (const k of Object.keys(t[group])) sentinel[group][k] = `S-${group}-${k}`.toUpperCase();
  const styles = inspectorStyles(sentinel);
  assert.deepEqual(Object.keys(styles).sort(), [...STYLE_PARTS].sort(), "exactly the parts, no more");
  for (const [part, css] of Object.entries(styles)) {
    assert.ok(css.startsWith("all:initial;"), `${part} starts from nothing of the page's`);
    assert.ok(!/#[0-9a-fA-F]{3,8}\b|\brgba?\(|system-ui|monospace/.test(css), `${part} names no colour or font of its own`);
    assert.ok(!css.includes(";;"), `${part} is one declaration after another`);
  }
  assert.ok(styles.box.includes("S-COLOR-SURFACE") && styles.box.includes("S-COLOR-BORDER") && styles.box.includes("S-RADIUS-CARD") && styles.box.includes("S-SHADOW-FLOATING") && styles.box.includes("S-FONT-UI"));
  assert.ok(styles.box.includes("color-scheme:dark"), "the form control inside follows the app's side");
  assert.ok(styles.box.includes(`width:${BOX_WIDTH}px`) && styles.box.includes("max-width:calc(100vw - 16px)"), "the box asks for its width and takes less");
  assert.ok(styles.outline.includes("border:2px solid S-COLOR-ACCENT") && styles.outline.includes("S-MOTION-FAST"), "the outline is the accent's edge, and glides");
  // The soft accent is opaque in every real theme: a fill would hide the very element pointed at.
  assert.ok(styles.outline.includes("background:transparent") && !styles.outline.includes("S-COLOR-ACCENTSOFT"), "an outline, never a fill — what you point at stays readable");
  assert.ok(styles.outline.includes("box-shadow:0 0 0 1px S-COLOR-SURFACE"), "a thin surface ring keeps the edge visible on a page of the accent's colour");
  // `all:initial` resets pointer-events to auto: every part that only marks says none again.
  for (const part of ["outline", "label", "badges", "badge"]) assert.ok(styles[part].includes("pointer-events:none"), `${part} never catches the pointer`);
  assert.ok(styles.label.includes("S-FONT-MONO") && styles.label.includes("S-COLOR-ACCENTCONTRAST"));
  assert.ok(styles.badge.includes("S-COLOR-ACCENT") && styles.badge.includes("S-COLOR-SURFACE"), "a badge is ringed in the surface so it sits on any page");
  // Where you stand is neutral (DESIGN, The Neutral Here Rule): the element in full ink on the pressed step, never the accent.
  assert.ok(styles["crumb-current"].includes("S-COLOR-TEXT") && styles["crumb-current"].includes("S-COLOR-SURFACE2") && !styles["crumb-current"].includes("S-COLOR-ACCENT"));
  // The close is drawn, never a glyph from a font: two strokes in the text's own colour.
  assert.ok(styles.close.includes("currentColor") && styles.close.includes("background-image:"), "the close draws its own mark");
  assert.ok(styles.add.includes("S-COLOR-ACCENT") && styles.add.includes("S-SHADOW-RAISED") && styles.add.includes("height:28px"), "Add is the kit's small primary button");
  assert.ok(styles["box-hint"].includes("S-COLOR-TEXTDIM") && styles["box-foot"].includes("justify-content:space-between"), "the foot says the keys and holds the one button");
  for (const [base, state] of [["crumb", "crumb:hover"], ["close", "close:hover"], ["input", "input:focus"], ["add", "add:hover"]]) {
    assert.ok(styles[state].startsWith(styles[base]) && styles[state].length > styles[base].length, `${state} is ${base} with more`);
  }
  assert.ok(styles["input:focus"].includes("box-shadow:0 0 0 1px S-COLOR-ACCENT"), "focus is the kit's one-pixel accent ring");
  assert.ok(styles["add:hover"].includes("brightness"), "a hovered Add darkens rather than borrowing a text role");
});
