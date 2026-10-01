/**
 * About's settings as one draft: what counts as a change, what refuses a
 * save, and the writes a save makes in order. Run with
 * `node --test desktop/src/views/_work/projectSettingsDraftModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { changeCount, changedScripts, changedSettings, emptyDraft, INHERIT, problems, savedWords, toolbarWords, withAccount, withInherit, withoutSetting, withPublish, withScripts, withSetting, writes, staleReads } from "./projectSettingsDraftModel.mjs";
import { MAX_SCRIPT_CHARS, PHASES, TIMEOUT_KEY } from "./workstreamScripts.mjs";

const current = () => ({
  settings: {
    "git.merge_strategy": { value: "merge", own: false },
    "git.pull": { value: "rebase", own: true },
    "editor.tab_size": { value: 2, own: false },
  },
  publish: "gated",
  account: null,
  scripts: { texts: { pre_create: "", post_create: "npm install", clean: "" }, timeout: 60 },
});

test("a draft counts only what would change: a new own value, a different one, an Inherit on an owned key — never a value the project already holds or an Inherit on a key it does not own", () => {
  const now = current();
  const none = emptyDraft();
  assert.equal(changeCount(none, now, null), 0);
  assert.deepEqual(changedSettings(withSetting(none, "git.merge_strategy", "squash"), now), ["git.merge_strategy"]);
  assert.deepEqual(changedSettings(withSetting(none, "git.merge_strategy", "merge"), now), ["git.merge_strategy"], "the workspace's value, made the project's own, is a change");
  assert.deepEqual(changedSettings(withSetting(none, "git.pull", "rebase"), now), [], "the project's own value, set again, is no change");
  assert.deepEqual(changedSettings(withInherit(none, "git.pull"), now), ["git.pull"]);
  assert.deepEqual(changedSettings(withInherit(none, "git.merge_strategy"), now), [], "nothing of the project's to clear");
  assert.deepEqual(changedSettings(withoutSetting(withSetting(none, "git.pull", "merge"), "git.pull"), now), [], "forgotten again");
  assert.equal(changeCount(withPublish(none, "gated"), now, null), 0);
  assert.equal(changeCount(withPublish(none, "manual"), now, null), 1);
  assert.equal(changeCount(withAccount(none, null), now, null), 0, "unpinning an unpinned repository is nothing");
  assert.equal(changeCount(withAccount(none, "ada"), now, null), 1);
  assert.equal(changeCount(none, now, { set: { "user.name": "Ada" }, unset: ["user.email"] }), 2, "each git config key once");
});

test("the scripts count per text and the timeout, and only a changed text asks for approval", () => {
  const now = current();
  const same = withScripts(emptyDraft(), { texts: { ...now.scripts.texts }, timeout: 60 });
  assert.deepEqual(changedScripts(same, now), { keys: [], texts: false });
  const text = withScripts(emptyDraft(), { texts: { ...now.scripts.texts, clean: "docker compose down" }, timeout: 60 });
  assert.deepEqual(changedScripts(text, now), { keys: [PHASES.find((p) => p.id === "clean").key], texts: true });
  const timeout = withScripts(emptyDraft(), { texts: { ...now.scripts.texts }, timeout: 90 });
  assert.deepEqual(changedScripts(timeout, now), { keys: [TIMEOUT_KEY], texts: false }, "a timeout alone approves nothing");
  assert.equal(changeCount(withScripts(text, { texts: { ...now.scripts.texts, clean: "x", pre_create: "y" }, timeout: 90 }), now, null), 3);
});

test("a save is refused by a script git would choke on or a git config value the form refused, each named under its thing", () => {
  const now = current();
  assert.deepEqual(problems(emptyDraft()), {});
  const long = withScripts(emptyDraft(), { texts: { ...now.scripts.texts, clean: "x".repeat(MAX_SCRIPT_CHARS + 1) }, timeout: 60 });
  assert.match(problems(long)["script:clean"], /\d/);
  assert.deepEqual(problems(emptyDraft(), { "user.email": "not an email" }), { "git:user.email": "not an email" });
});

test("the writes go in one order — the policy, the settings values in one call with the changed scripts, each inherited key, the git config, the approval, the pin last", () => {
  const now = current();
  let draft = emptyDraft();
  draft = withPublish(draft, "auto");
  draft = withSetting(draft, "git.merge_strategy", "squash");
  draft = withInherit(draft, "git.pull");
  draft = withScripts(draft, { texts: { ...now.scripts.texts, clean: "docker compose down" }, timeout: 90 });
  draft = withAccount(draft, "ada");
  const git = { set: { "user.name": "Ada" }, unset: [] };
  const clean = PHASES.find((p) => p.id === "clean").key;
  assert.deepEqual(writes(draft, now, git), [
    { op: "patch_project", publish: "auto" },
    { op: "set_settings", values: { "git.merge_strategy": "squash", [clean]: "docker compose down", [TIMEOUT_KEY]: 90 } },
    { op: "unset_setting", key: "git.pull" },
    { op: "git_config", write: git },
    { op: "approve_scripts" },
    { op: "account", login: "ada" },
  ]);
  assert.deepEqual(writes(emptyDraft(), now, null), [], "nothing to write");
  assert.deepEqual(writes(emptyDraft(), now, { set: {}, unset: [] }), [], "an empty git write is no write");
  assert.deepEqual(
    writes(withScripts(emptyDraft(), { texts: { ...now.scripts.texts }, timeout: 90 }), now, null),
    [{ op: "set_settings", values: { [TIMEOUT_KEY]: 90 } }],
    "a timeout alone: one settings write, no approval",
  );
  assert.deepEqual(writes(withAccount(emptyDraft(), null), { ...now, account: "ada" }, null), [{ op: "account", login: null }], "unpinning");
});

test("the git config form counts while the project reads are still out — an identity typed is never silently unsaved", () => {
  const write = { set: { "user.name": "Ada", "user.email": "ada@example.org" }, unset: [] };
  assert.equal(changeCount(emptyDraft(), null, write), 2);
  assert.equal(changeCount(emptyDraft(), null, null), 0);
  assert.equal(changeCount(emptyDraft(), null, { set: {}, unset: ["user.name"] }), 1);
});

test("the toolbar and the toast say the count in words", () => {
  assert.deepEqual(toolbarWords(0, false), { status: "All saved", save: "Save" });
  assert.deepEqual(toolbarWords(0, false, false), { status: "Still reading the project…", save: "Save" }, "not saved: not read yet");
  assert.deepEqual(toolbarWords(2, false, false), { status: "2 unsaved changes", save: "Save" });
  assert.deepEqual(toolbarWords(1, false), { status: "1 unsaved change", save: "Save" });
  assert.deepEqual(toolbarWords(3, true), { status: "3 unsaved changes", save: "Saving…" });
  assert.equal(savedWords(1, false), "Saved 1 change.");
  assert.equal(savedWords(4, true), "Saved 4 changes — the scripts are approved on this machine.");
});

test("a fact from elsewhere moves what the edits are compared with, and only this project's", () => {
  assert.deepEqual(staleReads({ type: "project_changed", project: "P1" }, "P1"), ["project", "scripts"]);
  assert.deepEqual(staleReads({ type: "project_changed", project: "P2" }, "P1"), [], "another project's change is not this draft's");
  assert.deepEqual(staleReads({ type: "settings_changed" }, "P1"), ["scripts"], "a script text or its approval moved");
  for (const none of [{ type: "step_changed" }, { type: "agent_streamed" }, {}, null, undefined, { type: 7 }]) assert.deepEqual(staleReads(none, "P1"), []);
});

test("an edit survives the values under it moving: the draft holds changes, never a copy of the record", () => {
  const draft = withSetting(emptyDraft(), "git.pull", "rebase");
  const before = { settings: { "git.pull": { value: "ff_only", own: false } }, publish: "gated", account: null, scripts: {} };
  assert.equal(changeCount(draft, before, null), 1);
  // Elsewhere, somebody set the same value for the project: the edit is no longer a change.
  const same = { ...before, settings: { "git.pull": { value: "rebase", own: true } } };
  assert.equal(changeCount(draft, same, null), 0, "nothing left to save");
  // Elsewhere, somebody set another: the person's edit still stands against it.
  const other = { ...before, settings: { "git.pull": { value: "merge", own: true } } };
  assert.equal(changeCount(draft, other, null), 1);
});

