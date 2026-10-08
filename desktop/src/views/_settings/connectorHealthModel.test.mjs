import { test } from "node:test";
import assert from "node:assert/strict";
import { CHECK_ALL_AT_ONCE, checkTargets, healthDetail, healthTone, healthWords } from "./connectorHealthModel.mjs";

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
