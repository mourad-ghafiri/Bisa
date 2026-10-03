import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CORE_AGENT_IDS, answerOf, attachedTo, detailStacked, isImplicitMember, memberFace, respondsTo, rosterLine, storedMembers, withoutMember, KIND_SKILL, KIND_TEAM, absentRecord, skillMoved, teamMoved, teamOptionLine, teamTakesWork } from "./rosterModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("the core agent ids are the ones types.hand.ts names", () => {
  const hand = readFileSync(join(here, "..", "types.hand.ts"), "utf8");
  for (const id of CORE_AGENT_IDS) assert.ok(hand.includes(`"${id}"`), id);
  assert.equal(CORE_AGENT_IDS.length, 2);
});

test("who an agent answers is one of two sentences", () => {
  assert.match(respondsTo({ respond: "owner_only" }), /Answers you only/);
  assert.equal(respondsTo({ respond: "anyone" }), "Answers any workspace member.");
  assert.equal(respondsTo(null), "Answers any workspace member.");
});

test("a session is attached to its goal first, then its run, then its work item, then its conversation, else its kind's word", () => {
  assert.deepEqual(attachedTo({ goal: "01JGOAL000ABCDEF", work_item: "01JWI", kind: "run" }), {
    label: "goal ABCDEF",
    route: { name: "goal", id: "01JGOAL000ABCDEF" },
  });
  assert.deepEqual(attachedTo({ run: "01JRUN0000UVWXYZ", work_item: "01JWI", kind: "run" }), {
    label: "run UVWXYZ",
    route: { name: "run", id: "01JRUN0000UVWXYZ" },
  });
  assert.deepEqual(attachedTo({ goal: "01JGOAL000ABCDEF", run: "01JRUN0000UVWXYZ", kind: "run" }).route, { name: "goal", id: "01JGOAL000ABCDEF" });
  assert.deepEqual(attachedTo({ work_item: "01JWORKITEM123456" }), { label: "work item 123456", route: null });
  assert.deepEqual(attachedTo({ conversation: "01JC" }), { label: "a conversation", route: { name: "conversation", id: "01JC" } });
  assert.equal(attachedTo({ kind: "terminal" }).label, "a terminal");
  assert.equal(attachedTo({ kind: "note" }).label, "a note");
  assert.equal(attachedTo({ kind: "conversation" }).label, "a channel or a goal's thread");
  assert.deepEqual(attachedTo({ kind: "run" }), { label: "—", route: null });
  assert.deepEqual(attachedTo(null), { label: "—", route: null });
});

test("a goal reads by its title when the window knows it, and by its id's tail when it does not", () => {
  const titles = new Map([["01JGOAL000ABCDEF", "Ship the login page"], ["01JGOAL000BLANK0", "  "]]);
  const titleOf = (id) => titles.get(id);
  assert.deepEqual(attachedTo({ goal: "01JGOAL000ABCDEF" }, titleOf), { label: "Ship the login page", route: { name: "goal", id: "01JGOAL000ABCDEF" } });
  assert.equal(attachedTo({ goal: "01JGOAL000UNREAD" }, titleOf).label, "goal UNREAD", "a goal the window has not read");
  assert.equal(attachedTo({ goal: "01JGOAL000BLANK0" }, titleOf).label, "goal BLANK0", "a blank title is no title");
  assert.equal(attachedTo({ run: "01JRUN0000UVWXYZ" }, titleOf).label, "run UVWXYZ", "the resolver names goals only");
});

test("a session's ask is answered on the Inbox row it lives under, and only a gate is an ask", () => {
  const waiting = (on) => ({ state: "waiting", on });
  const gate = waiting({ on: "permission", tool: "Bash", gate_id: "g1" });
  assert.deepEqual(answerOf({ state: gate, goal: "01G", conversation: "01C", workstream: "01W" }), { item: "01G" });
  assert.deepEqual(answerOf({ state: gate, conversation: "01C", workstream: "01W" }), { item: "01C" });
  assert.deepEqual(answerOf({ state: gate, workstream: "01W" }), { item: "01W" });
  assert.deepEqual(answerOf({ state: gate }), { item: null }, "the Inbox, no row named");
  assert.equal(answerOf({ state: waiting({ on: "question", text: "which?" }), goal: "01G" }), null, "a question with no gate is answered where it was asked");
  assert.equal(answerOf({ state: waiting({ on: "auth", provider: "github" }), goal: "01G" }), null, "a sign-in is not the Inbox's");
  assert.equal(answerOf({ state: "working", goal: "01G" }), null);
  assert.equal(answerOf(null), null);
});

test("the detail is stacked when it starts left of the roster's right edge", () => {
  assert.equal(detailStacked({ right: 600 }, { left: 0 }), true, "under the roster");
  assert.equal(detailStacked({ right: 600 }, { left: 624 }), false, "beside it, past the gap");
  assert.equal(detailStacked(null, { left: 0 }), false);
  assert.equal(detailStacked({ right: 600 }, undefined), false);
});

test("a roster line counts agents and people with their plurals, and the core agents are never stored", () => {
  const team = { members: [{ agent: "general-agent" }, { agent: "coder" }, { human: "abc" }, { agent: "workflow-agent" }] };
  assert.equal(rosterLine(team.members), "3 agents · 1 person");
  assert.equal(rosterLine([{ agent: "coder" }]), "1 agent · 0 people");
  assert.equal(rosterLine([]), "0 agents · 0 people");
  assert.equal(rosterLine(null), "0 agents · 0 people");
  assert.deepEqual(storedMembers(team), [{ agent: "coder" }, { human: "abc" }]);
  assert.ok(isImplicitMember({ agent: "general-agent" }));
  assert.ok(!isImplicitMember({ human: "general-agent" }), "a person's key that spells the word is not the agent");
  assert.deepEqual(storedMembers(null), []);
});

test("removing a member writes the stored roster whole, without that one and never with a core agent", () => {
  const team = { members: [{ agent: "general-agent" }, { agent: "coder" }, { human: "abc" }, { team: "stray" }] };
  assert.deepEqual(withoutMember(team, { human: "abc" }), { members: [{ agent: "coder" }, { team: "stray" }] });
  assert.deepEqual(withoutMember(team, { agent: "coder" }), { members: [{ human: "abc" }, { team: "stray" }] });
  assert.deepEqual(withoutMember(team, { agent: "nobody" }), { members: [{ agent: "coder" }, { human: "abc" }, { team: "stray" }] }, "a member not there removes nothing");
  assert.deepEqual(withoutMember(team, { agent: "general-agent" }), { members: [{ agent: "coder" }, { human: "abc" }, { team: "stray" }] }, "a core agent was never stored");
});

test("a member's face: an agent by its definition, a person by the directory, a stray team by its id", () => {
  const agents = [{ id: "coder", name: "Coder", pubkey: "pk-coder", photo: { sha256: "a" } }];
  const people = { nameOf: (k) => (k === "abc" ? "Ada" : "abc…"), photoOf: (k) => (k === "abc" ? { sha256: "p" } : null) };
  assert.deepEqual(memberFace({ agent: "coder" }, agents, people), { label: "Coder", avatarId: "pk-coder", photo: { sha256: "a" } });
  assert.deepEqual(memberFace({ agent: "gone" }, agents, people), { label: "gone", avatarId: "gone", photo: null }, "a definition that left is named by its id");
  assert.deepEqual(memberFace({ human: "abc" }, agents, people), { label: "Ada", avatarId: "abc", photo: { sha256: "p" } });
  assert.deepEqual(memberFace({ team: "ops" }, agents, people), { label: "team ops", avatarId: "ops", photo: null });
});

test("an id the roster does not hold: the node is asked, and its sentence is what is said — a record that cannot be read is never *not found*", async () => {
  assert.deepEqual(absentRecord({ data: null, error: null, missing: false, loading: true }), { state: "reading" }, "nothing is claimed before the node answers");
  assert.deepEqual(absentRecord({ data: { team: { id: "t1" } }, error: null, missing: false }), { state: "here" }, "the list was a moment behind: it is read again");
  assert.deepEqual(absentRecord({ data: null, error: "unknown team 01ARZ", missing: true }), { state: "gone" }, "the node's 404: the screen's own note");
  const cut = "teams/01ARZ.json could not be read: EOF while parsing a value at line 3";
  assert.deepEqual(absentRecord({ data: null, error: cut, missing: false }), { state: "unreadable", sentence: cut }, "the store's own sentence, naming the file");
  assert.deepEqual(absentRecord({ data: null, error: "node unreachable", missing: false }), { state: "unreadable", sentence: "node unreachable" }, "a node that is away is not a record that is gone");
  // Both roster screens ask through the one component, and neither decides *not found* by itself.
  const { readFileSync } = await import("node:fs");
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const part = read("./_work/AbsentRecord.tsx");
  assert.ok(part.includes("const said = absentRecord(record);") && part.includes('if (said.state === "unreadable") return <ErrorNote error={said.sentence} retry={record.reload} />;'));
  assert.ok(read("./Teams.tsx").includes("read={(tid, s) => teamApi.detail(tid, s)}") && read("./Agents.tsx").includes("read={(aid, s) => api.agent(aid, s)}"));
  assert.ok(read("./Agents.tsx").includes(") : id && ready ? (") && read("./Teams.tsx").includes(") : selectedId && data !== null ? ("), "asked once the list is read — an id the list simply has not brought yet is no question for the node — and never drawn as *pick an agent*");
});

test("a team's or a skill's record moving — here, at the prompt, on another node — is a snapshot frame of its kind, and the list reads again", async () => {
  const { readFileSync } = await import("node:fs");
  const kinds = readFileSync(new URL("../../../crates/bisa-core/src/kind.rs", import.meta.url), "utf8");
  assert.ok(kinds.includes(`pub const KIND_TEAM: u16 = ${KIND_TEAM};`) && kinds.includes(`pub const KIND_SKILL: u16 = ${KIND_SKILL};`), "the kinds are the core's");
  assert.equal(teamMoved({ scope: "01TEAM", kind: KIND_TEAM, snapshot: true }), true);
  assert.equal(teamMoved({ scope: "01SKILL", kind: KIND_SKILL, snapshot: true }), false);
  assert.equal(teamMoved({ scope: "c1", kind: 3407, author: "a", snippet: "hi" }), false, "a message is no record");
  assert.equal(teamMoved({ scope: "01TEAM", kind: KIND_TEAM }), false);
  assert.equal(teamMoved(null), false);
  assert.equal(skillMoved({ scope: "01SKILL", kind: KIND_SKILL, snapshot: true }), true);
  assert.equal(skillMoved({ scope: "01TEAM", kind: KIND_TEAM, snapshot: true }), false);
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  assert.ok(read("./Teams.tsx").includes("if (teamMoved(f)) reload();"));
  assert.ok(read("./_work/LibraryRefs.tsx").includes("if (skillMoved(f)) reload();"));
});

test("a team stood down is addressed by nothing: no picker offers it, and an assignment made before says why", async () => {
  assert.equal(teamTakesWork({ enabled: true }), true);
  assert.equal(teamTakesWork({}), true, "a team that does not say is taking work");
  assert.equal(teamTakesWork({ enabled: false }), false);
  assert.equal(teamTakesWork(null), false);
  const members = [{ agent: "reviewer" }, { agent: "scout" }, { human: "ab".repeat(32) }];
  assert.equal(teamOptionLine({ members, enabled: true }), "2 agents · 1 person");
  assert.equal(teamOptionLine({ members: [{ agent: "reviewer" }], enabled: true }), "1 agent · 0 people");
  assert.equal(teamOptionLine({ members, enabled: false }), "stood down — takes no work");
  // The picker: known, so an old assignment keeps its name; not offered, so no new one is made. The mention list leaves it out the same way.
  const { readFileSync } = await import("node:fs");
  const picker = readFileSync(new URL("./_work/AssigneePicker.tsx", import.meta.url), "utf8");
  assert.ok(picker.includes("offered: teamTakesWork(t),") && picker.includes("sub: teamOptionLine(t),"));
  assert.ok(picker.includes("options.filter((o) => o.offered && !chosen.has(o.key) && match(o))"), "what is offered is what may be addressed");
  assert.ok(readFileSync(new URL("./_studio/addressModel.mjs", import.meta.url), "utf8").includes(".filter((t) => t.enabled !== false)"), "and @-mentions never suggest it");
});

