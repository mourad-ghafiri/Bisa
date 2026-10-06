/**
 * One conversation surface, as a person meets it (13 — Conversations): the
 * drawer beside a note, the IDE's Agent pane and Agent mode, the designer's
 * Agent pane and the goal's tab all list, search, pick and **start** a
 * conversation the same way — the same bar, the same list, the same empty
 * state with one button, the same act: one click, untitled, the thread at
 * once. Stepped through the models the way the painter does, then the
 * wiring read from the sources; no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/conversationSurface.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

import { PICK_KEY, belongsTo, listQuery, ownerKey, parsePicks, pickedId, rememberPick, selectedOf, surfaceView, surfaceWords } from "../views/_studio/conversationSurfaceModel.mjs";
import { currentConversation, handOffTarget } from "../views/_workbench/conversationPaneModel.mjs";
import { titleOf } from "../views/_studio/conversationsModel.mjs";
import { rootKey } from "../views/_workbench/workbenchModel.mjs";

const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");

const NOTE = { owner: { kind: "note", id: "01JNOTE000000000000000000" } };
const WID = "01JSURFACEWORKSTREAM000000";
const PID = "01JSURFACEPROJECT00000000";
const IDE = { owner: { kind: "workstream", id: WID, project: PID }, project: PID };

const conv = (id, origin, extra = {}) => ({ id, origin, project: origin.project ?? null, title: null, first_line: null, author: "me", created_at: 100, last_message_at: null, message_count: 0, archived: false, agents: [], ...extra });

/** The painter's step: what the surface shows, from the facts the hook holds. */
const shows = ({ rows = [], pickId = null, record = null, listOpen = false, loading = false, recordLoading = false, error = null, source, hasFallback = false }) => {
  const found = selectedOf(rows, pickId) ?? record;
  const selected = found && belongsTo(found, source) ? found : null;
  const settling = pickId !== null && selected === null && recordLoading;
  return { selected, view: surfaceView({ selected, settling, error, listOpen, loading, count: rows.length, hasFallback }) };
};

test("a note's drawer opened fresh over three conversations shows them — never *none yet* — and picking one shows its thread", () => {
  const rows = [conv("c1", NOTE.owner, { first_line: "shorten the intro" }), conv("c2", NOTE.owner, { title: "Sources" }), conv("c3", NOTE.owner)];
  // Opened: the first page is on its way, then the rows.
  assert.equal(shows({ rows: [], loading: true, source: NOTE }).view, "reading");
  assert.equal(shows({ rows, source: NOTE }).view, "list", "conversations exist and nothing is picked: the list, not the empty state");
  assert.equal(surfaceWords(NOTE, { count: 3, open: true }).door, "Conversations");
  // Picked from the list: the thread, named by its title or its first line.
  let memory = parsePicks(null);
  memory = rememberPick(memory, ownerKey(NOTE.owner), "c1");
  const pickId = pickedId(null, memory[ownerKey(NOTE.owner)]);
  const on = shows({ rows, pickId, source: NOTE });
  assert.equal(on.view, "thread");
  assert.equal(titleOf(on.selected), "shorten the intro");
  // The drawer closed and opened again: the pick is kept for the note, the same thread.
  const reopened = parsePicks(JSON.parse(JSON.stringify(memory)));
  assert.equal(shows({ rows, pickId: pickedId(null, reopened[ownerKey(NOTE.owner)]), source: NOTE }).view, "thread");
  // Another note's drawer knows nothing of it.
  assert.equal(reopened[ownerKey({ kind: "note", id: "01JOTHER" })], undefined);
});

test("with no conversation at all, the empty state — one sentence, one button that says New conversation — and one click later the thread", () => {
  assert.equal(shows({ rows: [], source: NOTE }).view, "empty");
  assert.equal(surfaceWords(NOTE, { count: 0 }).empty, "No conversation about this note yet.");
  // The node made one: the pick names it before the list has it — the record is read by id, and nothing flashes.
  const made = conv("c9", NOTE.owner);
  assert.equal(shows({ rows: [], pickId: "c9", recordLoading: true, source: NOTE }).view, "reading", "never the list or the empty state on the way to the new thread");
  assert.equal(shows({ rows: [], pickId: "c9", record: made, source: NOTE }).view, "thread", "the record answers before the list reloads: the thread stands");
  assert.equal(shows({ rows: [made], pickId: "c9", source: NOTE }).view, "thread", "and once listed, the same");
  assert.equal(titleOf(made), "New conversation", "untitled until a person names it or says something");
  const empty = src("../views/_studio/NoConversation.tsx");
  assert.ok(empty.includes('t("studio-conversations-bar-new-conversation")') && !empty.includes("studio-no-conversation-start"), "the empty state's button and the list's foot say one word");
  assert.ok(src("../views/_studio/ConversationsBar.tsx").includes('t("studio-conversations-bar-new-conversation")'), "the foot");
});

test("the list is a view: a search and the Archived switch narrow the rows and leave the pick standing", () => {
  const rows = [conv("c1", NOTE.owner, { title: "Header colour" }), conv("c2", NOTE.owner, { title: "Sources" })];
  const pickId = "c1";
  // A search the node answers: the rows are the hits; the pick is not among them and is read by id.
  assert.deepEqual(listQuery(NOTE, { q: " sources " }), { origin: "note", id: NOTE.owner.id, q: "sources", archived: false, limit: 100 });
  const hits = [rows[1]];
  const searched = shows({ rows: hits, pickId, record: rows[0], listOpen: true, source: NOTE });
  assert.equal(searched.view, "list");
  assert.equal(searched.selected.id, "c1", "still on it");
  assert.equal(shows({ rows: hits, pickId, record: rows[0], source: NOTE }).view, "thread", "the door closed: the thread it was on, not the hit");
  // The Archived switch: the archived rows show; the live pick stands.
  const archivedRows = [conv("c3", NOTE.owner, { archived: true })];
  assert.equal(shows({ rows: archivedRows, pickId, record: rows[0], source: NOTE }).view, "thread");
  // An archived one picked from that list is shown as it is — its thread says so.
  assert.equal(shows({ rows: archivedRows, pickId: "c3", source: NOTE }).selected.archived, true);
});

test("a pick that is gone is dropped, another owner's is dropped, a network error drops nothing", () => {
  // The hook's rule, read from its source: a record that is not ours or the node's *not found* → back(); an error → the error view, the pick kept.
  const hook = src("../views/_studio/useConversationSurface.ts");
  assert.ok(hook.includes("if (pickId && ((found && !ours) || record.missing)) back();"), "dropped for a stale link or a deleted conversation");
  assert.ok(hook.includes("const settling = pickId !== null && selected === null && record.loading;"), "waiting is neither");
  assert.ok(hook.includes("record.error && !record.missing ? record.error : null"), "a network error is said, never mistaken for gone");
  // A stale link on the goal's tab names a workflow's conversation: not ours.
  const GOAL = { owner: { kind: "goal", id: "G1" } };
  const stale = conv("c7", { kind: "workflow", id: "F1" });
  assert.equal(shows({ rows: [], pickId: "c7", record: stale, source: GOAL, hasFallback: true }).selected, null);
  assert.equal(belongsTo(stale, GOAL), false);
  // The node down with nothing read yet: the error, never the empty state.
  assert.equal(shows({ rows: [], error: "the node did not answer", source: GOAL }).view, "error");
});

test("the IDE's pane is the same surface: the project's list, the checkout's own pick, and a hand-off that still knows where to land", () => {
  const own = conv("c1", { kind: "workstream", id: WID, project: PID }, { last_message_at: 100 });
  const sibling = conv("c2", { kind: "workstream", id: "01JSIBLING", project: PID }, { last_message_at: 200 });
  const rows = [sibling, own];
  const root = rootKey("workstream", WID);
  assert.equal(root, ownerKey(IDE.owner), "the root's key is the owner's: one pick memory");
  // A fresh checkout: nothing remembered — the list, one click to a conversation; the surface never guesses.
  let memory = parsePicks(null);
  assert.equal(shows({ rows, pickId: pickedId(null, memory[root]), source: IDE }).view, "list");
  // A hand-off from the editor does guess — the newest whose turns run here — and writes the pick, so the pane lands on it.
  const landing = currentConversation(rows, WID, PID, null);
  assert.equal(landing.id, "c1", "never the sibling's, though it moved last");
  assert.deepEqual(handOffTarget(landing, WID, PID), { conversation: "c1" });
  memory = rememberPick(memory, root, landing.id);
  assert.equal(shows({ rows, pickId: pickedId(null, memory[root]), source: IDE }).selected.id, "c1");
  // The sibling's, picked by hand, holds — it stands in the project.
  memory = rememberPick(memory, root, "c2");
  assert.equal(shows({ rows, pickId: pickedId(null, memory[root]), source: IDE }).selected.id, "c2");
  assert.deepEqual(handOffTarget(sibling, WID, PID), { start: { kind: "workstream", id: WID, project: PID } }, "a hand-off from here still starts one about this checkout instead");
  // The IDE's words: the project's list, the checkout's New conversation.
  assert.equal(surfaceWords(IDE, { count: 2 }).hint, "The conversations of this project, every checkout's — open one to continue it, or start another");
  assert.deepEqual(listQuery(IDE), { project: PID, archived: false, limit: 100 });
});

test("the goal's tab: its own thread stands while nothing is picked, and is the list's first row to come back to", () => {
  const GOAL = { owner: { kind: "goal", id: "G1" } };
  const rows = [conv("c1", GOAL.owner)];
  assert.equal(shows({ rows, source: GOAL, hasFallback: true }).view, "fallback", "the goal's thread, though a conversation exists");
  assert.equal(shows({ rows, listOpen: true, source: GOAL, hasFallback: true }).view, "list");
  assert.equal(shows({ rows, pickId: "c1", source: GOAL, hasFallback: true }).view, "thread");
  assert.equal(surfaceWords(GOAL, { count: 1, open: true, hasFallback: true }).hint, "Hide the conversations");
  const pane = src("../views/_goal/GoalConversationPane.tsx");
  assert.ok(pane.includes('useConversationSurface({ owner: { kind: "goal", id: goal.id } }, "route", { fallback: true })'), "the address holds the pick; the goal has a surface of its own");
  assert.ok(pane.includes("fallback={{") && pane.includes('label: t("goal-goal-conversation-pane-own-thread")') && pane.includes('<Conversation scope={goal.id} kind="goal"'), "the goal's thread, and its row's words");
  const painter = src("../views/_studio/ConversationSurface.tsx");
  assert.ok(painter.includes("home={fallback ? { label: fallback.label, icon, current: c.pickId === null, onPick: c.back } : null}"), "the home row exists where a fallback does, and nowhere else");
  assert.ok(!pane.includes("studio-ask-card-back") && !existsSync(new URL("../views/_studio/OwnedConversations.tsx", import.meta.url)), "no Back link, no second painter");
});

test("the wiring: five hosts, one painter, one hook, one label — and nothing asks a name", () => {
  const hosts = ["../views/_workbench/AgentPane.tsx", "../views/_workbench/AgentModeCenter.tsx", "../views/_studio/ConversationDrawer.tsx", "../views/_workflow/WorkflowAgentPane.tsx", "../views/_goal/GoalConversationPane.tsx"];
  for (const host of hosts) {
    const text = src(host);
    assert.ok(text.includes("<ConversationSurface"), `${host} paints the one surface`);
    assert.ok(!text.includes("PromptDialog") && !text.includes("api.createConversation("), `${host} asks no name and calls no node of its own`);
  }
  const painter = src("../views/_studio/ConversationSurface.tsx");
  assert.ok(painter.includes("onNew={c.start}") && painter.includes("<NoConversation title={c.words.empty} hint={hint} onNew={c.start} />"), "the list's foot and the empty state's button are the hook's one act");
  assert.ok(painter.includes("<ConversationsDoor {...c.door} controls={listId} />") && !painter.includes("useConversations("), "the painter paints; the hook reads");
  const hook = src("../views/_studio/useConversationSurface.ts");
  assert.ok(hook.includes("startConversation(source.owner)") && hook.includes("sel.pick(made.id);"), "one click: made, then picked");
  assert.ok(hook.includes("disabled: view === \"list\" && selected === null && !hasFallback"), "the door is held when the list is the only view");
  // The IDE's hook composes the surface with its own source and its root selection, and keeps the thread's wiring.
  const ide = src("../views/_workbench/useConversationPane.tsx");
  assert.ok(ide.includes('useConversationSurface({ owner: { kind: "workstream", id: wid, project: pid }, project: pid, place: idePlace(scopeKey) }, "root")'), "the project's list, the checkout's New conversation, the root's place");
  assert.ok(!ide.includes("filterConversations") && !ide.includes("useProjectConversations") && !ide.includes("startConversation("), "no list, no search and no start of its own");
  assert.ok(ide.includes("composerLeading") && ide.includes("changedFilesProp") && ide.includes("askCards"), "the thread's wiring stays the IDE's");
  const selection = src("../views/_studio/conversationSelection.ts");
  for (const strategy of ['case "route"', 'case "remembered"', 'case "kept"', 'case "root"']) assert.ok(selection.includes(strategy), strategy);
  assert.ok(selection.includes('setSearch({ conversation: null }, { replace: true })'), "a link's pick is taken off the IDE's address with no history entry");
  assert.ok(src("../views/_studio/ConversationDrawer.tsx").includes('useConversationSurface({ owner: origin }, "kept")'), "a drawer's pick is kept per note or drawing, never in the screen's address");
  // Agent mode's header says its count in the catalog's words; the workbench focuses the pick's composer.
  const centre = src("../views/_workbench/AgentModeCenter.tsx");
  assert.ok(centre.includes("countWords(s.rows.length)") && !/"conversations?"/.test(centre), "no bare English count");
  const workbench = src("../views/Workbench.tsx");
  assert.ok(workbench.includes("const [pickedConversation] = useRememberedPick(key);") && workbench.includes("if (pickedConversation) focusComposer(pickedConversation);") && !workbench.includes("focusComposer(id)"), "the Agents chord puts the caret in the picked conversation's composer");
  // One pick memory, forgotten once.
  assert.equal(PICK_KEY, "bisa.conversations.pick");
  const forget = src("../shell/whereIWas.ts");
  assert.equal((forget.match(/forgetPref\(webStorage\(\), PICK_KEY\)/g) ?? []).length, 1);
  assert.ok(!forget.includes("MEMORY_KEY"), "the IDE's own key is gone");
  assert.ok(src("../views/_workbench/EditorDoc.tsx").includes("useRememberedPick(rootKey(scope, id))"), "the editor follows the same pick");
  assert.ok(src("../views/_workbench/conversationsStore.ts").includes("keepPick(root, made.id);"), "a hand-off writes it");
});

test("a thread's sticky day divider sits at the true top of its scrollport and lays two sheets of the page's ground", () => {
  // The band: `bg` over `bg` — one sheet is translucent on a glass family and
  // the words scrolling under it ghosted through the rule (`themes.test.mjs`
  // holds two sheets to the sheet floor); never a pane.
  const divider = src("../ui/DayDivider.tsx");
  assert.match(divider, /className="sticky top-0 z-10 bg-bg">\{row\}/, "the sticky wrapper is a sheet of bg");
  assert.match(divider, /sticky \? "bg-bg" : ""/, "the row inside is a second sheet of bg only when sticky");
  assert.doesNotMatch(divider, /data-pane/, "a divider never frosts");
  // The scrollport: a sticky element is held inside its scrollport's padding
  // and its containing block's, so the thread's head room is a spacer in the
  // flow, never a `pt-*` on the viewport or on the content it scrolls.
  const chat = src("../views/_studio/Chat.tsx");
  const viewport = chat.match(/className="min-h-0 flex-1 overflow-y-auto[^"]*"\n\s*>\n\s*<div ref=\{content\}>\n\s*(<div[^>]*>)/);
  assert.ok(viewport, "the thread viewport wraps its content");
  assert.doesNotMatch(viewport[0].split("\n")[0], /\bpt-\d/, "no head padding on the viewport");
  assert.match(viewport[1], /className="h-2" aria-hidden="true"/, "the head room is a spacer in the flow");
});
