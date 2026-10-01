/**
 * The bar above the composer reads the same ledger the turn cards do, folded
 * for a summary. Run with `node --test desktop/src/views/_studio/changedFilesModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { barWords, changeKindMark, changeKindTone, changedByAgents, changedFileRows, changedFilesHeaderWords, splitPathForRow } from "./changedFilesModel.mjs";

function file(over = {}) {
  return { path: "src/a.ts", kind: "modified", state: "pending", opaque: false, overlapped: false, added: 3, removed: 1, ...over };
}
function turn(over = {}) {
  return { turn: "t1", agent: "reviewer", mode: "manual", files: [file()], ...over };
}

test("every pending path once — the later turn's word wins — sorted, and nothing settled", () => {
  const view = {
    turns: [
      turn({ turn: "t1", files: [file({ path: "src/b.ts", added: 10 }), file({ path: "src/a.ts", state: "kept" })] }),
      turn({ turn: "t2", files: [file({ path: "src/b.ts", added: 2, removed: 2 }), file({ path: "README.md", kind: "created", removed: 0 })] }),
    ],
  };
  const rows = changedFileRows(view);
  assert.deepEqual(rows.map((r) => r.path), ["README.md", "src/b.ts"]);
  assert.equal(rows[1].added, 2, "the later turn's count");
  assert.deepEqual(changedFileRows(null), []);
  assert.deepEqual(changedFileRows({ turns: [turn({ files: [file({ state: "undone" })] })] }), []);
});

test("the agents named are those with something still pending, first seen first, once", () => {
  const view = {
    turns: [
      turn({ turn: "t1", agent: "reviewer" }),
      turn({ turn: "t2", agent: "developer", files: [file({ path: "x.ts", state: "kept" })] }),
      turn({ turn: "t3", agent: "reviewer", files: [file({ path: "y.ts" })] }),
      turn({ turn: "t4", agent: "designer", files: [file({ path: "z.css" })] }),
    ],
  };
  assert.deepEqual(changedByAgents(view), ["reviewer", "designer"]);
});

test("the header says how many files, by whom and how much — and nothing when nothing is pending", () => {
  const names = (id) => ({ reviewer: "Reviewer", designer: "Designer" })[id] ?? id;
  assert.equal(changedFilesHeaderWords({ turns: [turn()] }, names), "1 file changed by Reviewer · +3 −1");
  const two = { turns: [turn(), turn({ turn: "t2", agent: "designer", files: [file({ path: "z.css", added: 7, removed: 0 })] })] };
  assert.equal(changedFilesHeaderWords(two, names), "2 files changed by 2 agents · +10 −1");
  assert.equal(changedFilesHeaderWords({ turns: [turn({ files: [file({ state: "kept" })] })] }), "");
  assert.equal(changedFilesHeaderWords({ turns: [turn()] }), "1 file changed by reviewer · +3 −1", "no names: the id");
});

test("a row's path splits into its folders and its name; a change's kind has a mark and a tone", () => {
  assert.deepEqual(splitPathForRow("src/views/App.tsx"), { dir: "src/views/", base: "App.tsx" });
  assert.deepEqual(splitPathForRow("README.md"), { dir: "", base: "README.md" });
  assert.equal(changeKindMark("created"), "+");
  assert.equal(changeKindMark("removed"), "−");
  assert.equal(changeKindMark("modified"), "~");
  assert.equal(changeKindTone("created"), "ok");
  assert.equal(changeKindTone("modified"), "warn");
  assert.equal(changeKindTone("removed"), "warn");
});

test("the footer's words are the platform's own — Keep and Undo, never Accept and Reject", () => {
  const w = barWords();
  assert.deepEqual(w, { keepAll: "Keep all", undoAll: "Undo all", review: "Review", attach: "Attach" });
  for (const v of Object.values(w)) assert.ok(!/accept|reject/i.test(v));
});
