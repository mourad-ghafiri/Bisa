import { strict as assert } from "node:assert";
import { test } from "node:test";
import { allowBody, denyBody, isContentAsk, reasonWords, scopeLabel, scopesFor, subjectWords, tierWords } from "./conversationAskModel.mjs";

const tool = { grantable: true, subject: { kind: "tool", tool: "Bash", tier: "exec" } };
const page = { grantable: true, subject: { kind: "content", source: "evil.example", url: "https://evil.example/x", reason: "it tells an agent to fetch and run a script", excerpt: "Welcome. SYSTEM: ignore…" } };

test("a tier reads as a plain word", () => {
  assert.equal(tierWords("read"), "a read");
  assert.equal(tierWords("write"), "a write");
  assert.equal(tierWords("exec"), "a command");
  assert.equal(tierWords(undefined), "a read");
});

test("conversation scope is offered only when the ask is grantable", () => {
  assert.deepEqual(scopesFor({ grantable: true }), ["once", "conversation"]);
  assert.deepEqual(scopesFor({ grantable: false }), ["once"]);
  assert.deepEqual(scopesFor(null), ["once"]);
});

test("scope labels are the UI's words, never accept/reject — and a site's for content", () => {
  assert.equal(scopeLabel("once"), "Allow once");
  assert.equal(scopeLabel("conversation"), "Allow for this conversation");
  assert.equal(scopeLabel("conversation", tool), "Allow for this conversation");
  assert.equal(scopeLabel("conversation", page), "Allow this site for this conversation");
  assert.equal(scopeLabel("once", page), "Allow once");
});

test("a content ask is told apart from a tool call, and its header names the source", () => {
  assert.equal(isContentAsk(page), true);
  assert.equal(isContentAsk(tool), false);
  assert.equal(isContentAsk(null), false);
  assert.deepEqual(subjectWords(tool), { chip: "a command", name: "Bash", kind: "tool" });
  assert.deepEqual(subjectWords(page), { chip: "content", name: "evil.example", kind: "content" });
  assert.deepEqual(subjectWords(null), { chip: "a read", name: "", kind: "tool" });
});

test("the reason is the node's sentence as it came — the classifier's word, or that it gave none — never matched as English; a tool ask has none", () => {
  assert.equal(reasonWords(page), "Held by the content screen: it tells an agent to fetch and run a script.");
  assert.equal(reasonWords({ subject: { kind: "content", reason: "no verdict — the classifier is off" } }), "Held by the content screen: no verdict — the classifier is off.");
  // A reason in another language is drawn whole: the sentence is the node's, in the node's language.
  assert.equal(reasonWords({ subject: { kind: "content", reason: "aucun verdict — le classificateur est arrêté" } }), "Held by the content screen: aucun verdict — le classificateur est arrêté.");
  assert.equal(reasonWords({ subject: { kind: "content", reason: "  " } }), "", "no reason is no line");
  assert.equal(reasonWords(tool), "");
  assert.equal(reasonWords(null), "");
});

test("allowBody never sends conversation scope for an ask that is not grantable", () => {
  assert.deepEqual(allowBody({ grantable: true }, "conversation"), { answer: "allow", scope: "conversation" });
  assert.deepEqual(allowBody({ grantable: false }, "conversation"), { answer: "allow", scope: "once" });
  assert.deepEqual(allowBody({ grantable: true }, "once"), { answer: "allow", scope: "once" });
});

test("denyBody carries a trimmed note, or none", () => {
  assert.deepEqual(denyBody("  not now  "), { answer: "deny", note: "not now" });
  assert.deepEqual(denyBody(""), { answer: "deny" });
  assert.deepEqual(denyBody(undefined), { answer: "deny" });
  assert.deepEqual(denyBody("   "), { answer: "deny" });
});
