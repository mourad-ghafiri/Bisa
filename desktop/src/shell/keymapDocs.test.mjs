import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { COMMANDS, PRESETS } from "./keymapModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const doc = readFileSync(join(here, "../../../docs/architecture/ide/15-keymap.md"), "utf8");

/** The one fenced ```text block in the keymap document: the command ids, one per line. */
function documentedIds() {
  const m = doc.match(/```text\n([\s\S]*?)\n```/);
  assert.ok(m, "ide/15-keymap.md carries a ```text block listing the command ids");
  return m[1]
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
}

test("the keymap document lists exactly the commands the model registers", () => {
  const ids = COMMANDS.map((c) => c.id);
  assert.deepEqual(documentedIds(), ids, "ids, in the model's order");
});

test("the keymap document names every preset", () => {
  for (const p of PRESETS) assert.ok(doc.includes(`\`${p}\``), `preset ${p} is named`);
});
