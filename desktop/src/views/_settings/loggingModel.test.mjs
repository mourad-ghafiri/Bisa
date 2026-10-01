/**
 * The Logging panel's facts: the process words and the crash kinds are the
 * Rust crate's, a name reads as its period or its moment, sizes read short,
 * the rows come newest first, the newest crash reads as one sentence and
 * the folder's line counts files and reports.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  CRASH_KINDS,
  FAMILIES,
  LOCAL_ONLY,
  bytesWords,
  crashRows,
  familyOf,
  familyRows,
  fileRows,
  latestCrashWords,
  periodOf,
  stampOf,
  totalWords,
} from "./loggingModel.mjs";

test("the families are the crate's process prefixes and the kinds its crash kinds", () => {
  const files = readFileSync(new URL("../../../../crates/bisa-log/src/files.rs", import.meta.url), "utf8");
  const prefixes = [...files.matchAll(/Process::[A-Za-z]+ => "([a-z]+)"/g)].map((m) => m[1]);
  assert.deepEqual(Object.keys(FAMILIES).sort(), [...prefixes].sort());
  const crash = readFileSync(new URL("../../../../crates/bisa-log/src/crash.rs", import.meta.url), "utf8");
  const kinds = [...crash.matchAll(/CrashKind::[A-Za-z]+ => "([a-z_]+)"/g)].map((m) => m[1]);
  assert.deepEqual(Object.keys(CRASH_KINDS).sort(), [...kinds].sort());
  assert.equal(familyOf("node.2026-09-08.jsonl"), "the node");
  assert.equal(familyOf("desktop.2026-09-08-14.jsonl"), "the desktop");
  assert.equal(familyOf("node.20260911T102233Z.12.json"), "the node");
  assert.equal(familyOf("stranger.jsonl"), "unknown");
  assert.match(LOCAL_ONLY, /never sent anywhere/);
  assert.match(LOCAL_ONLY, /crash report/);
});

test("a name reads as its period, daily or hourly, and a report's as its moment", () => {
  assert.equal(periodOf("node.2026-09-08.jsonl"), "2026-09-08");
  assert.equal(periodOf("cli.2026-09-08-14.jsonl"), "2026-09-08 14:00");
  assert.equal(periodOf("odd.txt"), "odd.txt");
  assert.equal(stampOf("node.20260911T102233Z.12.json"), "2026-09-11 10:22:33 UTC");
  assert.equal(stampOf("node.2026-09-08.jsonl"), "node.2026-09-08.jsonl");
});

test("sizes read short", () => {
  assert.equal(bytesWords(0), "0 B");
  assert.equal(bytesWords(-5), "0 B");
  assert.equal(bytesWords(512), "512 B");
  assert.equal(bytesWords(1536), "1.5 KB");
  assert.equal(bytesWords(200 * 1024), "200 KB");
  assert.equal(bytesWords(3.4 * 1024 * 1024), "3.4 MB");
  assert.equal(bytesWords(2 * 1024 * 1024 * 1024), "2.0 GB");
});

test("the rows come newest first with whose, when and how big, per family and for the reports", () => {
  const now = 1_800_000_000;
  const rows = fileRows(
    [
      { name: "node.2026-09-07.jsonl", bytes: 2048, modified_at: now - 86_400 },
      { name: "node.2026-09-08.jsonl", bytes: 10, modified_at: now - 60 },
    ],
    now,
  );
  assert.deepEqual(
    rows.map((r) => r.name),
    ["node.2026-09-08.jsonl", "node.2026-09-07.jsonl"],
  );
  assert.equal(rows[0].family, "the node");
  assert.equal(rows[0].period, "2026-09-08");
  assert.equal(rows[0].size, "10 B");
  assert.equal(rows[0].age, "1m");
  assert.equal(rows[1].age, "1d");

  const blocks = familyRows(
    [
      { process: "node", dir: "/x/logs/node", files: [{ name: "node.2026-09-08.jsonl", bytes: 10, modified_at: now - 60 }] },
      { process: "cli", dir: "/x/logs/cli", files: [] },
    ],
    now,
  );
  assert.equal(blocks.length, 2);
  assert.equal(blocks[0].whose, "the node");
  assert.equal(blocks[0].dir, "/x/logs/node");
  assert.equal(blocks[0].files[0].name, "node.2026-09-08.jsonl");
  assert.equal(blocks[1].whose, "a command");
  assert.deepEqual(blocks[1].files, []);

  const crashes = crashRows(
    [
      { name: "node.20260910T060640Z.7.json", bytes: 300, modified_at: now - 7200 },
      { name: "desktop.20260911T102233Z.12.json", bytes: 100, modified_at: now - 60 },
    ],
    now,
  );
  assert.deepEqual(
    crashes.map((r) => r.name),
    ["desktop.20260911T102233Z.12.json", "node.20260910T060640Z.7.json"],
  );
  assert.equal(crashes[0].family, "the desktop");
  assert.equal(crashes[0].at, "2026-09-11 10:22:33 UTC");
  assert.equal(crashes[0].size, "100 B");
  assert.equal(crashes[0].age, "1m");
});

test("the newest crash reads as one sentence, and nothing when there is none", () => {
  const now = Date.parse("2026-09-11T10:25:33Z") / 1000;
  assert.equal(
    latestCrashWords({ process: "node", kind: "panic", at: "2026-09-11T10:22:33Z", message: "one bad row" }, now),
    "The node panicked 3m ago: one bad row",
  );
  assert.equal(
    latestCrashWords({ process: "desktop", kind: "child_exit", at: "2026-09-11T10:22:33Z", message: "the node (pid 4) was ended by signal 11" }, now),
    "The desktop saw its node exit 3m ago: the node (pid 4) was ended by signal 11",
  );
  assert.equal(
    latestCrashWords({ process: "cli", kind: "abrupt_end", at: "not a date", message: "m" }, now),
    "A command ended without a goodbye not a date: m",
  );
  assert.equal(latestCrashWords(null, now), null);
  assert.equal(latestCrashWords(undefined, now), null);
});

test("the folder's line counts files and reports", () => {
  assert.equal(totalWords({ families: [{ files: [] }], crashes: [] }), "No file yet — nothing has gone wrong, or the log is off.");
  assert.equal(totalWords({ families: [{ files: [{ bytes: 1024 }] }], crashes: [] }), "1 file, 1.0 KB.");
  assert.equal(
    totalWords({ families: [{ files: [{ bytes: 1024 }] }, { files: [{ bytes: 1024 }] }], crashes: [{ bytes: 1024 }] }),
    "2 files and 1 crash report, 3.0 KB.",
  );
  assert.equal(totalWords({ families: [], crashes: [{ bytes: 10 }, { bytes: 10 }] }), "2 crash reports, 20 B.");
});
