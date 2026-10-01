/**
 * The Board's rules, held against the core's (`crates/bisa-core/src/board.rs`).
 * Run with `node --test desktop/src/views/_board/boardModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  COLUMNS,
  COLUMN_LABEL,
  MAX_INDEX,
  boardRows,
  canMoveTo,
  closeWords,
  columnForState,
  columnShown,
  countWords,
  daysUntil,
  dueTone,
  headerCounts,
  lastActivity,
  matches,
  optimisticMove,
  placeBody,
  placeIndex,
  projectWords,
  statusIndex,
  todayKey,
  wipState,
} from "./boardModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));

function ws(id, over = {}) {
  return {
    id,
    project: "p1",
    kind: { kind: "worktree", branch: `feature/${id}`, base: "main" },
    state: { state: "open" },
    created_at: 100,
    ...over,
  };
}
function ref(w, project_name = "Shop") {
  return { workstream: w, project_name, path: null, exists: true };
}

test("the five columns are the core's, in its order", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/board.rs"), "utf8");
  const enumBody = rust.split("pub enum BoardColumn {")[1].split("}")[0];
  const variants = [...enumBody.matchAll(/^\s+([A-Z][a-z]+),/gm)].map((m) => m[1].toLowerCase());
  assert.deepEqual([...COLUMNS], variants);
  for (const c of COLUMNS) assert.equal(typeof COLUMN_LABEL[c], "string");
});

test("an unplaced card follows the lifecycle and a closed one is always archived", () => {
  assert.equal(columnForState({ state: "open" }), "backlog");
  for (const s of ["dirty", "committed", "pushed", "pr_open"]) assert.equal(columnForState({ state: s }), "doing", s);
  assert.equal(columnForState({ state: "merged" }), "done");
  assert.equal(columnForState({ state: "closed" }), "archived");
  assert.equal(columnShown(ws("a", { state: { state: "committed" } })), "doing");
  assert.equal(columnShown(ws("a", { state: { state: "committed" }, board: { column: "todo" } })), "todo", "a choice stands");
  assert.equal(columnShown(ws("a", { state: { state: "closed" }, board: { column: "todo" } })), "archived");
});

test("a due date reads as overdue, today, soon or due, from the machine's day", () => {
  const today = "2026-09-07";
  assert.equal(dueTone(null, today).kind, "none");
  assert.deepEqual(dueTone("2026-09-05", today), { kind: "overdue", tone: "danger", label: "overdue by 2 days", days: -2 });
  assert.equal(dueTone("2026-09-06", today).label, "overdue by a day");
  assert.deepEqual(dueTone("2026-09-07", today), { kind: "today", tone: "warn", label: "due today", days: 0 });
  assert.equal(dueTone("2026-09-08", today).label, "due tomorrow");
  assert.deepEqual(dueTone("2026-09-10", today, 3), { kind: "soon", tone: "warn", label: "due in 3 days", days: 3 });
  assert.deepEqual(dueTone("2026-09-11", today, 3), { kind: "due", tone: "quiet", label: "due 2026-09-11", days: 4 });
  assert.equal(dueTone("2026-09-11", today, 7).kind, "soon", "the setting widens soon");
  assert.equal(daysUntil("2026-03-01", "2026-02-28"), 1, "across a month");
});

test("rows land in their columns, placed by rank first then oldest first, filtered and counted", () => {
  const refs = [
    ref(ws("a", { created_at: 300, board: { column: "todo", rank: 2048 } })),
    ref(ws("b", { created_at: 100 })),
    ref(ws("c", { created_at: 200, name: "Cart total", board: { column: "todo", rank: 1024 } })),
    ref(ws("d", { state: { state: "merged", number: 1, url: "u" } })),
    ref(ws("e", { state: { state: "closed" } })),
    ref(ws("f", { project: "p2", state: { state: "committed" } }), "Blog"),
  ];
  const statuses = { b: { branch: "feature/b-live" } };
  const out = boardRows({ refs, statuses });
  assert.deepEqual(out.columns.todo.map((r) => r.id), ["c", "a"], "by rank");
  assert.deepEqual(out.columns.backlog.map((r) => r.id), ["b"]);
  assert.deepEqual(out.columns.done.map((r) => r.id), ["d"]);
  assert.deepEqual(out.columns.doing.map((r) => r.id), ["f"]);
  assert.deepEqual(out.columns.archived, [], "archived hidden by default");
  assert.equal(out.total, 6);
  assert.equal(out.hidden, 1);
  assert.equal(boardRows({ refs, statuses, archived: true }).columns.archived.length, 1);
  assert.equal(out.columns.backlog[0].branch, "feature/b-live", "the live branch when the status knows it");
  assert.equal(out.columns.todo[0].title, "Cart total");
  assert.equal(out.columns.todo[1].title, "feature/a");
  const byProject = boardRows({ refs, statuses, projects: new Set(["p2"]) });
  assert.deepEqual(byProject.columns.doing.map((r) => r.id), ["f"]);
  assert.equal(byProject.hidden, 5);
  const byGroup = boardRows({ refs, statuses, projects: new Set(["p1", "p2"]) });
  assert.equal(byGroup.hidden, 1, "a group is its projects' cards — the Archived one still folded away");
  assert.equal(boardRows({ refs, statuses, projects: new Set() }).hidden, 6, "a selection with no project shows nothing");
  const byNeedle = boardRows({ refs, statuses, needle: "cart" });
  assert.deepEqual(byNeedle.columns.todo.map((r) => r.id), ["c"]);
  assert.ok(matches({ title: "x", branch: null, project: { name: "Blog" }, workstream: { note: null } }, "blo"), "the project's name counts");
  assert.ok(matches({ title: "x", branch: null, project: { name: "" }, workstream: { note: "fix the cart" } }, "cart"), "the note counts");
});

test("a card's title is the one the rail's row and the panel's header wear: the name, else the branch, else the primary's live branch or its word", () => {
  const refs = [
    ref(ws("a", { name: " Cart " })),
    ref(ws("b")),
    ref(ws("p1", { kind: { kind: "primary" } })),
    ref(ws("p2", { kind: { kind: "primary" }, project: "p2" })),
    ref(ws("abcdef", { kind: { kind: "copy" } })),
    ref(ws("blank", { name: "   " })),
  ];
  const { columns } = boardRows({ refs, statuses: { p1: { branch: "main" } } });
  const title = Object.fromEntries(columns.backlog.map((r) => [r.id, r.title]));
  assert.deepEqual(title, { a: "Cart", b: "feature/b", p1: "main", p2: "primary", abcdef: "copy · abcdef", blank: "feature/blank" });
  // One rule, not a second spelling of it: the model asks the card model.
  const source = readFileSync(join(here, "boardModel.mjs"), "utf8");
  assert.ok(source.includes("title: cardTitle(w, status)"));
});

test("the lifecycle rule is the core's, state by state, read from its source", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/board.rs"), "utf8");
  const body = rust.split("pub fn for_state(")[1].split("\n    }\n")[0];
  // `WorkstreamState::PrOpen { .. } => BoardColumn::Doing`, with the arms that share a column joined by `|`.
  const arms = [...body.matchAll(/((?:\s*\|?\s*WorkstreamState::[A-Za-z]+(?:\s*\{[^}]*\})?)+)\s*=>\s*BoardColumn::([A-Za-z]+)/g)];
  const snake = (s) => s.replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase();
  const rule = {};
  for (const [, states, column] of arms) for (const m of states.matchAll(/WorkstreamState::([A-Za-z]+)/g)) rule[snake(m[1])] = column.toLowerCase();
  assert.deepEqual(Object.keys(rule).sort(), ["closed", "committed", "dirty", "merged", "open", "pr_open", "pushed"], "every state of the record has its arm");
  for (const [state, column] of Object.entries(rule)) assert.equal(columnForState({ state }), column, state);
});

test("the Doing limit warns and never refuses; zero is no limit", () => {
  assert.deepEqual(wipState(3, 0), { over: false, label: "3", title: null });
  assert.deepEqual(wipState(3, 3), { over: false, label: "3 / 3", title: null });
  assert.deepEqual(wipState(4, 3), { over: true, label: "4 / 3", title: "Over the Doing limit — a warning, never a refusal" });
  assert.deepEqual(wipState(4, -1), { over: false, label: "4", title: null }, "a limit below zero is no limit");
});

test("the bar counts what the scope and the filters leave, in the catalog's number", () => {
  assert.equal(countWords(3, 12), "3 of 12 workstreams");
  assert.equal(countWords(1, 1), "1 of 1 workstream");
  assert.equal(countWords(0, 0), "0 of 0 workstreams");
});

test("a close says what it does here, then what ends with it — and nothing more when nothing stands there", () => {
  const does = "The record moves to Archived; the checkout stays on disk. Removing the checkout is the IDE's, with a recovery ref first.";
  assert.equal(closeWords(null), does);
  assert.equal(closeWords(""), does);
  assert.equal(closeWords("Closing it ends what stands in it: 1 shell closed."), `${does} Closing it ends what stands in it: 1 shell closed.`);
});

test("a card's project is a door by its name, and by a word when the workspace could not name it", () => {
  assert.equal(projectWords({ name: "Shop" }), "Shop");
  assert.equal(projectWords({ name: "" }), "project");
  assert.equal(projectWords(null), "project");
});

test("a card moves to any column but its own, and a closed one nowhere but Archived", () => {
  const open = { column: "backlog", workstream: ws("a") };
  assert.deepEqual(COLUMNS.filter((c) => canMoveTo(open, c)), ["todo", "doing", "done", "archived"]);
  const closed = { column: "archived", workstream: ws("a", { state: { state: "closed" } }) };
  assert.deepEqual(COLUMNS.filter((c) => canMoveTo(closed, c)), [], "the node refuses the same, by name");
});

test("the body a placement sends is a whole index the wire takes: never negative, never past a u32", () => {
  assert.deepEqual(placeBody("doing", 3), { column: "doing", index: 3 });
  assert.deepEqual(placeBody("doing", 2.9), { column: "doing", index: 2 });
  assert.deepEqual(placeBody("doing", -4), { column: "doing", index: 0 });
  assert.deepEqual(placeBody("doing", Number.MAX_SAFE_INTEGER), { column: "doing", index: MAX_INDEX }, "the index the menu's Move to once sent is one the node refuses");
  assert.deepEqual(placeBody("doing", Number.NaN), { column: "doing", index: 0 });
  assert.equal(MAX_INDEX, 2 ** 32 - 1);
});

test("a card set down on a narrowed board lands by its neighbour, counted among every placed card of the column as the node counts", () => {
  // Doing, by rank: a (Shop), x (Blog), b (Shop), y (Blog), then c (Shop) there by its lifecycle alone.
  const refs = [
    ref(ws("a", { state: { state: "committed" }, board: { column: "doing", rank: 1024 } })),
    ref(ws("x", { project: "p2", board: { column: "doing", rank: 2048 } }), "Blog"),
    ref(ws("b", { board: { column: "doing", rank: 3072 } })),
    ref(ws("y", { project: "p2", board: { column: "doing", rank: 4096 } }), "Blog"),
    ref(ws("c", { state: { state: "dirty" }, created_at: 50 })),
    ref(ws("m", { created_at: 10 })),
  ];
  const shop = boardRows({ refs, statuses: {}, projects: new Set(["p1"]) }).columns;
  assert.deepEqual(shop.doing.map((r) => r.id), ["a", "b", "c"], "the Blog's cards are left out by the rail's selection");
  const drop = (index, shown = shop, id = "m") => placeIndex({ refs, shown, id, column: "doing", index });
  assert.equal(drop(0), 0, "before a: the first of the column");
  assert.equal(drop(1), 2, "before b — past x, which the Board does not show");
  assert.equal(drop(2), 3, "after b and before c, which has no rank: right after b");
  assert.equal(drop(3), 3, "after c: a card with no rank has no say, so still after b");
  assert.equal(drop(99), 3, "past the end is the end of what is shown");
  assert.equal(drop(-1), 0);
  // The whole board: the index is the one among the placed cards.
  const all = boardRows({ refs, statuses: {} }).columns;
  assert.deepEqual(all.doing.map((r) => r.id), ["a", "x", "b", "y", "c"]);
  assert.deepEqual([0, 1, 2, 3, 4, 5].map((i) => drop(i, all)), [0, 1, 2, 3, 4, 4]);
  // A card moved along its own column is left out of its neighbours, as the node leaves it out.
  assert.equal(placeIndex({ refs, shown: all, id: "a", column: "doing", index: 1 }), 1, "a, set down after x: x is first, a second");
  assert.equal(placeIndex({ refs, shown: all, id: "a", column: "doing", index: 0 }), 0);
  // A column holding nothing placed, or nothing at all.
  assert.equal(placeIndex({ refs, shown: all, id: "a", column: "backlog", index: 1 }), 0, "m is in Backlog by its lifecycle alone");
  assert.equal(placeIndex({ refs, shown: all, id: "a", column: "done", index: 0 }), 0);
  assert.equal(placeIndex({ refs, shown: {}, id: "a", column: "doing", index: 7 }), 3, "nothing shown: last among the placed");
  // A closed workstream a person once placed in Doing is drawn in Archived and still counted by the node in Doing.
  const withClosed = [...refs, ref(ws("z", { state: { state: "closed" }, board: { column: "doing", rank: 512 } }))];
  const shown = boardRows({ refs: withClosed, statuses: {} }).columns;
  assert.ok(!shown.doing.some((r) => r.id === "z"));
  assert.equal(placeIndex({ refs: withClosed, shown, id: "m", column: "doing", index: 0 }), 1, "before a, which the node counts second");
});

test("the statuses are read by workstream, and a card last moved when its newest session did", () => {
  assert.deepEqual(statusIndex([{ workstream: "a", branch: "x" }, { workstream: "b", branch: "y" }]), { a: { workstream: "a", branch: "x" }, b: { workstream: "b", branch: "y" } });
  assert.deepEqual(statusIndex(null), {});
  const row = { id: "a", workstream: ws("a", { created_at: 100 }) };
  const sessions = [{ workstream: "a", last_activity: 300 }, { workstream: "a", last_activity: 250 }, { workstream: "b", last_activity: 900 }, { workstream: "a" }];
  assert.equal(lastActivity(sessions, row, 5_000_000), 300);
  assert.equal(lastActivity([], row, 5_000_000), 100, "nobody stands in it: when it was opened");
  assert.equal(lastActivity(null, { id: "a", workstream: ws("a", { created_at: 0 }) }, 5_000_000), 5000, "a record with no birth reads as now");
});

test("an optimistic move rearranges the columns and a move that changes nothing is the same object", () => {
  const out = boardRows({ refs: [ref(ws("a")), ref(ws("b")), ref(ws("c"))], statuses: {} });
  const cols = out.columns;
  const moved = optimisticMove(cols, "b", "doing", 0);
  assert.deepEqual(moved.backlog.map((r) => r.id), ["a", "c"]);
  assert.deepEqual(moved.doing.map((r) => r.id), ["b"]);
  assert.equal(moved.doing[0].column, "doing");
  assert.equal(moved.todo, cols.todo, "untouched columns keep their reference");
  const within = optimisticMove(cols, "c", "backlog", 0);
  assert.deepEqual(within.backlog.map((r) => r.id), ["c", "a", "b"]);
  assert.equal(optimisticMove(cols, "a", "backlog", 0), cols, "already there");
  assert.equal(optimisticMove(cols, "zzz", "doing", 0), cols, "unknown card");
  assert.deepEqual(optimisticMove(cols, "a", "backlog", 99).backlog.map((r) => r.id), ["b", "c", "a"], "past the end is last");
  // The preview while a card is in the air: into an empty column at its end, then back home at another slot.
  const previewed = optimisticMove(cols, "b", "doing", cols.doing.length);
  assert.deepEqual(previewed.doing.map((r) => r.id), ["b"], "an empty column's end is its one slot");
  const home = optimisticMove(previewed, "b", "backlog", 0);
  assert.deepEqual(home.backlog.map((r) => r.id), ["b", "a", "c"]);
  assert.deepEqual(home.doing, [], "the room it took closes behind it");
});

test("a header counts its cards and the overdue among them", () => {
  const rows = [{ due: "2026-09-01" }, { due: "2026-09-09" }, { due: null }];
  assert.deepEqual(headerCounts(rows, "2026-09-07", 3), { count: 3, overdue: 1 });
});

test("a day is a calendar day: across a month, a leap day, a year and a clock change, and never off by one at midnight", () => {
  assert.equal(daysUntil("2026-03-01", "2026-02-28"), 1);
  assert.equal(daysUntil("2028-03-01", "2028-02-28"), 2, "a leap year has its day");
  assert.equal(daysUntil("2027-01-01", "2026-12-31"), 1);
  // The days the clocks change are 23 and 25 hours long; a date is still one day from the next.
  assert.equal(daysUntil("2026-03-30", "2026-03-28"), 2);
  assert.equal(daysUntil("2026-10-26", "2026-10-24"), 2);
  // `todayKey` is this machine's day, at both ends of it.
  const lastSecond = new Date(2026, 8, 21, 23, 59, 59).getTime();
  const firstSecond = new Date(2026, 8, 22, 0, 0, 0).getTime();
  assert.equal(todayKey(lastSecond), "2026-09-21");
  assert.equal(todayKey(firstSecond), "2026-09-22");
  assert.equal(dueTone("2026-09-22", todayKey(lastSecond)).kind, "soon");
  assert.equal(dueTone("2026-09-22", todayKey(firstSecond)).kind, "today");
  assert.equal(dueTone("2026-09-21", todayKey(firstSecond)).label, "overdue by a day");
});

test("a due date nobody can read is no date, never a chip that says NaN", () => {
  for (const unreadable of ["soon", "2026-13", "", "09/21/2026", "2026-xx-01"]) {
    const d = dueTone(unreadable, "2026-09-21");
    assert.deepEqual([d.kind, d.label, d.days], ["none", "", null], JSON.stringify(unreadable));
  }
  assert.equal(headerCounts([{ workstream: ws("a", { board: { due: "soon" } }) }], "2026-09-21", 3).overdue ?? 0, 0);
});

test("a thousand cards land in their columns in order, quickly, and a card set down in its own place is the same board", () => {
  const n = 1000;
  const refs = Array.from({ length: n }, (_, i) => ref(ws(`w${i}`, { created_at: i, board: { column: COLUMNS[i % 4], rank: (n - i) * 1024 } })));
  const began = performance.now();
  const { columns } = boardRows({ refs, statuses: {} });
  const took = performance.now() - began;
  assert.ok(took < 1000, `${Math.round(took)} ms`);
  assert.equal(COLUMNS.reduce((sum, c) => sum + (columns[c]?.length ?? 0), 0), n, "every card once");
  for (const c of COLUMNS.slice(0, 4)) {
    const ranks = columns[c].map((r) => r.workstream.board.rank);
    assert.deepEqual(ranks, [...ranks].sort((a, b) => a - b), `${c} is in rank order`);
  }
  const first = columns.todo[0].workstream.id;
  assert.strictEqual(optimisticMove(columns, first, "todo", 0), columns, "set down where it is: nothing to redraw");
  const last = columns.todo.length - 1;
  assert.strictEqual(optimisticMove(columns, columns.todo[last].workstream.id, "todo", 9999), columns, "past the end is the end, where it already is");
});

