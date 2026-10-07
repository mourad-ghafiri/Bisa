/**
 * What is gone is forgotten: the paths a fact on the bus takes out of the
 * place memory, and the hosted workspaces a person is no longer in.
 * Run with `node --test desktop/src/shell/gonePlacesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { gonePaths, leftHosts, memberHostKeys, membershipsKnown } from "./gonePlacesModel.mjs";

test("a thing deleted names the paths that stood on it", () => {
  assert.deepEqual(gonePaths({ type: "goal_deleted", goal: "01G" }), ["/goals/01G", "/projects/goal/01G"]);
  assert.deepEqual(gonePaths({ type: "workflow_deleted", workflow: "01W" }), ["/workflows/01W"]);
  assert.deepEqual(gonePaths({ type: "project_deleted", project: "01P" }), ["/projects/workstream/01P"], "a project's own tree is its primary checkout");
  assert.deepEqual(gonePaths({ type: "conversation_changed", id: "01C", change: "deleted" }), ["/conversations/01C"]);
  assert.deepEqual(gonePaths({ type: "workstream_changed", workstream: "01S", state: { state: "closed" } }), ["/projects/workstream/01S"]);
});

test("an archived thing is still there, and so is one that only changed", () => {
  for (const payload of [
    { type: "goal_archived", goal: "01G", archived: true },
    { type: "workflow_archived", workflow: "01W" },
    { type: "project_archived", project: "01P", archived: true },
    { type: "conversation_changed", id: "01C", change: "archived" },
    { type: "conversation_changed", id: "01C", change: "renamed" },
    { type: "workstream_changed", workstream: "01S", state: { state: "open" } },
    { type: "workstream_changed", workstream: "01S" },
    { type: "settings_changed" },
  ]) {
    assert.deepEqual(gonePaths(payload), [], payload.type);
  }
});

test("a fact that names nothing removes nothing", () => {
  for (const payload of [{ type: "goal_deleted" }, { type: "goal_deleted", goal: "" }, { type: "workflow_deleted", workflow: 3 }, {}, null, undefined]) {
    assert.deepEqual(gonePaths(payload), []);
  }
});

test("a hosted workspace a person is no longer in gives up its places, once", () => {
  const remembered = ["/goals/01G", "/hosts/aa/channels/general", "/hosts/aa/messages/01D", "/hosts/bb/channels/general", "/channels/general"];
  assert.deepEqual(leftHosts(remembered, ["bb"]), ["/hosts/aa"]);
  assert.deepEqual(leftHosts(remembered, ["aa", "bb"]), []);
  assert.deepEqual(leftHosts(remembered, []).sort(), ["/hosts/aa", "/hosts/bb"]);
  assert.deepEqual(leftHosts([], []), []);
});

test("a host is read as the router wrote it", () => {
  assert.deepEqual(leftHosts(["/hosts/a%3Ab/channels/general"], ["a:b"]), []);
  assert.deepEqual(leftHosts(["/hosts/a%3Ab/channels/general"], ["other"]), ["/hosts/a%3Ab"]);
  assert.deepEqual(leftHosts(["/hosts/%E0%A4%A/channels/general"], []), ["/hosts/%E0%A4%A"], "a host that is no escape is no host");
});

test("a host's places are given up only on memberships the node answered: a read that failed, or none yet, says nothing of who left", async () => {
  assert.equal(membershipsKnown({ ready: true, offline: null, hostsRead: true }), true);
  assert.equal(membershipsKnown({ ready: true, offline: null, hostsRead: false }), false, "the hosts read did not answer: the list is the last one, or none — every hosted place would be forgotten for a 500");
  assert.equal(membershipsKnown({ ready: false, offline: null, hostsRead: false }), false, "nothing read yet");
  assert.equal(membershipsKnown({ ready: true, offline: "waiting for the node…", hostsRead: true }), false, "the node is away");
  // The app asks the model, and matches no line of the chrome's for a word.
  const { readFileSync } = await import("node:fs");
  const app = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
  assert.ok(app.includes("const hostsKnown = membershipsKnown(workspace);"));
  assert.ok(!app.includes('degraded.includes("hosts")'), "a degraded line is `hosts: <reason>` — it was never equal to `hosts`, so the guard could not fail");
  const shell = readFileSync(new URL("./useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(shell.includes("setHostsRead(!!hs);") && shell.includes("setHostsRead(true);") && shell.includes("setHostsRead(false);"), "the load and the re-read both say whether the memberships answered");
});

test("the member hosts' keys are read without a throw: a section missing its host, its key or its state is left out", () => {
  const member = (pubkey) => ({ host: { host: { pubkey }, state: { state: "member" } } });
  assert.deepEqual(memberHostKeys([member("aa"), member("bb")]), ["aa", "bb"]);
  assert.deepEqual(memberHostKeys([member("aa"), { host: { host: { pubkey: "left" }, state: { state: "left" } } }]), ["aa"], "a host the person left is no member");
  assert.deepEqual(memberHostKeys([member("aa"), { host: null }, {}, null, { host: { state: { state: "member" } } }, { host: { host: { pubkey: 7 }, state: { state: "member" } } }]), ["aa"], "a shape the build does not know costs its row, never the window");
  assert.deepEqual(memberHostKeys(null), []);
  assert.deepEqual(memberHostKeys(undefined), []);
});
