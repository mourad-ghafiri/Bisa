/**
 * What a person reads when an act failed. Run with
 * `node --test --import ./src/i18n/preload.mjs src/ui/failureModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { failureWords, nodeSentence, reasonWords, unexpectedWords } from "./failureModel.mjs";

const WHAT = "The branch could not be created.";

/** An `ApiError` as `api.ts` throws it: the node's sentence as its message. */
function apiError(message) {
  const e = new Error(message);
  e.name = "ApiError";
  return e;
}

test("the node's own refusal follows what failed, and nothing needs logging", () => {
  const out = failureWords(WHAT, apiError("A branch named main already exists."));
  assert.equal(out.text, "The branch could not be created. A branch named main already exists.");
  assert.equal(out.raw, false);
});

test("a raw exception is never shown: the words point to the diagnostic log, and the caller logs it", () => {
  for (const raw of [new TypeError("Cannot read properties of undefined (reading 'x')"), "ipc: channel closed", { code: 5 }, null, undefined]) {
    const out = failureWords(WHAT, raw);
    assert.equal(out.text, "The branch could not be created. The diagnostic log has the detail.");
    assert.equal(out.raw, true);
    assert.ok(!out.text.includes("Cannot read") && !out.text.includes("ipc:"), "no exception text reaches a person");
  }
});

test("an ApiError with no words is no reason: the log is pointed to instead", () => {
  assert.equal(nodeSentence(apiError("   ")), null);
  const out = failureWords(WHAT, apiError("   "));
  assert.equal(out.text, "The branch could not be created. The diagnostic log has the detail.");
  assert.equal(out.raw, true);
});

test("a reason slot gets the node's words, or where the detail went — never an exception's text", () => {
  assert.deepEqual(reasonWords(apiError("a.txt already exists")), { text: "a.txt already exists", raw: false });
  assert.deepEqual(reasonWords(new TypeError("x is undefined")), { text: "see the diagnostic log", raw: true });
  assert.deepEqual(reasonWords("ipc: channel closed"), { text: "see the diagnostic log", raw: true });
});

test("where nothing names what failed, one sentence that stands alone or after a colon", () => {
  assert.deepEqual(unexpectedWords(apiError("The goal is closed.")), { text: "The goal is closed.", raw: false });
  assert.deepEqual(unexpectedWords(new Error("fetch failed")), { text: "Something unexpected stopped it. The diagnostic log has the detail.", raw: true });
});

test("only the client's own error is the node speaking — another Error's message is not", () => {
  assert.equal(nodeSentence(new Error("socket hang up")), null);
  assert.equal(nodeSentence(apiError("The workstream is retired.")), "The workstream is retired.");
});
