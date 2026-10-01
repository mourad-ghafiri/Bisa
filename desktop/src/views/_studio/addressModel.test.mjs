/**
 * Who a conversation offers, and what it remembers you chose.
 *
 * Three of the rules here had no visible symptom when they broke, which is
 * why they are asserted rather than eyeballed: a re-seeded tray looks exactly
 * like a tray you set yourself, and an addressee with no chip looks exactly
 * like no addressee at all — until the message goes to it.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  CHECKOUT_KINDS,
  GENERAL_AGENT_ID,
  WORKFLOW_AGENT_ID,
  addressChips,
  addressable,
  addressableIn,
  canAnswer,
  coreAgents,
  initialAddressed,
  isCore,
  isWorkflow,
  orderByRoster,
  parseStored,
  reachableIn,
  rosterable,
  teamMentionables,
} from "./addressModel.mjs";

const CORE = { id: "general-agent", pubkey: "core-pk", origin: "core", enabled: true };

test("a team is offered under @ as its id, enabled ones only, with its purpose or its size", () => {
  const out = teamMentionables([
    { id: "design", name: "Design", purpose: "the visual side", enabled: true, members: [{}, {}] },
    { id: "off", name: "Off", enabled: false, members: [{}] },
    { id: "solo", name: "Solo", purpose: null, members: [{}] },
  ]);
  assert.deepEqual(
    out.map((t) => [t.id, t.kind, t.suggest]),
    [
      ["design", "team", true],
      ["solo", "team", true],
    ],
  );
  assert.equal(out[0].description, "the visual side");
  assert.match(out[1].description, /^a team of 1 member — /);
  assert.deepEqual(teamMentionables(null), []);
});
const WORKFLOW = { id: "workflow-agent", pubkey: "wf-pk", origin: "core", enabled: true };
const ADA = { id: "ada", pubkey: "ada-pk", origin: "local", enabled: true };
const OFF = { id: "off", pubkey: "off-pk", origin: "local", enabled: false };
const SCRIBE = { id: "scribe", pubkey: "scribe-pk", origin: { catalog: { slug: "s" } } };
const ALL = [CORE, WORKFLOW, ADA, OFF, SCRIBE];

test("the Workflow Agent is a goal's: reachable from a goal, a channel, a DM or a conversation about a goal, a workflow, the workspace or the node — never from a checkout", () => {
  assert.equal(WORKFLOW_AGENT_ID, "workflow-agent");
  assert.ok(isWorkflow(WORKFLOW) && !isWorkflow(CORE) && !isWorkflow(ADA));
  assert.equal(reachableIn(WORKFLOW, "workstream"), false);
  assert.equal(reachableIn(WORKFLOW, "project"), false, "a project's conversation runs in its primary: a checkout");
  assert.equal(reachableIn(WORKFLOW, "drawing"), false, "a picture has no workflow to shape (19)");
  assert.equal(reachableIn(WORKFLOW, "note"), false, "a scratchpad is somebody's own");
  for (const kind of ["goal", "workflow", "workspace", "node", "channel", "dm"]) assert.equal(reachableIn(WORKFLOW, kind), true, kind);
  assert.deepEqual([...CHECKOUT_KINDS], ["workstream", "project"]);
  for (const kind of ["goal", "channel", "dm", undefined]) assert.equal(reachableIn(WORKFLOW, kind), true, `${kind} keeps @Workflow Agent`);
  assert.equal(reachableIn(CORE, "workstream"), true, "the General Agent is every scope's");
  assert.equal(reachableIn(ADA, "workstream"), true);
  assert.deepEqual(addressableIn(ALL, "workstream"), [ADA, SCRIBE], "the picker never offered a core agent; now it can never resolve the Workflow Agent on a checkout either");
  assert.deepEqual(addressableIn(ALL, "goal"), addressable(ALL));
});

test("the Agents page pins both core agents, the General Agent first, and the rest is the roster", () => {
  assert.equal(GENERAL_AGENT_ID, "general-agent");
  assert.deepEqual(coreAgents([SCRIBE, WORKFLOW, ADA, CORE]), [CORE, WORKFLOW], "in the core's order, whatever the node's");
  assert.deepEqual(coreAgents([ADA]), []);
  assert.deepEqual(coreAgents(null), []);
  assert.deepEqual(rosterable([SCRIBE, WORKFLOW, ADA, CORE]), [SCRIBE, ADA], "the same split, the other half");
});

test("an agent with no enabled flag can answer, because absent means yes", () => {
  assert.equal(canAnswer(SCRIBE), true);
  assert.equal(canAnswer(OFF), false);
  assert.equal(isCore(CORE), true);
  assert.equal(isCore(SCRIBE), false);
});

test("an addressing surface offers neither the core agent nor a disabled one", () => {
  // The defect: the tray filtered on neither and the `@`-picker filtered on
  // `enabled`, so the same agent was pickable in one and invisible in the
  // other. Addressing a disabled agent posts a mention nothing will answer.
  assert.deepEqual(addressable(ALL), [ADA, SCRIBE]);
});

test("a roster offers a disabled agent, marked, and still never the core agent", () => {
  // A roster is a directory of who belongs in this room. An agent switched
  // off today has not stopped belonging — but the node stores the core agent
  // zero times, so a tick for it is a control that lies.
  assert.deepEqual(rosterable(ALL), [ADA, OFF, SCRIBE], "neither core agent — the store keeps them zero times");
});

test("rostered agents lead in the roster's order, and the rest follow", () => {
  assert.deepEqual(
    orderByRoster([ADA, OFF, SCRIBE], ["scribe", "off"]).map((a) => a.id),
    ["scribe", "off", "ada"],
  );
});

test("a roster id naming no agent contributes no row rather than a hole", () => {
  assert.deepEqual(
    orderByRoster([ADA], ["ghost", "ada"]).map((a) => a.id),
    ["ada"],
  );
  assert.deepEqual(orderByRoster(undefined, ["ada"]), []);
  assert.deepEqual(orderByRoster([ADA], undefined), [ADA]);
});

test("never chosen and chose nobody are different stored values", () => {
  assert.equal(parseStored(null), null);
  assert.equal(parseStored(undefined), null);
  assert.deepEqual(parseStored("[]"), []);
  assert.deepEqual(parseStored('["ada-pk"]'), ["ada-pk"]);
});

test("an unparseable preference falls back to the seed rather than to silence", () => {
  assert.equal(parseStored("{"), null);
  assert.equal(parseStored('{"not":"an array"}'), null);
  // Junk inside a real array is dropped; the array itself is still a choice.
  assert.deepEqual(parseStored('["ada-pk", 7, null]'), ["ada-pk"]);
});

test("the seed applies once, and never again over a reader who chose nobody", () => {
  const seed = ["ada-pk"];
  assert.deepEqual(initialAddressed(null, seed), ["ada-pk"]);
  // The defect: removing the last chip, navigating away and coming back
  // silently re-addressed the agent, and the next message went to it.
  assert.deepEqual(initialAddressed([], seed), []);
  assert.deepEqual(initialAddressed(["scribe-pk"], seed), ["scribe-pk"]);
});

test("the seed is copied, so the tray cannot mutate the audience it came from", () => {
  const seed = ["ada-pk"];
  const opened = initialAddressed(null, seed);
  opened.push("off-pk");
  assert.deepEqual(seed, ["ada-pk"]);
});

test("every addressed pubkey draws a chip, including one nothing here resolves", () => {
  // The defect: chips were a filter over the candidates, so a pubkey
  // persisted in one scope and read back in another was merged into the
  // outgoing mentions while drawing nothing at all.
  assert.deepEqual(addressChips(["ada-pk", "ghost-pk"], [ADA, SCRIBE]), [
    { pubkey: "ada-pk", agent: ADA },
    { pubkey: "ghost-pk", agent: null },
  ]);
});

test("chips keep the order they were addressed in, and an empty set draws none", () => {
  assert.deepEqual(
    addressChips(["scribe-pk", "ada-pk"], ALL).map((c) => c.pubkey),
    ["scribe-pk", "ada-pk"],
  );
  assert.deepEqual(addressChips([], ALL), []);
  assert.deepEqual(addressChips(undefined, ALL), []);
});

test("a disabled agent that is already addressed still resolves to a chip", () => {
  // It is offered nowhere any more, and it is still on the wire until it is
  // removed — so the chip has to be there, and has to carry the agent.
  assert.deepEqual(addressChips(["off-pk"], ALL), [{ pubkey: "off-pk", agent: OFF }]);
});

test("a reader's addressees are kept per conversation scope", async () => {
  const { storedAddressKey } = await import("./addressModel.mjs");
  assert.equal(storedAddressKey("01C"), "bisa:address:01C");
  assert.notEqual(storedAddressKey("a"), storedAddressKey("b"));
});
