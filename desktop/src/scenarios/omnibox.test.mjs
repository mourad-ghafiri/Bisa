/**
 * The palette as a person types into it (ide/12): the prefix picks the mode,
 * the words become one needle, a row matches on its label, hint and hidden
 * keywords, every section is capped so none floods the list, the sections
 * keep the order they were built in, and the cursor walks the flat list.
 * No DOM.
 *
 * Run with `node --test desktop/src/scenarios/omnibox.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { t } from "../i18n/l10n.mjs";

import { MAX_ITEMS, SECTION_CAPS, admit, groupItems, itemMatches, paletteNeedle, parsePrefix, rankPaths } from "../shell/quickOpenScore.mjs";

const item = (key, group, label, extra = {}) => ({ key, group, label, ...extra });

/** The palette's build, as `Omnibox.tsx` does it: match, admit within the cap, keep the build order. */
function build(candidates, q, mode) {
  const needle = paletteNeedle(q, mode);
  const counts = new Map();
  const out = [];
  for (const it of candidates) {
    if (!itemMatches(it, needle)) continue;
    if (!admit(counts, it.group)) continue;
    out.push(it);
  }
  return out.slice(0, MAX_ITEMS);
}

test("typing narrows: everything at first, the words then, a prefix picks the mode and its words search within it", () => {
  const candidates = [
    item("c:theme", "Commands", "Toggle theme", { keywords: "dark light appearance" }),
    item("c:wrap", "Commands", "Toggle word wrap", { hint: "Editor" }),
    item("g:1", "Goals", "Ship dark mode"),
    item("p:1", "Projects", "shop", { hint: "the web shop" }),
    item("f:1", "Files", "theme.css", { hint: "src/theme" }),
  ];
  assert.deepEqual(build(candidates, "", "all").map((i) => i.key), candidates.map((i) => i.key), "an empty field lists everything");
  assert.deepEqual(build(candidates, "dark", "all").map((i) => i.key), ["c:theme", "g:1"], "a hidden keyword matches as a label does");
  assert.deepEqual(parsePrefix(">wrap"), { prefix: ">", query: "wrap" });
  assert.deepEqual(build(candidates, ">wrap", "commands").map((i) => i.key), ["c:wrap"], "under the command prefix the words search within");
  assert.deepEqual(build(candidates, "theme", "all").map((i) => i.key), ["c:theme", "f:1"]);
  assert.deepEqual(build(candidates, "  SHOP ", "all").map((i) => i.key), ["p:1"], "trimmed and folded");
});

test("one category cannot flood the list: a section stops at its cap, the list at its length, and the sections keep the order they were met in", () => {
  const many = Array.from({ length: 30 }, (_, i) => item(`t:${i}`, "Terminals", `shell ${i}`));
  const goals = Array.from({ length: 3 }, (_, i) => item(`g:${i}`, "Goals", `goal ${i}`));
  const rows = build([...many, ...goals], "", "all");
  assert.equal(rows.filter((r) => r.group === "Terminals").length, SECTION_CAPS.Terminals);
  assert.equal(rows.filter((r) => r.group === "Goals").length, 3, "a capped section leaves room for the next");
  const { groups, flat } = groupItems(rows);
  assert.deepEqual(groups.map(([g]) => g), ["Terminals", "Goals"], "in the order they were built");
  assert.equal(flat.length, rows.length);
  assert.equal(flat[SECTION_CAPS.Terminals].group, "Goals", "the cursor crosses from one section into the next");
  const flood = Array.from({ length: 200 }, (_, i) => item(`s:${i}`, "Search", `hit ${i}`));
  assert.equal(build(flood, "", "all").length, MAX_ITEMS, "the list has a length whatever a section allows");
});

test("files are ranked, not filtered: the quick-open half of the palette scores paths and keeps the cap", () => {
  const index = ["src/theme/tokens.css", "src/ui/Tags.tsx", "docs/theme.md", "src/theme/themes/latte.css"];
  const ranked = rankPaths("theme", index, SECTION_CAPS.Files).map((r) => r.path);
  assert.ok(ranked.length > 0 && ranked.length <= SECTION_CAPS.Files);
  assert.ok(ranked.every((p) => p.toLowerCase().includes("theme")), "every ranked path carries the words");
  assert.deepEqual(rankPaths("", index, 2).length, 2, "an empty needle still lists within the cap");
});

test("the library's workflows are a section of their own, capped, found by their tags and their description, archived ones never built", () => {
  const group = t("shell-omnibox-workflows");
  assert.equal(SECTION_CAPS[group], 8, "eight, as the work items");
  const many = Array.from({ length: 20 }, (_, i) => item(`wf:${i}`, group, `workflow ${i}`, { keywords: i === 3 ? "release ship" : "", hint: i === 5 ? "Nightly build · On" : undefined }));
  const rows = build(many, "", "all");
  assert.equal(rows.length, SECTION_CAPS[group], "the cap holds the section");
  assert.deepEqual(build(many, "ship", "all").map((i) => i.key), ["wf:3"], "a tag matches as a label does");
  assert.deepEqual(build(many, "nightly", "all").map((i) => i.key), ["wf:5"], "the description matches");
  // The palette leaves an archived workflow out before it is built: the
  // source reads `archived` and continues.
  const source = readFileSync(new URL("../shell/Omnibox.tsx", import.meta.url), "utf8");
  assert.ok(source.includes("if (w.workflow.archived) continue;"), "an archived workflow is never a row");
  assert.ok(source.includes('api\n      .workflows({ scope: "library" }, ac.signal)'), "read once per opening, never from the workspace store");
});
