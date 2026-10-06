/**
 * The contrast promise, and the one theme-id list.
 *
 * `roles.test.mjs` says every theme answers every role; this file says the
 * answers are readable. People sit in front of this window for hours, so the
 * bars are set for reading, not for passing: body text at 7:1 on the page and
 * on a card, secondary text at 4.5:1, an accent's ink at 4.5:1 on a card, the
 * text on an accent or a danger fill at 4.5:1, status colours told apart from
 * the page at 3:1, a raised surface a visible step off the page, a border a
 * visible step off a card, and an accent far enough from `danger` and `warn`
 * in hue that a summons and a warning never blur.
 *
 * A glass family's surfaces carry alpha, so every bar is measured over the
 * **composite** — `surface` laid over `bg`, and `bg` laid over both white
 * and black, because what is behind a transparent window is unknowable and
 * the floors must hold on either. An opaque family composites to itself, so
 * its bars are the plain ones. The same file pins each family's material to
 * its word: an opaque family's colours carry no alpha and its blur is `0px`;
 * a glass family blurs; `material.css` frosts glass ids and nothing else.
 *
 * The same file holds the three copies of the theme-id list to one another:
 * the selectors in `themes/*.css`, `THEMES` in `shell/theme.ts` and the
 * `appearance.theme` choices in the Rust registry. A theme any one of them
 * does not know is a preference the node refuses or a tile that paints grey.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { composite, contrast, deltaL, hueDistance, parseOklch } from "./contrast.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const THEMES_DIR = join(HERE, "themes");
const DEFAULT_FAMILY = "glass";

function strip(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Every declaration block, flattened out of at-rules, as `{selector, values}`. */
function blocks(css) {
  const out = [];
  const walk = (text) => {
    let i = 0;
    while (i < text.length) {
      const open = text.indexOf("{", i);
      if (open === -1) return;
      let depth = 1;
      let close = open + 1;
      while (close < text.length && depth > 0) {
        if (text[close] === "{") depth++;
        else if (text[close] === "}") depth--;
        close++;
      }
      const head = text.slice(i, open).trim();
      const body = text.slice(open + 1, close - 1);
      if (head.startsWith("@")) walk(body);
      else if (head) {
        const values = {};
        for (const decl of body.split(";")) {
          const at = decl.indexOf(":");
          if (at === -1) continue;
          values[decl.slice(0, at).trim()] = decl.slice(at + 1).trim();
        }
        out.push({ selector: head.replace(/\s+/g, " "), values });
      }
      i = close;
    }
  };
  walk(css);
  return out;
}

const THEME_FILES = readdirSync(THEMES_DIR).filter((f) => f.endsWith(".css"));
const themeBlocks = THEME_FILES.flatMap((file) => blocks(strip(readFileSync(join(THEMES_DIR, file), "utf8"))).map((b) => ({ file, ...b })));

/** The families whose surfaces are glass: their ids, and the files that carry them. */
const GLASS_FAMILIES = ["glass"];
const GLASS_FILES = GLASS_FAMILIES.map((id) => `${id}.css`);
const isGlassFile = (file) => GLASS_FILES.includes(file);

/** The two grounds a transparent window can sit on; a glass `bg` must read over either. */
const GROUNDS = { white: "oklch(1 0 0)", black: "oklch(0 0 0)" };

const ROLE = (values, role) => values[`--color-${role}`];
const above = (ratio, floor, what) => assert.ok(ratio >= floor, `${what}: ${ratio.toFixed(2)} < ${floor}`);
const hasAlpha = (text) => (parseOklch(text)?.alpha ?? 1) < 1;

/**
 * A block's roles as a reader sees them on `ground`: `bg` composited over
 * the ground, the two surfaces and the border over that `bg`, everything
 * else as written (text, the accents and the status colours are solid in
 * every family — a translucent word is a word that changes with the
 * wallpaper). For an opaque block this is the identity.
 */
function effective(values, ground) {
  const bg = composite(ROLE(values, "bg"), ground);
  return (role) => {
    if (role === "bg") return bg;
    if (role === "surface" || role === "surface-2" || role === "border") return composite(ROLE(values, role), bg);
    return ROLE(values, role);
  };
}

/** Every pairing the app draws, held to its floor with `v` reading the roles. */
function keepsThePromise(v) {
  for (const role of ["bg", "surface", "surface-2", "border", "text", "text-dim", "accent", "accent-ink", "accent-contrast", "warn", "danger", "ok"]) {
    assert.ok(parseOklch(v(role)), `${role} is not an oklch() value: ${v(role)}`);
  }
  above(contrast(v("text"), v("bg")), 7, "text on the page");
  above(contrast(v("text"), v("surface")), 7, "text on a card");
  above(contrast(v("text-dim"), v("bg")), 4.5, "secondary text on the page");
  above(contrast(v("text-dim"), v("surface")), 4.5, "secondary text on a card");
  above(contrast(v("accent-ink"), v("surface")), 4.5, "accent ink on a card");
  above(contrast(v("accent-ink"), v("accent-soft")), 4.5, "accent ink on its own wash");
  above(contrast(v("accent-contrast"), v("accent")), 4.5, "text on an accent fill");
  above(contrast(v("accent-contrast"), v("danger")), 4.5, "text on a danger fill");
  for (const status of ["warn", "danger", "ok"]) above(contrast(v(status), v("bg")), 3, `${status} against the page`);
  above(contrast(v("danger"), v("danger-soft")), 4.5, "danger text on its wash");
  above(contrast(v("warn"), v("warn-soft")), 3, "warn text on its wash");
  above(contrast(v("ok"), v("ok-soft")), 3, "ok text on its wash");
  assert.ok(deltaL(v("surface-2"), v("bg")) >= 0.03, "a raised surface is a visible step off the page");
  assert.ok(deltaL(v("border"), v("surface")) >= 0.06, "a border is a visible step off a card");
  assert.ok(hueDistance(v("accent"), v("danger")) >= 30, "the accent and danger never blur");
  assert.ok(hueDistance(v("accent"), v("warn")) >= 30, "the accent and warn never blur");
}

for (const { file, selector, values } of themeBlocks) {
  const grounds = isGlassFile(file) ? Object.entries(GROUNDS) : [["itself", ROLE(values, "bg")]];
  for (const [ground, colour] of grounds) {
    const v = effective(values, colour);
    test(`${file} — ${selector} keeps the contrast promise over ${ground}`, () => keepsThePromise(v));
    test(`${file} — ${selector} never puts text on pure white or pure black over ${ground}`, () => {
      const bg = parseOklch(v("bg"));
      assert.ok(bg.l < 0.99 && bg.l > 0.12, `bg lightness ${bg.l}`);
    });
  }
  test(`${file} — ${selector} keeps its words solid`, () => {
    for (const role of ["text", "text-dim", "accent", "accent-ink", "accent-contrast", "warn", "danger", "ok"]) {
      assert.ok(!hasAlpha(ROLE(values, role)), `${role} carries alpha: a word that changes with the wallpaper`);
    }
  });
}

/** The material roles: an opaque family answers with no alpha and no blur; a glass family frosts. */
for (const { file, selector, values } of themeBlocks) {
  const blur = parseFloat(values["--material-blur"]);
  const saturate = Number(values["--material-saturate"]);
  if (isGlassFile(file)) {
    test(`${file} — ${selector} is glass: translucent surfaces behind a blur`, () => {
      assert.ok(blur > 0, `--material-blur is ${values["--material-blur"]}`);
      assert.ok(saturate >= 1, `--material-saturate is ${values["--material-saturate"]}`);
      for (const role of ["bg", "surface", "surface-2"]) assert.ok(hasAlpha(ROLE(values, role)), `${role} is opaque on a glass family`);
      assert.notEqual(values["--shadow-raised"], "none", "a pane of glass has an edge");
    });
  } else {
    test(`${file} — ${selector} is opaque: solid surfaces, nothing to frost`, () => {
      assert.equal(blur, 0, `--material-blur is ${values["--material-blur"]}`);
      assert.equal(saturate, 1, `--material-saturate is ${values["--material-saturate"]}`);
      for (const role of ["bg", "surface", "surface-2", "border"]) assert.ok(!hasAlpha(ROLE(values, role)), `${role} carries alpha on an opaque family`);
    });
  }
}

test("material.css frosts the glass families — the bare root among them, since the default is glass — and nothing else, with backdrop and edge properties only", () => {
  assert.ok(GLASS_FAMILIES.includes(DEFAULT_FAMILY), "a root with no data-theme is the default family; it frosts only if that family is glass");
  const css = strip(readFileSync(join(HERE, "material.css"), "utf8"));
  const rules = blocks(css);
  assert.ok(rules.length > 0, "material.css declares nothing");
  const allowed = new Set(["backdrop-filter", "-webkit-backdrop-filter", "background-color", "box-shadow"]);
  for (const { selector, values } of rules) {
    const heads = selector.split(",").map((s) => s.trim());
    const bare = heads.filter((h) => h.startsWith(":root:not([data-theme])"));
    const ids = [...selector.matchAll(/\[data-theme="([a-z-]+)"\]/g)].map((m) => m[1]);
    assert.ok(ids.length > 0, `${selector} is not keyed on a theme id`);
    assert.ok(bare.length > 0, `${selector} does not frost the bare root, which is the default family and glass`);
    for (const id of ids) assert.ok(GLASS_FAMILIES.some((g) => id === g || id === `${g}-dark`), `${selector} frosts ${id}, which is not glass`);
    for (const prop of Object.keys(values)) assert.ok(allowed.has(prop), `${selector} sets ${prop}; the material is a backdrop and an edge, never a colour of its own`);
  }
});

test("the element's wrap defaults sit in the base layer, so a utility on the element — `truncate`, `whitespace-nowrap` — wins", () => {
  const styles = strip(readFileSync(join(HERE, "..", "styles.css"), "utf8"));
  const base = styles.indexOf("@layer base");
  assert.ok(base >= 0, "styles.css keeps its element defaults in a base layer");
  const layer = styles.slice(base, styles.indexOf("text-wrap: pretty", base) + 40);
  assert.match(layer, /h3\s*\{\s*text-wrap:\s*balance;/, "headings balance, in the layer");
  assert.match(layer, /p\s*\{\s*text-wrap:\s*pretty;/, "paragraphs avoid a lone word, in the layer");
  const unlayered = styles.slice(0, base) + styles.slice(styles.indexOf("text-wrap: pretty", base) + 60);
  assert.ok(!/(^|[};])\s*(?:p|h[1-6])(?:\s*,\s*(?:p|h[1-6]))*\s*\{[^}]*text-wrap/.test(unlayered), "no unlayered bare-element rule sets text-wrap: it would outrank every layered utility");
});

test("the page's ground frosts on a layer under the content, never on body — a frosted body is every pane's Backdrop Root, and no pane would frost", () => {
  const css = strip(readFileSync(join(HERE, "material.css"), "utf8"));
  for (const { selector, values } of blocks(css)) {
    if (!("backdrop-filter" in values) && !("-webkit-backdrop-filter" in values)) continue;
    for (const head of selector.split(",").map((s) => s.trim())) {
      assert.ok(!/(^|\s)body$/.test(head) && !/(^|\s)html$/.test(head), `${head} frosts an ancestor of every pane (drafts.csswg.org/filter-effects-2, Backdrop Root)`);
    }
  }
  const styles = strip(readFileSync(join(HERE, "..", "styles.css"), "utf8"));
  assert.match(styles, /body::before\s*\{[^}]*position:\s*fixed;[^}]*z-index:\s*-1;[^}]*pointer-events:\s*none;/, "the frost layer covers the window, under the content, and catches nothing");
});

test("the theme files are the five families, and the default owns bare :root", () => {
  assert.deepEqual([...THEME_FILES].sort(), ["dune.css", "glass.css", "harbor.css", "orchard.css", "suede.css"]);
  const root = themeBlocks.filter((b) => b.selector.split(",").some((s) => s.trim() === ":root"));
  assert.equal(root.length, 1, "exactly one block owns bare :root");
  assert.equal(root[0].file, `${DEFAULT_FAMILY}.css`);
  assert.equal(root[0].values["color-scheme"], "light", "bare :root is the default family's light side");
  const system = themeBlocks.filter((b) => b.selector.includes(":root:not([data-theme])"));
  assert.equal(system.length, 1, "exactly one block answers System on a dark machine");
  assert.equal(system[0].file, `${DEFAULT_FAMILY}.css`, "System on a dark machine is the default family's dark side");
  const dark = themeBlocks.find((b) => b.selector === `:root[data-theme="${DEFAULT_FAMILY}-dark"]`);
  assert.deepEqual(system[0].values, dark.values, "System's dark face is the default family's dark side, value for value");
});

test("the default family is first in the picker and in the registry, right after System", () => {
  const ts = readFileSync(join(HERE, "../shell/theme.ts"), "utf8");
  const families = ts.match(/export const FAMILIES[^=]*=\s*\[([\s\S]*?)\n\];/);
  assert.ok(families, "shell/theme.ts lost its FAMILIES list");
  assert.equal([...families[1].matchAll(/id:\s*"([a-z-]+)"/g)][0][1], DEFAULT_FAMILY, "FAMILIES[0] is the default");
  assert.deepEqual([...idsInTs()].slice(0, 3), ["system", DEFAULT_FAMILY, `${DEFAULT_FAMILY}-dark`], "THEMES: System, then the default's two sides");
  assert.deepEqual([...idsInRust()].slice(0, 3), ["system", DEFAULT_FAMILY, `${DEFAULT_FAMILY}-dark`], "appearance.theme: System, then the default's two sides");
});

/** The ids each of the three copies knows. */
function idsInCss() {
  const ids = new Set(["system"]);
  for (const { selector } of themeBlocks) for (const m of selector.matchAll(/\[data-theme="([a-z-]+)"\]/g)) ids.add(m[1]);
  return ids;
}
function idsInTs() {
  const ts = readFileSync(join(HERE, "../shell/theme.ts"), "utf8");
  const block = ts.match(/export const THEMES[^=]*=\s*\[([\s\S]*?)\n\];/);
  assert.ok(block, "shell/theme.ts lost its THEMES list");
  return new Set([...block[1].matchAll(/id:\s*"([a-z-]+)"/g)].map((m) => m[1]));
}
function idsInRust() {
  const rs = readFileSync(join(HERE, "../../../crates/bisa-core/src/settings.rs"), "utf8");
  const block = rs.match(/"appearance\.theme",\s*Choice\(&\[([\s\S]*?)\]\)/);
  assert.ok(block, "settings.rs lost the appearance.theme choices");
  return new Set([...block[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]));
}

test("the three copies of the theme-id list agree — the stylesheets, the picker, the registry", () => {
  const css = [...idsInCss()].sort();
  assert.deepEqual([...idsInTs()].sort(), css, "shell/theme.ts THEMES vs theme/themes/*.css");
  assert.deepEqual([...idsInRust()].sort(), css, "settings.rs appearance.theme vs theme/themes/*.css");
  assert.deepEqual(css, ["dune", "dune-dark", "glass", "glass-dark", "harbor", "harbor-dark", "orchard", "orchard-dark", "suede", "suede-dark", "system"]);
});

/** The accents, each with the side it is written for. */
function accentOverlays() {
  const accents = blocks(strip(readFileSync(join(HERE, "accents.css"), "utf8")));
  assert.ok(accents.length >= 6, "three accents, two sides each");
  return accents.map(({ selector, values }) => ({
    selector,
    side: selector.includes("dark") || selector.includes("not([data-scheme=\"light\"])") ? "dark" : "light",
    v: (role) => values[`--color-${role}`],
  }));
}

/** An overlay's four roles against a family's `bg` and `surface` as `read` reports them. */
function accentReadsOn(overlay, read, where) {
  const { selector, v } = overlay;
  above(contrast(v("accent-ink"), read("surface")), 4.5, `${selector} on ${where}: accent ink on a card`);
  above(contrast(v("accent-ink"), v("accent-soft")), 4.5, `${selector} on ${where}: accent ink on its wash`);
  above(contrast(v("accent-contrast"), v("accent")), 4.5, `${selector} on ${where}: text on the accent fill`);
  above(contrast(v("accent"), read("bg")), 3, `${selector} on ${where}: the accent against the page`);
}

test("every accent reads on an opaque family's surfaces, both sides", () => {
  const harbor = themeBlocks.filter((b) => b.file === "harbor.css");
  const light = harbor.find((b) => b.values["color-scheme"] === "light").values;
  const dark = harbor.find((b) => b.selector.includes("harbor-dark")).values;
  for (const overlay of accentOverlays()) {
    const side = overlay.side === "dark" ? dark : light;
    accentReadsOn(overlay, (role) => ROLE(side, role), "harbor");
  }
});

test("every accent reads on frosted glass — the default family — over either ground", () => {
  for (const id of GLASS_FAMILIES) {
    const family = themeBlocks.filter((b) => b.file === `${id}.css`);
    const light = family.find((b) => b.values["color-scheme"] === "light").values;
    const dark = family.find((b) => b.values["color-scheme"] === "dark").values;
    for (const overlay of accentOverlays()) {
      const side = overlay.side === "dark" ? dark : light;
      for (const [ground, colour] of Object.entries(GROUNDS)) accentReadsOn(overlay, effective(side, colour), `${id} over ${ground}`);
    }
  }
});

/**
 * A surface a person reads and writes on for minutes — the notes overlay —
 * lays `surface` over the page's ground, `bg`, the pair the main window reads
 * as (`notes/NoteOverlay.tsx`). How much of what is behind it a sheet lets
 * through is then the two roles' alphas multiplied, and a glass family may
 * tune either; this is the floor under both.
 */
const SHEET_FLOOR = 0.95;

test("a reading sheet — surface over bg — is nearly opaque on every family, glass included", () => {
  const sheets = themeBlocks.filter((b) => ROLE(b.values, "bg") && ROLE(b.values, "surface"));
  assert.ok(sheets.length >= THEME_FILES.length, "every family states both roles, each side");
  for (const { file, selector, values } of sheets) {
    const bg = parseOklch(ROLE(values, "bg"));
    const surface = parseOklch(ROLE(values, "surface"));
    assert.ok(bg && surface, `${file} ${selector}: both roles are oklch`);
    const opacity = 1 - (1 - bg.alpha) * (1 - surface.alpha);
    assert.ok(opacity >= SHEET_FLOOR, `${file} ${selector}: a sheet is ${(opacity * 100).toFixed(1)}% opaque, under ${SHEET_FLOOR * 100}%`);
    // And alone, on glass, a surface is not: which is why a floating reading surface lays its ground.
    if (isGlassFile(file)) assert.ok(surface.alpha < SHEET_FLOOR, `${file} ${selector}: surface alone is glass`);
  }
});

/**
 * A band that stays put while words scroll under it — a thread's sticky day
 * divider (`ui/DayDivider.tsx`) — is the page's own ground, not a sheet laid
 * over it, so it lays `bg` over `bg`: the same colour twice, compounding to
 * the sheet floor on a glass family and changing nothing on an opaque one.
 */
test("a sticky band on the page's ground — bg over bg — reaches the sheet floor on every family", () => {
  const grounds = themeBlocks.filter((b) => ROLE(b.values, "bg"));
  assert.ok(grounds.length >= THEME_FILES.length, "every family states its ground, each side");
  for (const { file, selector, values } of grounds) {
    const bg = parseOklch(ROLE(values, "bg"));
    assert.ok(bg, `${file} ${selector}: bg is oklch`);
    const opacity = 1 - (1 - bg.alpha) * (1 - bg.alpha);
    assert.ok(opacity >= SHEET_FLOOR, `${file} ${selector}: two sheets of bg are ${(opacity * 100).toFixed(1)}% opaque, under ${SHEET_FLOOR * 100}%`);
  }
});

