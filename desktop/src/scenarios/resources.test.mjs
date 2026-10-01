/**
 * The footer's resources, stepped as the store steps them: the machine's facts
 * land once, a reading lands as the numbers, a reading that fails keeps the
 * last numbers marked stale and is logged once per reason, a reading that
 * lands again clears the mark; the process rows re-render only when a row or
 * the interval moved. Models stepped in sequence, no DOM — as every scenario.
 *
 * Run with `node --test desktop/src/scenarios/resources.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { EMPTY_HOST, EMPTY_PROCESSES, afterFailure, afterReading, failureIsNews, processesAfter, withInfo } from "../shell/statsModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("a person watches the footer through a node hiccup and sees numbers the whole time", () => {
  let host = EMPTY_HOST;
  assert.equal(host.load, null, "before the first read the bar has nothing to draw");

  host = withInfo(host, { cores: 10, memory_bytes: 32e9 });
  host = afterReading(host, { cpu_pct: 8, memory_used_bytes: 12e9 });
  assert.deepEqual(host.load, { cpu_pct: 8, memory_used_bytes: 12e9 });
  assert.equal(host.stale, null);

  // The shell stops answering for three ticks: the numbers stand, marked, and
  // the log hears about it once.
  const said = [];
  for (let tick = 0; tick < 3; tick++) {
    const why = "the shell did not answer";
    if (failureIsNews(host, why)) said.push(why);
    host = afterFailure(host, why);
  }
  assert.deepEqual(host.load, { cpu_pct: 8, memory_used_bytes: 12e9 }, "the last reading stands");
  assert.equal(host.stale, "the shell did not answer");
  assert.deepEqual(said, ["the shell did not answer"], "one line in the log, not three");

  // A different reason is news again; a reading that lands clears everything.
  assert.equal(failureIsNews(host, "refused"), true);
  host = afterReading(host, { cpu_pct: 30, memory_used_bytes: 14e9 });
  assert.equal(host.stale, null);
  assert.equal(host.info.cores, 10, "the machine's facts were read once and kept");
});

test("an open overlay re-renders its rows only when a row or the interval moved", () => {
  let procs = EMPTY_PROCESSES;
  const rows = [
    { pid: 10, name: "bisa", cpu_pct: 1 },
    { pid: 20, name: "claude", cpu_pct: 4 },
  ];
  let after = processesAfter(procs, { processes: rows, intervalSecs: 5, readMs: 40 });
  assert.equal(after.changed, true, "the first rows are news");
  procs = after.procs;

  after = processesAfter(procs, { processes: rows.map((r) => ({ ...r })), intervalSecs: 5, readMs: 55 });
  assert.equal(after.changed, false, "the same rows read again are not");
  assert.equal(after.procs.readMs, 55, "but the footnote's read time moves");
  procs = after.procs;

  after = processesAfter(procs, { processes: [rows[0]], intervalSecs: 5, readMs: 70 });
  assert.equal(after.changed, true, "a session that ended is a change");
});

test("the store reads its rules from the model and keeps only the polling", () => {
  const store = src("../shell/statsStore.ts");
  for (const rule of ["withInfo", "afterReading", "afterFailure", "failureIsNews", "processesAfter", "samePart"]) {
    assert.ok(store.includes(`${rule}(`), `the store calls ${rule}`);
  }
  assert.ok(!store.includes("JSON.stringify(next)"), "no inline comparison remains beside the model's");
});
