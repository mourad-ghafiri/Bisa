/**
 * How a Settings panel reads, tested where the rule lives. Run with
 * `node --test desktop/src/views/_settings/loadModel.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";
import { PENDING_ROWS, agoWords, firstFailure, pendingRows, phase, phaseOf, readWords } from "./loadModel.mjs";

test("a read is pending with nothing yet, failed when it refused with nothing to show, and ready once it has an answer — whatever a re-read did", () => {
  assert.equal(phase({ data: null, loading: true, error: null }), "pending");
  assert.equal(phase({ data: null, loading: false, error: "the node answered 500" }), "failed");
  assert.equal(phase({ data: { ok: true }, loading: false, error: null }), "ready");
  assert.equal(phase({ data: [], loading: false, error: null }), "ready", "an empty answer is an answer");
  assert.equal(phase({ data: { ok: true }, loading: false, error: "refused" }), "ready", "a failed re-read keeps the last answer on screen");
  assert.equal(phase({ data: undefined }), "pending", "a shape that never loaded is pending, not failed");
});

test("a place that waits on several reads: failed beats pending beats ready, and the first failure is named", () => {
  const ready = { data: 1 };
  const pending = { data: null, loading: true };
  const failed = { data: null, error: "no" };
  assert.equal(phaseOf([ready, ready]), "ready");
  assert.equal(phaseOf([ready, pending]), "pending");
  assert.equal(phaseOf([pending, failed, ready]), "failed");
  assert.equal(phaseOf([]), "ready", "nothing to wait on");
  assert.equal(firstFailure([ready, { data: null, error: "first" }, { data: null, error: "second" }]), "first");
  assert.equal(firstFailure([ready, pending]), null);
});

test("the status line says what is being read, that it is read again, what failed with the last answer kept, or when it was read", () => {
  const what = "the GitHub CLI";
  assert.deepEqual(readWords({ what, loading: true, data: null }), { text: "reading the GitHub CLI…", failed: false });
  assert.deepEqual(readWords({ what, refreshing: true, data: { ok: true }, at: 100 }, 112), { text: "checking again…", failed: false });
  assert.deepEqual(readWords({ what, error: "gh timed out", data: { ok: true }, at: 100 }, 112), {
    text: "could not read the GitHub CLI: gh timed out — showing the last answer",
    failed: true,
  });
  assert.deepEqual(readWords({ what, error: "gh timed out", data: null }), { text: "could not read the GitHub CLI: gh timed out", failed: true });
  assert.deepEqual(readWords({ what, data: { ok: true }, at: 100 }, 112), { text: "read 12 s ago", failed: false });
  assert.equal(readWords({ what, data: { ok: true }, at: null }), null, "settled with no time to tell says nothing");
  assert.equal(readWords({ what, refreshing: true, data: { ok: true }, error: "old", at: 1 }, 2).text, "checking again…", "a re-read in flight outranks the last failure");
});

test("ago reads in the unit that fits and never goes negative", () => {
  assert.equal(agoWords(12), "12 s ago");
  assert.equal(agoWords(59), "59 s ago");
  assert.equal(agoWords(60), "1 min ago");
  assert.equal(agoWords(3599), "59 min ago");
  assert.equal(agoWords(3600), "1 h ago");
  assert.equal(agoWords(86400 * 2), "2 d ago");
  assert.equal(agoWords(-5), "0 s ago");
});

test("every section the panels wait on has a row count, and an unknown one draws three", () => {
  for (const what of ["the GitHub CLI", "how to sign in", "the settings", "the resolved values", "the keymap", "the caches", "the harnesses", "the governance", "the pets", "the catalog", "the library", "the registry", "the connectors", "your SSH keys", "your git profiles", "your global git config", "the log files", "the workspace", "your rules", "the security status", "the agents", "the grant", "the node", "this Mac's network", "the proxy"]) {
    assert.ok(Number.isInteger(PENDING_ROWS[what]) && PENDING_ROWS[what] >= 1, `${what} has its rows`);
    assert.equal(pendingRows(what), PENDING_ROWS[what]);
  }
  assert.equal(pendingRows("something new"), 3);
  assert.equal(PENDING_ROWS["this Mac's network"], 4, "the Internet card's sentence and its three rows");
  assert.equal(pendingRows("the built-in rules"), 4);
  // A section is named by a message's words: a key spelt here in English would match in English alone.
  const source = readFileSync(new URL("./loadModel.mjs", import.meta.url), "utf8");
  const table = source.slice(source.indexOf("export const PENDING_ROWS"), source.indexOf("});", source.indexOf("export const PENDING_ROWS")));
  const rows = table.split("\n").slice(1).filter((line) => line.trim());
  assert.ok(rows.length > 25);
  for (const row of rows) assert.match(row, /^\s+\[t\("[a-z0-9-]+"\)\]: \d+,$/, `a key is a message: ${row.trim()}`);
});
