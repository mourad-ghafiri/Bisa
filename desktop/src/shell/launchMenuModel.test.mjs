/**
 * What a place can open in a terminal — one rule and one list for the
 * Terminal caret, the centre strip's + caret, the palette and the rail.
 * Run with `node --test desktop/src/shell/launchMenuModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { entryLabel, entryTarget, harnessEntries, launchMenu, placeNoun, resumesHere } from "./launchMenuModel.mjs";
import { emptyTerminals, launchKey, openTerminal } from "./terminalsModel.mjs";

const claude = { id: "claude-code", label: "Claude Code", installed: true, detail: null, launch: { program: "claude", resume_args: ["--continue"] } };
const codex = { id: "codex", label: "Codex", installed: true, detail: null, launch: { program: "codex", resume_args: ["resume", "--last"] } };
/** A harness with an interactive form and no way to continue a session. */
const plain = { id: "preset:aider", label: "Aider", installed: true, detail: null, launch: { program: "aider" } };
const gemini = { id: "gemini-cli", label: "Gemini CLI", installed: false, detail: "gemini is not on the PATH", launch: { program: "gemini", resume_args: ["--resume", "latest"] } };
const unnamed = { id: "preset:goose", label: "Goose", installed: false, detail: null, launch: { program: "goose", resume_args: ["session", "--resume"] } };

const ran = (...places) => places.map(([harness, scope, id]) => launchKey(harness, scope, id));

test("a harness resumes in a place only when it has a way to resume and has run in that very place", () => {
  const launched = ran(["claude-code", "workstream", "w1"], ["preset:aider", "workstream", "w1"]);
  assert.equal(resumesHere(claude, launched, "workstream", "w1"), true);
  assert.equal(resumesHere(claude, launched, "workstream", "w2"), false, "another place is another history");
  assert.equal(resumesHere(claude, launched, "goal", "w1"), false, "a goal's folder is not the workstream of the same id");
  assert.equal(resumesHere(codex, launched, "workstream", "w1"), false, "another harness ran here, not this one");
  assert.equal(resumesHere(plain, launched, "workstream", "w1"), false, "it ran here and has no form to continue with");
  assert.equal(resumesHere(claude, [], "workstream", "w1"), false);
  assert.equal(resumesHere(claude, null, "workstream", "w1"), false, "a memory that could not be read remembers nothing");
  assert.equal(resumesHere(null, launched, "workstream", "w1"), false);
});

test("each harness is one entry, and a fresh one beside it only when the first would resume", () => {
  const launched = ran(["claude-code", "workstream", "w1"], ["gemini-cli", "workstream", "w1"]);
  const entries = harnessEntries([claude, codex, gemini], launched, "workstream", "w1");
  assert.deepEqual(
    entries.map((e) => [e.id, e.kind, e.harness, e.resumes, e.installed]),
    [
      ["open:claude-code", "open", "claude-code", true, true],
      ["fresh:claude-code", "fresh", "claude-code", false, true],
      ["open:codex", "open", "codex", false, true],
      // It ran here once and is gone from this machine: one entry, nothing to resume or start.
      ["open:gemini-cli", "open", "gemini-cli", false, false],
    ],
  );
  assert.deepEqual(harnessEntries(null, launched, "workstream", "w1"), []);
  assert.deepEqual(harnessEntries([claude], [], "workstream", "w1").map((e) => e.id), ["open:claude-code"], "never run here: no second entry that would do the same thing");
});

test("an entry opens its harness in the place, and only the fresh one says so in as many words", () => {
  const place = { scope: "workstream", id: "w1", label: "main" };
  const [open, fresh] = harnessEntries([claude], ran(["claude-code", "workstream", "w1"]), "workstream", "w1");
  assert.deepEqual(entryTarget(open, place), { scope: "workstream", id: "w1", label: "main", harness: "claude-code" });
  assert.deepEqual(entryTarget(fresh, place), { scope: "workstream", id: "w1", label: "main", harness: "claude-code", resume: false });
  // The store reads them as they were meant: the open one continues, the fresh one does not.
  let s = openTerminal(emptyTerminals(), entryTarget(open, place), 1);
  assert.equal(s.sessions[0].resume, false, "the first time there is nothing to continue");
  s = openTerminal(s, entryTarget(open, place), 2);
  assert.equal(s.sessions[1].resume, true, "it has run here: the same entry continues");
  s = openTerminal(s, entryTarget(fresh, place), 3);
  assert.equal(s.sessions[2].resume, false, "the fresh entry beats *has run here before*");
});

test("an entry says what it will do: the name, resume, fresh session, or why it cannot open", () => {
  const launched = ran(["claude-code", "workstream", "w1"]);
  const words = harnessEntries([claude, codex, gemini, unnamed], launched, "workstream", "w1").map(entryLabel);
  assert.deepEqual(words, ["Claude Code — resume", "Claude Code — fresh session", "Codex", "Gemini CLI — gemini is not on the PATH", "Goose — not installed"]);
});

test("the list is the shell, the project's run command when it sets one, then every harness — the Terminal caret's list, item for item", () => {
  const run = { id: "run", label: "Run `npm run dev`", hint: "in a terminal here", icon: "play", disabled: false, separatorBefore: true, command: "run_project" };
  const launched = ran(["claude-code", "workstream", "w1"]);
  const list = launchMenu({ harnesses: [claude, codex, gemini], launched, scope: "workstream", id: "w1", shells: 0, run, shellCommand: null });
  assert.deepEqual(list, [
    { id: "shell", kind: "shell", label: "Shell" },
    { id: "run", kind: "run", label: "Run `npm run dev`", disabled: false, separatorBefore: true, command: "run_project" },
    { id: "open:claude-code", kind: "open", harness: "claude-code", label: "Claude Code — resume", disabled: false, separatorBefore: true },
    { id: "fresh:claude-code", kind: "fresh", harness: "claude-code", label: "Claude Code — fresh session", disabled: false, separatorBefore: false },
    { id: "open:codex", kind: "open", harness: "codex", label: "Codex", disabled: false, separatorBefore: false },
    { id: "open:gemini-cli", kind: "open", harness: "gemini-cli", label: "Gemini CLI — gemini is not on the PATH", disabled: true, separatorBefore: false },
  ]);

  // A live shell already rooted here: the first item opens another, and says so. Asked to, it wears the chord that does the same.
  const again = launchMenu({ harnesses: [claude], launched: [], scope: "workstream", id: "w1", shells: 2, run: null, shellCommand: "new_terminal" });
  assert.deepEqual(again, [
    { id: "shell", kind: "shell", label: "New shell here", command: "new_terminal" },
    { id: "open:claude-code", kind: "open", harness: "claude-code", label: "Claude Code", disabled: false, separatorBefore: true },
  ]);

  // A command this machine has not approved is still an item — a press opens the card — and wears no chord.
  const held = { id: "approve-run", label: "Run `npm run dev` — not approved here", hint: "", icon: "play", disabled: false, separatorBefore: true };
  const unapproved = launchMenu({ harnesses: [], launched: [], scope: "workstream", id: "w1", shells: 0, run: held, shellCommand: null });
  assert.deepEqual(unapproved, [
    { id: "shell", kind: "shell", label: "Shell" },
    { id: "approve-run", kind: "run", label: "Run `npm run dev` — not approved here", disabled: false, separatorBefore: true },
  ]);
  assert.deepEqual(launchMenu({ harnesses: null, launched: null, scope: "goal", id: "g1", shells: 0, run: null, shellCommand: null }), [{ id: "shell", kind: "shell", label: "Shell" }], "nothing known yet: a shell is still a shell");
});

test("a place has a noun for the sentence that names it", () => {
  assert.equal(placeNoun("workstream"), "this workstream");
  assert.equal(placeNoun("goal"), "this goal's folder");
  assert.equal(placeNoun("work_item"), "where this work item runs");
  assert.equal(placeNoun("machine"), "your home directory");
});
