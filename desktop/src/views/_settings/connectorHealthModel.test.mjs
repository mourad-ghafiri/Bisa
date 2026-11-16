import { test } from "node:test";
import assert from "node:assert/strict";
import { CHECK_ALL_AT_ONCE, checkAllSummary, checkTargets, detailReloads, healthDetail, healthTone, healthWords, listReloads } from "./connectorHealthModel.mjs";

test("a row's health reads as a tone and words, the platform's reason under a failing one", () => {
  assert.equal(healthTone(undefined), "quiet");
  assert.equal(healthTone({ state: "unknown" }), "quiet");
  assert.equal(healthTone({ state: "ok", check: "connected" }), "ok");
  assert.equal(healthTone({ state: "failing", check: "refused" }), "danger", "a refused credential is the person's to fix");
  assert.equal(healthTone({ state: "failing", check: "unreachable" }), "warn", "a platform that did not answer is not");
  assert.equal(healthWords(null), "Not checked yet.");
  assert.equal(healthWords({ state: "unknown" }), "Not checked yet.");
  assert.equal(healthWords({ state: "ok", check: "connected", status: 200 }), "connected (200)");
  assert.equal(healthWords({ state: "failing", check: "refused", status: 401 }), "refused (401)");
  assert.equal(healthWords({ state: "failing", check: "unreachable" }), "unreachable");
  assert.ok(healthWords({ state: "failing", check: "no_check" }, "Slack").includes("Slack"), "the connector is named");
  assert.ok(healthWords({ state: "failing", check: "throttled" }).includes("throttled"), "a word this desktop does not know is said");
  assert.equal(healthDetail({ state: "failing", check: "refused", reason: "invalid_auth" }), "invalid_auth");
  assert.equal(healthDetail({ state: "ok", check: "connected", reason: null }), "");
  assert.equal(healthDetail(undefined), "");
});

test("Check all dials the accounts that can answer, of connectors that name a check, a few at a time", () => {
  const slack = { connector: { id: "slack", auth: { scheme: "bearer" }, check: "auth_test" }, accounts: [{ id: "a", secrets_set: ["token"] }, { id: "b", secrets_set: [] }] };
  assert.deepEqual(checkTargets(slack), [{ connector: "slack", account: "a" }], "an account with no secret would only be refused for want of one");
  const open = { connector: { id: "open", auth: { scheme: "none" }, check: "ping" }, accounts: [{ id: "a", secrets_set: [] }] };
  assert.deepEqual(checkTargets(open), [{ connector: "open", account: "a" }], "a scheme that needs no secret dials every account");
  const mute = { connector: { id: "x", auth: { scheme: "bearer" }, check: null }, accounts: [{ id: "a", secrets_set: ["token"] }] };
  assert.deepEqual(checkTargets(mute), [], "no check operation, nothing to dial");
  assert.deepEqual(checkTargets(undefined), []);
  assert.equal(CHECK_ALL_AT_ONCE, 4);
});

// added by the connectors quality programme

test("a health with no check word reads by its state; Check all says how many connectors it could not read, and nothing when it read them all", () => {
  assert.equal(healthWords({ state: "ok", check: null, status: 200 }), "connected (200)");
  assert.equal(healthWords({ state: "ok", check: null }), "connected");
  assert.equal(healthWords({ state: "failing", check: null }), "failing");
  assert.equal(healthWords({ state: "failing", check: "refused", status: null }), "refused", "no status, no parenthesis");
  assert.equal(checkAllSummary({ queued: 3, unread: 0 }), null);
  assert.equal(checkAllSummary({ queued: 0, unread: 0 }), null);
  assert.equal(checkAllSummary({ queued: 2, unread: 1 }), "1 connector could not be read; its accounts were not checked.");
  assert.equal(checkAllSummary({ queued: 0, unread: 2 }), "2 connectors could not be read; their accounts were not checked.");
});

test("a change moves the list and every detail; a check moves only the detail of the connector it was about, and nothing while none is chosen", () => {
  assert.equal(listReloads({ type: "connectors_changed", what: "accounts" }), true);
  assert.equal(listReloads({ type: "connector_checked", connector: "slack", account: "a", ok: true }), false, "the list's rows carry no health");
  assert.equal(listReloads({ type: "run_finished" }), false);
  assert.equal(listReloads(null), false);
  assert.equal(detailReloads({ type: "connectors_changed", what: "definitions" }, "slack"), true);
  assert.equal(detailReloads({ type: "connector_checked", connector: "slack", account: "a", ok: false }, "slack"), true);
  assert.equal(detailReloads({ type: "connector_checked", connector: "jira", account: "a", ok: true }, "slack"), false, "another connector's check");
  assert.equal(detailReloads({ type: "connectors_changed", what: "accounts" }, null), false, "no detail while none is chosen");
  assert.equal(detailReloads({ type: "run_finished" }, "slack"), false);
});

import { checkAllLabel, checkAllReport } from "./connectorHealthModel.mjs";

test("Check all ends in one sentence: every account that did not answer as connected, named, and what could not be read", () => {
  assert.equal(checkAllReport({ queued: 3, unread: 0, failed: [] }), null, "all fine: the chips have said it");
  assert.equal(checkAllReport({ queued: 0, unread: 0, failed: [] }), null);
  const one = checkAllReport({ queued: 3, unread: 0, failed: [{ connector: "Slack", account: "work", words: "refused (401)" }] });
  assert.equal(one, "1 of 3 accounts did not answer as connected: Slack · work — refused (401).");
  const many = checkAllReport({
    queued: 9,
    unread: 1,
    failed: Array.from({ length: 6 }, (_, i) => ({ connector: "Jira", account: `a${i}`, words: "unreachable" })),
  });
  assert.ok(many.startsWith("6 of 9 accounts did not answer as connected: Jira · a0 — unreachable; "), many);
  assert.ok(many.includes("and 2 more"), "the sentence names four and counts the rest");
  assert.ok(many.endsWith("1 connector could not be read; its accounts were not checked."), many);
  assert.equal(checkAllReport({ queued: 2, unread: 1, failed: [] }), "1 connector could not be read; its accounts were not checked.");
});

test("the Check all button says what it is doing, and a row's own check is not its business", () => {
  assert.equal(checkAllLabel({ phase: "idle", done: 0, total: 0 }), "Check all");
  assert.equal(checkAllLabel({ phase: "reading", done: 0, total: 0 }), "Reading the accounts…");
  assert.equal(checkAllLabel({ phase: "checking", done: 2, total: 7 }), "Checking 2 of 7…");
});
