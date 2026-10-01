/**
 * The notes panel is a sheet, not a film over the screen. On a glass family
 * `surface` is a translucent colour; the panel floats over whatever screen is
 * open with no ground under it, so it lays its own — `surface` over `bg` — and
 * is a pane, so a glass family frosts what little still shows
 * (`theme/material.css`). A lone `bg-surface` on the panel is the defect this
 * holds the door against: text behind the panel reading through the note.
 *
 * Reads the source; never renders, writes or runs anything.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const source = readFileSync(new URL("./NoteOverlay.tsx", import.meta.url), "utf8");

/** The panel's opening tag, whole: `<section` to the `>` that ends it. */
function panelTag() {
  const from = source.indexOf("<section");
  assert.notEqual(from, -1, "the panel is a <section>");
  let depth = 0;
  for (let at = from; at < source.length; at++) {
    const c = source[at];
    if (c === "{") depth++;
    else if (c === "}") depth--;
    else if (c === ">" && depth === 0) return source.slice(from, at + 1);
  }
  throw new Error("the panel's tag never closes");
}

const classesIn = (text) => new Set([...text.matchAll(/"([^"]*)"/g)].flatMap((m) => m[1].split(/\s+/)).filter(Boolean));

test("the panel is a pane and lays the page's ground, with the surface on top of it", () => {
  const tag = panelTag();
  assert.match(tag, /\bdata-pane\b/, "a glass family frosts what is behind it");
  const frame = classesIn(tag);
  assert.ok(frame.has("bg-bg"), "the ground is the panel's own");
  assert.ok(!frame.has("bg-surface"), "a lone translucent surface is the screen showing through the note");
  assert.ok(frame.has("overflow-hidden") && frame.has("rounded-card"), "the frame clips both layers to one shape");

  // The first element inside the panel is the surface, filling it.
  const after = source.slice(source.indexOf(tag) + tag.length);
  const first = after.match(/<div className="([^"]*)"/);
  assert.ok(first, "the panel's content sits in a wrapper");
  const sheet = new Set(first[1].split(/\s+/));
  assert.ok(sheet.has("bg-surface"), "surface over bg — the pair the main window reads as");
  assert.ok(sheet.has("h-full") && sheet.has("w-full"), "it covers the whole ground");
  assert.ok(sheet.has("flex"), "the resize handle and the column are its row");
});

test("no colour or opacity of the panel's own: the theme's two roles decide how solid it is", () => {
  const tag = panelTag();
  assert.ok(!/opacity-|\/\d{1,3}\b|backdrop-/.test([...classesIn(tag)].join(" ")), "no literal alpha, opacity or blur on the panel — the material is the theme's");
});
