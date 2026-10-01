/**
 * The Changes toolbar's `▾`. Run with
 * `node --test desktop/src/views/_work/changesBulkModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { bulkMenu } from "./changesBulkModel.mjs";
import { groupGitFiles, stageScopes } from "./gitFiles.mjs";

const row = (path, index, worktree, over = {}) => ({
  path,
  index,
  worktree,
  staged: index !== "." && index !== "?",
  unstaged: worktree !== "." && worktree !== "?",
  untracked: false,
  conflicted: false,
  ...over,
});

test("the ▾ is the other stage scopes, then apart the two throw-aways — each counted, off at zero, the throw-aways one red group", () => {
  const scopes = stageScopes(groupGitFiles([row("a.rs", ".", "M"), row("b.rs", "M", "."), row("new.txt", "?", "?", { untracked: true }), row("also.txt", "?", "?", { untracked: true })]));
  const items = bulkMenu(scopes);
  assert.deepEqual(
    items.map((i) => [i.id, i.label, i.disabled, i.separatorBefore ?? false, i.danger ?? false]),
    [
      ["stage_tracked", "Stage tracked (1)", false, false, false],
      ["stage_untracked", "Stage untracked (2)", false, false, false],
      ["unstage_all", "Unstage all (1)", false, true, false],
      ["discard_all", "Discard all changes… (1)", false, true, true],
      ["delete_untracked", "Delete all untracked files… (2)", false, false, true],
    ],
  );
  assert.deepEqual(items.at(-1).paths, ["also.txt", "new.txt"], "every file git has never seen — the untracked scope's, so one source");
  assert.deepEqual(items.find((i) => i.id === "discard_all").paths, ["a.rs"], "a working-tree change, never an untracked file");
  assert.deepEqual(items.find((i) => i.id === "stage_untracked").paths, items.at(-1).paths, "stage untracked and delete untracked reach the same files");
});

test("nothing to take: every verb is off, each still naming its zero", () => {
  const none = bulkMenu(stageScopes(null));
  assert.ok(none.every((i) => i.disabled));
  assert.equal(none.at(-1).label, "Delete all untracked files… (0)");
  assert.equal(none[0].label, "Stage tracked (0)");
  const only = bulkMenu(stageScopes(groupGitFiles([row("new.txt", "?", "?", { untracked: true })])));
  assert.ok(only.find((i) => i.id === "discard_all").disabled, "an untracked file is not git's to discard");
  assert.ok(!only.find((i) => i.id === "delete_untracked").disabled);
});
