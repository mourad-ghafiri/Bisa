/**
 * A person's afternoon with the agents in the Project IDE, as the models see
 * it (13 — Conversations): opening a checkout with no conversation, sending —
 * which starts one — watching the turn on the panel and not on the rail,
 * naming it, starting a second, coming back, searching, reading the memory
 * words, archiving, and what a restart brings back — and the Workflow
 * Designer's Agent pane coming back to its conversation. A scenario steps the
 * models the way the components do and asserts the facts a screen would
 * show after each step — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/conversations.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

import { addressee } from "../views/_workbench/agentRailModel.mjs";
import { currentConversation, handOffTarget, handOffWords, parseRemembered, remember, sessionsOf, turnSummary } from "../views/_workbench/conversationPaneModel.mjs";
import { projectConversations, routeOf, sortConversations, titleOf } from "../views/_studio/conversationsModel.mjs";
import { PICK_KEY, listQuery, ownerKey, parsePicks, pickedId, rememberPick, selectedOf, surfaceWords } from "../views/_studio/conversationSurfaceModel.mjs";
import { claimedSessions, isDrawn, workstreamSessionRows } from "../views/_workbench/workstreamSessionsModel.mjs";
import { footerSessions } from "../shell/footerSessionsModel.mjs";
import { rootKey } from "../views/_workbench/workbenchModel.mjs";

const WID = "01JSCENARIOWORKSTREAM000000";
const PID = "01JSCENARIOPROJECT00000000";
const ROOT = rootKey("workstream", WID);
/** The IDE's surface: the project's list, New conversation about the checkout. */
const IDE = { owner: { kind: "workstream", id: WID, project: PID }, project: PID };

const conv = (id, extra = {}) => ({
  id,
  origin: { kind: "workstream", id: WID, project: PID },
  project: PID,
  title: null,
  first_line: null,
  author: "me",
  created_at: 100,
  last_message_at: null,
  message_count: 0,
  archived: false,
  agents: [],
  ...extra,
});
const turn = (id, conversation, state) => ({
  id,
  kind: "conversation",
  state,
  since: 5,
  started: 5,
  harness: "claude-code",
  model: "claude-opus-5",
  agent: "general-agent",
  work_item: null,
  goal: null,
  workstream: WID,
  project: PID,
  conversation,
  cost: { input_tokens: 10, output_tokens: 5, usd_cents: 0 },
  children: [],
  last_activity: 5,
});
const worker = (id) => ({ ...turn(id, null, { state: "running", tool: "Bash", args: "cargo test" }), kind: "worker", agent: null });

test("a checkout opens with no conversation; the first hand-off starts one about the checkout and the panel lands on it", () => {
  // Nothing listed yet: the door says so, and a hand-off starts a conversation rather than naming a session.
  let rows = [];
  let memory = parseRemembered(null);
  assert.equal(currentConversation(rows, WID, PID, memory[ROOT]), null);
  assert.equal(surfaceWords(IDE, { count: rows.length }).hint, "No conversation about this project yet — start one");
  assert.deepEqual(handOffTarget(null, WID, PID), { start: { kind: "workstream", id: WID, project: PID } });
  // The node made one; the store remembers it for this root.
  const first = conv("c1", { first_line: "make the header blue", message_count: 1, last_message_at: 101 });
  rows = [first];
  memory = remember(memory, ROOT, "c1");
  assert.equal(currentConversation(rows, WID, PID, memory[ROOT]).id, "c1");
  assert.equal(titleOf(first), "make the header blue", "its first line names it until a person does");
  assert.equal(handOffWords("General Agent", true), "Sent to General Agent in a new conversation.");
  assert.deepEqual(handOffTarget(first, WID, PID), { conversation: "c1" }, "the next hand-off goes to it");
});

test("the agent's turn shows on the panel — its state line, its Stop — and never as a row of the checkout on the rail or in the footer", () => {
  const roster = [turn("t1", "c1", { state: "running", tool: "Edit", args: "www/index.html" }), worker("w1")];
  const terminals = [{ key: "term-1", scope: "workstream", id: WID, harness: null, running: null, liveness: { status: "live", code: 0 }, openedAt: 1, exitedAt: null }];
  // The panel: the conversation's turn, and what it says.
  const panelRows = sessionsOf(roster, "c1");
  assert.deepEqual(panelRows.map((r) => r.id), ["t1"]);
  const summary = turnSummary(panelRows);
  assert.equal(summary.words, "1 working");
  assert.equal(summary.activity, "general-agent — running Edit · www/index.html");
  assert.equal(summary.stoppable.id, "t1", "Stop is the panel's, on the turn");
  // The rail: harnesses and terminals only.
  const claimed = claimedSessions(roster, terminals);
  assert.equal(isDrawn(roster[0], claimed), false, "a conversation's turn is never a row of the checkout");
  assert.deepEqual(
    workstreamSessionRows(roster, terminals, WID).map((r) => [r.kind, r.id]),
    [
      ["terminal", "term-1"],
      ["agent", "w1"],
    ],
    "the shell and the worker; the turn is reached through the Agent panel",
  );
  // The footer's harness rows follow the same rule.
  const footer = footerSessions(terminals, roster);
  assert.ok(!footer.harnesses.some((h) => h.id === "t1"), "the footer never lists a conversation's turn as a running harness");
  assert.ok(footer.harnesses.some((h) => h.id === "w1"));
});

test("the addressee follows the conversation's origin: a checkout's never reaches the Workflow Agent, a goal's may", () => {
  const agents = [
    { id: "general-agent", name: "General Agent", enabled: true },
    { id: "workflow-agent", name: "Workflow Agent", enabled: true },
    { id: "reviewer", name: "Reviewer", enabled: true },
  ];
  assert.equal(addressee(agents, "workflow-agent", "general-agent", "workstream").id, "general-agent");
  assert.equal(addressee(agents, "reviewer", "general-agent", "workstream").id, "reviewer");
  assert.equal(addressee(agents, "workflow-agent", "general-agent", "goal").id, "workflow-agent");
});

test("naming, a second conversation, coming back, searching, archiving — and what a restart brings back", () => {
  let memory = parseRemembered({ [ROOT]: "c1" });
  const first = conv("c1", { title: "Header colour", first_line: "make the header blue", message_count: 6, last_message_at: 110, agents: ["general-agent"] });
  const second = conv("c2", { first_line: "add a footer", message_count: 2, last_message_at: 200, agents: ["reviewer"] });
  const rows = sortConversations([first, second]);
  assert.deepEqual(rows.map((r) => r.id), ["c2", "c1"], "the list is newest activity first");
  // A person starts the second: it is the pick now — the surface shows the pick's own record, whatever the list does.
  memory = remember(memory, ROOT, "c2");
  assert.equal(pickedId(null, memory[ROOT]), "c2");
  assert.equal(selectedOf(rows, pickedId(null, memory[ROOT])).id, "c2");
  assert.equal(surfaceWords(IDE, { count: rows.length }).door, "Conversations");
  // Coming back to the first is a choice too.
  memory = remember(memory, ROOT, "c1");
  assert.equal(selectedOf(rows, pickedId(null, memory[ROOT])).id, "c1");
  // Searching is the node's: the pane asks `?q=` with the words typed, trimmed — the list narrows, the pick stands.
  assert.equal(listQuery(IDE, { q: " header " }).q, "header");
  assert.equal(listQuery(IDE, { q: "" }).q, undefined, "no words, no search");
  // Archiving the first hides it from the live list; a hand-off from the checkout lands on the other.
  const afterArchive = rows.map((r) => (r.id === "c1" ? { ...r, archived: true } : r));
  assert.equal(currentConversation(afterArchive, WID, PID, memory[ROOT]).id, "c2", "an archived choice is no choice for a hand-off");
  assert.deepEqual(projectConversations(afterArchive, PID, true).map((r) => r.id), ["c1"], "the panel's Archived switch still lists it");
  assert.deepEqual(projectConversations(afterArchive, PID).map((r) => r.id), ["c2"], "and the live list no longer does");
  // A sibling checkout's conversation is the project's: listed here, opened on a click — but a hand-off from this checkout never lands in it.
  const sibling = conv("c4", { origin: { kind: "workstream", id: "01JSCENARIOSIBLING000000000", project: PID }, last_message_at: 300 });
  const withSibling = sortConversations([...afterArchive, sibling]);
  assert.deepEqual(projectConversations(withSibling, PID).map((r) => r.id), ["c4", "c2"], "the project's list has the sibling's, newest first");
  assert.equal(currentConversation(withSibling, WID, PID, null).id, "c2", "the checkout lands on its own, not on the sibling's though it moved last");
  assert.equal(currentConversation(withSibling, WID, PID, "c4").id, "c4", "picked, the sibling's holds");
  assert.deepEqual(handOffTarget(sibling, WID, PID), { start: { kind: "workstream", id: WID, project: PID } }, "a hand-off from here starts one about this checkout instead");
  // A restart reads the memory back from storage, as JSON.
  const restored = parsePicks(JSON.parse(JSON.stringify(memory)));
  assert.equal(restored[ROOT], "c1");
  assert.equal(selectedOf(rows, pickedId(null, restored[ROOT])).id, "c1", "the pick survives a restart");
  // A row about the checkout opens in the IDE on its panel; one about a goal opens on the goal's Conversation tab.
  assert.deepEqual(routeOf(second), { route: { name: "workbench", scope: "workstream", id: WID }, search: { conversation: "c2", panel: "agents" } });
  assert.deepEqual(routeOf(conv("c3", { origin: { kind: "goal", id: "G1" } })), { route: { name: "goal", id: "G1" }, search: { tab: "conversation", conversation: "c3" } });
  // One about a workflow opens the designer with its Agent pane showing, on it.
  assert.deepEqual(routeOf(conv("c4", { origin: { kind: "workflow", id: "W1" } })), { route: { name: "workflow", id: "W1" }, search: { panel: "agent", conversation: "c4" } });
});

test("no screen lists every conversation: each owner lists its own", () => {
  // A conversation is reached where it is about — the sidebar draws no
  // conversations section, there is no `#/conversations` list, and the
  // goal's page, the workflow's designer and the IDE's panel each list theirs.
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  assert.equal(existsSync(new URL("../views/Conversations.tsx", import.meta.url)), false, "the global screen is gone");
  assert.ok(!src("../shell/Sidebar.tsx").includes("useConversations"), "the sidebar lists channels and direct messages, never conversations");
  assert.ok(!src("../routeModel.mjs").includes('"/conversations", "conversations"'), "no index route");
  assert.ok(src("../routeModel.mjs").includes('"/conversations/:id", "conversation"'), "the door a link goes through stays");
  assert.ok(src("../views/_goal/GoalConversationPane.tsx").includes("<ConversationSurface") && src("../views/_goal/GoalConversationPane.tsx").includes("fallback={{"), "a goal lists its own, its thread standing while nothing is picked");
  assert.ok(src("../views/GoalDetail.tsx").includes("<GoalConversationPane"), "the goal's Conversation tab draws the pane");
  assert.ok(src("../views/_workflow/WorkflowAgentPane.tsx").includes("useConversationSurface"), "a workflow lists its own — in the designer's Agent pane");
  assert.ok(src("../views/WorkflowDesigner.tsx").includes("<WorkflowAgentPane"), "the designer's Agent pane is the right panel's");
  assert.ok(src("../views/_workbench/AgentPane.tsx").includes("<ConversationSurface"), "the IDE lists its project's on the same surface");
  assert.ok(src("../views/_workbench/useConversationPane.tsx").includes("project: pid"), "the IDE's one read is by project");
});

test("the IDE's Agent pane has one door and one view at a time, and nothing about compaction, sessions or standing alone", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const pane = src("../views/_workbench/AgentPane.tsx");
  const centre = src("../views/_workbench/AgentModeCenter.tsx");
  const hook = src("../views/_workbench/useConversationPane.tsx");
  for (const [name, text] of [["AgentPane", pane], ["AgentModeCenter", centre], ["useConversationPane", hook]]) {
    assert.ok(!text.includes("AgentSessions") && !text.includes("SessionsDoor"), `${name}: the sessions list is gone — the bar's chip, the composer's Stop and the pet keep the roster's facts`);
    assert.ok(!/\bCompact\b/.test(text) && !text.includes("compactConversation"), `${name}: no Compact verb — the harness compacts its own context`);
    assert.ok(!text.includes("standalone"), `${name}: nothing says standalone`);
  }
  assert.equal(existsSync(new URL("../views/_workbench/AgentSessions.tsx", import.meta.url)), false, "the sessions list's file is gone");
  assert.ok(src("../views/_studio/useConversationSurface.ts").includes("surfaceView("), "the pane shows one view at a time, by the surface's rule");
  const selection = src("../views/_studio/conversationSelection.ts");
  assert.ok(selection.includes('case "root"') && selection.includes("useFold(owner)"), "the list view is remembered per root");
  assert.ok(!src("../api.ts").includes("compactConversation") && src("../api.ts").includes("conversationLive"), "the client reads the live turns and compacts nothing");
  assert.ok(!/\bCompact\b/.test(src("../views/_studio/ConversationThread.tsx")), "the thread's header has no Compact either");
  assert.ok(!src("../views/_settings/settingsLink.mjs").includes('"conversations"'), "Settings has no Conversations tab");
  assert.ok(!src("../views/_workbench/contextChips.mjs").includes("standalone"), "the frame never says standalone");
});

test("a reply streams into the timeline paced and parsed block by block, one row re-rendering, waiting for its message; the thinking is auto, shown or hidden", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const chat = src("../views/_studio/Chat.tsx");
  assert.ok(chat.includes("useLiveTurnAgents(") && chat.includes("useLiveTurn(") && chat.includes("LiveTurnRow"), "the timeline lists the agents; each row reads its own turn");
  assert.ok(!chat.includes("liveLength") && !chat.includes("useLiveTurns("), "no frame re-renders the timeline");
  assert.ok(chat.includes("ResizeObserver") && chat.includes("ref={content}"), "the foot is followed by the content's size");
  assert.ok(chat.includes("useStreamPacer(") && chat.includes("<StreamedMarkdown"), "the words are paced and parsed block by block");
  assert.ok(chat.includes("retireLandedTurns(") && chat.includes("turn.working"), "a landed row waits for its message; a tool running is one line");
  assert.ok(chat.includes("primeLiveTurns"), "a reader that joins mid-turn is primed from the node");
  assert.ok(chat.includes("ThinkingBlock") && chat.includes("message.thinking"), "a landed reply keeps its thinking above its words");
  assert.ok(chat.includes("<ThinkingPicker") && chat.includes("setThinkingMode") && !chat.includes("toggleThinkingMode"), "the footer's control picks auto, shown or hidden");
  assert.ok(src("../views/_studio/ThinkingPicker.tsx").includes("<ChoiceMenu"), "the control is the kit's choice menu");
  const block = src("../ui/ThinkingBlock.tsx");
  assert.ok(block.includes("glimpse(") && block.includes("<StreamedMarkdown") && block.includes("since"), "a folded live thinking shows its tail moving; open, it parses block by block and counts the seconds");
  const streamed = src("../ui/StreamedMarkdown.tsx");
  assert.ok(streamed.includes("settledBlocks(") && streamed.includes("memo("), "a settled block is parsed once");
  const pacer = src("../views/_studio/useStreamPacer.ts");
  assert.ok(pacer.includes("requestAnimationFrame") && pacer.includes("prefersReducedMotion"), "paced by animation frame, still under reduced motion");
  assert.ok(src("../shell/motion.ts").includes("prefers-reduced-motion") && src("../pet/PetSprite.tsx").includes('from "../shell/motion"'), "one home for the reduced-motion question");
  const types = src("../types.hand.ts");
  assert.ok(types.includes('type: "agent_streamed"; scope: string; agent: string; text: string; thinking: string; working: string | null'), "the frame carries the tool line");
  assert.ok(types.includes('type: "agent_replied"; scope: string; agent: string; posted: boolean; message: string | null'), "the reply names its message");
  const store = src("../views/_studio/liveTurnsStore.ts");
  assert.ok(store.includes('"agent_streamed"') && store.includes("settleTurn(") && store.includes("retireLanded("), "frames add up to the turn; the landed reply settles it until its message is drawn");
  assert.ok(src("../views/_studio/thinkingStore.ts").includes('"bisa.chat.thinking"'), "the choice is remembered");
  const hook = src("../views/_studio/useScopeMessages.ts");
  assert.ok(hook.includes("nudgeRead(frame, host != null)") && hook.includes("api.message(id, ctrl.signal)"), "a frame that names its message reads that one message, not the page (`threadCacheModel.nudgeRead`)");
  assert.ok(store.includes("landedOf(p), watching.has(p.scope)"), "the turn waits for the message the frame says landed — none when nothing was posted — and only where a timeline reads");
});

test("the Workflow Designer is one screen with a right panel — Properties and Agent on a rail, no modes anywhere — and no button that asks in a direct message", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  assert.equal(existsSync(new URL("../views/_workflow/AskWorkflowAgent.tsx", import.meta.url)), false, "the Ask button is gone");
  for (const gone of ["../views/_workflow/WorkflowAgentCenter.tsx", "../views/_workflow/designerModeModel.mjs", "../views/_workflow/designerModeStore.ts"]) {
    assert.equal(existsSync(new URL(gone, import.meta.url)), false, `${gone}: the mode vocabulary is gone`);
  }
  const screen = src("../views/WorkflowDesigner.tsx");
  const tab = src("../views/_workflow/GoalWorkflowTab.tsx");
  const card = src("../views/_goal/DesigningCard.tsx");
  for (const [name, text] of [["WorkflowDesigner", screen], ["GoalWorkflowTab", tab], ["DesigningCard", card]]) {
    assert.ok(!text.includes("AskWorkflowAgent") && !text.includes("openDm"), `${name}: nothing asks the agent through a button or a direct message`);
    assert.ok(!text.includes("SegmentedControl") && !text.includes("DesignerMode") && !text.includes('mode: "agent"'), `${name}: no mode switch, no mode`);
  }
  assert.ok(screen.includes("<IconRail") && screen.includes("useDesignerPanel") && screen.includes("data-designer-screen"), "the screen's rail, the panel's store, the keymap scope live");
  assert.ok(screen.includes("<WorkflowAgentPane") && screen.includes("<Inspector") && screen.includes("pressDesignerPane"), "the column shows the Agent pane or the Inspector; a rail press is the store's rule");
  assert.ok(screen.includes("back={{") && !screen.includes("screens-settings-library"), "the header's arrow is the door back to Workflows; the Library button is gone");
  assert.ok(!tab.includes("data-designer-screen") && !tab.includes("GoalConversationPane"), "the goal's tab is the canvas; its thread is the Conversation tab");
  assert.ok(src("../views/GoalDetail.tsx").includes("<GoalConversationPane"), "the goal's Conversation tab draws the pane");
  assert.ok(card.includes('tab: "conversation"'), "the Designing card's door lands on the goal's Conversation tab");
  assert.ok(!screen.includes("AuxPortal") && !screen.includes('aux.open("thread"'), "the Details pane no longer holds the conversations");
  // One hook, one painter, every owner.
  const hook = src("../views/_studio/useConversationSurface.ts");
  assert.ok(hook.includes("useConversations(") && hook.includes("startConversation(source.owner)") && hook.includes("useConversationSelection(selection, ownerKey(source.owner))"), "the hook reads the list, starts one untitled in one click, and follows a selection strategy");
  assert.ok(hook.includes("useConversation(pickId)") && hook.includes("belongsTo(found, source)"), "the pick's record is read by id and held to the owner");
  assert.ok(src("../views/_studio/conversationSelection.ts").includes('useSearchValue("conversation")'), "a screen's selection is the address");
  const painter = src("../views/_studio/ConversationSurface.tsx");
  assert.ok(!painter.includes("PromptDialog") && painter.includes("onNew={c.start}"), "no name is asked before a conversation starts");
  for (const file of ["../views/_workflow/WorkflowAgentPane.tsx", "../views/_studio/ConversationDrawer.tsx", "../views/_goal/GoalConversationPane.tsx"]) {
    assert.ok(src(file).includes("<ConversationSurface") && !src(file).includes("PromptDialog"), `${file}: paints the one surface`);
  }
  assert.ok(src("../views/_workflow/WorkflowAgentPane.tsx").includes('useConversationSurface({ owner: { kind: "workflow", id: workflow.id } }, "remembered")'), "the designer's pane keeps its pick in the address, and remembers it");
  assert.ok(!painter.includes("useConversations("), "the painter paints; the hook reads");
  assert.equal(existsSync(new URL("../views/_studio/OwnedConversations.tsx", import.meta.url)), false, "the goal's own painter is gone: one surface");
});

test("the designer's Agent pane comes back to the conversation left open, whatever door leads back, and forgets it on Back", () => {
  const W1 = { kind: "workflow", id: "01JSCENARIOWORKFLOW0000000" };
  const W2 = { kind: "workflow", id: "01JSCENARIOWORKFLOW0000001" };
  const owner = ownerKey(W1);
  const rows = sortConversations([conv("c1", { origin: W1, project: null, last_message_at: 110 }), conv("c2", { origin: W1, project: null, last_message_at: 120 })]);
  // Nothing picked yet: a bare `#/workflows/<id>` names no conversation — no guess; the list shows.
  let memory = parsePicks(null);
  assert.equal(pickedId(null, memory[owner]), null);
  // The person picks c1: the address names it, and the memory keeps it.
  memory = rememberPick(memory, owner, "c1");
  assert.equal(selectedOf(rows, pickedId("c1", memory[owner])).id, "c1");
  // Off to the Inbox; the designer unmounts. Back by the library's card, the
  // Inbox row or the Pulse — a bare address — after a remount that reads the
  // memory back from storage, as JSON: the same conversation.
  const stored = JSON.stringify(memory);
  const remounted = parsePicks(JSON.parse(stored));
  assert.equal(selectedOf(rows, pickedId(null, remounted[owner])).id, "c1", "the bare address comes back to it");
  // A link that names the other one wins, and is copied into the memory.
  memory = rememberPick(remounted, owner, "c2");
  assert.equal(pickedId("c2", memory[owner]), "c2");
  assert.equal(pickedId(null, memory[owner]), "c2", "the next bare visit lands on the linked one");
  // Another workflow's pane keeps its own.
  assert.equal(pickedId(null, memory[ownerKey(W2)]), null);
  // The conversation no longer listed — searched away, or beyond the page — is still the pick: its record is read by id.
  assert.equal(pickedId(null, memory[owner]), "c2");
  assert.equal(selectedOf(rows.filter((r) => r.id !== "c2"), "c2"), null, "not among the rows: the hook reads the record, and drops the pick only on the node's not-found");
  // The pane's Back forgets the pick: the next visit starts from the list.
  memory = rememberPick(memory, owner, null);
  assert.equal(pickedId(null, memory[owner]), null);
  // Where it lives — one memory for every owner — and that the desktop page says so.
  assert.equal(PICK_KEY, "bisa.conversations.pick");
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  assert.ok(src("../../../docs/architecture/crates/desktop.md").includes("`bisa.conversations.pick`"), "the key is listed");
  const store = src("../views/_studio/conversationPickStore.ts");
  assert.ok(store.includes("PICK_KEY") && store.includes("readPref(") && store.includes("writePref("), "kept through the one storage door, which never throws");
  const selection = src("../views/_studio/conversationSelection.ts");
  assert.ok(selection.includes('"remembered"') && selection.includes("useRememberedPick(owner)"), "the remembered strategy reads the memory");
  const hook = src("../views/_studio/useConversationSurface.ts");
  assert.ok(hook.includes("pickedId(sel.wanted, sel.remembered)") && hook.includes("keep(wanted)"), "the pane shows the pick — the address's or the memory's — and copies a linked pick in");
});

test("the designer comes back on the step it had picked and where its canvas looked, and Back never loops on ?panel=", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const screen = src("../views/WorkflowDesigner.tsx");
  assert.ok(screen.includes("designerMemoryOf(id)") && screen.includes("rememberSelectedStep(id, selected)") && screen.includes("rememberCanvasViewport(id, v)"), "the screen reads its memory once and writes it as the person picks and pans");
  assert.ok(screen.includes("startViewport={memory.viewport}"), "the canvas opens where it was left");
  assert.ok(screen.includes("setSearch({ [PANEL_PARAM]: null }, { replace: true })"), "a link's ?panel= is taken off the address without a history entry");
  const canvas = src("../ui/FlowCanvas.tsx");
  assert.ok(canvas.includes("defaultViewport={opening ?? undefined}") && canvas.includes("fitView={!opening}") && canvas.includes("onMoveEnd="), "the canvas opens on a place or fits, and reports where it settles");
  // The step and the viewport are kept through the view memory, under the workflow's place — so they outlive
  // the window — and quietly: the screen holds what it shows, and nothing on it follows the memory.
  const memory = src("../views/_workflow/designerMemoryStore.ts");
  assert.ok(memory.includes('placeOf({ name: "workflow", id: workflow })') && memory.includes("viewState.read(place, STEP)") && memory.includes("viewState.keepQuietly(designerPlace(workflow), STEP, keptStep(step))") && memory.includes("viewState.keepQuietly(place, VIEWPORT, next)"), "read from the view memory and kept through it, quietly");
  assert.ok(!memory.includes("viewState.keep(") && !memory.includes("localStorage") && !memory.includes("writePref") && !memory.includes("readPref"), "never by a storage call of its own");
});

test("a held page is asked about in the conversation as a content ask: the source, the reason, an excerpt, and Allow this site", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const card = src("../views/_studio/AskCard.tsx");
  assert.ok(card.includes('ask.subject.kind === "content"'), "the card switches on the subject's kind");
  assert.ok(card.includes("reasonWords(") && card.includes("subjectWords(") && card.includes("scopeLabel(scope, ask)"), "the words are the model's");
  assert.ok(card.includes("content.excerpt") && card.includes("<FoldedText"), "the excerpt is shown, folded");
  assert.ok(!card.includes("<a ") && !card.includes("href="), "a URL is text, never navigated");
  assert.ok(src("../types.hand.ts").includes('type: "content_screened"'), "the screen's frame is typed");
  const settings = src("../views/Settings.tsx");
  assert.ok(settings.includes('"security.content.screen"') && settings.includes('"security.content.on_harmful"'), "the two keys are drawn on the Security panel");
  assert.ok(/def!\(\s*"security\.content\.screen",\s*Bool,\s*json!\(true\)/.test(src("../../../crates/bisa-core/src/settings.rs")), "the screen is on by default");
});
