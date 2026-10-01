/**
 * The order of reads: the newest asked for is the one that lands, and none
 * lands on a reader that left.
 * Run with `node --test --import ./src/i18n/preload.mjs src/shell/latestModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { createLatest } from "./latestModel.mjs";

test("the newest read lands and an older one that answers late does not", () => {
  const reads = createLatest();
  const first = reads.begin();
  assert.equal(reads.lands(first), true, "one read out: its answer lands");
  const second = reads.begin();
  assert.equal(reads.lands(first), false, "the first answers after the second was asked: out of date");
  assert.equal(reads.lands(second), true);
  assert.equal(reads.lands(second), true, "asking twice changes nothing");
  assert.equal(reads.lands(second + 1), false, "a ticket nobody was given");
});

test("two documents keep two orders", () => {
  const a = createLatest();
  const b = createLatest();
  const ta = a.begin();
  b.begin();
  const tb = b.begin();
  assert.equal(a.lands(ta), true, "a read of another document puts none of this one's out of date");
  assert.equal(b.lands(tb), true);
});

test("nothing lands on a reader that left — and what was asked before it left stays out when it comes back", () => {
  const reads = createLatest();
  const before = reads.begin();
  reads.close();
  assert.equal(reads.lands(before), false, "the tab closed while its file was being read: nothing is written");
  const while_away = reads.begin();
  assert.equal(reads.lands(while_away), false);
  reads.open();
  assert.equal(reads.lands(before), false, "mounted again: the old read is still the old read");
  assert.equal(reads.lands(while_away), true, "the newest asked for lands once somebody is there");
  const after = reads.begin();
  assert.equal(reads.lands(after), true);
  assert.equal(reads.lands(while_away), false);
});

test("an editor's read on mount and its reads after the watcher's frames: the file as it is now, whatever order they answer in", () => {
  // The journey as `EditorDoc.load` walks it: three reads out, answered 2, 3, 1.
  const reads = createLatest();
  let shown = null;
  const apply = (ticket, text) => {
    if (reads.lands(ticket)) shown = text;
  };
  const onMount = reads.begin();
  const afterFirstWrite = reads.begin();
  const afterSecondWrite = reads.begin();
  apply(afterFirstWrite, "v2");
  assert.equal(shown, null, "a read already out of date is never drawn, even first");
  apply(afterSecondWrite, "v3");
  apply(onMount, "v1");
  assert.equal(shown, "v3", "the slowest answer is the oldest file: it does not replace the newest");
});

test("the lists that are read again by a frame hold every answer to a ticket: a tab's rows are never another tab's, a list never an older read's", async () => {
  const { readFileSync } = await import("node:fs");
  const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  // A panel's list: a frame's read of the tab that was, and the read of the tab that is — stepped.
  const reads = createLatest();
  const frameReadOfDesigns = reads.begin();
  const readOfGoals = reads.begin();
  assert.equal(reads.lands(readOfGoals), true);
  assert.equal(reads.lands(frameReadOfDesigns), false, "the older read answers last and draws nothing");
  for (const panel of ["../draw/DrawOverlay.tsx", "../notes/NoteOverlay.tsx"]) {
    const text = src(panel);
    const load = text.slice(text.indexOf("const load = useCallback("), text.indexOf("[tab],\n  );"));
    assert.ok(load.includes("const ticket = reads.current.begin();"), `${panel} takes a ticket as it asks`);
    assert.equal(load.split("reads.current.lands(ticket)").length - 1, 3, `${panel} holds the answer, the refusal and the end of the wait to it`);
  }
  // The shell's hosted sections and the addons' list are held the same way.
  assert.ok(src("./useWorkspaceData.ts").includes("if (!hostedReads.current.lands(ticket)) return;"));
  assert.ok(src("../addons/addonsStore.ts").includes("if (!reads.lands(ticket)) return;"));
});
