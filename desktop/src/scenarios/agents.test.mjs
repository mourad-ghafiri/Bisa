/**
 * The Agents screen under a quick hand: a second click before the first
 * one's answer is on screen.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { answerPlace, asksDelete, definitionRemoved, rosterNotOffered, stillStopping } from "../views/rosterModel.mjs";
import { heldFor, holding } from "../views/_work/heldAnswerModel.mjs";
import { answerableAddressed } from "../views/_studio/addressModel.mjs";

test("two skills attached quickly: both are on the agent", () => {
  // The roster's copy of the agent, as the window last read it.
  const ids = ["survey"];
  let held = null;
  const shown = () => heldFor(ids, held) ?? ids;

  // Attach one; the node answers the list it now holds, and the roster is being read again.
  const first = [...shown(), "brevity"];
  held = holding(ids, first);
  // Attach another before that read lands: built from the answer.
  const second = [...shown(), "house-style"];
  assert.deepEqual(second, ["survey", "brevity", "house-style"]);
  // The read lands with both; the held answer steps aside.
  const reread = ["survey", "brevity", "house-style"];
  assert.equal(heldFor(reread, holding(ids, second)), null);
});

test("abort pressed twice: asked once, and the button is the person's again when the node refuses or the row is done", () => {
  const live = [{ id: "s1" }, { id: "s2" }];
  let asked = [];
  const press = (id) => {
    if (stillStopping(asked, live).includes(id)) return false;
    asked = [...asked, id];
    return true;
  };
  assert.equal(press("s1"), true, "the first press asks the node");
  assert.equal(press("s1"), false, "the second finds the button latched");
  assert.equal(press("s2"), true, "another session is another button");
  // s1 ends: its row is no longer something to stop, and nothing is latched for it.
  assert.deepEqual(stillStopping(asked, [{ id: "s2" }]), ["s2"]);
  // The node refuses s2: the ask is taken back.
  asked = asked.filter((id) => id !== "s2");
  assert.deepEqual(stillStopping(asked, [{ id: "s2" }]), []);
});

test("Answer on a session that asked with no gate opens where it asked, never an Inbox with nothing picked", () => {
  const door = { name: "conversation", id: "01C" };
  const asking = { state: { state: "waiting", on: { on: "question", text: "which branch?" } }, conversation: "01C" };
  assert.deepEqual(answerPlace(asking, door), { door });
  const gated = { state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "g1" } }, conversation: "01C" };
  assert.deepEqual(answerPlace(gated, door), { inbox: "01C" });
});

test("an agent disabled, then removed, from another window: no picker lies, no dialog reopens, no send is refused for it", () => {
  const agents = [
    { id: "general-agent", origin: "core", pubkey: "pk-g", enabled: true },
    { id: "workflow-agent", origin: "core", pubkey: "pk-w", enabled: true },
    { id: "scout", origin: "local", pubkey: "pk-scout", enabled: false },
  ];
  assert.deepEqual(rosterNotOffered(agents), ["general-agent", "workflow-agent"], "a roster offers the disabled scout, marked, and neither core agent");
  // A conversation remembered the scout as its addressee.
  assert.deepEqual(answerableAddressed(["pk-scout"], agents, true), [], "disabled: let go of, so the next send names nobody and is triaged");
  // Then it is removed while its delete dialog stood open here.
  const frame = { scope: "scout", kind: 33403, snapshot: true, removed: true };
  assert.equal(definitionRemoved(frame), true);
  const after = agents.filter((a) => a.id !== "scout");
  assert.equal(asksDelete("scout", after.find((a) => a.id === "scout") ?? null), false);
  assert.deepEqual(answerableAddressed(["pk-scout"], after, true), []);
});
