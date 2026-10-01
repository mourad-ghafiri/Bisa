/**
 * A floating panel's maximize (guide/the-desktop § Notes and § Draw): the
 * Notes and Draw overlays fill the content column — everything but the
 * header, the footer and the sidebar — on one shared frame,
 * `shell/maximizedPanel.ts`, and wear one shared button,
 * `shell/MaximizeToggle.tsx`. Source-text guards, like `draw.test.mjs`: the
 * rule is that the mechanism is written once, and each panel only says
 * "maximized" and hands over its toggle.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

const OVERLAYS = ["notes/NoteOverlay.tsx", "draw/DrawOverlay.tsx"];
const EDITORS = ["notes/NoteEditor.tsx", "draw/DrawEditor.tsx"];
const SETTINGS = ["views/_settings/NotesPanel.tsx", "views/_settings/DrawPanel.tsx"];

test("the frame is one hook over the content box, a surface while it stands, and a fixed style", () => {
  const hook = read("shell/maximizedPanel.ts");
  assert.ok(hook.includes("useContentBox()"), "the box is the content column's, published from App.tsx");
  assert.ok(hook.includes("useSurface(open && maximized)"), "maximized, the panel is a surface the native layer yields to");
  assert.ok(hook.includes("maximizedStyle(box)"), "the rectangle becomes the panel's fixed style");
  assert.ok(hook.includes("export function useMaximizedPanel(open: boolean, maximized: boolean)"));
  for (const file of OVERLAYS) {
    const overlay = read(file);
    assert.ok(overlay.includes("useMaximizedPanel(open, maximized)"), `${file} takes the frame from the hook`);
    assert.ok(!overlay.includes("useSurface(") && !overlay.includes("useContentBox(") && !overlay.includes("maximizedStyle("), `${file} does not rebuild the frame itself`);
  }
});

test("each overlay wears the frame the same way: fixed and square when maximized, a corner card with handles otherwise", () => {
  for (const file of OVERLAYS) {
    const overlay = read(file);
    assert.ok(overlay.includes('data-maximized={fixed ? "true" : undefined}'), `${file} says so on the pane`);
    assert.ok(overlay.includes("style={fixed ?? { width, height }}"), `${file}: the fixed style, else the remembered size`);
    assert.ok(overlay.includes('fixed ? "fixed rounded-none" : "fixed right-4 bottom-20'), `${file}: square at the box, a card in the corner`);
    assert.equal(overlay.split("{!fixed && <ResizeHandle").length - 1, 2, `${file}: neither handle is drawn while maximized`);
    assert.ok(overlay.includes("<MaximizeToggle maximized={maximized}"), `${file}: the shared button in the list header`);
  }
  for (const file of EDITORS) {
    assert.ok(read(file).includes("<MaximizeToggle maximized={maximized}"), `${file}: the shared button in the editor header`);
  }
});

test("the button is one component with a pressed state and the shared words", () => {
  const toggle = read("shell/MaximizeToggle.tsx");
  assert.ok(toggle.includes("aria-pressed={maximized}"));
  assert.ok(toggle.includes('t("shell-panel-restore")') && toggle.includes('t("shell-panel-maximize")'), "one pair of words for both panels");
  assert.ok(toggle.includes("ICON.collapse") && toggle.includes("ICON.expand"));
  assert.ok(!read("ui/index.ts").includes("MaximizeToggle"), "chrome, not kit: the toggle is not on the kit index");
  for (const file of [...OVERLAYS, ...EDITORS]) {
    assert.ok(!read(file).includes("ICON.expand"), `${file} does not draw its own maximize button`);
  }
});

test("Escape restores the notes panel unless the find bar or the search took the key first", () => {
  const overlay = read("notes/NoteOverlay.tsx");
  assert.ok(overlay.includes('if (e.key !== "Escape" || !maximized || e.defaultPrevented) return;'), "yield to whatever handled Escape");
  assert.ok(overlay.includes("setNotesMaximized(false)"), "then restore");
  assert.ok(overlay.includes("onKeyDown={onPanelKey}"), "listened on the pane itself");
  // Those two prevent the default when they take Escape — the reason the rule above is enough.
  assert.ok(read("ui/find/FindBar.tsx").includes('if (e.key === "Escape") {\n      e.preventDefault();'), "the find bar claims its Escape");
  assert.ok(overlay.includes('if (e.key === "Escape" && query) {\n                          e.preventDefault();'), "the search box claims its Escape while it holds a query");
});

test("the flag is stored per viewer, off by default, and Settings offers to open maximized", () => {
  const store = read("notes/notesStore.ts");
  assert.ok(store.includes('const MAXIMIZED_KEY = "bisa.notes.maximized";'), "a lowercase key, so the docs guard sees it");
  assert.ok(store.includes('readPref(webStorage(), MAXIMIZED_KEY, (raw) => raw === "1", false)'), "off until a hand asks");
  assert.ok(store.includes("export function setNotesMaximized(on: boolean)") && store.includes("export function toggleNotesMaximized()"));
  for (const file of SETTINGS) {
    const panel = read(file);
    assert.ok(/onChange=\{set(Notes|Draw)Maximized\}/.test(panel), `${file}: the switch drives the store`);
    assert.ok(/settings-(notes|draw)-panel-open-maximized/.test(panel), `${file}: labelled Open maximized`);
  }
});
