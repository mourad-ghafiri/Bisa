/**
 * What a typed setting commits, and when Settings asks before it leaves a
 * panel — tested where the rules live. Run with
 * `node --test desktop/src/views/_settings/settingDraftModel.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { SAVED_NOTE_MS, committedValue, draftOf } from "./settingDraftModel.mjs";
import { leaveAsks, markForm } from "./unsavedModel.mjs";

const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("a box shows the value's own spelling, and blank for none", () => {
  assert.equal(draftOf("http://proxy:3128"), "http://proxy:3128");
  assert.equal(draftOf(0.25), "0.25");
  assert.equal(draftOf(null), "");
  assert.equal(draftOf(undefined), "");
});

test("text commits what was typed, and nothing when the draft says what is already there", () => {
  const text = { type: "text" };
  assert.equal(committedValue(text, "https://example.test", ""), "https://example.test");
  assert.equal(committedValue(text, "", "https://example.test"), "", "clearing a box is a value");
  assert.equal(committedValue(text, "same", "same"), undefined, "a blur is not an edit");
  assert.equal(committedValue(text, "", null), undefined, "blank over nothing writes nothing");
});

test("a number commits only a number, held to its bounds; a blank or a word puts the value back", () => {
  const ratio = { type: "number", min: 0, max: 1 };
  assert.equal(committedValue(ratio, "0.35", 0.5), 0.35);
  assert.equal(committedValue(ratio, " 2 ", 0.5), 1, "held to the max");
  assert.equal(committedValue(ratio, "-1", 0.5), 0, "held to the min");
  assert.equal(committedValue(ratio, "", 0.5), undefined, "a blank is not 0");
  assert.equal(committedValue(ratio, "half", 0.5), undefined);
  assert.equal(committedValue(ratio, "0.5", 0.5), undefined, "unchanged");
  const count = { type: "integer", min: 1, max: 100 };
  assert.equal(committedValue(count, "12.9", 4), 12, "an integer drops the decimal, never rounds");
  assert.equal(committedValue(count, "0", 4), 1);
});

test("the Saved note is a moment, not a fixture", () => {
  assert.ok(SAVED_NOTE_MS >= 1000 && SAVED_NOTE_MS <= 4000);
});

test("leaving a panel asks only while a form holds unsaved edits, and only when the rail moves", () => {
  const none = new Set();
  const one = markForm(none, "identity-profile", true);
  assert.equal(markForm(one, "identity-profile", true), one, "marked twice: the same set");
  assert.equal(markForm(none, "global-git", false), none, "clean and clean: the same set");
  assert.deepEqual([...one], ["identity-profile"]);
  assert.equal(none.size, 0, "the set handed in is not changed");
  assert.equal(leaveAsks(one, "identity", "appearance"), true);
  assert.equal(leaveAsks(one, "identity", "identity"), false, "staying put asks nothing");
  assert.equal(leaveAsks(none, "identity", "appearance"), false);
  assert.equal(markForm(one, "identity-profile", false).size, 0, "saved or discarded: clean again");
});

test("the wiring: boxes commit on blur or Enter and are never disabled under their own write; Save forms share one footer", () => {
  const control = read("./SettingControl.tsx");
  assert.ok(control.includes("committedValue(def.kind, draft, value)") && control.includes("onBlur={commit}") && control.includes('e.key === "Enter"'), "a draft, committed on blur or Enter");
  assert.ok(control.includes("<NumberInput") && !control.includes('type="number"'), "integers are the kit's number box");
  const registry = read("./RegistryPanel.tsx");
  assert.ok(!registry.includes("disabled={busy}") && registry.includes("<SavedNote at={savedAt} />"), "the registry row stays live and says Saved");
  assert.ok(registry.includes('<Card className="flex flex-col divide-y divide-hairline">'), "one card, rows on hairlines");
  const network = read("./NetworkPanel.tsx");
  assert.ok(!network.includes("disabled={busy} />") && network.includes("<SavedNote at={savedAt} />"), "the proxy fields too");
  for (const file of ["./GlobalGitPanel.tsx", "./IdentityPanel.tsx", "./SecurityPanels.tsx"]) assert.ok(read(file).includes("<SaveFooter"), `${file}: the shared footer`);
  assert.ok(read("./SaveFooter.tsx").includes("useUnsavedForm(form, dirty)"), "the footer tells the rail");
  const screen = read("../Settings.tsx");
  assert.ok(screen.includes("leaveAsks(unsaved, panel.id, to)") && screen.includes("<ConfirmDialog") && screen.includes("forgetUnsaved();"), "the rail asks first, and a yes forgets the drafts");
  assert.ok(screen.includes('<div className="flex max-w-3xl flex-col gap-6 px-6 pb-10">'), "one content width");
});
