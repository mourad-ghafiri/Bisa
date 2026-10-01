/**
 * A code host sign-in as a terminal target: carried on the session, kept
 * across a restart, and never anything but `{kind, host}`. Run with
 * `node --test desktop/src/shell/terminalsModel.login.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { emptyTerminals, loginOf, openTerminal, restoreTerminals, serializeTerminals } from "./terminalsModel.mjs";

test("a sign-in target opens a session that carries its kind and host, and survives a restart", () => {
  const opened = openTerminal(emptyTerminals(), { scope: "machine", id: "home", label: "Sign in to GitHub", login: { kind: "github", host: "github.com" } }, 100);
  const session = opened.sessions[0];
  assert.deepEqual(session.login, { kind: "github", host: "github.com" });
  assert.equal(session.harness, null, "a sign-in is not a harness");
  assert.equal(session.label, "Sign in to GitHub");
  const plain = openTerminal(emptyTerminals(), { scope: "machine", id: "home" }, 100).sessions[0];
  assert.equal(plain.login, null, "a shell carries no sign-in");
  const restored = restoreTerminals(JSON.parse(JSON.stringify(serializeTerminals(opened))));
  assert.deepEqual(restored.sessions[0].login, { kind: "github", host: "github.com" }, "kept across a restart");
});

test("only a {kind, host} pair is a sign-in", () => {
  assert.deepEqual(loginOf({ kind: "gitlab", host: "gitlab.com" }), { kind: "gitlab", host: "gitlab.com" });
  assert.equal(loginOf({ kind: "gitlab" }), null);
  assert.equal(loginOf({ kind: "", host: "x" }), null);
  assert.equal(loginOf("gh auth login"), null, "never a command");
  assert.equal(loginOf(null), null);
  assert.deepEqual(loginOf({ kind: "github", host: "github.com", program: "rm" }), { kind: "github", host: "github.com" }, "anything else on it is dropped");
});
