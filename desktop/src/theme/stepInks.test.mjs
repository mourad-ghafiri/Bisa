/**
 * The step families' inks (`tokens.css`, `--color-step-*`): the colour a
 * step's kind glyph wears, so a workflow scans by family — events,
 * gateways, loops, tasks — beside the shape that already tells them apart.
 *
 * Three promises, held here because a palette is edited by eye and these
 * break silently:
 *
 * - **A kind is never a summons or a state.** Every ink is a tinted
 *   neutral (chroma ≤ 0.08, against the 0.12 and more of every accent and
 *   status), and a hued ink keeps 30° from the three status hues and from
 *   every family's own accent.
 * - **An accent overlay moves the ink, never the other way round.** With
 *   violet, teal or rose chosen, every ink in force keeps 30° from it.
 * - **A glyph can be seen.** Every ink holds 3:1 — the floor for a glyph —
 *   against every family's card on its side, a glass card laid over its
 *   ground over white (light) or black (dark), since what is behind the
 *   window is unknowable.
 */

import test from "node:test";
import { strict as assert } from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { composite, contrast, hueDistance, parseOklch } from "./contrast.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const FAMILIES = ["event", "gateway", "loop", "task"];
const MIN_HUE = 30;
const MAX_CHROMA = 0.08;
/** Below this an ink has no hue to speak of (the tasks' slate): it is held to the floor, not to the distances. */
const NEUTRAL = 0.03;

const strip = (css) => css.replace(/\/\*[\s\S]*?\*\//g, "");

/** Every innermost block as `{ selector, props }`, a nested `@media` block's own selector included. */
function blocks(css) {
  const out = [];
  for (const m of strip(css).matchAll(/([^{}]*)\{([^{}]*)\}/g)) {
    const props = new Map();
    for (const d of m[2].matchAll(/(--[\w-]+|color-scheme)\s*:\s*([^;]+);/g)) props.set(d[1], d[2].trim());
    out.push({ selector: m[1].trim(), props });
  }
  return out;
}

const isDark = (selector) => selector.includes('data-scheme="dark"') || selector.includes(':not([data-scheme="light"])');
const accentOf = (selector) => /data-accent="(\w+)"/.exec(selector)?.[1] ?? null;

const tokens = blocks(readFileSync(join(HERE, "tokens.css"), "utf8"));

/** The inks in force on one side, with an overlay's moves applied when one is chosen. */
function inks(side, accent = null) {
  const out = {};
  for (const { selector, props } of tokens) {
    const sideOk = side === "dark" ? isDark(selector) || selector === "@theme" : !isDark(selector);
    if (!sideOk) continue;
    const own = accentOf(selector);
    if (own !== null && own !== accent) continue;
    for (const f of FAMILIES) {
      const v = props.get(`--color-step-${f}`);
      if (!v) continue;
      // The light side's `@theme` values are the base the dark side's blocks replace.
      if (side === "dark" && selector === "@theme" && out[f]) continue;
      out[f] = v;
    }
  }
  return out;
}

/** The roles every theme file declares, per selector. */
const themes = readdirSync(join(HERE, "themes"))
  .filter((f) => f.endsWith(".css"))
  .flatMap((f) => blocks(readFileSync(join(HERE, "themes", f), "utf8")).map((b) => ({ file: f, ...b })))
  .filter((b) => b.props.has("--color-surface"));

const statusHues = [...new Set(themes.flatMap((t) => ["--color-ok", "--color-warn", "--color-danger"].map((r) => t.props.get(r))))].filter(Boolean);
const familyAccents = [...new Set(themes.map((t) => t.props.get("--color-accent")))].filter(Boolean);
const overlays = blocks(readFileSync(join(HERE, "accents.css"), "utf8")).filter((b) => b.props.has("--color-accent"));

test("there are four inks on each side, and the dark side replaces every one", () => {
  // The other tests read these sets; empty ones would pass them all.
  assert.ok(themes.length >= 10, "every family's light and dark side is read");
  assert.ok(overlays.length >= 3 && statusHues.length >= 3 && familyAccents.length >= 5, "the overlays, statuses and accents are read");
  for (const side of ["light", "dark"]) {
    const set = inks(side);
    for (const f of FAMILIES) assert.ok(set[f] && parseOklch(set[f]), `${side}: --color-step-${f} is an oklch() value`);
  }
  assert.notDeepEqual(inks("light"), inks("dark"), "the dark side has inks of its own");
});

test("every ink is a tinted neutral, never as saturated as an accent or a status", () => {
  for (const side of ["light", "dark"]) {
    for (const accent of [null, ...new Set(overlays.map((o) => accentOf(o.selector)))]) {
      for (const [f, v] of Object.entries(inks(side, accent))) {
        assert.ok(parseOklch(v).c <= MAX_CHROMA, `${side}/${accent ?? "own"}: ${f} ${v} has chroma over ${MAX_CHROMA}`);
      }
    }
  }
});

test("a hued ink keeps 30° from every status hue and every family's own accent", () => {
  for (const side of ["light", "dark"]) {
    for (const [f, v] of Object.entries(inks(side))) {
      if (parseOklch(v).c <= NEUTRAL) continue;
      for (const s of statusHues) assert.ok(hueDistance(v, s) >= MIN_HUE, `${side}: ${f} ${v} sits ${hueDistance(v, s).toFixed(0)}° from the status ${s}`);
      for (const a of familyAccents) assert.ok(hueDistance(v, a) >= MIN_HUE, `${side}: ${f} ${v} sits ${hueDistance(v, a).toFixed(0)}° from the accent ${a}`);
    }
  }
});

test("a chosen accent overlay keeps 30° from every hued ink in force with it", () => {
  for (const o of overlays) {
    const accent = accentOf(o.selector);
    const side = isDark(o.selector) ? "dark" : "light";
    const value = o.props.get("--color-accent");
    for (const [f, v] of Object.entries(inks(side, accent))) {
      if (parseOklch(v).c <= NEUTRAL) continue;
      assert.ok(hueDistance(v, value) >= MIN_HUE, `${accent} (${side}): ${f} ${v} sits ${hueDistance(v, value).toFixed(0)}° from the accent ${value}`);
    }
  }
});

test("every ink holds 3:1 on every family's card, on its own side", () => {
  for (const t of themes) {
    const dark = t.props.get("color-scheme") === "dark";
    const behind = dark ? "oklch(0 0 0)" : "oklch(1 0 0)";
    const card = composite(t.props.get("--color-surface"), composite(t.props.get("--color-bg"), behind));
    for (const [f, v] of Object.entries(inks(dark ? "dark" : "light"))) {
      const ratio = contrast(v, card);
      assert.ok(ratio >= 3, `${t.file} ${t.selector}: ${f} ${v} is ${ratio.toFixed(2)}:1 on its card`);
    }
  }
});
