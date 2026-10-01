/**
 * Saving on purpose, leaving with unsaved work, deleting on purpose (ide/03
 * §Tabs, for the Notes and Draw editors): one leave guard both panel stores
 * own, one *Save · Don't save · Cancel* dialog the IDE's tabs draw too, a
 * Save button and ⌘S in each editor, a confirmed Delete. Source-text guards,
 * like `draw.test.mjs`.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

test("every way out of an open note or drawing goes through the store's leave guard", () => {
  for (const [file, guard, clear, open, show] of [
    ["notes/notesStore.ts", "notesGuard", "clearActiveNote", "openNote", "setNotesOpen"],
    ["draw/drawStore.ts", "drawGuard", "clearActiveDrawing", "openDrawing", "setDrawOpen"],
  ]) {
    const store = read(file);
    assert.ok(store.includes(`export const ${guard} = createLeaveGuard(`), `${file}: one guard`);
    for (const fn of [clear, open, show]) {
      const body = store.slice(store.indexOf(`export function ${fn}(`));
      assert.ok(body.slice(0, body.indexOf("\n}")).includes(`${guard}.leave(`), `${file}: ${fn} asks the guard`);
    }
  }
});

test("each editor holds its document, saves on the button and on the keymap's save, and asks before Delete", () => {
  for (const [file, guard, kind] of [
    ["notes/NoteEditor.tsx", "notesGuard", "note"],
    ["draw/DrawEditor.tsx", "drawGuard", "drawing"],
  ]) {
    const editor = read(file);
    assert.ok(editor.includes(`useDocumentHold(${guard}, "${kind}",`), `${file}: the hold is the guard's and the quit question's`);
    assert.ok(editor.includes(`data-${kind === "note" ? "note" : "draw"}-editor`) && editor.includes("if (isSaveChord(e)) {"), `${file}: ⌘S saves this document`);
    assert.ok(editor.includes("disabled={!dirty || saving || conflict !== null} onClick={() => void saveNow()}"), `${file}: a Save button, live only with something to save`);
    assert.ok(editor.includes("<ConfirmDialog") && editor.includes("danger\n"), `${file}: Delete asks first, in red`);
    assert.ok(editor.includes("const discard = useCallback(") && editor.includes("discard();"), `${file}: nothing is saved at a record being deleted or let go of`);
  }
});

test("the question is one dialog — the IDE's tabs, the notes panel and the Draw panel draw it", () => {
  const dialog = read("shell/UnsavedDialog.tsx");
  assert.equal((dialog.match(/<Button\b/g) ?? []).length, 3, "Cancel · Don't save · Save");
  assert.ok(dialog.includes("GUARD_VERBS.cancel") && dialog.includes("GUARD_VERBS.discard") && dialog.includes("GUARD_VERBS.save"), "the platform's own words");
  for (const file of ["views/Workbench.tsx", "notes/NoteOverlay.tsx", "draw/DrawOverlay.tsx"]) {
    assert.ok(read(file).includes("<UnsavedDialog"), `${file} draws it`);
  }
  assert.ok(!read("views/Workbench.tsx").includes("GUARD_VERBS"), "the workbench no longer spells the footer itself");
  const guard = read("shell/documentGuard.ts");
  assert.ok(guard.includes("registerDirtySource(key, next)"), "a hold is a dirty source: the quit question counts it and the close flow saves it");
  assert.ok(guard.includes("if (clean) go();"), "Save proceeds only once the hold saved clean");
});
