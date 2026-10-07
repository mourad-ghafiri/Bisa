/**
 * Why a session exists, in words — tested against the Rust enums the model
 * mirrors and the index the window builds. Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { KINDS, ORIGINS, doorOf, emptyOriginIndex, kindWord, originIndex, originOf, originOfRow, originWords, placeOf, titleOf } from "./sessionOriginModel.mjs";
import { pointLabel } from "../views/_settings/decisionsModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CRATES = join(HERE, "../../../crates");

/** snake_case variants of a Rust enum, read from the source (recipe 7). */
function variantsOf(file, name) {
  const src = readFileSync(join(CRATES, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
}

const WS = {
  projects: [{ project: { id: "p1", name: "Bisa" } }],
  workstreams: [{ project_name: "Bisa", workstream: { id: "w1", project: "p1", name: null, kind: { kind: "worktree", branch: "feat/a", base: "main" } } }],
  goals: [{ id: "g1", label: "Ship the cart" }],
  agents: [{ id: "general-agent", name: "General Agent" }, { id: "workflow-agent", name: "Workflow Agent" }],
  channels: [{ channel: { id: "c1", name: "general" } }],
  dms: [{ channel: { id: "d1" } }],
  nameOf: (p) => (p === "ab" ? "Ada" : p),
};
const index = originIndex(WS, { "claude-code": "Claude Code" });
const worker = (over = {}) => ({ id: "s1", kind: "worker", state: { state: "running" }, harness: "claude-code", agent: "general-agent", goal: "g1", run: "01JRUN0000UVWXYZ", workstream: "w1", cwd: "/ws/w1", origin: { origin: "step", step: "build", name: "Build", resumed: false }, ...over });

test("the kinds and the origins are the Rust enums', in their order, each with a word", () => {
  assert.deepEqual([...KINDS], variantsOf("bisa-store/src/index.rs", "SessionKind"));
  assert.deepEqual([...ORIGINS], variantsOf("bisa-core/src/session.rs", "SessionOrigin"));
  for (const k of KINDS) assert.ok(kindWord(k).length > 0, k);
  assert.equal(kindWord("ask"), "one-shot ask");
  assert.equal(kindWord("something-new"), "something-new", "a kind this build does not know is said by its wire word");
});

test("a worker's row: the agent by name, its step by name on its goal, its checkout, its goal as the door", () => {
  const o = originOf(worker(), index);
  assert.deepEqual(o, { title: "General Agent", origin: "step Build of a run on Ship the cart", place: "Bisa › feat/a", door: { name: "goal", id: "g1" }, kindWord: "worker" });
  assert.equal(originWords(worker({ origin: { origin: "step", step: "build", resumed: true } }), index), "step build, resumed after a restart, of a run on Ship the cart", "by its id when the name is missing; a resume is said");
  assert.equal(originWords(worker({ goal: null, origin: { origin: "step", resumed: false } }), index), "a step of a run on run ·UVWXYZ", "a run of the workspace by its tail");
  assert.deepEqual(doorOf(worker({ goal: null }), index), { name: "run", id: "01JRUN0000UVWXYZ" });
  assert.equal(originWords(worker({ goal: null, run: null, origin: { origin: "step", name: "Build", resumed: false } }), index), "step Build of a run", "nothing to name: the step alone");
  assert.equal(titleOf(worker({ agent: "unknown-agent" }), index), "unknown-agent", "an agent the window does not know, by its id");
  assert.equal(titleOf(worker({ agent: null }), index), "Claude Code", "no agent: the harness by its label");
});

test("a design wake, a turn, a terminal and an ask each say what they are for, and where they open", () => {
  const design = { kind: "guided", agent: "workflow-agent", goal: "g1", cwd: "/data/goals/g1/scratch", origin: { origin: "design", phase: "repair" } };
  assert.equal(originWords(design, index), "the Workflow Agent repairing Ship the cart");
  assert.equal(titleOf(design, index), "Workflow Agent");
  assert.equal(placeOf(design, index), "in g1/scratch", "no checkout: the folder's last two segments");
  assert.deepEqual(doorOf(design, index), { name: "goal", id: "g1" });
  assert.equal(originWords({ kind: "conversation", agent: "general-agent", origin: { origin: "turn", scope: "c1" } }, index), "a turn in #general");
  assert.deepEqual(doorOf({ kind: "conversation", origin: { origin: "turn", scope: "c1" } }, index), { name: "channel", id: "c1" });
  assert.equal(originWords({ kind: "conversation", origin: { origin: "turn", scope: "d1", on_behalf_of: "ab" } }, index), "a turn in a direct message — woken by Ada");
  assert.deepEqual(doorOf({ kind: "conversation", origin: { origin: "turn", scope: "d1" } }, index), { name: "dm", id: "d1" });
  assert.equal(originWords({ kind: "conversation", goal: "g1", origin: { origin: "turn", scope: "g1" } }, index), "a turn in the thread of Ship the cart");
  assert.equal(originWords({ kind: "conversation", workstream: "w1", conversation: "k1", origin: { origin: "turn", scope: "k1" } }, index), "a turn about Bisa › feat/a");
  assert.deepEqual(doorOf({ kind: "conversation", workstream: "w1", conversation: "k1", origin: { origin: "turn", scope: "k1" } }, index), { name: "conversation", id: "k1" });
  assert.equal(originWords({ kind: "terminal", workstream: "w1", origin: { origin: "terminal" } }, index), "a terminal in Bisa › feat/a");
  assert.equal(originWords({ kind: "terminal", origin: { origin: "terminal" } }, index), "a terminal");
  assert.deepEqual(doorOf({ kind: "terminal", workstream: "w1", origin: { origin: "terminal" } }, index), { name: "workbench", scope: "workstream", id: "w1" });
  const ask = (purpose, over = {}) => ({ kind: "ask", harness: "claude-code", agent: "general-agent", goal: "g1", cwd: "/data/agents/general-agent/scratch", origin: { origin: "ask", purpose }, ...over });
  assert.equal(originWords(ask({ kind: "classifier" }), index), "the classifier reading a call for Ship the cart");
  assert.equal(titleOf(ask({ kind: "classifier" }), index), "Classifier", "an ask is named by its purpose, not its reader");
  assert.equal(originWords(ask({ kind: "decision", point: "assign.pick" }), index), `the Decision-Making Agent judging ${pointLabel("assign.pick")} for Ship the cart`);
  assert.equal(originWords(ask({ kind: "decision" }, { goal: null }), index), "the Decision-Making Agent judging a try from Settings");
  assert.equal(originWords(ask({ kind: "commit_message" }, { goal: null, workstream: "w1" }), index), "suggesting a commit message for Bisa › feat/a");
  assert.equal(originWords(ask({ kind: "pull_request_message" }, { goal: null }), index), "suggesting a pull request message");
  assert.equal(titleOf(ask({ kind: "pull_request_message" }), index), "Pull request message");
  assert.equal(kindWord("ask"), originOf(ask({ kind: "classifier" }), index).kindWord);
});

test("a row from an older node — no origin, no cwd — reads by its kind and its ids, and stays a client of it", () => {
  assert.deepEqual(originOfRow({ kind: "worker", goal: "g1" }), { origin: "step", resumed: false });
  assert.deepEqual(originOfRow({ kind: "guided" }), { origin: "design", phase: "design" });
  assert.deepEqual(originOfRow({ kind: "conversation", conversation: "k1" }), { origin: "turn", scope: "k1" });
  assert.deepEqual(originOfRow({ kind: "terminal" }), { origin: "terminal" });
  assert.equal(originWords({ kind: "worker", goal: "g1" }, index), "a step of a run on Ship the cart");
  assert.equal(originWords({ kind: "guided", goal: "g1" }, index), "the Workflow Agent designing Ship the cart");
  assert.equal(placeOf({ kind: "worker" }, index), "not in a checkout");
  assert.equal(originWords({ kind: "worker", origin: { origin: "launch" } }, emptyOriginIndex()), "launch", "an origin this build does not know is said by its wire word");
  assert.equal(doorOf({ kind: "ask" }, emptyOriginIndex()), null, "nowhere to open");
  assert.deepEqual(originOf(null), { title: "a session", origin: "a session", place: "not in a checkout", door: null, kindWord: "" }, "nothing to go on: nothing made up");
});
