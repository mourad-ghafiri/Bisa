/**
 * Which engine facts move a workstream, held against the engine's own list
 * and against the three surfaces that re-read on them.
 * Run with `node --test --import ./src/i18n/preload.mjs src/shell/workstreamFramesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { PROJECT_FRAMES, RECORD_FRAMES, movesStatuses, movesWorkstream } from "./workstreamFramesModel.mjs";

const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");

/** The engine's fact tags, read from `EnginePayload` — a variant's name in snake case. */
function engineTags() {
  const rust = read("../../../crates/bisa-engine/src/events.rs");
  const body = rust.slice(rust.indexOf("pub enum EnginePayload"));
  const tags = [...body.matchAll(/^ {4}([A-Z][A-Za-z0-9]+)\s*[{(,]/gm)].map((m) => m[1].replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase());
  assert.ok(tags.includes("workstream_changed") && tags.length > 40, "the engine's facts are read");
  return new Set(tags);
}

test("every fact a workstream is re-read for is one the engine says, and none is listed twice", () => {
  const tags = engineTags();
  for (const type of [...RECORD_FRAMES, ...PROJECT_FRAMES]) assert.ok(tags.has(type), `${type} is no engine fact — a re-read nobody triggers`);
  assert.equal(new Set([...RECORD_FRAMES, ...PROJECT_FRAMES]).size, RECORD_FRAMES.length + PROJECT_FRAMES.length);
});

test("every fact of the engine that says a workstream's record moved is in the list — a new one is a row to add", () => {
  const moved = [...engineTags()].filter((t) => /^workstream_(opened|changed|committed|edited)$/.test(t));
  assert.deepEqual(moved.sort(), [...RECORD_FRAMES].sort());
});

test("a commit re-reads the statuses whichever door made it, and so do a rename, a card placed and a committer set", () => {
  assert.equal(movesStatuses("workstream_committed"), true, "the workstream's own commit and the Changes view's say the same fact");
  assert.equal(movesStatuses("workstream_changed"), true, "open or dirty to committed: the state the Board's column and the chips follow");
  assert.equal(movesStatuses("workstream_edited"), true, "a status carries the record's name: a rename is read at once, not on the next poll");
  assert.equal(movesStatuses("workstream_opened"), true);
  assert.equal(movesStatuses("committer_set"), true);
  assert.equal(movesStatuses("project_changed"), true, "a plain folder made a repository, a publishing policy changed");
});

test("what moves no record re-reads nothing: a publish that failed, a script that ran, a file, a type nobody can read", () => {
  for (const type of ["workstream_publish_failed", "workstream_script_ran", "file_changed", "session_state", "project_deleted", "", null, undefined, 7, {}]) {
    assert.equal(movesStatuses(type), false, JSON.stringify(type));
  }
});

test("one checkout follows the facts that name it and the facts about its project — never another workstream's", () => {
  const changed = { type: "workstream_changed", workstream: "w1", state: { state: "committed" } };
  assert.equal(movesWorkstream(changed, "w1", "p1"), true);
  assert.equal(movesWorkstream(changed, "w2", "p1"), false, "a commit next door reads nothing here");
  assert.equal(movesWorkstream({ type: "workstream_committed", workstream: "w1", branch: "work/x", commit: "abc" }, "w1", null), true, "before the record was read, the workstream's own id is enough");
  assert.equal(movesWorkstream({ type: "workstream_edited", workstream: "w1", project: "p1" }, "w1", "p1"), true);
  assert.equal(movesWorkstream({ type: "workstream_edited", workstream: "w9", project: "p1" }, "w1", "p1"), false, "a sibling renamed is the rail's to draw, not this panel's to read");
  // Who commits is the repository's: every checkout of the project reads again.
  assert.equal(movesWorkstream({ type: "committer_set", workstream: "p1", project: "p1", identity: { name: "Ada", email: "ada@example.com" } }, "w1", "p1"), true);
  assert.equal(movesWorkstream({ type: "committer_set", workstream: "p2", project: "p2" }, "w1", "p1"), false);
  assert.equal(movesWorkstream({ type: "project_changed", project: "p1" }, "w1", "p1"), true);
  assert.equal(movesWorkstream({ type: "project_changed", project: "p2" }, "w1", "p1"), false);
  assert.equal(movesWorkstream({ type: "project_changed", project: "" }, "w1", ""), false, "no project read yet names no project");
  for (const payload of [null, undefined, {}, { type: 7 }, { type: "workstream_publish_failed", workstream: "w1", project: "p1", what: "push", reason: "refused" }]) {
    assert.equal(movesWorkstream(payload, "w1", "p1"), false, JSON.stringify(payload));
  }
});

test("the status store, the one checkout's hook and the Board read this list and spell none of their own", () => {
  const store = read("./workstreamStatusStore.ts");
  assert.ok(store.includes("movesStatuses("), "the store asks the model");
  const hook = read("../views/_work/useWorkstream.ts");
  assert.ok(hook.includes("movesWorkstream("), "the hook asks the model");
  const board = read("../views/_board/BoardCenter.tsx");
  assert.ok(board.includes("useWorkstreamStatuses("), "the Board reads the rail's statuses");
  assert.ok(!board.includes("allWorkstreamStatuses"), "and polls nothing of its own");
  for (const [name, text] of [["workstreamStatusStore.ts", store], ["useWorkstream.ts", hook], ["BoardCenter.tsx", board]]) {
    const code = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
    for (const type of RECORD_FRAMES) assert.ok(!code.includes(`"${type}"`), `${name} spells ${type} itself`);
  }
});
