/**
 * An assignee as a step's form holds it: one, or an input; a list whose
 * input references ride behind the fixed ones. Run with
 * `node --test --import ./src/i18n/preload.mjs src/views/_workflow/forms/assigneeRefModel.test.mjs`
 * from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";

import { fixedWords, inputNames, picked, pickedAgent, refOf, toggleInput, valueOf, withFixed, wordOf } from "./assigneeRefModel.mjs";

const KEY = "ab".repeat(32);

test("a word and the assignee it names are one another's, and an input reference is neither", () => {
  for (const [word, ref] of [["agent:developer", { agent: "developer" }], ["team:01TEAM", { team: "01TEAM" }], [`human:${KEY}`, { human: KEY }]]) {
    assert.equal(wordOf(ref), word);
    assert.deepEqual(refOf(word), ref);
  }
  for (const none of [null, undefined, { input: "who" }, "agent:developer"]) assert.equal(wordOf(none), null);
  // A word that names nobody is nobody — never a person with the word for a key.
  for (const none of ["developer", "", "robot:x", null, undefined]) assert.equal(refOf(none), null);
  assert.deepEqual(refOf("agent:a:b"), { agent: "a:b" }, "the id is everything after the first colon, as the platform's grammar reads it");
});

test("a field that takes one holds the input or the word, and writes the input, the assignee or nothing", () => {
  assert.equal(valueOf(null), null);
  assert.equal(valueOf({ team: "01TEAM" }), "team:01TEAM");
  assert.deepEqual(valueOf({ input: "who" }), { input: "who" });
  assert.deepEqual(picked("agent:developer"), { agent: "developer" });
  assert.deepEqual(picked({ input: "who" }), { input: "who" });
  for (const none of [null, undefined, "", "developer"]) assert.equal(picked(none), null, "nothing chosen is `null`, never an empty reference");
  // Round trip: what the field shows is what it would write back.
  for (const ref of [{ agent: "developer" }, { human: KEY }, { input: "who" }]) assert.deepEqual(picked(valueOf(ref)), ref);
});

test("who speaks a notify is an agent or an input, and nobody else", () => {
  assert.deepEqual(pickedAgent("agent:writer"), { agent: "writer" });
  assert.deepEqual(pickedAgent({ input: "voice" }), { input: "voice" });
  for (const none of ["team:01TEAM", `human:${KEY}`, null, ""]) assert.equal(pickedAgent(none), null);
});

test("a list keeps its input references behind whatever the picker writes", () => {
  const refs = [{ agent: "developer" }, { input: "reviewer" }, { team: "01TEAM" }, { input: "owner" }];
  assert.deepEqual(fixedWords(refs), ["agent:developer", "team:01TEAM"]);
  assert.deepEqual(inputNames(refs), ["reviewer", "owner"]);
  assert.deepEqual(withFixed(refs, ["team:01TEAM", `human:${KEY}`]), [{ team: "01TEAM" }, { human: KEY }, { input: "reviewer" }, { input: "owner" }]);
  assert.deepEqual(withFixed(refs, []), [{ input: "reviewer" }, { input: "owner" }], "every fixed one removed, the inputs stay");
  assert.deepEqual(withFixed(null, ["agent:developer", "nobody"]), [{ agent: "developer" }], "a word that names nobody is left out");
  assert.deepEqual(toggleInput(refs, "owner"), [{ agent: "developer" }, { input: "reviewer" }, { team: "01TEAM" }]);
  assert.deepEqual(toggleInput(refs, "lead").at(-1), { input: "lead" });
  assert.deepEqual(toggleInput(undefined, "lead"), [{ input: "lead" }]);
  assert.equal(refs.length, 4, "the list handed over is left as it was");
  for (const none of [null, undefined, []]) assert.deepEqual([fixedWords(none), inputNames(none)], [[], []]);
});

test("no step form turns a word into an assignee itself: each reads the model", () => {
  const here = new URL("./", import.meta.url);
  const forms = readdirSync(here).filter((name) => name.endsWith(".tsx"));
  assert.ok(forms.length >= 25, `the forms are read: ${forms.length}`);
  const spelling = [];
  for (const name of forms) {
    const text = readFileSync(new URL(name, here), "utf8");
    if (/split\(":"|`(agent|team|human):\$\{/.test(text)) spelling.push(name);
  }
  assert.deepEqual(spelling, []);
  assert.throws(() => readFileSync(new URL("./assigneeRef.ts", here)), "the second copy of the grammar is gone");
});
