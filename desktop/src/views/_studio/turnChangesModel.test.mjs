import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  autoKeptHint,
  fileChipTone,
  fileChipWords,
  fileVerbs,
  pendingPathsOf,
  skippedWords,
  turnCardsByAnchor,
  turnHasPending,
  turnVerbs,
  turnWords,
} from "./turnChangesModel.mjs";

function file(over = {}) {
  return { path: "src/a.ts", kind: "modified", state: "pending", opaque: false, overlapped: false, added: 3, removed: 1, ...over };
}

function turn(over = {}) {
  return { turn: "t1", prompt: "m1", reply: "m2", agent: "reviewer", mode: "manual", files: [file()], ...over };
}

test("turnWords tallies files and lines", () => {
  assert.equal(turnWords([]), "0 files · +0 −0");
  assert.equal(turnWords([file(), file({ path: "b.ts", added: 10, removed: 2 })]), "2 files · +13 −3");
  assert.equal(turnWords([file()]), "1 file · +3 −1");
});

test("a file's chip says its state, or that it overlaps an outside write", () => {
  assert.equal(fileChipWords(file({ state: "pending" })), "to review");
  assert.equal(fileChipWords(file({ state: "kept" })), "kept");
  assert.equal(fileChipWords(file({ state: "undone" })), "undone");
  assert.equal(fileChipWords(file({ state: "gone" })), "gone");
  assert.equal(fileChipWords(file({ state: "pending", overlapped: true })), "also edited by someone else");
  assert.equal(fileChipTone(file({ state: "pending" })), "accent");
  assert.equal(fileChipTone(file({ state: "kept" })), "quiet");
  assert.equal(fileChipTone(file({ overlapped: true })), "warn");
});

test("only a pending file offers verbs, and it offers all three", () => {
  assert.deepEqual(fileVerbs(file({ state: "pending" })), ["keep", "undo", "undo_with_note"]);
  assert.deepEqual(fileVerbs(file({ state: "kept" })), []);
  assert.deepEqual(fileVerbs(file({ state: "undone" })), []);
  assert.deepEqual(fileVerbs(file({ state: "gone" })), []);
  assert.deepEqual(fileVerbs(null), []);
});

test("a turn's bulk verbs mirror whether anything in it is pending", () => {
  assert.equal(turnHasPending(turn()), true);
  assert.deepEqual(turnVerbs(turn()), ["keep", "undo", "undo_with_note"]);
  const settled = turn({ files: [file({ state: "kept" })] });
  assert.equal(turnHasPending(settled), false);
  assert.deepEqual(turnVerbs(settled), []);
});

test("cards key to the reply, fall back to the prompt, then to the end of the timeline", () => {
  const view = {
    turns: [
      turn({ turn: "t1", prompt: "m1", reply: "m2" }),
      turn({ turn: "t2", prompt: "m3", reply: null }),
      turn({ turn: "t3", prompt: null, reply: null }),
    ],
  };
  const { byMessage, atEnd } = turnCardsByAnchor(view);
  assert.deepEqual([...byMessage.keys()].sort(), ["m2", "m3"]);
  assert.equal(byMessage.get("m2")[0].turn, "t1");
  assert.equal(byMessage.get("m3")[0].turn, "t2");
  assert.equal(atEnd.length, 1);
  assert.equal(atEnd[0].turn, "t3");
});

test("a reply not on this timeline falls back to the prompt, then to the end", () => {
  const view = {
    turns: [
      turn({ turn: "t1", prompt: "m1", reply: "gone-from-page" }),
      turn({ turn: "t2", prompt: "also-gone", reply: "also-gone-2" }),
    ],
  };
  const { byMessage, atEnd } = turnCardsByAnchor(view, ["m1"]);
  assert.deepEqual([...byMessage.keys()], ["m1"]);
  assert.equal(atEnd.length, 1);
  assert.equal(atEnd[0].turn, "t2");
});

test("two turns replying to the same message both anchor there, in order", () => {
  const view = { turns: [turn({ turn: "t1", reply: "m2" }), turn({ turn: "t2", reply: "m2" })] };
  const { byMessage } = turnCardsByAnchor(view);
  assert.deepEqual(
    byMessage.get("m2").map((c) => c.turn),
    ["t1", "t2"],
  );
});

test("pendingPathsOf collects every still-pending path across every turn, deduplicated", () => {
  const view = {
    turns: [
      turn({ files: [file({ path: "a.ts", state: "pending" }), file({ path: "b.ts", state: "kept" })] }),
      turn({ files: [file({ path: "a.ts", state: "pending" }), file({ path: "c.ts", state: "pending" })] }),
    ],
  };
  assert.deepEqual([...pendingPathsOf(view)].sort(), ["a.ts", "c.ts"]);
  assert.deepEqual([...pendingPathsOf(null)], []);
});

test("auto's hint says the changes are kept on the next message", () => {
  assert.match(autoKeptHint(), /next message/);
});

test("skippedWords reads a settle's skipped list, or nothing", () => {
  assert.equal(skippedWords([]), null);
  assert.equal(skippedWords(null), null);
  assert.equal(skippedWords([{ path: "a.ts", why: "changed since" }]), "a.ts — changed since");
  assert.equal(
    skippedWords([
      { path: "a.ts", why: "changed since" },
      { path: "b.ts", why: "gone" },
    ]),
    "a.ts — changed since; b.ts — gone",
  );
});
