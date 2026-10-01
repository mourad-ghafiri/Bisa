/**
 * The Board as a person works it (ide/16): a workstream's column is its
 * lifecycle state until a person places it, a closed one is Archived whatever
 * was chosen, a drag lands where it was dropped before the node answers and
 * the same drop twice changes nothing, the rail's selection narrows the rows,
 * and the headers count what needs a hand. Stepped through the models the
 * way the components do; no DOM.
 *
 * Run with `node --test desktop/src/scenarios/board.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { COLUMNS, boardRows, columnForState, columnShown, dueTone, headerCounts, optimisticMove, wipState } from "../views/_board/boardModel.mjs";

const ws = (id, state, over = {}) => ({ id, project: over.project ?? "p1", kind: { kind: "worktree", branch: `work/${id}`, base: "main" }, state: { state }, created_at: over.created_at ?? 100, board: over.board ?? null, ...over });
const ref = (w, project_name = "Shop") => ({ workstream: w, project_name, path: null, exists: true });
const ids = (rows, column) => rows.columns[column].map((r) => r.id);

test("a workstream moves through its states and the columns follow; a person's placement wins until the workstream closes", () => {
  assert.ok(COLUMNS.includes("backlog") && COLUMNS.includes("doing") && COLUMNS.includes("done") && COLUMNS.includes("archived"), "the columns are the core's");
  const open = ws("w1", "open");
  assert.equal(columnForState(open.state), "backlog");
  for (const s of ["dirty", "committed", "pushed", "pr_open"]) assert.equal(columnForState({ state: s }), "doing", `${s} is work in progress`);
  assert.equal(columnForState({ state: "merged" }), "done");
  assert.equal(columnForState({ state: "closed" }), "archived");
  // Placed on Done by hand while still open: the placement is shown.
  assert.equal(columnShown(ws("w2", "open", { board: { column: "done", rank: 1024 } })), "done");
  // Closed: Archived, whatever the person had chosen.
  assert.equal(columnShown(ws("w3", "closed", { board: { column: "doing", rank: 1024 } })), "archived");
});

test("the rows: placed cards by rank then the rest oldest first, archived out unless asked, the rail's selection and the search narrow, and a drop lands once", () => {
  const refs = [
    ref(ws("a", "open", { created_at: 300 })),
    ref(ws("b", "open", { created_at: 100 })),
    ref(ws("c", "open", { created_at: 200, board: { column: "backlog", rank: 2048 } })),
    ref(ws("d", "committed", { project: "p2" }), "Docs"),
    ref(ws("e", "closed")),
  ];
  const statuses = { a: { branch: "work/a" } };
  let board = boardRows({ refs, statuses });
  assert.deepEqual(ids(board, "backlog"), ["c", "b", "a"], "the placed card first by rank, then the oldest");
  assert.deepEqual(ids(board, "doing"), ["d"]);
  assert.deepEqual(ids(board, "archived"), [], "the closed one is hidden until asked");
  assert.equal(board.total, 5);
  assert.equal(board.hidden, 1);
  assert.deepEqual(ids(boardRows({ refs, statuses, archived: true }), "archived"), ["e"]);
  assert.deepEqual(ids(boardRows({ refs, statuses, projects: new Set(["p2"]) }), "backlog"), [], "the rail's selection narrows every column");
  assert.deepEqual(ids(boardRows({ refs, statuses, projects: new Set(["p2"]) }), "doing"), ["d"]);
  assert.deepEqual(ids(boardRows({ refs, statuses, needle: "Docs" }), "doing"), ["d"], "the search reads the project's name");

  // A drag from Backlog to Doing, dropped first: the card is there before the node answers.
  const moved = optimisticMove(board.columns, "b", "doing", 0);
  assert.deepEqual(moved.doing.map((r) => r.id), ["b", "d"]);
  assert.deepEqual(moved.backlog.map((r) => r.id), ["c", "a"]);
  assert.equal(moved.doing[0].column, "doing", "the card wears its new column");
  // The same drop again changes nothing — the same object, so nothing re-renders.
  assert.equal(optimisticMove(moved, "b", "doing", 0), moved);
  // A drop past the end lands last; an unknown card is a no-op.
  assert.deepEqual(optimisticMove(moved, "a", "doing", 99).doing.map((r) => r.id), ["b", "d", "a"]);
  assert.equal(optimisticMove(moved, "nope", "doing", 0), moved);
});

test("the headers say what needs a hand: a WIP limit reached, a card overdue, and a due date's tone as the days pass", () => {
  assert.deepEqual(wipState(2, 3), { over: false, label: "2 / 3", title: null });
  assert.deepEqual(wipState(3, 3), { over: false, label: "3 / 3", title: null }, "at the limit is not over it");
  assert.equal(wipState(4, 3).over, true);
  assert.ok(wipState(4, 3).title, "over the limit the header says so in words");
  assert.deepEqual(wipState(1, 0), { over: false, label: "1", title: null }, "no limit: the count alone");
  const today = "2026-09-16";
  assert.equal(dueTone("2026-09-10", today).kind, "overdue");
  assert.equal(dueTone("2026-09-16", today).kind, "today");
  assert.equal(dueTone("2026-09-18", today).kind, "soon");
  assert.equal(dueTone("2026-10-30", today).kind, "due");
  assert.equal(dueTone(null, today).kind, "none");
  const rows = [
    { id: "x", due: "2026-09-10", status: { holder: "you" } },
    { id: "y", due: null, status: null },
  ];
  const counts = headerCounts(rows, today, 3);
  assert.ok(counts && typeof counts === "object", "the header's counts are one object");
});
