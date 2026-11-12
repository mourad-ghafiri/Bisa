import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CORE_AGENT_IDS, switchOf, busyRows, definitionMoved, definitionRemoved, mcpMoved, registryChanges, agentOptionLine, answerPlace, asksDelete, goneMemberName, rosterNotOffered, shownTeam, stillStopping, answerOf, detailStacked, isImplicitMember, memberFace, respondsTo, rosterLine, storedMembers, withoutMember, KIND_SKILL, KIND_TEAM, absentRecord, skillMoved, teamMoved, teamOptionLine, teamTakesWork } from "./rosterModel.mjs";

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
  assert.deepEqual(memberFace({ agent: "scout-a1b2c3" }, agents, people), { label: "Agent a1b2c3", avatarId: "scout-a1b2c3", photo: null }, "a definition that left is named as every picker names it");
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

test("a roster picker never offers a core agent, and offers a disabled agent marked", () => {
  const agents = [
    { id: "general-agent", origin: "core", enabled: true },
    { id: "workflow-agent", origin: "core", enabled: true },
    { id: "coder", origin: "local", enabled: true },
    { id: "resting", origin: { catalog: { slug: "resting" } }, enabled: false },
  ];
  assert.deepEqual(rosterNotOffered(agents), ["general-agent", "workflow-agent"]);
  assert.deepEqual(rosterNotOffered(null), []);
  assert.equal(agentOptionLine({ enabled: true }, "claude-code · opus"), "claude-code · opus");
  assert.equal(agentOptionLine({ enabled: false }, "claude-code · opus"), "disabled — takes no work · claude-code · opus");
  assert.equal(agentOptionLine(null, "x"), "x");
  // Every picker draws the line; the three roster pickers leave the core agents out.
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  assert.ok(read("./_work/AssigneePicker.tsx").includes("sub: agentOptionLine(a, `${harnessLabels[a.harness] ?? a.harness} · ${planSummary(a.models)}`),"));
  for (const picker of ["./Teams.tsx", "./_work/GoalInspector.tsx", "./_work/ProjectDetail.tsx"]) {
    assert.ok(read(picker).includes("exclude={notOffered}"), `${picker} leaves the core agents out`);
    assert.ok(read(picker).includes("rosterNotOffered("), picker);
  }
});

test("a member that is gone has one name everywhere", () => {
  assert.equal(goneMemberName("agent", "scout-a1b2c3"), "Agent a1b2c3");
  assert.equal(goneMemberName("human", "ab".repeat(32)), "Person ababab");
  assert.equal(goneMemberName("team", "ops"), "Team ops");
  assert.ok(readFileSync(new URL("./_work/AssigneePicker.tsx", import.meta.url), "utf8").includes("goneMemberName(kind, id)"), "the picker's chip names it the same way");
});

test("a delete is asked of one record: gone, it takes the ask with it", () => {
  assert.equal(asksDelete("scout", { id: "scout" }), true);
  assert.equal(asksDelete("scout", { id: "mapper" }), false, "the next record picked opens no delete nobody asked for");
  assert.equal(asksDelete("scout", null), false, "removed elsewhere while its dialog stood open");
  assert.equal(asksDelete(null, { id: "scout" }), false);
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  for (const screen of ["./Agents.tsx", "./Teams.tsx"]) {
    assert.ok(read(screen).includes("open={asksDelete(deleting, "), screen);
    assert.ok(!read(screen).includes("setDeleting(true)"), `${screen} keeps no bare yes`);
  }
});

test("a team's pane draws what the node last answered, and the next change is made from it", () => {
  const listed = { id: "crew", name: "Crew", members: [{ agent: "coder" }, { agent: "mapper" }, { human: "abc" }, { agent: "general-agent" }, { agent: "workflow-agent" }] };
  assert.equal(shownTeam(listed, null), listed, "nothing written here: the list's copy");
  // One member removed: the node answers the stored roster, and the core agents stay drawn.
  const answered = { id: "crew", name: "Crew", members: [{ agent: "mapper" }, { human: "abc" }] };
  const shown = shownTeam(listed, answered);
  assert.deepEqual(shown.members, [{ agent: "mapper" }, { human: "abc" }, { agent: "general-agent" }, { agent: "workflow-agent" }]);
  // A second removal, before the list is read again, is made from the answer: the first member stays gone.
  assert.deepEqual(withoutMember(shown, { human: "abc" }), { members: [{ agent: "mapper" }] });
  // Made from the list's stale copy it would have put the first one back.
  assert.deepEqual(withoutMember(listed, { human: "abc" }), { members: [{ agent: "coder" }, { agent: "mapper" }] });
  // An edit made in the dialog meanwhile is in the list's fresh copy, which the pane draws: a rename is kept.
  const renamed = { ...listed, name: "Deck crew" };
  assert.equal(shownTeam(renamed, null).name, "Deck crew");
  const pane = readFileSync(new URL("./Teams.tsx", import.meta.url), "utf8");
  assert.ok(pane.includes("const current = shownTeam(team, answered);"), "the pane draws the list's fresh copy, or the answer of its own write");
  assert.ok(!pane.includes("data?.team ?? team"), "never a read of its own that an edit left behind");
  assert.ok(pane.includes("api.patchTeam(current.id, withoutMember(current, m))"));
});

test("Answer goes to the Inbox row for a gate and to where it was asked otherwise", () => {
  const waiting = (on) => ({ state: "waiting", on });
  const door = { name: "goal", id: "01G" };
  assert.deepEqual(answerPlace({ state: waiting({ on: "permission", tool: "Bash", gate_id: "g1" }), goal: "01G" }, door), { inbox: "01G" });
  assert.deepEqual(answerPlace({ state: waiting({ on: "permission", tool: "Bash", gate_id: "g1" }) }, door), { inbox: null });
  assert.deepEqual(answerPlace({ state: waiting({ on: "question", text: "which?" }), goal: "01G" }, door), { door }, "a question with no gate is answered where it was asked");
  assert.equal(answerPlace({ state: waiting({ on: "question", text: "which?" }) }, null), null, "nowhere to go: no button");
  assert.equal(answerPlace({ state: waiting({ on: "auth", provider: "github" }), goal: "01G" }, door), null, "a sign-in is not answered here");
  assert.equal(answerPlace({ state: { state: "running" }, goal: "01G" }, door), null);
  assert.equal(answerPlace(null, door), null);
  assert.ok(readFileSync(new URL("./Agents.tsx", import.meta.url), "utf8").includes("const answer = answerPlace(s, words.door);"));
});

test("an abort is asked once: its button stays latched while the session is still there to stop", () => {
  assert.deepEqual(stillStopping(["a", "b"], [{ id: "a" }, { id: "c" }]), ["a"], "b left the roster, or ended");
  assert.deepEqual(stillStopping([], [{ id: "a" }]), []);
  assert.deepEqual(stillStopping(["a"], []), []);
  const screen = readFileSync(new URL("./Agents.tsx", import.meta.url), "utf8");
  assert.ok(screen.includes("disabled={stopping.includes(s.id)}"), "a second press cannot ask twice");
});

test("a record removed — here, at the prompt, on another node — is a snapshot frame that says so, and every list that follows frames reads again", () => {
  const removed = { scope: "scout", kind: 33403, snapshot: true, removed: true };
  assert.equal(definitionRemoved(removed), true);
  assert.equal(definitionMoved(removed), true, "a removal is a move: a reader of snapshot frames needs nothing new to drop the record");
  assert.equal(teamMoved({ scope: "crew", kind: KIND_TEAM, snapshot: true, removed: true }), true);
  assert.equal(skillMoved({ scope: "brevity", kind: KIND_SKILL, snapshot: true, removed: true }), true);
  assert.equal(definitionRemoved({ scope: "scout", kind: 33403, snapshot: true }), false, "a write is no removal");
  assert.equal(definitionMoved({ scope: "c1", kind: 3407, author: "a", snippet: "hi" }), false, "a message is no record");
  assert.equal(definitionRemoved(null), false);
  // The node sends it (`StoreEvent::DefinitionRemoved`), and the window's one store reads every list again on any snapshot frame.
  const node = readFileSync(new URL("../../../crates/bisa-node/src/lib.rs", import.meta.url), "utf8");
  assert.ok(node.includes('"payload": {"scope": d, "kind": kind, "snapshot": true, "removed": true}'));
  const store = readFileSync(new URL("../shell/useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(store.includes("if (f.snapshot) {") && store.includes("schedule();"), "the roster, the channels, the teams: read again");
  assert.ok(readFileSync(new URL("./_settings/CatalogPanel.tsx", import.meta.url), "utf8").includes("if (definitionMoved(f)) reload();"), "and the catalog's *installed*");
});

test("an open MCP registry follows: a probe anywhere, the node back, a change made in this window", () => {
  assert.equal(mcpMoved({ type: "mcp_probed", id: "platform", ok: true }), true);
  assert.equal(mcpMoved({ type: "judged" }), false);
  assert.equal(mcpMoved(null), false);
  const word = registryChanges();
  let heard = 0;
  const stop = word.listen(() => heard++);
  const other = word.listen(() => heard++);
  word.changed();
  assert.equal(heard, 2, "every open registry reads again");
  stop();
  word.changed();
  assert.equal(heard, 3, "one that left hears nothing");
  other();
  word.changed();
  assert.equal(heard, 3);
  const refs = readFileSync(new URL("./_work/LibraryRefs.tsx", import.meta.url), "utf8");
  assert.ok(refs.includes("if (mcpMoved(e.payload)) reload();") && refs.includes("useEffect(() => mcpRegistry.listen(reload), [reload]);"));
  assert.ok(refs.split("export function useMcpRegistry")[1].split("export function")[0].includes("useReloadOnReconnect(reload);"));
  const panel = readFileSync(new URL("./_settings/McpPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("mcpRegistry.changed();"), "the panel says so after every write of its own");
});

test("the catalog marks the rows that are out, one by one, and reads again whatever the node answered", () => {
  let busy = [];
  busy = busyRows(busy, "agent/scout", true);
  busy = busyRows(busy, "team/venture", true);
  assert.deepEqual(busy, ["agent/scout", "team/venture"], "two rows out at once, two marks");
  busy = busyRows(busy, "agent/scout", false);
  assert.deepEqual(busy, ["team/venture"], "the first one back releases its own mark and no other");
  assert.deepEqual(busyRows(busy, "team/venture", true), ["team/venture"], "pressed twice is marked once");
  assert.deepEqual(busyRows(null, "x", false), []);
  const panel = readFileSync(new URL("./_settings/CatalogPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("busy={busy.includes(key)}"));
  assert.ok(panel.includes("if (!installed) {") && panel.includes("if (!outcome) {"), "a refusal reads the rows again too");
});

test("a delete the node refused is said in the dialog, which asks again who holds the record and promises nothing", () => {
  const dialog = readFileSync(new URL("./_work/LibraryRefs.tsx", import.meta.url), "utf8").split("export function DeleteDialog")[1];
  assert.ok(dialog.includes("setRefused(message);") && dialog.includes("usage.reload();"), "the refusal is kept, and the holders are asked for again");
  assert.ok(dialog.includes("{refused !== null && <ErrorNote error={refused} />}"), "said in the dialog, not in a toast that is gone");
  assert.ok(dialog.includes('{refused === null && <p>{tr("work-library-refs-nothing-points-so-will-go-through")}</p>}'), "*this will go through* is not repeated under a refusal");
  assert.ok(dialog.includes("if (open) setRefused(null);"), "and a dialog opened again starts clean");
});

test("nothing of the team relation is kept that nothing calls", () => {
  const relation = readFileSync(new URL("./_work/teamRelation.ts", import.meta.url), "utf8");
  assert.ok(!relation.includes("assign:"), "the goal's own assignee picker writes a goal's assignees");
  assert.ok(relation.includes("detail: (id: string, s?: AbortSignal) => api.team(id, s)"));
});

test("an agent is disabled and enabled, a team stood down and brought back — by `enabled` alone, and never a core agent", () => {
  const scout = { id: "scout", name: "Scout", origin: "local", enabled: true };
  const off = switchOf("agent", scout);
  assert.deepEqual({ on: off.on, label: off.label, doing: off.doing, body: off.body }, { on: true, label: "Disable", doing: "Disabling…", body: { enabled: false } });
  assert.match(off.done, /^Scout is disabled/);
  const on = switchOf("agent", { ...scout, enabled: false });
  assert.deepEqual({ on: on.on, label: on.label, doing: on.doing, body: on.body }, { on: false, label: "Enable", doing: "Enabling…", body: { enabled: true } });
  assert.equal(on.done, "Scout is enabled again.");
  assert.equal(switchOf("agent", { id: "general-agent", origin: "core", enabled: true }), null, "a control that exists only to refuse is no control");
  assert.equal(switchOf("agent", { id: "workflow-agent", enabled: true }), null);
  assert.equal(switchOf("agent", null), null);
  const crew = { id: "crew", name: "Crew", enabled: true };
  assert.deepEqual([switchOf("team", crew).label, switchOf("team", crew).doing], ["Stand down", "Standing down…"]);
  assert.deepEqual(switchOf("team", crew).body, { enabled: false });
  assert.deepEqual([switchOf("team", { ...crew, enabled: false }).label, switchOf("team", { ...crew, enabled: false }).body], ["Bring back", { enabled: true }]);
  assert.match(switchOf("team", crew).done, /^Crew is stood down/);
  assert.equal(switchOf("team", { id: "bare" }).on, true, "a team that does not say is taking work");
  // One field on the wire, a latch a record, and the state said on the card and in the pane.
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const agents = read("./Agents.tsx");
  assert.ok(agents.includes("api.patchAgent(a.id, turn.body)") && agents.includes("if (!turn || switching.includes(a.id)) return;"));
  assert.ok(agents.includes('{!a.enabled && <Chip tone="warn">{t("screens-agents-disabled")}</Chip>}'));
  const teams = read("./Teams.tsx");
  assert.ok(teams.includes("api.patchTeam(team.id, turn.body)") && teams.includes("if (!turn || switching.includes(team.id)) return;"));
  assert.ok(teams.includes('{t.enabled === false && <Chip tone="warn">{tr("screens-roster-stood-down")}</Chip>}'));
});

test("the roster's words are people's: a harness by its name, a role by its word, a per-row button by whom it is for", () => {
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const picker = read("./_work/AssigneePicker.tsx");
  assert.ok(picker.includes("harnessLabels[a.harness] ?? a.harness") && picker.includes("roleLabel(m.role)"));
  const agents = read("./Agents.tsx");
  assert.ok(agents.includes("<Chip>{harnessLabels[a.harness] ?? a.harness}</Chip>"));
  assert.ok(agents.includes('aria-label={t("screens-agents-abort-named", { who: words.title })}'));
  assert.ok(read("./_work/LibraryRefs.tsx").includes('aria-label={tr("screens-agents-detach-named", { name: e.name })}'));
  assert.ok(read("./Teams.tsx").includes('aria-label={tr("screens-teams-remove-named", { name: o.name })}'));
  assert.ok(!read("./_work/Origin.tsx").includes('CORE_AGENT_IDS.join(" and ")'), "the permanent agents are named, never spelled by id");
});
