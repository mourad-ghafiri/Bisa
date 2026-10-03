/**
 * Draw, as the desktop draws it (19 — Drawings) — source-text facts that
 * hold the shape across the files: the overlay and the bridge are mounted
 * outside the routed screen; the panel is a pane at `z-40` that yields to the
 * native layer when maximized; the canvas is reached through the kit's third
 * entry, with its assets pointed home and the image tool off; an agent's
 * skeleton keeps its ids and a snapshot rides as an upload; the footer, the
 * keymap and Settings carry the feature. Run with
 * `node --test desktop/src/scenarios/draw.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { COMMANDS } from "../shell/keymapModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(src, "..", "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const readRoot = (rel) => readFileSync(join(root, rel), "utf8");

test("the overlay and the bridge are mounted in App.tsx outside the routed screen, and the content box is published for maximize", () => {
  const app = read("App.tsx");
  const screen = app.indexOf("<Screen route={route} />");
  for (const mount of ["<DrawOverlay />", "<DrawPanel />"]) {
    const at = app.indexOf(mount);
    assert.ok(at > screen, `${mount} is mounted, and not inside the screen`);
  }
  assert.ok(app.includes('OverlayBoundary name="draw panel"'), "the panel has its own boundary");
  assert.ok(app.includes("publishContentBox(el.getBoundingClientRect())"), "the content column's box is published from the one observer");
  assert.ok(app.includes("LAYOUT_CHANGED, onLayout"), "and re-read when the layout moves");
});

test("the panel is a pane at z-40 on the shared maximized frame, and Escape asks the canvas first", () => {
  const overlay = read("draw/DrawOverlay.tsx");
  assert.ok(overlay.includes("data-pane") && overlay.includes("z-40") && overlay.includes("bg-bg"), "a pane, under every dialog, laying its own ground");
  // The frame itself — the content box, the surface, the fixed style — is `shell/maximizedPanel.ts`'s, guarded in `scenarios/maximize.test.mjs`.
  assert.ok(overlay.includes("useMaximizedPanel(open, maximized)"), "maximized on the frame Notes shares");
  assert.ok(overlay.includes("restoreOnEscape(canvasApi.current?.getAppState()"), "Escape restores only when the canvas has nothing to cancel");
  assert.ok(overlay.includes("createPortal(body, document.body)"));
  assert.ok(overlay.includes('<RepoStrip repo={api.drawingsRepo} subject="Drawings" pulls={false}'), "the repository strip, with no pull");
  assert.ok(overlay.includes("<NewDrawingDialog") && !overlay.includes("newMenuItems"), "New is a dialog, never a menu of places × templates");
  const dialog = read("draw/NewDrawingDialog.tsx");
  assert.ok(dialog.includes("<Tile") && dialog.includes("<TemplatePreview") && dialog.includes('role="radiogroup"'), "a gallery of tiles with previews");
  assert.ok(dialog.includes("titleFollowsTemplate(") && dialog.includes("targets.length > 1 &&"), "the title follows the template; Where only when several places are offered");
  assert.ok(!read("draw/TemplatePreview.tsx").includes("excalidraw"), "a preview never loads the canvas");
});

test("the canvas is the kit's third entry, pointed at this app's assets, with the image tool off and no outside door", () => {
  const entry = read("ui/excalidraw.ts");
  assert.ok(entry.indexOf("EXCALIDRAW_ASSET_PATH") < entry.indexOf('import("@excalidraw/excalidraw")'), "assets pointed home before the module loads");
  assert.ok(entry.includes("/excalidraw/"), "the app's own path");
  const canvas = read("draw/ExcalidrawCanvas.tsx");
  assert.ok(canvas.includes("image: false"), "vector only");
  assert.ok(canvas.includes("loadScene: false") && canvas.includes("saveToActiveFile: false") && canvas.includes("export: false"), "nothing leaves from the canvas's menu");
  assert.ok(!canvas.includes("WelcomeScreen") && !canvas.includes("LiveCollaboration"), "no welcome screen, no collaboration door");
  assert.ok(read("ui/excalidraw.css").includes(".library-menu-browse-button"), "the public library site is not offered");
  assert.ok(!read("ui/index.ts").toLowerCase().includes("excalidraw"), "the kit index carries no canvas");
  const pkg = JSON.parse(readRoot("desktop/package.json"));
  assert.equal(pkg.scripts.predev, "node scripts/sync-excalidraw-assets.mjs");
  assert.equal(pkg.scripts.prebuild, "node scripts/sync-excalidraw-assets.mjs");
  assert.ok(readRoot(".gitignore").includes("desktop/public/excalidraw/"), "the copied fonts are not committed");
});

test("the bridge keeps an agent's ids, saves through the guarded PATCH, and uploads a snapshot rather than sending bytes", () => {
  const bridge = read("draw/drawBridge.ts");
  assert.ok(bridge.includes("regenerateIds: false"), "an agent's ids are kept");
  assert.ok(bridge.includes("api.patchDrawing(detail.id, { scene: { elements, app_state }, base_hash: base })"), "the same guarded save the editor uses");
  assert.ok(bridge.includes("api.uploadAttachment(file, AbortSignal.timeout(UPLOAD_MS))"), "a snapshot is an upload");
  assert.ok(bridge.includes("api.answerDrawingRequest(pending.id, result)"), "and every request is answered");
  assert.ok(bridge.includes("liveSceneFor(detail.id)") && bridge.includes("offscreenApi()"), "the live canvas when the drawing is open, the offscreen one otherwise");
  const panel = read("draw/DrawPanel.tsx");
  assert.ok(panel.includes("DRAW_PRESENCE_MS") && panel.includes(".drawingRequests()"), "the presence read is the desktop's heartbeat");
  assert.ok(panel.includes('e.payload.type === "drawing_request"'), "and each frame is heard");
  const offscreen = read("draw/OffscreenScene.tsx");
  assert.ok(offscreen.includes('visibility: "hidden"') && !offscreen.includes('display: "none"'), "the offscreen canvas has a laid-out box");
});

test("the editor saves against the store's hash, adopts theirs on a conflict, and hears its own drawing's changes", () => {
  const editor = read("draw/DrawEditor.tsx");
  assert.ok(editor.includes("base_hash: lastSaved(s.id)?.hash ?? s.auto.savedHash"), "the hash the store last gave");
  assert.ok(editor.includes("saveConflicted") && editor.includes("CaptureUpdateAction.NEVER"), "a 409 freezes saving; adopting theirs is not undoable");
  assert.ok(editor.includes('e.payload.type !== "drawing_changed" || e.payload.drawing !== live.current.id'), "the editor hears its own drawing");
  assert.ok(editor.includes("reloadDecision("), "and decides with the model");
  assert.ok(editor.includes('origin={{ kind: "drawing", id: detail.id }}') && editor.includes("<ConversationDrawer"), "the Ask drawer is the shared conversation drawer, about the drawing");
});

test("the footer, the keymap, Settings and the wire carry the feature", () => {
  const footer = read("shell/StatusBar.tsx");
  assert.ok(footer.includes("<DrawStat />"), "the footer draws the one Draw switch");
  const stat = read("shell/DrawStat.tsx");
  assert.ok(stat.includes("icon={ICON.draw}") && stat.includes("setDrawDockVisible(!dockVisible)"), "the switch shows or hides the dock");
  assert.ok(stat.includes("useBusyDrawings()") && stat.includes("<WorkingDot"), "and wears a dot while an agent draws");
  const command = COMMANDS.find((c) => c.id === "toggle_draw");
  assert.ok(command, "toggle_draw is a keymap command");
  assert.equal(command.chords.default, "Mod+Alt+D");
  assert.ok(read("shell/shortcuts.ts").includes('case "toggle_draw":'), "and the handler answers it");
  const settings = read("views/Settings.tsx");
  // The rail is `settingsLink.mjs`'s; the screen draws the panel under the tab's id.
  assert.ok(read("views/_settings/settingsLink.mjs").includes('panel("draw", t("screens-settings-draw"),'), "the rail has the Draw panel");
  assert.ok(settings.includes('panel.id === "draw" &&') && settings.includes('<RegistryPanel group="draw" />') && settings.includes("<DrawPanel />"), "Settings draws both panels under it");
  const hand = read("types.hand.ts");
  assert.ok(hand.includes('type: "drawing_changed"') && hand.includes('type: "drawing_request"'), "both frames are typed");
  assert.ok(read("activity.ts").includes("drawing_changed: true") && read("activity.ts").includes("drawing_request: true"), "and the exhaustiveness map knows them");
  const noteOverlay = read("notes/NoteOverlay.tsx");
  assert.ok(noteOverlay.includes('<RepoStrip repo={api.notesRepo} subject="Notes" pulls'), "the notes overlay draws the same strip");
});

test("the panel's × is there whatever is open — the list, a note, a drawing — and an agent is asked with the agent glyph", () => {
  // The × closes the panel and keeps the open record the open one (`setNotesOpen(false)` / `setDrawOpen(false)`, through
  // the leave guard); Back is what returns to the list. Both live in every header, so the way out never moves.
  for (const [file, close, label] of [
    ["notes/NoteOverlay.tsx", "setNotesOpen(false)", "notes-note-overlay-close-notes"],
    ["notes/NoteEditor.tsx", "setNotesOpen(false)", "notes-note-overlay-close-notes"],
    ["draw/DrawOverlay.tsx", "setDrawOpen(false)", "draw-overlay-close"],
    ["draw/DrawEditor.tsx", "setDrawOpen(false)", "draw-overlay-close"],
  ]) {
    const src = read(file);
    assert.ok(src.includes(`onClick={() => ${close}}`) && src.includes(`aria-label={${file.endsWith("Editor.tsx") ? "t" : "tr"}("${label}")}`), `${file}: the panel's ×`);
  }
  for (const file of ["notes/NoteEditor.tsx", "draw/DrawEditor.tsx"]) {
    const src = read(file);
    assert.ok(src.includes("<ICON.agent size={14} aria-hidden />") && !src.includes("ICON.dm"), `${file}: Ask an agent wears the agent glyph, not a message bubble`);
  }
});

test("a note or a drawing is deleted from its list row, asked first in the editor's own words", () => {
  // The trash is the row's sibling, never nested in the row's button, and shows on hover or focus (`row-actions`);
  // it asks in the danger dialog the editor asks in, then calls the same delete, and a record already gone just leaves.
  for (const [file, del, label, words, gone] of [
    ["notes/NoteOverlay.tsx", "api.deleteNote(n.id)", "notes-note-overlay-delete-note", "notes-note-editor-delete-title", 'noteRefusal(e.status) === "gone"'],
    ["draw/DrawOverlay.tsx", "api.deleteDrawing(r.id)", "draw-overlay-delete-drawing", "draw-editor-delete-title", "e.status === 404"],
  ]) {
    const src = read(file);
    assert.ok(src.includes('className="group relative"') && src.includes("row-actions anim absolute"), `${file}: the row holds its trash beside the row's button`);
    assert.ok(src.includes(`aria-label={tr("${label}"`) && src.includes("<ICON.delete size={13} aria-hidden />"), `${file}: the trash names the record it deletes`);
    assert.ok(src.includes("<ConfirmDialog") && src.includes(`tr("${words}"`) && /confirmLabel=\{[^}]+\}\s+danger/.test(src), `${file}: asked first, in a danger dialog`);
    assert.ok(src.includes(del) && src.includes(gone) && src.includes("sayFailure("), `${file}: the same delete, gone counts as done, a failure said through the kit`);
  }
});
