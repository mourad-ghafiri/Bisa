/**
 * The notes model, tested where it lives.
 *
 * Nothing here renders — there is no jsdom in this repo. What can actually be
 * wrong in a way a person notices: text spliced at the wrong offset so a
 * toolbar click eats a character, a tab listing another kind's notes, a new
 * note filed somewhere you were not standing, and a search that drops a note
 * that says the words.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  listedNote,
  NOTE_TABS,
  NOTE_TAB_LABEL,
  NOTE_VIEWS,
  OWNER_GONE_FRAMES,
  SAVE_DEFAULT_MS,
  SAVE_MAX_MS,
  SAVE_MIN_MS,
  adoptIncoming,
  landedNotes,
  latestNote,
  parseDraft,
  restoredDraft,
  clampSaveDelay,
  draftKey,
  filterNotes,
  matchAt,
  movesNoteCount,
  noteTab,
  noteTargets,
  ownerGone,
  noteView,
  routeTarget,
  sameScope,
  scopeOfRow,
  scopeQuery,
  scopeWords,
  tabAdmits,
  tabKind,
  tabQuery,
  wrapSelection,
} from "./notesModel.mjs";

test("wrapping a selection puts the markers outside it and selects the words back", () => {
  // The caret range that comes back is the *text*, not the markers: pressing
  // Bold twice must not nest, and the reader must be able to keep typing over
  // what they just emphasised.
  const r = wrapSelection("the token is short-lived", 4, 9, "bold");
  assert.equal(r.text, "the **token** is short-lived");
  assert.equal(r.text.slice(r.from, r.to), "token");
});

test("a toolbar click with nothing selected leaves placeholder text selected", () => {
  // Otherwise the button inserts `****` and drops the caret somewhere inside
  // it, which is the one outcome that sends you back to the mouse.
  const r = wrapSelection("", 0, 0, "bold");
  assert.equal(r.text, "**bold text**");
  assert.equal(r.text.slice(r.from, r.to), "bold text");
});

test("a backwards drag marks the same range as a forwards one", () => {
  // The browser reports a selection dragged right-to-left with from > to, and
  // every offset in the splice assumes it is ordered.
  const forwards = wrapSelection("alpha beta", 6, 10, "code");
  const backwards = wrapSelection("alpha beta", 10, 6, "code");
  assert.equal(forwards.text, "alpha `beta`");
  assert.deepEqual(backwards, forwards);
});

test("a line mark prefixes every selected line, and never twice", () => {
  // Selecting three lines and pressing Bullet must produce three bullets, not
  // one marker at the top of the block.
  const text = "one\ntwo\nthree";
  const r = wrapSelection(text, 0, text.length, "bullet");
  assert.equal(r.text, "- one\n- two\n- three");

  // Pressing it again on an already-bulleted block leaves it alone rather
  // than stacking `- - `.
  const again = wrapSelection(r.text, r.from, r.to, "bullet");
  assert.equal(again.text, "- one\n- two\n- three");
});

test("a line mark reaches the start of the line even when the caret is mid-word", () => {
  // A heading applied from where the caret happens to sit would otherwise
  // land in the middle of the sentence.
  const r = wrapSelection("the auth flow", 8, 8, "heading");
  assert.equal(r.text, "## the auth flow");
});

test("a link selects the url, because that is the part still missing", () => {
  const r = wrapSelection("see the RFC", 8, 11, "link");
  assert.equal(r.text, "see the [RFC](url)");
  assert.equal(r.text.slice(r.from, r.to), "url");
});

test("an out-of-range selection is clamped rather than producing torn text", () => {
  // State and the DOM can disagree for a frame after a programmatic edit.
  const r = wrapSelection("short", 2, 999, "italic");
  assert.equal(r.text, "sh_ort_");
});

const NAMES = {
  projects: [
    { id: "01SHOP", name: "Shop" },
    { id: "01SITE", name: "Site" },
  ],
  goals: [{ id: "01GOAL", name: "Launch v2" }],
  workflows: [{ id: "01WF", name: "Release" }],
  channels: [{ id: "engineering", name: "Engineering" }],
};

test("the tabs are the seven, All first, and an unknown stored tab lands on All", () => {
  assert.deepEqual(NOTE_TABS, ["all", "workspace", "projects", "goals", "workflows", "channels", "node"]);
  for (const tab of NOTE_TABS) assert.equal(typeof NOTE_TAB_LABEL[tab], "string");
  assert.equal(NOTE_TAB_LABEL.all, "All");
  assert.equal(noteTab("goals"), "goals");
  for (const raw of ["", null, undefined, "everything", "goal"]) assert.equal(noteTab(raw), "all");
});

test("a tab is a kind the node filters by, and All is no kind at all", () => {
  // A plural tab is *every record of the kind*, so it sends the kind alone —
  // the node's word for that — and All sends nothing, which is every note.
  assert.equal(tabKind("all"), null);
  assert.equal(tabKind("projects"), "project");
  assert.equal(tabKind("node"), "node");
  assert.equal(tabQuery("all"), "");
  assert.equal(tabQuery("projects"), "?scope=project");
  assert.equal(tabQuery("workspace"), "?scope=workspace");
  assert.equal(tabQuery("channels"), "?scope=channel");
});

test("a frame redraws the list only when its kind is on the tab", () => {
  assert.ok(tabAdmits("all", "goal"));
  assert.ok(tabAdmits("goals", "goal"));
  assert.ok(!tabAdmits("goals", "project"));
  assert.ok(tabAdmits("node", "node"));
  assert.ok(!tabAdmits("workspace", "node"));
});

test("the dock's count is read again on a note changed and on a place gone with its notes — never on an archive, an edit's own save, or a channel, which says nothing", () => {
  assert.deepEqual([...OWNER_GONE_FRAMES], ["project_deleted", "goal_deleted", "workflow_deleted"]);
  assert.ok(movesNoteCount("note_changed"), "created, deleted, or appended to by an agent");
  for (const gone of OWNER_GONE_FRAMES) assert.ok(movesNoteCount(gone) && ownerGone(gone), gone);
  for (const still of ["drawing_changed", "project_archived", "goal_archived", "workflow_archived", "file_changed", "channel_deleted", "channel_changed", "", undefined, null, 7]) {
    assert.ok(!movesNoteCount(still), `${String(still)} moves no note count`);
    assert.ok(!ownerGone(still), `${String(still)} is no place gone`);
  }
});

test("the route says where a new note goes: a goal, a workflow, a channel, a project, else the workspace", () => {
  assert.deepEqual(routeTarget({ name: "goal", id: "01GOAL" }), { scope: "goal", id: "01GOAL" });
  assert.deepEqual(routeTarget({ name: "workflow", id: "01WF" }), { scope: "workflow", id: "01WF" });
  assert.deepEqual(routeTarget({ name: "channel", id: "engineering" }), { scope: "channel", id: "engineering" });
  // A project's root is its primary workstream, whose id is the project's.
  assert.deepEqual(routeTarget({ name: "workbench", scope: "workstream", id: "01PROJ" }), {
    scope: "project",
    id: "01PROJ",
  });
  // Any other workstream files its notes under the project it checks out —
  // a note under a workstream would vanish when it closed — and one nobody
  // can place falls to the workspace, never to nothing.
  assert.deepEqual(
    routeTarget({ name: "workbench", scope: "workstream", id: "01WS" }, (w) => (w === "01WS" ? "01PROJ" : null)),
    { scope: "project", id: "01PROJ" },
  );
  assert.deepEqual(routeTarget({ name: "workbench", scope: "workstream", id: "01WS" }), {
    scope: "project",
    id: "01WS",
  });
  for (const route of [{ name: "inbox" }, { name: "agents" }, { name: "settings" }, { name: "workflows" }, null]) {
    assert.deepEqual(routeTarget(route), { scope: "workspace" });
  }
});

test("a standalone tab has one target, a plural tab every record with the current one first", () => {
  assert.deepEqual(noteTargets("workspace", { name: "inbox" }, NAMES), [
    { scope: { scope: "workspace" }, label: "Workspace", here: true },
  ]);
  assert.deepEqual(noteTargets("node", { name: "inbox" }, NAMES), [
    { scope: { scope: "node" }, label: "Node", here: false },
  ]);
  // On the Site project, Site goes first; Shop keeps its place after it.
  const projects = noteTargets("projects", { name: "workbench", scope: "workstream", id: "01SITE" }, NAMES);
  assert.deepEqual(
    projects.map((t) => [t.label, t.here]),
    [
      ["Site", true],
      ["Shop", false],
    ],
  );
  assert.deepEqual(projects[0].scope, { scope: "project", id: "01SITE" });
  // Away from any project, the directory's own order stands.
  assert.deepEqual(
    noteTargets("projects", { name: "inbox" }, NAMES).map((t) => t.label),
    ["Shop", "Site"],
  );
  assert.deepEqual(
    noteTargets("channels", { name: "channel", id: "engineering" }, NAMES).map((t) => [t.label, t.here]),
    [["Engineering", true]],
  );
  // No directory yet: a plural tab has nowhere to file, and says so with an empty list.
  assert.deepEqual(noteTargets("goals", { name: "inbox" }, null), []);
});

test("All offers the workspace and the node, plus where you stand when that is a record", () => {
  assert.deepEqual(
    noteTargets("all", { name: "inbox" }, NAMES).map((t) => [t.label, t.here]),
    [
      ["Workspace", true],
      ["Node", false],
    ],
  );
  const onGoal = noteTargets("all", { name: "goal", id: "01GOAL" }, NAMES);
  assert.deepEqual(
    onGoal.map((t) => [t.label, t.here]),
    [
      ["Launch v2", true],
      ["Workspace", false],
      ["Node", false],
    ],
  );
  assert.deepEqual(onGoal[0].scope, { scope: "goal", id: "01GOAL" });
  // A goal the directory does not know is not offered by an id nobody can read.
  assert.deepEqual(
    noteTargets("all", { name: "goal", id: "01GHOST" }, NAMES).map((t) => t.label),
    ["Workspace", "Node"],
  );
});

test("a row's scope is named by the record, or by its kind when the name is unknown", () => {
  assert.equal(scopeWords({ scope: "workspace" }, NAMES), "Workspace");
  assert.equal(scopeWords({ scope: "node" }, NAMES), "Node");
  assert.equal(scopeWords({ scope: "project", id: "01SHOP" }, NAMES), "Shop");
  assert.equal(scopeWords({ scope: "goal", id: "01GOAL" }, NAMES), "Launch v2");
  assert.equal(scopeWords({ scope: "workflow", id: "01WF" }, NAMES), "Release");
  assert.equal(scopeWords({ scope: "channel", id: "engineering" }, NAMES), "Engineering");
  assert.equal(scopeWords({ scope: "goal", id: "01GHOST" }, NAMES), "A goal");
  assert.equal(scopeWords({ scope: "channel", id: "x" }, null), "A channel");
  assert.equal(scopeWords(null, NAMES), "Workspace");
});

test("a row's flat pair reads back as a scope", () => {
  assert.deepEqual(scopeOfRow({ scope: "goal", scope_id: "01GOAL" }), { scope: "goal", id: "01GOAL" });
  assert.deepEqual(scopeOfRow({ scope: "node" }), { scope: "node" });
  assert.deepEqual(scopeOfRow({ scope: "workspace", scope_id: null }), { scope: "workspace" });
});

test("scopes compare by both halves, so two goals are not the same place", () => {
  assert.ok(sameScope({ scope: "goal", id: "a" }, { scope: "goal", id: "a" }));
  assert.ok(!sameScope({ scope: "goal", id: "a" }, { scope: "goal", id: "b" }));
  assert.ok(!sameScope({ scope: "goal", id: "a" }, { scope: "project", id: "a" }));
  assert.ok(sameScope({ scope: "workspace" }, { scope: "workspace" }));
  assert.ok(!sameScope({ scope: "workspace" }, { scope: "node" }));
  assert.ok(!sameScope(null, { scope: "workspace" }));
});

test("the workspace query carries no id, because a workspace note names nothing", () => {
  // The node refuses `scope=workspace` with an `id`, so sending one would turn
  // every workspace listing into a 400.
  assert.equal(scopeQuery({ scope: "workspace" }), "?scope=workspace");
  assert.equal(scopeQuery({ scope: "goal", id: "01A" }), "?scope=goal&id=01A");
});

test("a search keeps the notes that say every word, in title or body, and keeps their order", () => {
  const notes = [
    { id: "1", title: "Cache TTL", body: "why the cache is a cache" },
    { id: "2", title: "Release", body: "the TTL of a token" },
    { id: "3", title: "Nothing", body: "" },
  ];
  assert.deepEqual(filterNotes(notes, "").map((n) => n.id), ["1", "2", "3"]);
  assert.deepEqual(filterNotes(notes, "   ").map((n) => n.id), ["1", "2", "3"]);
  assert.deepEqual(filterNotes(notes, "ttl").map((n) => n.id), ["1", "2"], "case does not matter");
  assert.deepEqual(filterNotes(notes, "cache ttl").map((n) => n.id), ["1"], "every word, anywhere");
  assert.deepEqual(filterNotes(notes, "token release").map((n) => n.id), ["2"], "title and body together");
  assert.deepEqual(filterNotes(notes, "nowhere"), []);
  assert.equal(filterNotes(notes, ""), notes, "an empty query is the same list, not a copy");
});

test("the match the bar stands on is the one at its index, or none", () => {
  const matches = [
    { start: 0, end: 2 },
    { start: 5, end: 7 },
  ];
  assert.deepEqual(matchAt(matches, 1), { start: 5, end: 7 });
  assert.equal(matchAt(matches, -1), null);
  assert.equal(matchAt(matches, 2), null);
  assert.equal(matchAt(null, 0), null);
});

test("an agent's append lands under whatever you have typed, live — nothing of yours is in its way", () => {
  // The ordinary case: an agent appended while you sat reading, so take it.
  assert.deepEqual(adoptIncoming("mine", "mine", "mine\n\ntheirs"), { body: "mine\n\ntheirs", outcome: "appended" });
  // And while you were typing: the block lands under your text, no banner,
  // and the next save carries both at the new hash. Before, this was a
  // conflict that hid the agent's block until you merged — and the merge
  // repeated the text you had already saved.
  assert.deepEqual(adoptIncoming("mine, still typing", "mine", "mine\n\ntheirs"), { body: "mine, still typing\n\ntheirs", outcome: "appended" });
  assert.deepEqual(adoptIncoming("typed into empty", "", "\n\n---\n\n**agent** · now\n\nhello"), { body: "typed into empty\n\n---\n\n**agent** · now\n\nhello", outcome: "appended" }, "an empty note is extended by anything");
  // A title that moved alone: the body is what it was, nothing to lay under.
  assert.deepEqual(adoptIncoming("mine, still typing", "mine", "mine"), { body: "mine, still typing", outcome: "appended" });
});

test("an agent's rewrite is taken over a clean buffer, with the way back the caller's, and is a conflict over unsaved text", () => {
  // Asked to change the note, the agent rewrote it (`note_write`): with
  // nothing typed since the last save it shows, and the caller keeps what
  // was confirmed so *Restore my version* can bring it back.
  assert.deepEqual(adoptIncoming("mine", "mine", "theirs instead"), { body: "theirs instead", outcome: "taken" });
  // This is the bug that made the panel unusable: the editor re-seeded from
  // every incoming copy, so text typed since the last save was replaced by an
  // older version of itself — and the difference was then saved, which
  // produced the next incoming copy. A rewrite over your typing stays yours.
  const r = adoptIncoming("mine, still typing", "mine", "theirs instead");
  assert.equal(r.body, "mine, still typing", "the buffer must survive");
  assert.equal(r.outcome, "conflict", "and the caller has to be told");
});

test("a copy identical to the buffer is not a change at all", () => {
  // The echo case. Returning the same string matters: a caller that sets state
  // unconditionally here re-renders on every frame, which is how a redraw
  // becomes a loop.
  const local = "unchanged";
  const r = adoptIncoming(local, "something else entirely", local);
  assert.equal(r.body, local);
  assert.equal(r.outcome, "same", "identical text is never a conflict");
});

test("a parked draft carries the hash it was typed against, and is restored as a conflict when the note moved since", () => {
  assert.deepEqual(parseDraft(JSON.stringify({ body: "typed", base_hash: "h1" })), { body: "typed", base_hash: "h1" });
  assert.deepEqual(parseDraft("bare text from an older build"), { body: "bare text from an older build", base_hash: null });
  assert.deepEqual(parseDraft('{"body": 1}'), { body: '{"body": 1}', base_hash: null }, "not a draft's shape: the raw text");
  assert.deepEqual(parseDraft("[1]"), { body: "[1]", base_hash: null });
  const note = { body: "saved", hash: "h1" };
  assert.deepEqual(restoredDraft(null, note), { body: "saved", conflict: false, restored: false }, "no draft");
  assert.deepEqual(restoredDraft({ body: "saved", base_hash: "h0" }, note), { body: "saved", conflict: false, restored: false }, "a draft that says what the note says is none");
  assert.deepEqual(restoredDraft({ body: "typed", base_hash: "h1" }, note), { body: "typed", conflict: false, restored: true }, "typed against this text: restored, saved once typed into");
  // The window closed with a draft; an agent appended since. Before, the draft
  // autosaved against the fresh hash and erased the agent's block.
  assert.deepEqual(restoredDraft({ body: "typed", base_hash: "h0" }, note), { body: "typed", conflict: true, restored: true }, "typed against another text: the person decides");
  assert.deepEqual(restoredDraft({ body: "typed", base_hash: null }, note), { body: "typed", conflict: true, restored: true }, "an older build's draft is of unknown base: never a silent save");
});

test("a list read lands under the saves that landed while it was out, and a save never replaces a strictly newer row", () => {
  const read = { id: "n1", body: "older", updated_at: 10 };
  const saved = { id: "n1", body: "mine", updated_at: 11 };
  assert.equal(latestNote(read, saved), saved, "the read is from before the save");
  assert.equal(latestNote({ ...read, updated_at: 11 }, saved), saved, "a tie in a seconds clock goes to the save");
  assert.equal(latestNote({ ...read, body: "mine and the agent's block", updated_at: 12 }, saved).body, "mine and the agent's block", "a strictly newer read — the agent wrote since — stands");
  const rows = [read, { id: "n2", body: "other", updated_at: 5 }];
  assert.equal(landedNotes(rows, new Map()), rows, "nothing saved meanwhile: the same array");
  const landed = landedNotes(rows, new Map([["n1", saved]]));
  assert.deepEqual(landed.map((r) => r.body), ["mine", "other"], "the read from before the save never takes the row back");
});

test("the editor parks a draft with its hash, saves a restored one only once typed into, keeps the agent's block under the typing, and the overlay lands reads under saves", async () => {
  const { readFileSync } = await import("node:fs");
  const editor = readFileSync(new URL("./NoteEditor.tsx", import.meta.url), "utf8");
  assert.ok(editor.includes("readPref(webStorage(), draftKey(id), parseDraft, null)"), "a draft is read with its hash");
  assert.ok(editor.includes("writePref(webStorage(), draftKey(id), { body, base_hash });"), "and parked with it");
  assert.ok(editor.includes("restoredDraft(readDraft(note.id), note)") && editor.includes("restoredDraft(readDraft(note.id), { body: note.body, hash: note.hash })"), "the editor opens as the model says, on the first note and on each it moves to");
  const park = editor.slice(editor.indexOf("// Park the text and arm the timer."), editor.indexOf("// Flush once, on unmount"));
  assert.ok(park.includes("if (s.id !== note.id) return;"), "a draft is never parked under another note's key");
  assert.ok(park.includes("if (conflict || s.abandoned || !s.typed) return;"), "a restored draft is not saved until typed into");
  const save = editor.slice(editor.indexOf("const save = useCallback(async (): Promise<boolean> => {"), editor.indexOf("/** Save now and say whether the note is clean afterwards"));
  assert.ok(save.includes("s.conflicted = false;"), "a landed save settles a conflict flagged meanwhile — saving never stops for good in silence");
  assert.ok(editor.includes('if (outcome === "taken") setRestorable(s.confirmed.body);'), "a rewrite taken keeps the way back");
  assert.ok(editor.includes("(note.hash === s.hash && note.title === s.confirmed.title)"), "a title that moved alone is heard");
  for (const key of ["notes-note-editor-take-theirs", "notes-note-editor-keep-mine", "notes-note-editor-restore-my-version", "notes-note-editor-agent-rewrote-note", "notes-note-editor-draft-from-before"]) assert.ok(editor.includes(`t("${key}")`), key);
  assert.ok(!editor.includes("put-theirs-above-mine"), "the merge that repeated saved text is gone");
  const overlay = readFileSync(new URL("./NoteOverlay.tsx", import.meta.url), "utf8");
  assert.ok(overlay.includes("landedNotes(notes, savedSince.current)"), "a read lands under the saves that landed while it was out");
  assert.ok(overlay.includes("savedSince.current.set(next.id, next);") && overlay.includes("latestNote(n, next)"), "a save is remembered for a read in the air and never replaces a newer row");
  assert.ok(overlay.includes("e.payload.note !== active"), "a frame naming the open note reloads whatever the tab");
});

test("draft keys are per note and follow the composer's convention", () => {
  // Colons, like `bisa:draft:<scope>` — the same kind of thing, so it
  // matches its sibling rather than the dot-separated window preferences.
  assert.equal(draftKey("01NOTE"), "bisa:draft:note:01NOTE");
  assert.notEqual(draftKey("a"), draftKey("b"));
});

test("an unknown stored view lands on Write rather than on nothing", () => {
  // The preference outlives the vocabulary: a value written by a build that
  // had a fourth view must not leave the editor with no view selected.
  for (const id of NOTE_VIEWS) assert.equal(noteView(id), id);
  for (const junk of [null, undefined, "", "preview", "WRITE", "0"]) {
    assert.equal(noteView(junk), "write", `${junk}`);
  }
  assert.equal(NOTE_VIEWS[0], "write", "the fallback is the first option");
});

test("the autosave delay cannot be set low enough to fire between words", () => {
  // The floor is the whole point of clamping this one. A delay inside typing
  // rhythm turns every pause into a PATCH, a bus event and a re-render —
  // which is the request storm this feature was rewritten to stop.
  assert.equal(clampSaveDelay(0), SAVE_MIN_MS);
  assert.equal(clampSaveDelay(50), SAVE_MIN_MS);
  assert.equal(clampSaveDelay(-1000), SAVE_MIN_MS);
  assert.ok(SAVE_MIN_MS >= 300, "below 300ms a pause between two words saves");

  assert.equal(clampSaveDelay(999_999), SAVE_MAX_MS);
  assert.ok(SAVE_MAX_MS <= 5000, "past a few seconds 'Saved' stops being worth reading");

  // Junk from a hand-edited localStorage is the default, never a throw and
  // never NaN — `setTimeout(fn, NaN)` fires immediately, which would be the
  // storm with extra steps.
  for (const junk of [null, undefined, "", "soon", {}, []]) {
    assert.equal(clampSaveDelay(junk), SAVE_DEFAULT_MS, `${JSON.stringify(junk) ?? junk}`);
  }
  // A stored string is the ordinary case, not junk.
  assert.equal(clampSaveDelay("900"), 900);
  assert.ok(Number.isInteger(clampSaveDelay(700.6)));
});

test("the footer says one word, the heaviest first — conflict, error, saving, unsaved, saved — and a keystroke parks a draft only when it differs from what was saved", async () => {
  const { draftAction, noteStatusWords } = await import("./notesModel.mjs");
  assert.equal(noteStatusWords({ conflict: "An agent wrote to this note while you were editing.", error: "x", saving: true, dirty: true }), "An agent wrote to this note while you were editing.");
  assert.equal(noteStatusWords({ error: "could not save", saving: true, dirty: true }), "could not save");
  assert.equal(noteStatusWords({ saving: true, dirty: true }), "Saving…");
  assert.equal(noteStatusWords({ dirty: true }), "Unsaved");
  assert.equal(noteStatusWords({}), "Saved");
  assert.equal(draftAction("same", "same"), "forget");
  assert.equal(draftAction("same ", "same"), "park", "a trailing space is a draft");
  assert.equal(draftAction("", "was"), "park", "an emptied body is a draft too — the person meant it");
});

test("a note kept from the last window that no longer exists opens nothing", () => {
  assert.equal(listedNote("n1", "n1", ["n0", "n1"]), "n1", "still listed: it opens");
  assert.equal(listedNote("n1", "n1", ["n0"]), null, "gone while the app was closed");
  assert.equal(listedNote("n1", "n1", []), null);
});

test("a note opened in this window is left to the list that is about to hold it", () => {
  assert.equal(listedNote("n2", "n1", ["n0"]), "n2", "another note was opened since");
  assert.equal(listedNote("n2", null, []), "n2", "nothing came back from the last window");
  assert.equal(listedNote(null, "n1", ["n0"]), null, "nothing open is nothing open");
});

test("a note's refusal is read by its status: a 409 is a lost save, a 404 a note that is gone, and nothing is matched by words", async () => {
  const { noteGoneWords, noteRefusal, noteStatusWords } = await import("./notesModel.mjs");
  assert.equal(noteRefusal(409), "conflict");
  assert.equal(noteRefusal(404), "gone", "deleted under the editor — the node answers 404 on GET, PATCH and DELETE");
  assert.equal(noteRefusal(400), "failed", "a 400 is an input the node refused, never a note that is gone");
  assert.equal(noteRefusal(500), "failed");
  assert.equal(noteRefusal(0), "failed");
  assert.equal(noteRefusal(null), "failed");
  assert.equal(noteGoneWords("unknown note 01ARZ"), "This note is gone — unknown note 01ARZ. What you wrote stays here to copy; nothing more is saved to it.");
  assert.equal(noteStatusWords({ gone: "This note is gone — x.", conflict: "c", error: "e", saving: true, dirty: true }), "This note is gone — x.", "said before everything else");
  // The editor: a save, a re-read and a delete each ask the model; none matches a status number for *gone*, none a word.
  const { readFileSync } = await import("node:fs");
  const editor = readFileSync(new URL("./NoteEditor.tsx", import.meta.url), "utf8");
  assert.equal(editor.split('noteRefusal(e.status) === "gone"').length - 1, 2, "the re-read and the delete");
  assert.ok(editor.includes('const refusal = e instanceof ApiError ? noteRefusal(e.status) : "failed";'), "the save");
  assert.ok(!editor.includes("status === 400") && !editor.includes("status === 404") && !/not found/i.test(editor.replace(/\/\/.*$/gm, "")), "no old status and no words of a refusal");
  assert.ok(editor.includes("s.abandoned = true;\n          setGone("), "a note that is gone is saved to no more");
  assert.ok(editor.includes("dirtyRef.current && !live.current.abandoned"), "and holds nobody back at the leave guard");
  const remove = editor.slice(editor.indexOf("const remove = async () => {"), editor.indexOf("return (", editor.indexOf("const remove = async () => {")));
  assert.ok(remove.includes('noteRefusal(e.status) === "gone"') && remove.indexOf("onDeleted();", remove.indexOf('=== "gone"')) > 0, "a delete of a note already gone leaves, as a delete that happened");
});


test("a save that lands after the panel moved to another note writes nothing of the note it left onto the one on screen", async () => {
  const { readFileSync } = await import("node:fs");
  const editor = readFileSync(new URL("./NoteEditor.tsx", import.meta.url), "utf8");
  const save = editor.slice(editor.indexOf("const save = useCallback(async (): Promise<boolean> => {"), editor.indexOf("/** Save now and say whether the note is clean afterwards"));
  assert.ok(save.includes("const sending = { id: s.id, title: s.title, body: s.body };"), "the save remembers which note it is about");
  assert.ok(save.includes("await api.patchNote(sending.id,") && save.includes("forgetPref(webStorage(), draftKey(sending.id));"), "the note it was sent for, and that note's draft");
  const landed = save.slice(save.indexOf("await api.patchNote("), save.indexOf("} catch (e) {"));
  assert.ok(landed.indexOf("if (s.id !== sending.id) return false;") < landed.indexOf("s.hash = next.hash;"), "the left note's hash is never the baseline of the one on screen — that made the next save a conflict nobody caused");
  const refused = save.slice(save.indexOf("} catch (e) {"), save.indexOf("} finally {"));
  assert.ok(refused.indexOf("if (s.id !== sending.id) return false;") < refused.indexOf("s.conflicted = true;"), "nor its refusal the other note's conflict");
});
