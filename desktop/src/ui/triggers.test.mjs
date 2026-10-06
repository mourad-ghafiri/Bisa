/**
 * A menu or a popover never nests a button in the button Radix renders: a
 * trigger that is already a button — a `<button>`, the kit's `Button` —
 * becomes the trigger (`isButtonElement`, `asChild`), and every other trigger
 * is a styled `<span>` or a `Tooltip` around one, never a `<div>`. And a
 * popover's panel carries a name. Source assertions, no DOM. Run with
 * `node --test desktop/src/ui/triggers.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { sourceFiles } from "../testWalk.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("Menu and Popover make a button they are handed the trigger itself, never a button inside theirs", () => {
  const menu = read("ui/Menu.tsx");
  assert.ok(menu.includes('import { isButtonElement } from "./triggers";') && menu.includes("isButtonElement(trigger) ? (") && menu.includes("<M.Trigger asChild"), "the menu reads its trigger");
  const popover = read("ui/Popover.tsx");
  assert.ok(popover.includes("asChild || isButtonElement(trigger) ? (") && popover.includes("<P.Trigger asChild>"), "the popover the same");
  const triggers = read("ui/triggers.ts");
  assert.ok(triggers.includes('node.type === "button" || node.type === Button'), "a bare button and the kit's");
});

test("a popover's panel is named: the label given, else the trigger's own", () => {
  const popover = read("ui/Popover.tsx");
  assert.ok(popover.includes('const name = label ?? (isValidElement(trigger) ? (trigger.props as { "aria-label"?: string })["aria-label"] : undefined);'));
  assert.ok(popover.includes("aria-label={name}"), "on the content");
});

test("every trigger a screen hands a menu or a popover is a span, a tooltip around one, or a button — never a div", () => {
  const files = sourceFiles(src, (p) => p.endsWith(".tsx"));
  const seen = [];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/trigger=\{\s*(?:\/\/[^\n]*\n\s*)*<([A-Za-z.]+)/g)) {
      seen.push({ file: file.slice(src.length + 1), tag: m[1] });
    }
  }
  assert.ok(seen.length >= 15, `the call sites are read: ${seen.length}`);
  const divs = seen.filter((s) => s.tag === "div");
  assert.deepEqual(divs, [], "a div is reachable by nothing");
});

test("a tile's preview is hidden from assistive technology, so its caption is its name", () => {
  const tile = readFileSync(new URL("./Tile.tsx", import.meta.url), "utf8");
  assert.match(tile, /<span aria-hidden="true" className=\{cn\("anim relative overflow-hidden rounded-card border"/, "the preview box is aria-hidden");
});
