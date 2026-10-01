import { test } from "node:test";
import assert from "node:assert/strict";
import { LANE_W, REF_SCOPES, commitCount, emptyRows, historyMenu, holesIn, laneColor, laneX, lanesWidth, layoutWord, matches, mergeWindow, nextIndex, refScopeWords, searchStatus, strokesFor } from "./graphModel.mjs";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

test("the one filter mirrors the node's ref scopes, and the header's menu says search, the scope in force and lay out again", () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const rust = readFileSync(join(here, "../../../../crates/bisa-vcs/src/git.rs"), "utf8");
  const body = rust.match(/pub enum RefScope \{([\s\S]*?)\n\}/);
  assert.ok(body, "git.rs declares RefScope");
  const variants = [...body[1].matchAll(/^\s{4}([A-Z][a-z]+),/gm)].map((m) => m[1].toLowerCase());
  assert.deepEqual([...REF_SCOPES], variants);
  assert.match(refScopeWords("head").label, /only the current branch/);
  assert.match(refScopeWords("all").label, /all branches and tags/);
  const menu = historyMenu({ refs: "head", searching: false });
  assert.deepEqual(
    menu.map((i) => [i.id, i.icon, i.active ?? false, i.separatorBefore ?? false]),
    [
      ["search", "search", false, false],
      ["refs:all", null, false, true],
      ["refs:head", "check", true, false],
      ["relayout", "refresh", false, true],
    ],
  );
  assert.equal(menu[0].label, "Search…");
  assert.equal(historyMenu({ refs: "all", searching: true })[0].label, "Hide the search");
  assert.equal(historyMenu({ refs: "all", searching: true })[1].icon, "check");
});

const row = (over) => ({
  id: "a".repeat(40),
  short: "aaaaaaa",
  lane: 0,
  passing: [0],
  edges: [],
  refs: [],
  author: "Ada",
  timestamp: 1,
  subject: "a commit",
  parents: 1,
  ...over,
});

test("a linear row is one line through the node", () => {
  const s = strokesFor(row(), row());
  assert.deepEqual(
    s.map((x) => [x.kind, x.y1, x.y2]),
    [
      ["line", 0, 0.5],
      ["line", 0.5, 1],
    ],
  );
  assert.equal(s[0].x1, laneX(0));
});

test("the first row has nothing entering, and a root has nothing leaving", () => {
  assert.deepEqual(strokesFor(row(), null).map((x) => x.y1), [0.5]);
  assert.deepEqual(strokesFor(row({ passing: [] }), row()).map((x) => x.y2), [0.5]);
});

test("a fork curves out of the node and a merge curves into it, while other lanes pass straight", () => {
  // Merge commit at lane 0 with a second parent in lane 1; lane 2 passes by.
  const merge = row({ lane: 0, passing: [0, 1, 2], edges: [{ from: 0, to: 1, kind: "fork" }] });
  const above = row({ passing: [0, 2] });
  const s = strokesFor(merge, above);
  const fork = s.find((x) => x.kind === "curve");
  assert.deepEqual([fork.lane, fork.x1, fork.x2, fork.y1, fork.y2], [1, laneX(0), laneX(1), 0.5, 1]);
  const passing = s.filter((x) => x.lane === 2);
  assert.equal(passing.length, 2, "lane 2 enters and leaves untouched");
  assert.ok(passing.every((x) => x.kind === "line" && x.x1 === laneX(2)));

  // The root that both lanes converge on: lane 1 merges into lane 0.
  const root = row({ lane: 0, passing: [], edges: [{ from: 1, to: 0, kind: "merge" }] });
  const s2 = strokesFor(root, merge);
  const into = s2.find((x) => x.kind === "curve");
  assert.deepEqual([into.lane, into.x1, into.x2, into.y1, into.y2], [1, laneX(1), laneX(0), 0, 0.5]);
  assert.ok(!s2.some((x) => x.lane === 2), "lane 2 stopped above, nothing enters from it");
});

test("width follows the widest lane in the window, and colours cycle", () => {
  assert.equal(lanesWidth([row(), row({ passing: [0, 3] })]), 4 * LANE_W);
  assert.equal(lanesWidth([]), LANE_W);
  assert.equal(laneColor(0), laneColor(6));
  assert.notEqual(laneColor(0), laneColor(1));
});

test("search matches subject, author, id prefix and refs — the node's rule, so a highlight is a hit", () => {
  const rows = [
    row({ subject: "Fix cart", author: "Ada" }),
    row({ id: "b".repeat(40), short: "bbbbbbb", subject: "Add SSO", author: "Grace", refs: [{ name: "feature/sso", kind: "branch" }] }),
    row({ id: "c".repeat(40), short: "ccccccc", subject: "Docs", author: "Ada" }),
  ];
  assert.ok(matches(rows[0], "CART"));
  assert.ok(matches(rows[1], "sso"));
  assert.ok(matches(rows[1], "bbbb"));
  assert.ok(!matches(rows[2], "cart"));
  assert.ok(matches(rows[2], ""), "empty matches everything");
});

test("the rows are sparse: a window lands where it belongs and the holes around the screen are what gets fetched", () => {
  let rows = emptyRows(1000);
  assert.equal(rows.length, 1000);
  assert.deepEqual(holesIn(rows, 0, 30), [{ from: 0, count: 400 }], "one page covers the first screen");
  assert.deepEqual(holesIn(rows, 390, 420), [{ from: 0, count: 400 }, { from: 400, count: 400 }], "a screen across a page edge asks for both");
  assert.deepEqual(holesIn(rows, 900, 1000), [{ from: 800, count: 200 }], "the last page is as long as what is left");
  const page = (from, n) => ({ total: 1000, done: true, stale: false, from, rows: Array.from({ length: n }, (_, i) => row({ id: String(from + i).padStart(40, "0"), short: String(from + i) })) });
  rows = mergeWindow(rows, page(0, 400));
  assert.equal(rows[0].short, "0");
  assert.equal(rows[399].short, "399");
  assert.equal(rows[400], undefined);
  assert.deepEqual(holesIn(rows, 0, 30), [], "filled: nothing to fetch");
  assert.deepEqual(holesIn(rows, 390, 420), [{ from: 400, count: 400 }], "only the page with the hole");
  const same = mergeWindow(rows, page(0, 400));
  assert.equal(same, rows, "the same rows again change nothing");
  const grown = mergeWindow(rows, { ...page(1000, 5), total: 1005 });
  assert.equal(grown.length, 1005, "the layout grew: the array follows");
  assert.equal(grown[0].short, "0", "and keeps what it had");
  assert.equal(grown[1004].short, "1004");
  const shrunk = mergeWindow(rows, { ...page(0, 2), total: 2 });
  assert.equal(shrunk.length, 2, "the repository moved on to fewer rows");
  assert.deepEqual(holesIn(emptyRows(0), 0, 10), [], "an empty log has no holes");
});

test("the next match index wraps in both directions, from a cursor that need not be a match", () => {
  const found = [3, 10, 42];
  assert.equal(nextIndex(found, -1, 1), 3);
  assert.equal(nextIndex(found, 3, 1), 10);
  assert.equal(nextIndex(found, 42, 1), 3, "wraps forward");
  assert.equal(nextIndex(found, 5, 1), 10, "from between matches");
  assert.equal(nextIndex(found, 3, -1), 42, "wraps backward");
  assert.equal(nextIndex(found, 20, -1), 10);
  assert.equal(nextIndex([], 0, 1), -1);
});

test("the search words say how many, which, and whether the whole log was there to search", () => {
  assert.equal(searchStatus(null, -1), "");
  assert.equal(searchStatus({ indices: [], searched: 50, done: true, truncated: false }, -1), "no match");
  assert.equal(searchStatus({ indices: [], searched: 1000, done: false, truncated: false }, -1), "no match in the first 1000 rows — still laying out");
  assert.equal(searchStatus({ indices: [3], searched: 50, done: true, truncated: false }, -1), "1 match");
  assert.equal(searchStatus({ indices: [3, 9], searched: 50, done: true, truncated: false }, -1), "2 matches");
  assert.equal(searchStatus({ indices: [3, 9], searched: 50, done: true, truncated: false }, 9), "2 of 2");
  assert.equal(searchStatus({ indices: [3, 9], searched: 50, done: true, truncated: true }, 3), "1 of 2+");
  assert.equal(searchStatus({ indices: [3], searched: 1000, done: false, truncated: false }, 3), "1 of 1 in the first 1000 rows — still laying out");
});

test("the header sentence", () => {
  assert.equal(commitCount({ total: 1, done: true, stale: false }), "1 commit");
  assert.equal(commitCount({ total: 1000, done: false, stale: false }), "1,000 so far", "the count stands on its own while the tail lays out");
  assert.equal(commitCount({ total: 5, done: true, stale: true }), "5 so far", "a stale count is not final");
  assert.equal(layoutWord({ total: 1, done: true, stale: false }), null, "a settled graph has no status word");
  assert.equal(layoutWord({ total: 1000, done: false, stale: false }), "laying out");
  assert.match(layoutWord({ total: 5, done: true, stale: true }), /moved on/);
});

test("the current branch is the one HEAD's loaded row names, none detached or unloaded; the keyboard moves the cursor within the rows", async () => {
  const { currentBranchOf, cursorAfterKey } = await import("./graphModel.mjs");
  const loaded = [
    { id: "h", refs: [{ kind: "tag", name: "v1" }, { kind: "branch", name: "main" }] },
    { id: "d", refs: [{ kind: "tag", name: "v0" }] },
  ];
  assert.equal(currentBranchOf("h", loaded), "main");
  assert.equal(currentBranchOf("d", loaded), null, "detached at a tag");
  assert.equal(currentBranchOf("nope", loaded), null, "HEAD's row not loaded yet");
  assert.equal(currentBranchOf(null, loaded), null);
  assert.equal(cursorAfterKey("ArrowDown", 0, 3), 1);
  assert.equal(cursorAfterKey("ArrowDown", 2, 3), 2, "clamped at the end");
  assert.equal(cursorAfterKey("ArrowUp", 0, 3), 0, "clamped at the start");
  assert.equal(cursorAfterKey("Home", 2, 3), 0);
  assert.equal(cursorAfterKey("End", 0, 3), 2);
  assert.equal(cursorAfterKey("Enter", 0, 3), null, "not a move");
  assert.equal(cursorAfterKey("ArrowDown", 0, 0), null, "no rows, nowhere to go");
});
