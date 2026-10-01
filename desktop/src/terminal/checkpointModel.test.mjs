/**
 * When a failing scrollback checkpoint is said. Run with
 * `node --test desktop/src/terminal/checkpointModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { FAILURES_SAID, failureTally } from "./checkpointModel.mjs";

test("a run of failures is said once, at the third, and a run that goes on is not said again", () => {
  const tally = failureTally();
  const said = Array.from({ length: 8 }, () => tally.failed("t1"));
  assert.deepEqual(said, [false, false, true, false, false, false, false, false]);
  assert.equal(said.indexOf(true) + 1, FAILURES_SAID);
  assert.equal(tally.count("t1"), 8);
});

test("a checkpoint that goes through ends the run, and the next run is said afresh", () => {
  const tally = failureTally();
  tally.failed("t1");
  tally.failed("t1");
  tally.succeeded("t1");
  assert.equal(tally.count("t1"), 0);
  assert.deepEqual([tally.failed("t1"), tally.failed("t1"), tally.failed("t1")], [false, false, true], "two and a success is not three in a row");
});

test("each terminal has its own run: one's good checkpoint never forgives another's bad one", () => {
  const tally = failureTally();
  tally.failed("bad");
  tally.failed("bad");
  tally.succeeded("good");
  tally.failed("good");
  assert.equal(tally.failed("bad"), true, "the third for this terminal, whatever the other did");
  assert.equal(tally.count("good"), 1);
  tally.forget("bad");
  assert.equal(tally.count("bad"), 0, "a closed terminal leaves nothing behind");
  tally.succeeded("never-seen");
  tally.forget("never-seen");
});

test("the terminal keeps no count of its own", () => {
  const view = readFileSync(new URL("./Terminal.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("failureTally()") && !view.includes("let checkpointFailures"));
});
