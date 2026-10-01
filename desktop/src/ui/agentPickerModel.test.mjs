/**
 * One predicate, one ranking, one cursor — for seven surfaces.
 *
 * What these pin down is not that the arithmetic is right; it is that the
 * *same* typing finds the *same* agent everywhere. Four predicates used to
 * coexist, and the visible symptom was a reader who found the Reviewer on the
 * Agents screen, went to the address tray to reach it, typed the same word
 * and got nothing.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  clampCursor,
  cursorAction,
  groupRows,
  haystack,
  matches,
  rank,
  toggleValue,
} from "./agentPickerModel.mjs";

const ADA = {
  id: "ada",
  name: "Ada Lovelace",
  description: "Writes the first programs.",
  harness: "claude-code",
  tags: ["engineering", "code"],
};
const REVIEWER = {
  id: "reviewer",
  name: "Second Pair",
  description: "Reads a diff and says what is wrong with it.",
  harness: "codex",
  tags: ["review", "quality"],
};
const SCRIBE = {
  id: "scribe",
  name: "Technical Writer",
  description: "Keeps the docs honest.",
  harness: "claude-code",
  tags: ["writing"],
};
const ALL = [ADA, REVIEWER, SCRIBE];

test("a query reaches the description, the tags and the harness, not only the name", () => {
  // The whole point of the rewrite: none of these three find anything under a
  // name-only predicate, and all three are how somebody actually looks.
  assert.deepEqual(rank(ALL, "diff"), [REVIEWER]);
  assert.deepEqual(rank(ALL, "quality"), [REVIEWER]);
  assert.deepEqual(rank(ALL, "codex"), [REVIEWER]);
  assert.deepEqual(rank(ALL, "claude-code"), [ADA, SCRIBE]);
});

test("an id matches by prefix, never by substring", () => {
  const hex = { id: "aabbccdd", name: "Keyed", tags: [] };
  assert.equal(matches(hex, "aabb"), true);
  // A 64-hex pubkey contains every three-letter run somewhere. Matching a
  // substring would put rows in the list that nobody can explain.
  assert.equal(matches(hex, "bbcc"), false);
});

test("an empty query offers everyone, in the order they arrived", () => {
  // Arrival order is the roster's order, which is most of what a roster buys.
  assert.deepEqual(rank(ALL, ""), ALL);
  assert.deepEqual(rank(ALL, "   "), ALL);
});

test("an exact name beats a prefix beats a word beats a mere mention", () => {
  const pair = [
    { id: "b", name: "Ada Lovelace Jr", tags: [] },
    { id: "a", name: "Ada", tags: [] },
    { id: "c", name: "Nobody", description: "asks Ada for help", tags: [] },
    { id: "d", name: "Second Ada", tags: [] },
  ];
  assert.deepEqual(
    rank(pair, "ada").map((c) => c.id),
    ["a", "b", "d", "c"],
  );
});

test("the ranking is stable, so two equally good matches keep the roster's order", () => {
  const rostered = [
    { id: "one", name: "Reviewer One", tags: [] },
    { id: "two", name: "Reviewer Two", tags: [] },
    { id: "three", name: "Reviewer Three", tags: [] },
  ];
  assert.deepEqual(
    rank(rostered, "reviewer").map((c) => c.id),
    ["one", "two", "three"],
  );
});

test("a limit is a screenful, and zero means no cap", () => {
  const many = Array.from({ length: 20 }, (_, i) => ({ id: `a${i}`, name: `Agent ${i}` }));
  assert.equal(rank(many, "agent", 6).length, 6);
  assert.equal(rank(many, "agent", 0).length, 20);
  assert.equal(rank(many, "agent").length, 20);
});

test("an absent list is empty rather than a crash", () => {
  assert.deepEqual(rank(undefined, "x"), []);
  assert.deepEqual(rank(null, ""), []);
  assert.deepEqual(groupRows(undefined), []);
  assert.equal(haystack(undefined), "");
});

test("groups keep the order they first appear in, and no group is one bucket", () => {
  const mixed = [
    { id: "a", name: "A", group: "Agents" },
    { id: "p", name: "P", group: "People" },
    { id: "b", name: "B", group: "Agents" },
  ];
  assert.deepEqual(
    groupRows(mixed).map((g) => [g.label, g.rows.map((r) => r.id)]),
    [
      ["Agents", ["a", "b"]],
      ["People", ["p"]],
    ],
  );
  assert.deepEqual(groupRows([{ id: "a", name: "A" }]), [
    { label: "", rows: [{ id: "a", name: "A" }] },
  ]);
});

test("the cursor clamps at both ends rather than wrapping", () => {
  assert.deepEqual(cursorAction(3, 0, "ArrowUp"), { type: "move", index: 0 });
  assert.deepEqual(cursorAction(3, 2, "ArrowDown"), { type: "move", index: 2 });
  assert.deepEqual(cursorAction(3, 0, "ArrowDown"), { type: "move", index: 1 });
  assert.deepEqual(cursorAction(3, 1, "Home"), { type: "move", index: 0 });
  assert.deepEqual(cursorAction(3, 1, "End"), { type: "move", index: 2 });
});

test("Enter and Tab pick; Escape dismisses even with nothing to pick", () => {
  assert.deepEqual(cursorAction(3, 1, "Enter"), { type: "pick", index: 1 });
  assert.deepEqual(cursorAction(3, 1, "Tab"), { type: "pick", index: 1 });
  assert.deepEqual(cursorAction(0, 0, "Escape"), { type: "dismiss" });
});

test("a key the picker does not own is left alone, or Enter would stop sending", () => {
  assert.equal(cursorAction(3, 0, "a"), null);
  assert.equal(cursorAction(3, 0, "Shift"), null);
  // An empty list owns nothing except Escape: Enter has to reach the composer.
  assert.equal(cursorAction(0, 0, "Enter"), null);
  assert.equal(cursorAction(0, 0, "ArrowDown"), null);
});

test("a cursor past the end of a filtered list is pulled back onto a real row", () => {
  assert.equal(clampCursor(9, 3), 2);
  assert.equal(clampCursor(-1, 3), 0);
  // Nothing to point at is index zero, not minus one: the caller renders no
  // `aria-activedescendant` at all in that case.
  assert.equal(clampCursor(4, 0), 0);
});

test("a single-choice picker swaps rather than refusing", () => {
  assert.deepEqual(toggleValue(["a"], "b", 1), ["b"]);
  assert.deepEqual(toggleValue(["a"], "a", 1), []);
});

test("at the cap a new pick is a no-op, and says so by returning the same array", () => {
  const value = ["a", "b"];
  assert.equal(toggleValue(value, "c", 2), value);
  assert.deepEqual(toggleValue(value, "b", 2), ["a"]);
  assert.deepEqual(toggleValue(value, "c"), ["a", "b", "c"]);
});
