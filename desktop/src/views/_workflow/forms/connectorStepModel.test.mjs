/**
 * The connector step's form facts. Run with `node --test desktop/src/views/_workflow/forms/connectorStepModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { DEFAULT_ACCOUNT, UNATTENDED_HINT, UNATTENDED_LABEL, accountFromValue, accountValue, accountsNote, connectorPickerHint, definitionHint, fixedValue, inputValue, strayAccount, strayConnector, strayOperation, strayParams, withConnector, withOperation, withParam, writeWords } from "./connectorStepModel.mjs";

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

test("the unattended switch has a label and a longer hint, in the catalog's words", () => {
  assert.ok(UNATTENDED_LABEL.length > 0 && UNATTENDED_HINT.length > UNATTENDED_LABEL.length);
  assert.notEqual(UNATTENDED_LABEL, UNATTENDED_HINT);
  assert.ok(!/^workflow-/.test(UNATTENDED_LABEL) && !/^workflow-/.test(UNATTENDED_HINT), "said, never a bare message id");
});

// added by the connectors quality programme

test("an account the call names and the picker does not offer is said in the picker — gone from this machine, an input gone, not an account input, or another connector's — never the default over it", () => {
  const inputs = [
    { name: "who", kind: "account", connector: "slack" },
    { name: "other", kind: "account", connector: "jira" },
    { name: "none", kind: "account", connector: null },
    { name: "title", kind: "text" },
  ];
  const here = [{ id: "a1" }, { id: "a2" }];
  assert.equal(strayAccount(null, here, inputs, "slack"), null, "the default");
  assert.equal(strayAccount("a1", here, inputs, "slack"), null, "one of this machine's");
  assert.equal(strayAccount("gone", here, inputs, "slack"), "gone (not an account on this machine)");
  assert.equal(strayAccount("gone", null, inputs, "slack"), null, "nothing said while the accounts are not read");
  assert.equal(strayAccount({ input: "who" }, here, inputs, "slack"), null);
  assert.equal(strayAccount({ input: "who" }, null, inputs, "slack"), null, "an input needs no read");
  assert.equal(strayAccount({ input: "vanished" }, here, inputs, "slack"), "input · vanished (no such input)");
  assert.equal(strayAccount({ input: "title" }, here, inputs, "slack"), "input · title (not an account input)");
  assert.equal(strayAccount({ input: "other" }, here, inputs, "slack"), "input · other (an account of jira, not slack)");
  assert.equal(strayAccount({ input: "none" }, here, inputs, "slack"), "input · none (an account input naming no connector)");
  assert.equal(strayAccount({ input: "none" }, here, inputs, null), null, "a call naming no connector either: the same nothing");
});

test("a read that failed is said as such — never nothing installed, never no account — and the operation's hint is the one place the definition's failure stands", () => {
  assert.equal(connectorPickerHint({ count: 0, error: null }), "Nothing is installed here yet — Settings › Capabilities › Connectors.");
  assert.equal(connectorPickerHint({ count: 2, error: null }), "A connector installed here, by slug.");
  assert.equal(connectorPickerHint({ count: 0, error: "the node is away" }), "The connectors could not be read: the node is away");
  assert.equal(definitionHint(null, "Posts a message."), "Posts a message.");
  assert.equal(definitionHint("refused", "Posts a message."), "The connector's definition could not be read: refused");
  assert.equal(accountsNote({ loading: true, error: null, count: 0, inputs: 0 }), null, "nothing over a read on its way");
  assert.equal(accountsNote({ loading: false, error: "refused", count: 0, inputs: 0 }), null, "the operation's hint says it");
  assert.ok(accountsNote({ loading: false, error: null, count: 0, inputs: 0 }).startsWith("This machine holds no account"));
  assert.ok(accountsNote({ loading: false, error: null, count: 0, inputs: 1 }).startsWith("The connector's default account"), "an input stands in");
  assert.ok(accountsNote({ loading: false, error: null, count: 2, inputs: 0 }).startsWith("The connector's default account"));
});

test("a parameter's default reads as the value the call would send, and a parameter with none says nothing", async () => {
  const { paramHint } = await import("./connectorStepModel.mjs");
  assert.equal(paramHint({ name: "q", default: "latest" }), "latest");
  assert.equal(paramHint({ name: "max", default: 100 }), "100");
  assert.equal(paramHint({ name: "all", default: false }), "false");
  assert.equal(paramHint({ name: "body", default: { a: [1, 2] } }), '{"a":[1,2]}');
  assert.equal(paramHint({ name: "q" }), "");
  assert.equal(paramHint({ name: "q", default: null }), "");
  assert.equal(paramHint(null), "");
});
