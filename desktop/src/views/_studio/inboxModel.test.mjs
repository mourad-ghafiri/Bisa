import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { KIND_LABEL, MARKS_AT_ONCE, RELOAD_DEBOUNCE_MS, RELOAD_ON, SOURCES, aboutWords, applyDelta, askHint, askVerbs, browserHomeOf, compareRows, conversationOf, counts, doorLabel, doorOf, emptyWords, filterOf, filterSegments, groupRows, isHandled, isNewestRead, joinOf, joinWords, keyAction, kindLabel, markBatches, matchesFilter, matchesSource, needsOf, needsReload, nextSelection, noticeLine, noticesOf, readOnSelect, rowGlyph, rowState, sourceIdOf, sourceTabs, summarize, unreadKeys, unreadNoticesOf, visibleRows, waitingOf, waitingWords, wantsList, withReadMark, askTitle } from "./inboxModel.mjs";

/** A row with everything the node sends, so a test only states what it means. */
function row(over = {}) {
  return {
    key: "K",
    kind: "goal",
    title: "build the thing",
    latest_at: 100,
    unread_count: 0,
    read: true,
    handled: false,
    mentioned: false,
    needs_action: [],
    notices: [],
    unread_notices: 0,
    ...over,
  };
}

/** A notice as the node sends it: a Pulse row with its kind. */
function notice(over = {}) {
  return {
    notice: "run_failed",
    seq: 7,
    at: 120,
    concept: "goals",
    kind: "run_finished",
    source: { kind: "goal", id: "K" },
    title: "build the thing",
    event: { type: "run_finished", run: "01J0RUN", outcome: "failed" },
    ...over,
  };
}

function gate(over = {}) {
  return {
    gate_kind: "approval",
    question: "Adopt this workflow?",
    expects: { kind: "decision" },
    durable: false,
    ...over,
  };
}

test("a_read_row_is_a_state_not_an_absence", () => {
  assert.equal(rowState(row({ read: true })), "read");
  assert.equal(rowState(row({ read: false, unread_count: 2 })), "unread");
});

test("a_gate_outranks_having_been_read", () => {
  // You can read a gate and still owe it a decision. If read won here, the
  // one row that is actually blocking would sink under the quiet ones.
  const r = row({ read: true, needs_action: [gate()] });
  assert.equal(rowState(r), "waiting");
});

test("a_row_with_no_messages_can_still_be_unread", () => {
  // The whole reason `read` is not `unread_count === 0`: a gate-only row has
  // no messages to count, and is new to you until you open it.
  const r = row({ unread_count: 0, read: false, needs_action: [gate()] });
  assert.equal(r.unread_count, 0);
  assert.notEqual(rowState(r), "read");
});

test("handled_needs_the_decision_to_be_over", () => {
  assert.equal(isHandled(row({ handled: true })), true);
  assert.equal(isHandled(row({ handled: true, needs_action: [gate()] })), false);
  assert.equal(isHandled(row({ handled: false })), false);
});

test("the_unread_filter_keeps_a_row_you_have_not_seen", () => {
  assert.equal(matchesFilter(row({ read: false }), "unread"), true);
  assert.equal(matchesFilter(row({ read: true }), "unread"), false);
  assert.equal(matchesFilter(row({ read: true }), "all"), true);
});

test("needs_you_is_about_what_is_owed_not_what_is_new", () => {
  assert.equal(matchesFilter(row({ read: false }), "needs_you"), false);
  assert.equal(
    matchesFilter(row({ read: true, needs_action: [gate()] }), "needs_you"),
    true,
  );
});

test("the tabs stand in one order — Any · Messages · Projects · Workflows · Goals · People — each with its glyph", () => {
  assert.deepEqual(SOURCES.map((s) => s.id), ["any", "messages", "projects", "workflows", "goals", "people"]);
  assert.deepEqual(SOURCES.map((s) => s.label), ["Any", "Messages", "Projects", "Workflows", "Goals", "People"]);
  assert.deepEqual(sourceTabs({ any: 3, messages: 1, projects: 0, workflows: 0, goals: 2, people: 0 }).map((t) => [t.id, t.count]), [["any", 3], ["messages", 1], ["projects", 0], ["workflows", 0], ["goals", 2], ["people", 0]], "the strip draws them in that order, each with its count");
  assert.ok(SOURCES.every((s) => typeof s.icon === "string" && s.icon.length > 0), "every tab has its glyph, for a window too narrow for the words");
});

test("every row sits under the source the node says — a conversation under what it is about — and the source narrows within the bucket", () => {
  assert.equal(matchesSource(row({ kind: "goal", source: "goals" }), "messages"), false);
  assert.equal(matchesSource(row({ kind: "channel", source: "messages" }), "messages"), true);
  assert.equal(matchesSource(row({ kind: "people", source: "people" }), "any"), true);
  assert.equal(matchesSource(row({ kind: "workflow", source: "workflows" }), "workflows"), true);
  assert.equal(matchesSource(row({ kind: "workflow", source: "workflows" }), "projects"), false, "a workflow is not where it runs");
  // A conversation's source is its origin's — the node's word, never a rule mirrored here: about a goal, under Goals; about the workspace, a message.
  const aboutGoal = row({ kind: "conversation", source: "goals", origin: { kind: "goal", id: "01GOAL" } });
  assert.equal(matchesSource(aboutGoal, "goals"), true);
  assert.equal(matchesSource(aboutGoal, "messages"), false, "a conversation about a goal is no direct message");
  assert.equal(matchesSource(row({ kind: "conversation", source: "messages", origin: { kind: "workspace" } }), "messages"), true);
  assert.equal(matchesSource(row({ kind: "trigger", source: undefined }), "any"), true, "a row with no source this build knows is under Any alone");
  assert.equal(matchesSource(row({ kind: "trigger", source: undefined }), "messages"), false);
  const rows = [
    row({ key: "a", kind: "dm", source: "messages", read: false }),
    row({ key: "b", kind: "goal", source: "goals", read: false }),
    row({ key: "c", kind: "dm", source: "messages", read: true }),
    row({ key: "d", kind: "people", source: "people", read: false }),
    row({ key: "e", kind: "workflow", source: "workflows", read: false }),
  ];
  assert.deepEqual(
    visibleRows(rows, "unread", "messages").map((r) => r.key),
    ["a"],
  );
  assert.deepEqual(
    visibleRows(rows, "unread", "people").map((r) => r.key),
    ["d"],
  );
  assert.deepEqual(
    visibleRows(rows, "unread", "workflows").map((r) => r.key),
    ["e"],
  );
});

test("a conversation about a thing says so and wears that thing's glyph; one about nothing in particular says nothing and wears its own — never a direct message's", () => {
  const names = { goal: (id) => (id === "01GOAL" ? "Dark mode" : null), project: (id) => (id === "01PRJ" ? "web" : null), workstream: (id) => (id === "01WS" ? "cart" : null) };
  assert.equal(aboutWords(row({ kind: "conversation", origin: { kind: "goal", id: "01GOAL" } }), names), "about goal Dark mode");
  assert.equal(aboutWords(row({ kind: "conversation", origin: { kind: "workstream", id: "01WS", project: "01PRJ" } }), names), "about workstream cart of web");
  assert.equal(aboutWords(row({ kind: "conversation", origin: { kind: "goal", id: "01ARZ3NDEKTSV4RRFFQ69G5FAV" } }), names), "about goal 9G5FAV", "a goal the list no longer has: the six last letters of its id, never nothing");
  assert.equal(aboutWords(row({ kind: "conversation", origin: { kind: "workspace" } }), names), null, "its tab already says");
  assert.equal(aboutWords(row({ kind: "conversation", origin: { kind: "node" } }), names), null);
  assert.equal(aboutWords(row({ kind: "conversation", origin: null }), names), null);
  assert.equal(aboutWords(row({ kind: "dm" }), names), null, "no conversation, nothing to say");
  assert.equal(rowGlyph(row({ kind: "conversation", origin: { kind: "goal", id: "01GOAL" } })), "goal");
  assert.equal(rowGlyph(row({ kind: "conversation", origin: { kind: "workstream", id: "01WS", project: "01PRJ" } })), "workstream");
  assert.equal(rowGlyph(row({ kind: "conversation", origin: { kind: "workflow", id: "01W" } })), "workflow");
  assert.equal(rowGlyph(row({ kind: "conversation", origin: { kind: "workspace" } })), "conversation", "a conversation's own glyph");
  assert.notEqual(rowGlyph(row({ kind: "conversation", origin: { kind: "workspace" } })), rowGlyph(row({ kind: "dm" })), "never the direct message's bubble");
  for (const kind of ["goal", "channel", "dm", "workstream", "project", "workflow", "people", "session"]) assert.equal(typeof rowGlyph(row({ kind })), "string", kind);
  assert.equal(rowGlyph(row({ kind: "trigger" })), "inbox", "a kind this build no longer knows wears the inbox's");
  // The screen draws through the model: the glyph and the words, on the row and on the detail's header.
  const screen = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  assert.ok(screen.includes("ICON[rowGlyph(row)]") && screen.includes("ICON[rowGlyph(current)]"), "the glyph is the model's, on the row and the header");
  assert.ok(screen.includes("aboutWords(row, names)") && screen.includes("aboutWords(current, names)"), "and so are the words");
  assert.ok(!screen.includes("INBOX_KIND_ICON[row.kind]") && !screen.includes("INBOX_KIND_ICON[current.kind]"), "no row reads its glyph from its kind alone");
});

test("the strip's counts say what each bucket holds, and each source within the bucket that is on", () => {
  const rows = [
    row({ key: "a", kind: "goal", source: "goals", read: false, needs_action: [gate()] }),
    row({ key: "b", kind: "dm", source: "messages", read: false }),
    row({ key: "c", kind: "workstream", source: "projects", read: true }),
    row({ key: "d", kind: "people", source: "people", read: false }),
    row({ key: "e", kind: "workflow", source: "workflows", read: true }),
    // A conversation about a goal is the goals': the node said so, and the strip counts it there.
    row({ key: "f", kind: "conversation", source: "goals", origin: { kind: "goal", id: "01G" }, read: false }),
  ];
  const c = counts(rows, "unread");
  assert.deepEqual(c.buckets, { needs_you: 1, unread: 4, all: 6 });
  assert.deepEqual(c.sources, { any: 4, messages: 1, projects: 0, workflows: 0, goals: 2, people: 1 }, "within Unread, the read workstream and the read workflow are not counted; the goal's conversation counts under Goals");
  assert.deepEqual(counts(rows, "all").sources, { any: 6, messages: 1, projects: 1, workflows: 1, goals: 2, people: 1 });
  assert.equal(counts([row({ key: "t", kind: "trigger", source: undefined, read: false })], "all").sources.any, 1, "a row with no source this build knows is counted under Any alone, never a source of its own");
  assert.ok(!("triggers" in c.sources), "no Triggers source");
  assert.deepEqual(Object.keys(c.sources), SOURCES.map((s) => s.id), "the counts stand in the tabs' order");
});

test("the list is drawn in three groups, empty ones left out, each in the list's order", () => {
  const owed = row({ key: "owed", latest_at: 1, needs_action: [gate()] });
  const fresh = row({ key: "fresh", latest_at: 5, read: false });
  const kept = row({ key: "kept", latest_at: 50, read: true });
  const groups = groupRows(visibleRows([kept, fresh, owed], "all", "any"));
  assert.deepEqual(
    groups.map((g) => [g.id, g.title, g.rows.map((r) => r.key)]),
    [
      ["waiting", "Waiting on you", ["owed"]],
      ["new", "New", ["fresh"]],
      ["kept", "Kept", ["kept"]],
    ],
  );
  assert.deepEqual(groupRows([kept]).map((g) => g.id), ["kept"]);
  assert.deepEqual(groupRows([]), []);
});

test("rows_order_by_what_is_owed_then_what_is_new_then_newest", () => {
  const owed = row({ key: "owed", latest_at: 1, needs_action: [gate()] });
  const fresh = row({ key: "fresh", latest_at: 5, read: false });
  const older = row({ key: "older", latest_at: 50, read: true });
  const newer = row({ key: "newer", latest_at: 60, read: true });
  assert.deepEqual(
    [older, newer, fresh, owed].sort(compareRows).map((r) => r.key),
    ["owed", "fresh", "newer", "older"],
  );
});

test("selection_stays_put_when_a_filter_hides_it", () => {
  // The bug this replaces: reading a row deleted it, and the selection then
  // silently landed on a different conversation.
  const known = [row({ key: "a", read: true }), row({ key: "b", read: false })];
  const visible = visibleRows(known, "unread", "any");
  assert.deepEqual(visible.map((r) => r.key), ["b"]);
  assert.equal(nextSelection(visible, known, "a"), "a");
});

test("selection_moves_only_when_the_conversation_is_gone", () => {
  const known = [row({ key: "b" })];
  assert.equal(nextSelection(known, known, "vanished"), "b");
  assert.equal(nextSelection([], [], "vanished"), null);
});

test("selection_is_chosen_when_there_is_none", () => {
  const known = [row({ key: "a" }), row({ key: "b" })];
  assert.equal(nextSelection(known, known, null), "a");
});

test("a_delta_changes_a_row_in_place", () => {
  const rows = [row({ key: "a", read: false, unread_count: 3, notices: [notice()], unread_notices: 1 }), row({ key: "b" })];
  const next = applyDelta(rows, {
    key: "a",
    unread_count: 0,
    needs_action_count: 0,
    latest_at: 200,
    read: true,
    handled: true,
  });
  assert.equal(next[0].read, true);
  assert.equal(next[0].unread_count, 0);
  assert.equal(next[0].latest_at, 200);
  assert.equal(next[0].title, "build the thing", "identity survives the patch");
  assert.equal(next[0].notices.length, 1, "a frame without the notice counts leaves the notices as they are");
  assert.equal(next[0].unread_notices, 1);
  assert.equal(next[1], rows[1], "an untouched row is not rebuilt");
});

test("a delta's notice counts clear what they can and never invent what they cannot carry", () => {
  const rows = [row({ key: "a", read: false, notices: [notice()], unread_notices: 1 })];
  const read = applyDelta(rows, { key: "a", unread_count: 0, needs_action_count: 0, latest_at: 200, read: true, handled: false, notice_count: 1, unread_notices: 0 });
  assert.equal(read[0].notices.length, 1, "the notice stays, read");
  assert.equal(read[0].unread_notices, 0);
  const gone = applyDelta(rows, { key: "a", unread_count: 0, needs_action_count: 0, latest_at: 200, read: true, handled: false, notice_count: 0, unread_notices: 0 });
  assert.deepEqual(gone[0].notices, [], "a count of zero clears");
  const more = { key: "a", unread_count: 0, needs_action_count: 0, latest_at: 300, read: false, handled: false, notice_count: 2, unread_notices: 2 };
  assert.equal(applyDelta(rows, more)[0].notices.length, 1, "the frame carries no text to add a notice with");
  assert.equal(needsReload(rows, more), true, "so the list is read again");
  assert.equal(needsReload(rows, { ...more, notice_count: 1 }), false);
  assert.equal(needsReload(rows, { key: "a", unread_count: 0, needs_action_count: 1, latest_at: 1, read: false, handled: false }), true, "a question the row does not hold");
  assert.equal(needsReload(rows, { key: "zz", unread_count: 0, needs_action_count: 0, latest_at: 1, read: false, handled: false }), true, "a row nobody holds");
});

test("a_delta_for_an_unknown_row_changes_nothing", () => {
  const rows = [row({ key: "a" })];
  const frame = {
    key: "z",
    unread_count: 1,
    needs_action_count: 0,
    latest_at: 1,
    read: false,
    handled: false,
  };
  assert.equal(applyDelta(rows, frame), rows);
});

test("a_delta_never_invents_a_question_it_does_not_carry", () => {
  // The frame counts what is waiting; it has no text to refill a list with,
  // so it may clear one and never trim one.
  const rows = [row({ key: "a", needs_action: [gate(), gate()] })];
  const keep = applyDelta(rows, {
    key: "a",
    unread_count: 0,
    needs_action_count: 1,
    latest_at: 1,
    read: true,
    handled: false,
  });
  assert.equal(keep[0].needs_action.length, 2);
  const cleared = applyDelta(rows, {
    key: "a",
    unread_count: 0,
    needs_action_count: 0,
    latest_at: 1,
    read: true,
    handled: true,
  });
  assert.deepEqual(cleared[0].needs_action, []);
});

test("a_gate_row_says_what_is_being_decided", () => {
  const s = summarize(row({ needs_action: [gate({ subject: "plan-7" })] }));
  assert.match(s.text, /plan-7/);
  assert.equal(s.urgent, true);
});

test("a_gate_with_no_subject_still_names_its_kind", () => {
  assert.equal(summarize(row({ needs_action: [gate()] })).text, "approval gate");
});

test("a_gates_subject_is_read_as_words", () => {
  // The engine's subjects have a grammar; the row says what is being decided
  // rather than printing the grammar.
  const adopt = summarize(row({ needs_action: [gate({ subject: "adopt:01J0WF@2" })] }));
  assert.equal(adopt.text, "Adopt the proposed workflow?");
  assert.equal(adopt.icon, "gate:approval");
  assert.equal(summarize(row({ needs_action: [gate({ subject: "amend:01J0RUN@01J0WF" })] })).text, "Approve the amendment?");
  assert.equal(summarize(row({ needs_action: [gate({ subject: "approval:01J0RUN/ship" })] })).text, "approval: step ship");
  assert.equal(
    summarize(row({ needs_action: [gate({ gate_kind: "escalation", subject: "step:01J0RUN/ask", step: "ask" })] })).text,
    "escalation: step ask",
  );
  assert.equal(
    summarize(row({ needs_action: [gate({ gate_kind: "publish", subject: "push fix/total" })] })).text,
    "publish: push fix/total",
  );
});

test("a notice you have not read is the row's line, in the Pulse's words and tone, and an ask outranks it", () => {
  const failed = row({ notices: [notice()], unread_notices: 1, read: false });
  const s = summarize(failed);
  assert.match(s.text, /failed/);
  assert.equal(s.tone, "fail", "a failure reads as a failure");
  assert.equal(s.icon, "notice:run_failed");
  assert.equal(s.urgent, false);
  const two = summarize(row({ notices: [notice(), notice({ seq: 6, at: 100, notice: "step_blocked" })], unread_notices: 2, read: false }));
  assert.match(two.text, /\(\+1\)$/, "the newest is the line; the rest are counted");
  const owed = summarize(row({ notices: [notice()], unread_notices: 1, needs_action: [gate()] }));
  assert.equal(owed.tone, "accent");
  assert.match(owed.text, /Adopt|approval/);
  const readNotice = summarize(row({ kind: "goal", notices: [notice()], unread_notices: 0, read: true }));
  assert.equal(readNotice.text, "Goal", "a read notice gives way");
  assert.equal(unreadNoticesOf(row({ notices: [notice()], unread_notices: 5 })), 1, "never more unread than there are");
  assert.deepEqual(noticesOf({ notices: "nope" }), []);
  const line = noticeLine(notice());
  assert.equal(line.tone, "fail");
  assert.equal(line.at, 120);
  assert.equal(line.notice, "run_failed");
  assert.deepEqual(line.source, { kind: "goal", id: "K" });
});

test("a start event that could not start a run is a notice on its workflow's row, or on the row of the goal that listens — opened in the designer, or on the goal's page", () => {
  const failed = (listener, source) =>
    notice({
      notice: "listener_failed",
      concept: source.kind === "goal" ? "goals" : "workflows",
      kind: "listener_failed",
      source,
      event: { type: "listener_failed", listener, signal: "S1", error: "the inputs no longer bind: ticket" },
    });
  const wf = row({ kind: "workflow", key: "WF1", source: "workflows", read: false, notices: [failed("workspace:WF1/ticket", { kind: "workflow", id: "WF1" })], unread_notices: 1 });
  const line = summarize(wf);
  assert.equal(line.text, "a workflow's start event could not start a run: the inputs no longer bind: ticket", "the Pulse's words");
  assert.equal(line.tone, "fail", "a failure reads as a failure");
  assert.equal(line.icon, "notice:listener_failed");
  assert.equal(matchesSource(wf, "workflows"), true, "under Workflows, as the node says");
  assert.deepEqual(counts([wf], "all").sources, { any: 1, messages: 0, projects: 0, workflows: 1, goals: 0, people: 0 }, "never a source of its own");
  assert.deepEqual(doorOf(wf), { route: { name: "workflow", id: "WF1" }, search: null }, "the designer, where the start is mended");
  const goal = row({ kind: "goal", key: "G1", source: "goals", read: false, notices: [failed("goal:G1/ticket", { kind: "goal", id: "G1" })], unread_notices: 1 });
  assert.match(summarize(goal).text, /^a goal's start event could not start a run/);
  assert.equal(matchesSource(goal, "goals"), true);
  assert.deepEqual(doorOf(goal), { route: { name: "goal", id: "G1" }, search: null }, "the goal's page, where its listening stands");
});

test("a_handled_row_says_what_was_decided", () => {
  const s = summarize(
    row({ handled: true, decided: { gate_kind: "approval", approve: true } }),
  );
  assert.equal(s.text, "approval approved");
  assert.equal(s.urgent, false);
});

test("a_declined_gate_does_not_read_as_approved", () => {
  const s = summarize(
    row({ handled: true, decided: { gate_kind: "publish", approve: false } }),
  );
  assert.equal(s.text, "publish declined");
});

test("a_question_reads_as_a_question_not_a_clock", () => {
  const s = summarize(row({ needs_action: [gate({ expects: { kind: "answer" } })] }));
  assert.equal(s.text, "A question for you");
});

test("a_question_with_options_is_still_a_question", () => {
  const s = summarize(
    row({
      needs_action: [
        gate({ expects: { kind: "answer", options: [{ id: "eur", label: "EUR" }] } }),
      ],
    }),
  );
  assert.equal(s.text, "A question for you");
  assert.equal(s.icon, "question");
});

test("a_question_never_reads_as_undefined_gate", () => {
  // `expects` became an object and this branch kept comparing it to the string
  // it used to be, so every question fell through to the gate arm and rendered
  // its `gate_kind` — which a question does not have a meaningful one of.
  const s = summarize(row({ needs_action: [gate({ expects: { kind: "answer" }, gate_kind: undefined })] }));
  assert.doesNotMatch(s.text, /undefined/);
});

test("a row without its actions owes nothing — read through needsOf everywhere, never a throw above the boundary", () => {
  const bare = row();
  delete bare.needs_action;
  assert.deepEqual(needsOf(bare), []);
  assert.deepEqual(needsOf(null), []);
  assert.deepEqual(needsOf({ needs_action: "not a list" }), []);
  assert.equal(rowState({ ...bare, read: true }), "read");
  assert.equal(isHandled({ ...bare, handled: true }), true);
  assert.equal(matchesFilter(bare, "needs_you"), false);
  assert.equal(summarize({ ...bare, kind: "goal" }).text, "Goal");
});

test("a row's kind has a word, and a kind this list has no word for is itself", () => {
  assert.equal(kindLabel("dm"), "Direct message");
  assert.equal(kindLabel("workstream"), "Workstream");
  assert.equal(kindLabel("conversation"), "Conversation");
  assert.equal(kindLabel("project"), "Project");
  assert.equal(kindLabel("workflow"), "Workflow");
  assert.equal(kindLabel("trigger"), "trigger", "a retired kind has no word of its own");
  assert.equal(kindLabel("something_else"), "something_else");
  assert.equal(kindLabel(undefined), "");
});

test("every kind has a door, and a message stream when it has one — a conversation its own, a workstream's, a project's and a workflow's none", () => {
  assert.deepEqual(doorOf(row({ kind: "goal", key: "g" })), { route: { name: "goal", id: "g" }, search: null });
  assert.deepEqual(doorOf(row({ kind: "dm", key: "d" })).route, { name: "dm", id: "d" });
  assert.deepEqual(doorOf(row({ kind: "channel", key: "c" })).route, { name: "channel", id: "c" });
  assert.deepEqual(doorOf(row({ kind: "workstream", key: "w" })).route, { name: "workbench", scope: "workstream", id: "w" });
  assert.deepEqual(doorOf(row({ kind: "project", key: "p" })).route, { name: "workbench", scope: "workstream", id: "p" });
  assert.equal(doorOf(row({ kind: "trigger", key: "t" })), null, "a retired kind has no door: there is no Triggers screen");
  assert.deepEqual(doorOf(row({ kind: "workflow", key: "wf" })), { route: { name: "workflow", id: "wf" }, search: null }, "the designer");
  const asked = row({ kind: "workflow", key: "wf", needs_action: [{ id: "a1", kind: "step", home: { home: "run", run: "R1" } }] });
  assert.deepEqual(doorOf(asked), { route: { name: "run", id: "R1" }, search: null }, "a run of the workspace that asks: its page");
  const goalAsk = row({ kind: "workflow", key: "wf", needs_action: [{ id: "a2", kind: "adopt", home: { home: "goal", goal: "G1" } }] });
  assert.deepEqual(doorOf(goalAsk).route, { name: "workflow", id: "wf" }, "an ask no run of the workspace owes: the designer");
  assert.equal(doorOf(row({ kind: "nope" })), null);
  assert.deepEqual(conversationOf(row({ kind: "goal", key: "g" })), { kind: "goal", scope: "g" });
  assert.equal(conversationOf(row({ kind: "project", key: "p" })), null, "a project's rows are notices; its conversations are their own rows");
  assert.equal(conversationOf(row({ kind: "workstream", key: "w" })), null);
  assert.deepEqual(conversationOf(row({ kind: "conversation", key: "c9" })), { kind: "conversation", scope: "c9" });
  assert.deepEqual(doorOf(row({ kind: "conversation", key: "c9" })), { route: { name: "conversation", id: "c9" }, search: null });
  assert.equal(readOnSelect(row({ kind: "conversation", key: "c9" })), false, "a conversation reads itself as it is shown");
  assert.equal(readOnSelect(row({ kind: "project", key: "p" })), true, "nothing else reads a project's row");
  assert.equal(conversationOf(row({ kind: "workflow" })), null, "a workflow's rows are notices; its door is the designer");
  assert.equal(readOnSelect(row({ kind: "workflow" })), true, "nothing else reads a workflow's row");
  assert.equal(readOnSelect(row({ kind: "goal" })), false, "the conversation reads itself");
  assert.deepEqual(unreadKeys([row({ key: "a", read: false }), row({ key: "b", read: true }), row({ key: "c", read: false })]), ["a", "c"]);
});

test("the chords: j k move, Enter and o open, a focuses the ask, e and u mark, Escape clears — never while typing, inside an ask, or with a modifier", () => {
  const ctx = { inInput: false, inAsk: false, modifier: false, hasSelection: true };
  assert.equal(keyAction("j", ctx), "next");
  assert.equal(keyAction("k", ctx), "prev");
  assert.equal(keyAction("Enter", ctx), "open");
  assert.equal(keyAction("o", ctx), "open");
  assert.equal(keyAction("a", ctx), "focus_ask");
  assert.equal(keyAction("e", ctx), "mark_read");
  assert.equal(keyAction("u", ctx), "mark_unread");
  assert.equal(keyAction("Escape", ctx), "clear");
  assert.equal(keyAction("x", ctx), null);
  assert.equal(keyAction("j", { ...ctx, inInput: true }), null);
  assert.equal(keyAction("j", { ...ctx, inAsk: true }), null);
  assert.equal(keyAction("j", { ...ctx, modifier: true }), null);
  assert.equal(keyAction("j", { ...ctx, hasSelection: false }), "next", "moving needs no selection");
  assert.equal(keyAction("Enter", { ...ctx, hasSelection: false }), null, "opening does");
  assert.equal(keyAction("e", { ...ctx, hasSelection: false }), null);
});

test("the list's keys are the list's: on a control elsewhere — a tab, a filter, a menu, the detail's buttons — none of them act", () => {
  const ctx = { inInput: false, inAsk: false, modifier: false, hasSelection: true };
  for (const key of ["j", "k", "Enter", "o", "a", "e", "u", "Escape"]) {
    assert.equal(keyAction(key, { ...ctx, onControl: true }), null, `${key} on a control outside the list`);
    assert.notEqual(keyAction(key, { ...ctx, onControl: false }), null, `${key} in the list, or with nothing focused`);
  }
  assert.equal(keyAction("Escape", { ...ctx, onControl: true }), null, "an Escape that closes a menu keeps the selection");
  assert.equal(keyAction("j", ctx), "next", "said without the flag, the list has the keys — as before");
  const view = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("onControl,") && view.includes("document.activeElement"), "the screen says where focus stands, from the document");
});

test("a held step's release has one verb, a decision two", () => {
  assert.deepEqual(askVerbs({ subject: "release:01J0RUN/hold" }), { approve: "Release", decline: null });
  assert.deepEqual(askVerbs({ subject: "approval:01J0RUN/ship" }), { approve: "Approve", decline: "Decline" });
  assert.match(askHint({ subject: "guard:Bash" }), /kept for this goal/);
  assert.equal(askHint({ subject: "approval:01J0RUN/ship" }), null);
  assert.equal(askHint({ subject: null }), null);
  assert.deepEqual(askVerbs({}), { approve: "Approve", decline: "Decline" });
  assert.equal(summarize(row({ needs_action: [gate({ subject: "release:01J0RUN/hold" })] })).text, "Release the held step?");
});

test("every source tab wears a glyph that exists — the mark before its word, and the whole tab on a window too narrow for the words", () => {
  const icons = readFileSync(new URL("../../ui/icons.ts", import.meta.url), "utf8");
  for (const s of SOURCES) {
    assert.ok(s.icon, `${s.id} names a glyph`);
    assert.match(icons, new RegExp(`^  ${s.icon}: `, "m"), `${s.id} wears a glyph that exists`);
  }
  assert.equal(new Set(SOURCES.map((s) => s.icon)).size, SOURCES.length, "each source its own glyph");
});

test("the strip's words carry their counts", () => {
  assert.deepEqual(filterSegments({ needs_you: 3, unread: 0, all: 12 }), [
    { id: "needs_you", label: "Needs you · 3" },
    { id: "unread", label: "Unread" },
    { id: "all", label: "All · 12" },
  ]);
  const tabs = sourceTabs({ any: 2, goals: 1, projects: 0, workflows: 0, messages: 1, people: 0 });
  assert.deepEqual(
    tabs.map((t) => t.id),
    SOURCES.map((s) => s.id),
    "the tabs in the sources' order",
  );
  assert.deepEqual(tabs.map((t) => t.label), ["Any", "Messages", "Projects", "Workflows", "Goals", "People"], "five sources and Any, in the order the tabs stand");
  assert.deepEqual(tabs.map((t) => t.icon), ["inbox", "dm", "project", "workflow", "goal", "members"]);
  assert.deepEqual(tabs.map((t) => [t.id, t.count]), [["any", 2], ["messages", 1], ["projects", 0], ["workflows", 0], ["goals", 1], ["people", 0]]);
  assert.equal(sourceTabs({ any: 4 }).find((t) => t.id === "goals")?.count, 0, "a source the tally does not name counts nothing");
});

test("a session row is a harness waiting in its terminal: owed, from projects, titled by the harness, opened on its workstream, gone when its frame says so", () => {
  assert.equal(kindLabel("session"), "Terminal");
  const waiting = { session: "s1", harness: "claude-code", workstream: "w1", on: { on: "permission", tool: "Bash" }, words: "permission: Bash", since: 9 };
  // The node files a harness in a terminal under Projects: it stands in a checkout.
  const row = { key: "s1", kind: "session", source: "projects", title: "claude-code · main", latest_at: 9, read: true, needs_action: [], notices: [], waiting };
  assert.equal(matchesSource(row, "projects"), true);
  assert.equal(rowState(row), "waiting", "a harness at its prompt is owed");
  assert.equal(matchesFilter(row, "needs_you"), true);
  assert.equal(waitingWords(row, { "claude-code": "Claude Code" }), "Claude Code is waiting on you — permission: Bash");
  assert.equal(waitingWords(row), "claude-code is waiting on you — permission: Bash", "no label: the id");
  assert.equal(waitingOf({ ...row, waiting: undefined }), null);
  assert.deepEqual(summarize(row), { text: "waiting on you — permission: Bash", icon: "waiting", urgent: true, tone: "accent" });
  assert.deepEqual(doorOf(row), { route: { name: "workbench", scope: "workstream", id: "w1" }, search: null }, "the door is the workstream; the screen adds the tab");
  assert.equal(doorOf({ ...row, waiting: { ...waiting, workstream: undefined } }), null, "nowhere to go without a checkout");
  assert.equal(conversationOf(row), null, "a terminal has no message stream");
  assert.equal(readOnSelect(row), true);
  assert.equal(counts([row], "needs_you").sources.projects, 1);
  assert.equal(counts([row], "needs_you").buckets.needs_you, 1);
  // The frame: the wait over drops the row; a new wait for a row nobody holds is the list's to fetch.
  assert.deepEqual(applyDelta([row], { key: "s1", kind: "session", waiting: false, unread_count: 0, needs_action_count: 0, latest_at: 0, read: true, handled: false }), []);
  assert.equal(applyDelta([row], { key: "s1", kind: "session", waiting: true, unread_count: 0, needs_action_count: 1, latest_at: 0, read: true, handled: false })[0], row, "still waiting: the row stands");
  assert.equal(needsReload([], { key: "s2", kind: "session", waiting: true, unread_count: 0, needs_action_count: 1, latest_at: 0, read: true, handled: false }), true);
  assert.equal(needsReload([], { key: "s2", kind: "session", waiting: false, unread_count: 0, needs_action_count: 0, latest_at: 0, read: true, handled: false }), false, "a wait that ended before it was shown moves nothing");
  assert.equal(needsReload([row], { key: "s1", kind: "session", waiting: true, unread_count: 0, needs_action_count: 1, latest_at: 0, read: true, handled: false }), false);
});

test("a people row is its own source, waits while a join is open, and opens Settings › People", () => {
  assert.equal(kindLabel("people"), "Person");
  const row = { key: "ab".repeat(32), kind: "people", source: "people", title: "abababab…", read: true, needs_action: [], notices: [], join: { invite: "i1", role: "guest", label: "Bob", at: 5 } };
  assert.equal(matchesSource(row, "people"), true);
  assert.equal(matchesSource(row, "messages"), false);
  assert.equal(rowState(row), "waiting");
  assert.equal(joinWords(row), "Bob asks to join as a guest.");
  assert.equal(joinOf({ ...row, join: undefined }), null);
  assert.deepEqual(doorOf(row), { route: { name: "settings" }, search: { tab: "people" } });
});

test("a browser tab opened beside an Inbox row is at home in the goal, channel, message, conversation, workstream or workflow the row is — and nowhere for the rest", () => {
  for (const kind of ["goal", "channel", "dm", "conversation", "workstream", "workflow"]) assert.deepEqual(browserHomeOf({ kind, key: "k1" }), { scope: kind, id: "k1" }, kind);
  for (const kind of ["people", "session", "project"]) assert.equal(browserHomeOf({ kind, key: "k1" }), null, `${kind} is no home for a tab`);
  assert.equal(browserHomeOf(null), null);
});

test("an id is a direct channel, a standing channel or nothing; only a channel or a direct channel row seeds the tray", async () => {
  const { channelOfRow, scopeKindOf } = await import("./inboxModel.mjs");
  const channels = [{ channel: { id: "general", name: "General" } }];
  const dms = [{ channel: { id: "dm1", name: "Ada" } }];
  assert.equal(scopeKindOf("dm1", channels, dms), "dm");
  assert.equal(scopeKindOf("general", channels, dms), "channel");
  assert.equal(scopeKindOf("nope", channels, dms), null);
  assert.equal(channelOfRow({ kind: "channel", key: "general" }, channels, dms)?.name, "General");
  assert.equal(channelOfRow({ kind: "dm", key: "dm1" }, channels, dms)?.name, "Ada");
  assert.equal(channelOfRow({ kind: "goal", key: "general" }, channels, dms), null, "a goal row seeds nothing, whatever its key");
  assert.equal(channelOfRow({ kind: "channel", key: "gone" }, channels, dms), null);
  assert.equal(channelOfRow(null, channels, dms), null);
});

test("a frame that is not the inbox's is worth the list only when it moves what is owed or what happened", () => {
  const engine = (type) => ({ stream: "engine", payload: { payload: { type } } });
  for (const type of ["gate_opened", "run_finished", "listener_failed", "listener_fired", "listening_changed", "invite_changed"]) assert.ok(wantsList(engine(type)), type);
  for (const type of ["agent_streamed", "agent_thinking", "changes_moved", "invented_later", "trigger_fired", "trigger_failed", undefined]) assert.ok(!wantsList(engine(type)), `${type} moves nothing here`);
  assert.ok(wantsList({ stream: "conversation", payload: { snapshot: true } }), "a thread that was rewritten");
  assert.ok(!wantsList({ stream: "conversation", payload: { scope: "K", snippet: "hi" } }), "a message has its own inbox frame");
  assert.ok(!wantsList({ stream: "inbox", payload: { key: "K" } }), "the inbox's own frames are applyDelta's and needsReload's");
  for (const broken of [null, undefined, "frame", {}, { stream: "engine" }, { stream: "engine", payload: null }, { stream: "engine", payload: { payload: null } }]) {
    assert.equal(wantsList(broken), false, `${JSON.stringify(broken)} is read as nothing, never thrown on`);
  }
  assert.ok(RELOAD_DEBOUNCE_MS >= 100 && RELOAD_DEBOUNCE_MS <= 1000, "a burst is one reload, and a person does not wait for it");
  assert.equal(new Set(RELOAD_ON).size, RELOAD_ON.length, "no tag twice");
});

test("every tag the node reads a notice from reloads the list: a notice never waits for an unrelated frame", () => {
  const rust = readFileSync(new URL("../../../../crates/bisa-engine/src/notices.rs", import.meta.url), "utf8");
  const block = rust.slice(rust.indexOf("pub const NOTICE_TAGS"), rust.indexOf("];", rust.indexOf("pub const NOTICE_TAGS")));
  const tags = [...block.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
  assert.ok(tags.length >= 10 && tags.includes("run_finished"), "the engine's list is read");
  for (const tag of tags) assert.ok(RELOAD_ON.includes(tag), `${tag} is a notice the Inbox would not reload for`);
});

test("the view holds no reload policy of its own", () => {
  const view = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("wantsList(frame)") && view.includes("RELOAD_DEBOUNCE_MS"));
  assert.ok(!view.includes('"gate_opened"'), "no tag is spelt in the view");
});

test("a mark is shown before the node says so: read clears what was new and never what is owed; twice is once", () => {
  const rows = [row({ key: "a", read: false, unread_count: 3, unread_notices: 2, notices: [notice()], needs_action: [gate()] }), row({ key: "b", read: false, unread_count: 1 })];
  const read = withReadMark(rows, "a", true);
  assert.deepEqual([read[0].read, read[0].unread_count, read[0].unread_notices], [true, 0, 0]);
  assert.equal(read[0].needs_action.length, 1, "reading a question does not answer it");
  assert.equal(read[0].notices.length, 1, "what happened is still there, no longer new");
  assert.equal(rowState(read[0]), "waiting");
  assert.strictEqual(read[1], rows[1], "the other rows are the same objects");
  assert.strictEqual(withReadMark(read, "a", true), read, "said twice, nothing changes — not even the array");
  const unread = withReadMark(read, "a", false);
  assert.deepEqual([unread[0].read, unread[0].unread_count], [false, 0], "unread flips the mark; what is new again is the node's to count");
  assert.strictEqual(withReadMark(rows, "nobody", true), rows, "a key nobody holds changes nothing");
  assert.deepEqual(withReadMark([], "a", true), []);
});


test("the address names a bucket and a source, and anything else is the first of each", () => {
  for (const id of ["needs_you", "unread", "all"]) assert.equal(filterOf(id), id);
  for (const raw of [undefined, null, "", "handled", "ALL", 7, {}]) assert.equal(filterOf(raw), "needs_you", `${JSON.stringify(raw)}`);
  for (const s of SOURCES) assert.equal(sourceIdOf(s.id), s.id);
  for (const raw of [undefined, null, "", "triggers", "Goals", 7]) assert.equal(sourceIdOf(raw), "any", `${JSON.stringify(raw)}`);
});

test("every kind's door has its words, and a kind this list has no word for opens plainly", () => {
  assert.equal(doorLabel("goal"), "Open goal");
  assert.equal(doorLabel("workflow"), "Open workflow");
  assert.equal(doorLabel("people"), "Open People");
  assert.equal(doorLabel("conversation"), "Open the conversation");
  assert.equal(doorLabel("workstream"), "Open in the IDE");
  assert.equal(doorLabel("project"), "Open in the IDE", "a project's door is its primary workstream's");
  assert.equal(doorLabel("session"), "Open the terminal");
  assert.equal(doorLabel("channel"), "Open");
  assert.equal(doorLabel("dm"), "Open");
  for (const raw of ["invented", "", null, undefined]) assert.equal(doorLabel(raw), "Open");
  // Every kind the list has a word for has a door with words.
  for (const kind of Object.keys(KIND_LABEL)) assert.ok(doorLabel(kind).startsWith("Open"), kind);
  const view = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("{doorLabel(current.kind)}") && !view.includes('tr("screens-inbox-open-goal")'), "the screen asks the model");
  // The words a mark wears are the catalog's too — one word each, which no scanner sees.
  assert.ok(view.includes('tr("screens-inbox-mark-read-word")') && view.includes('tr("screens-inbox-mark-unread-word")'));
  assert.ok(!view.includes('aria-label="read"') && !view.includes(': "unread"') && !/\} new</.test(view), "no word is spelt beside an expression");
});

test("an empty list says why: nothing waits, nothing is new, or nothing from the source it was narrowed to", () => {
  assert.deepEqual(emptyWords("needs_you", "any").title, "Nothing waits on you");
  assert.equal(emptyWords("needs_you", "projects").title, "Nothing waits on you", "the bucket speaks first, whatever the source");
  assert.equal(emptyWords("unread", "goals").title, "Nothing new");
  assert.equal(emptyWords("all", "any").title, "Nothing here yet");
  assert.equal(emptyWords("all", "workflows").title, "Nothing from workflows yet");
  assert.equal(emptyWords("all", "people").title, "Nothing from people yet");
  assert.equal(emptyWords("all", "invented").title, "Nothing here yet", "a source nobody knows narrows nothing");
  for (const f of ["needs_you", "unread", "all"]) assert.ok(emptyWords(f, "any").hint.length > 20, `${f} says what would land here`);
});

test("mark all read goes a few at a time, each row once, and nothing for nothing", () => {
  assert.deepEqual(markBatches([]), []);
  assert.deepEqual(markBatches(["a"]), [["a"]]);
  const keys = Array.from({ length: 20 }, (_, i) => `k${i}`);
  const batches = markBatches(keys);
  assert.deepEqual(batches.map((b) => b.length), [MARKS_AT_ONCE, MARKS_AT_ONCE, 4]);
  assert.deepEqual(batches.flat(), keys, "in the list's order");
  assert.deepEqual(markBatches(["a", "b", "a"]), [["a", "b"]], "a row named twice is marked once");
  assert.deepEqual(markBatches(["a", "b", "c"], 2), [["a", "b"], ["c"]]);
  for (const size of [0, -1, 1.5, Number.NaN, null]) assert.deepEqual(markBatches(["a", "b"], size), [["a", "b"]], `${size} is no size: the model's own`);
  assert.ok(MARKS_AT_ONCE >= 2 && MARKS_AT_ONCE <= 16);
  const view = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  const mark = view.slice(view.indexOf("const markAllRead"), view.indexOf("const move = "));
  assert.ok(mark.includes("for (const batch of markBatches(keys)) await Promise.all(batch.map((k) => api.markRead(k)))"), "the screen sends them as the model groups them");
  assert.ok(/finally \{[\s\S]*ws\.refresh\(\);[\s\S]*await load\(\);/.test(mark), "whichever marks landed, the list is read: a mark that failed halfway leaves no row lying");
});

test("only the newest read of the list is drawn: an older answer that lands last puts nothing back", () => {
  assert.equal(isNewestRead(3, 3), true);
  assert.equal(isNewestRead(2, 3), false, "a newer read was asked for meanwhile");
  assert.equal(isNewestRead(1, 0), false);
  const view = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  const load = view.slice(view.indexOf("const load = useCallback"), view.indexOf("useEffect(() => {\n    const ctrl"));
  assert.ok(load.includes("const asked = ++reads.current"), "every read takes a number");
  assert.equal(load.split("isNewestRead(asked, reads.current)").length, 4, "the rows, the failure and the busy mark are each the newest read's");
});

test("a gate's and a decision's words are the catalog's: the model builds no sentence of its own", () => {
  const model = readFileSync(new URL("./inboxModel.mjs", import.meta.url), "utf8");
  assert.ok(!/`\$\{[^`]*\}: step/.test(model) && !model.includes('"approved"') && !model.includes('"declined"'), "no English is assembled in code");
  assert.equal(summarize(row({ handled: true, decided: null })).text, "decision approved", "a decision nobody described still reads");
});

test("an ask's chip says what it is about — a question, or its gate by its subject — and the card spells none of it", () => {
  const gate = (subject, gate_kind = "approval") => ({ subject, gate_kind });
  assert.equal(askTitle(gate("approval:01RUN/review"), true), "question", "a question is a question, whatever gate carries it");
  assert.equal(askTitle(gate("adopt:01WF@3"), false), "Adopt this workflow?");
  assert.equal(askTitle(gate("amend:01RUN@2"), false), "Approve this amendment?");
  assert.equal(askTitle(gate("release:01RUN/hold"), false), "release step hold");
  assert.equal(askTitle(gate("approval:01RUN/review"), false), "approval step review");
  assert.equal(askTitle(gate("guard:Bash", "escalation"), false), "Allow Bash?");
  assert.equal(askTitle(gate("permission:Write", "escalation"), false), "Allow Write?");
  assert.equal(askTitle(gate("publish:web", "publish"), false), "publish gate");
  assert.equal(askTitle(gate(null, "escalation"), false), "escalation gate");
  const card = readFileSync(new URL("./NeedsAction.tsx", import.meta.url), "utf8");
  assert.ok(card.includes("{askTitle(action, isQuestion)}"));
  assert.ok(!card.includes("startsWith(") && !card.includes('"question"'), "no rule and no word of its own");
});

test("an owed decision is told from the next by what it is — the gate, else the step, else its kind and question — never by its place", async () => {
  const { needsKey } = await import("./inboxModel.mjs");
  const live = { gate_id: "g1", run: "r1", step: "s1", gate_kind: "approval", question: "Ship it?" };
  assert.equal(needsKey(live), "g1");
  assert.equal(needsKey({ ...live, gate_id: null }), "r1:s1", "rebuilt from the run: the step it completes");
  assert.equal(needsKey({ gate_id: null, run: null, step: null, gate_kind: "adoption", question: "Adopt the design?" }), "adoption:Adopt the design?", "an adoption from the journal");
  assert.notEqual(needsKey({ ...live, gate_id: null, step: "s2" }), needsKey({ ...live, gate_id: null }), "two waiting steps are two cards");
  const { readFileSync } = await import("node:fs");
  const inbox = readFileSync(new URL("../Inbox.tsx", import.meta.url), "utf8");
  assert.ok(inbox.includes("key={needsKey(a)}") && !inbox.includes("`durable:${i}`"), "a card once keyed by its place took the state of the one before it as one was decided");
});
