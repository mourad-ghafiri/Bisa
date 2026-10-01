/**
 * The connector step's form facts. Run with `node --test desktop/src/views/_workflow/forms/connectorStepModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { DEFAULT_ACCOUNT, accountFromValue, accountValue, fixedValue, inputValue, strayConnector, strayOperation, strayParams, withConnector, withOperation, withParam, writeWords } from "./connectorStepModel.mjs";

test("the account travels as one select value and comes back as the step's shape", () => {
  assert.equal(accountValue(null), DEFAULT_ACCOUNT);
  assert.equal(accountValue(undefined), DEFAULT_ACCOUNT);
  assert.equal(accountValue("01ARZ3NDEKTSV4RRFFQ69G5FAV"), "fixed:01ARZ3NDEKTSV4RRFFQ69G5FAV");
  assert.equal(accountValue({ input: "who" }), "input:who");
  assert.equal(accountFromValue(DEFAULT_ACCOUNT), null);
  assert.equal(accountFromValue("fixed:01ARZ3NDEKTSV4RRFFQ69G5FAV"), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
  assert.deepEqual(accountFromValue("input:who"), { input: "who" });
  for (const account of [null, "01ARZ3NDEKTSV4RRFFQ69G5FAV", { input: "who" }]) {
    assert.deepEqual(accountFromValue(accountValue(account)), account, "round trip");
  }
  assert.equal(fixedValue("a1"), accountValue("a1"));
  assert.equal(inputValue("who"), accountValue({ input: "who" }));
});

test("a parameter the operation no longer declares is a stray, shown to be removed", () => {
  const declared = [{ name: "text" }, { name: "thread" }];
  assert.deepEqual(strayParams({ text: "hi", old: "x" }, declared), ["old"]);
  assert.deepEqual(strayParams({}, declared), []);
  assert.deepEqual(strayParams(null, declared), []);
  assert.deepEqual(strayParams({ a: "1" }, []), ["a"]);
});

test("a write is either gated or owned, and the words say which", () => {
  assert.ok(writeWords(false).includes("approval step before it"));
  assert.ok(writeWords(false).includes("unattended"));
  assert.ok(writeWords(true).includes("your word"));
});

test("the person's word on a write is said of one write: another operation, or another connector, never inherits it", () => {
  const owned = { id: "post", kind: "connector", connector: "slack", operation: "post_message", params: { text: "hi" }, account: "01ACC", unattended: true, retries: 2 };
  const another = withOperation(owned, "create_channel");
  assert.deepEqual(another, { id: "post", kind: "connector", connector: "slack", operation: "create_channel", params: {}, account: "01ACC", retries: 2 }, "the operation changed: its parameters go, the account stays, and the word is gone — absent, as the step was born");
  assert.equal("unattended" in another, false);
  assert.equal("unattended" in withOperation(owned, "list_channels"), false, "meaningless on a read, and not left to ride to the next write");
  assert.equal(withOperation(owned, "").operation, null, "the empty option is null, never an empty string");
  const moved = withConnector(owned, "jira");
  assert.deepEqual(moved, { id: "post", kind: "connector", connector: "jira", operation: null, params: {}, account: null, retries: 2 }, "everything chosen under the old connector was the old connector's");
  assert.equal(withConnector(owned, "").connector, null);
  assert.deepEqual(owned.params, { text: "hi" }, "the step handed in is not changed");
  // A connector start is the same call without the word.
  assert.deepEqual(withConnector({ event: "connector", connector: "jira", operation: "search", params: { jql: "x" }, account: { input: "who" }, every: 300, key: "id" }, "linear"), { event: "connector", connector: "linear", operation: null, params: {}, account: null, every: 300, key: "id" });
});

test("a parameter emptied is absent from the wire, never an empty string", () => {
  assert.deepEqual(withParam({ a: "1" }, "b", "two"), { a: "1", b: "two" });
  assert.deepEqual(withParam({ a: "1", b: "2" }, "b", "  "), { a: "1" });
  assert.deepEqual(withParam(null, "a", "x"), { a: "x" });
  assert.deepEqual(withParam(undefined, "a", ""), {});
});

test("a picker shows what the definition names even when it is not among its options — never *pick one* over a name still written", () => {
  const installed = [{ id: "slack" }, { id: "jira" }];
  assert.equal(strayConnector("slack", installed), null);
  assert.equal(strayConnector(null, installed), null);
  assert.equal(strayConnector("gone", installed), "gone (not installed here)");
  const all = [{ id: "list", writes: false }, { id: "post", writes: true }];
  const reads = all.filter((o) => !o.writes);
  assert.equal(strayOperation("list", reads, all), null);
  assert.equal(strayOperation(null, reads, all), null);
  assert.equal(strayOperation("post", reads, all), "post (writes — a poll only reads)", "a write named by a poll is said as one");
  assert.equal(strayOperation("post", all, all), null, "a step may name it");
  assert.equal(strayOperation("vanished", all, all), "vanished (not one of its operations)");
  assert.equal(strayOperation("vanished", [], null), null, "nothing is said before the definition is read");
});
