/**
 * What a resources reading changes, and what a failed one keeps.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { EMPTY_HOST, EMPTY_PROCESSES, afterFailure, afterReading, failureIsNews, processesAfter, samePart, withInfo } from "./statsModel.mjs";

test("a reading lands as the load and clears the mark; a failure marks and keeps the last reading", () => {
  let host = withInfo(EMPTY_HOST, { cores: 8 });
  host = afterReading(host, { cpu: 12 });
  assert.deepEqual(host, { info: { cores: 8 }, load: { cpu: 12 }, stale: null });
  host = afterFailure(host, "the shell did not answer");
  assert.deepEqual(host, { info: { cores: 8 }, load: { cpu: 12 }, stale: "the shell did not answer" }, "the numbers stand, marked");
  host = afterReading(host, { cpu: 40 });
  assert.equal(host.stale, null, "a reading that lands clears the mark");
});

test("a failure is news once per reason", () => {
  const host = afterFailure(EMPTY_HOST, "timeout");
  assert.equal(failureIsNews(EMPTY_HOST, "timeout"), true);
  assert.equal(failureIsNews(host, "timeout"), false);
  assert.equal(failureIsNews(host, "refused"), true);
});

test("a read that only moves readMs and keeps the rows is not a change; a new row or a new interval is", () => {
  const rows = [{ pid: 1, cpu: 3 }];
  const first = processesAfter(EMPTY_PROCESSES, { processes: rows, intervalSecs: 5, readMs: 100 });
  assert.equal(first.changed, true);
  const same = processesAfter(first.procs, { processes: [{ pid: 1, cpu: 3 }], intervalSecs: 5, readMs: 200 });
  assert.equal(same.changed, false);
  assert.equal(same.procs.readMs, 200, "the footnote's read time still moves");
  assert.equal(same.procs.processes, rows, "the rows keep their reference, so readers re-render nothing");
  assert.equal(processesAfter(same.procs, { processes: [{ pid: 1, cpu: 9 }], intervalSecs: 5, readMs: 300 }).changed, true);
  assert.equal(processesAfter(same.procs, { processes: rows, intervalSecs: 10, readMs: 300 }).changed, true);
});

test("a part compares by what it says, not by reference", () => {
  assert.equal(samePart({ disk: { bytes: 1 }, dataDir: "/w" }, { disk: { bytes: 1 }, dataDir: "/w" }), true);
  assert.equal(samePart({ disk: { bytes: 1 }, dataDir: "/w" }, { disk: { bytes: 2 }, dataDir: "/w" }), false);
});
