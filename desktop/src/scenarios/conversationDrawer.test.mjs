/**
 * One conversation experience (13 — Conversations): the Notes and Draw
 * editors mount the one `ConversationDrawer` — resizable, its width
 * remembered, its pick kept per note or drawing — every host paints the one
 * surface and starts a conversation in one click and untitled, the thread's
 * composer carries the IDE's addressee pill and *Stop*, and the empty state
 * is one component whose button says what the list's foot says. Source-text
 * guards, like `draw.test.mjs`.
 */
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("both editors mount the shared drawer, about their own record, each remembering its own width — and the one-shot ask box is gone", () => {
  const draw = read("draw/DrawEditor.tsx");
  const note = read("notes/NoteEditor.tsx");
  assert.ok(draw.includes("<ConversationDrawer") && draw.includes('origin={{ kind: "drawing", id: detail.id }}') && draw.includes('widthKey="bisa.draw.askwidth"'));
  assert.ok(note.includes("<ConversationDrawer") && note.includes('origin={{ kind: "note", id: note.id }}') && note.includes('widthKey="bisa.notes.askwidth"'));
  assert.ok(note.includes("setNotesAskOpen(!askOpen)") && note.includes('aria-pressed={askOpen}'), "the header's toggle, as Draw's");
  const everything = readdirSync(src, { recursive: true })
    .filter((f) => /\.(tsx?|mjs)$/.test(f) && !f.includes("node_modules"))
    .map((f) => [f, readFileSync(join(src, f), "utf8")]);
  for (const [file, text] of everything) {
    if (file.endsWith("conversationDrawer.test.mjs")) continue;
    assert.ok(!/\bNoteAsk\b|api\.askNote\(|\bagentAsk\b|\/notes\/\$\{[a-z]+\}\/ask/.test(text), `${file}: the one-shot ask box is gone`);
  }
});

test("the drawer resizes by its left edge, keeps its pick per note or drawing, and starts in one click", () => {
  const drawer = read("views/_studio/ConversationDrawer.tsx");
  assert.ok(drawer.includes('<ResizeHandle side="left"') && drawer.includes("useStoredSize(widthKey, DRAWER_DEFAULT_WIDTH"), "a remembered, draggable width");
  assert.ok(drawer.includes('useConversationSurface({ owner: origin }, "kept")'), "the pick is the owner's, remembered — never the screen's address");
  assert.ok(drawer.includes("<ConversationSurface surface={c}"), "the drawer paints the one surface every owner has");
  const surface = read("views/_studio/ConversationSurface.tsx");
  assert.ok(surface.includes("onNew={c.start}") && surface.includes("<NoConversation title={c.words.empty} hint={hint} onNew={c.start} />"), "one click starts one, from the list's foot or the empty state");
  assert.ok(surface.includes("<ConversationThread") && surface.includes("<ConversationList"), "the same list and thread every owner has");
});

test("the thread's composer has the IDE's addressee pill and Stop, from the roster", () => {
  const thread = read("views/_studio/ConversationThread.tsx");
  assert.ok(thread.includes("addressee={addresseeChip}") && thread.includes("stop={stop}"), "passed to the one Conversation");
  assert.ok(thread.includes("turnSummary(sessionsOf(sessionRows, row.id))") && thread.includes("stopSession(stoppable.id)"), "Stop names the roster's stoppable turn");
  assert.ok(thread.includes("openDrawing(origin.id)") && thread.includes("openNote(origin.id)"), "the origin chip opens the drawing or the note");
});

test("the empty state is one component, drawn by the one surface every host paints, and its button says New conversation", () => {
  const empty = read("views/_studio/NoConversation.tsx");
  assert.ok(empty.includes('t("studio-conversations-bar-new-conversation")') && !empty.includes("studio-no-conversation-start"), "one label for one act");
  assert.ok(read("views/_studio/ConversationSurface.tsx").includes('from "./NoConversation"'), "the surface draws it");
  for (const file of ["views/_workbench/AgentPane.tsx", "views/_workbench/AgentModeCenter.tsx", "views/_studio/ConversationDrawer.tsx", "views/_workflow/WorkflowAgentPane.tsx", "views/_goal/GoalConversationPane.tsx"]) {
    const text = read(file);
    assert.ok(text.includes("<ConversationSurface") && !text.includes("NoConversation"), `${file} paints the surface and nothing of its own`);
  }
});
