import test from "node:test";
import assert from "node:assert/strict";
import { COMMANDS, WHENS, bindingsIn, canonicalChord, chordFor, chordFromEvent, commandForEvent, conflictFor, filterBindings, interceptsInTerminal, keymapMarkdown, matchesEvent, parseChord, relayChords, resolveKeymap } from "./keymapModel.mjs";

const ev = (key, m = {}) => ({ key, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...m });

test("chords parse, canonicalise and match events on both platforms", () => {
  assert.deepEqual(parseChord("shift+mod+p"), { mod: true, ctrl: false, shift: true, alt: false, key: "P" });
  assert.equal(canonicalChord("shift+mod+p"), "Mod+Shift+P");
  assert.equal(canonicalChord("Mod+,"), "Mod+Comma");
  assert.equal(parseChord("Mod+A+B"), null, "two keys is not a chord");
  assert.equal(parseChord("Mod+Shift"), null, "a chord needs a key");
  assert.ok(matchesEvent(ev("p", { metaKey: true, shiftKey: true }), "Mod+Shift+P", true));
  assert.ok(matchesEvent(ev("p", { ctrlKey: true, shiftKey: true }), "Mod+Shift+P", false));
  assert.ok(!matchesEvent(ev("p", { ctrlKey: true, shiftKey: true }), "Mod+Shift+P", true), "Ctrl is not Mod on a Mac");
  assert.ok(matchesEvent(ev("`", { ctrlKey: true }), "Ctrl+Backquote", true));
  assert.ok(matchesEvent(ev("ArrowLeft", { altKey: true }), "Alt+Left", true));
  assert.equal(chordFromEvent(ev("s", { metaKey: true }), true), "Mod+S");
  assert.equal(chordFromEvent(ev("Shift", { shiftKey: true }), true), null, "a bare modifier is not a chord");
  assert.equal(chordFromEvent(ev("a"), true), null, "typing is not a chord");
  assert.equal(chordFromEvent(ev("Enter"), true), "Enter", "Enter is never typing in a tree");
  assert.equal(chordFromEvent(ev("F2"), true), "F2");
  assert.equal(chordFromEvent(ev("Delete"), false), "Delete");
  assert.equal(chordFromEvent(ev("Backspace", { metaKey: true }), true), "Mod+Backspace");
  assert.equal(chordFromEvent(ev("Tab", { ctrlKey: true, shiftKey: true }), true), "Ctrl+Shift+Tab");
  assert.equal(chordFromEvent(ev("Escape"), true), null, "Escape cancels a recording; it is never recorded");
});

test("off a Mac an explicit Ctrl chord is the Mod chord — Ctrl+Backquote and Ctrl+Tab fire there too", () => {
  assert.ok(matchesEvent(ev("`", { ctrlKey: true }), "Ctrl+Backquote", false));
  assert.ok(matchesEvent(ev("Tab", { ctrlKey: true }), "Ctrl+Tab", false));
  assert.ok(matchesEvent(ev("Tab", { ctrlKey: true }), "Ctrl+Tab", true));
  assert.ok(!matchesEvent(ev("Tab", { metaKey: true }), "Ctrl+Tab", true), "⌘Tab is the OS's, not Ctrl+Tab");
  assert.ok(matchesEvent(ev("g", { ctrlKey: true, shiftKey: true }), "Ctrl+Shift+G", false));
});

test("the default preset has no conflicts, and overrides sit on top", () => {
  const km = resolveKeymap("default", null);
  assert.deepEqual(km.warnings, []);
  assert.equal(chordFor(km, "quick_open"), "Mod+P");
  assert.equal(chordFor(km, "inbox"), "Mod+Shift+A");
  // The occupants each have a chord, and none collides with a browser-owned one.
  assert.equal(chordFor(km, "panel_agents"), "Mod+Shift+M");
  assert.equal(chordFor(km, "panel_about"), "Mod+Shift+D");
  assert.equal(chordFor(km, "panel_workstreams"), "Mod+Shift+U");
  assert.equal(chordFor(km, "attach_selection"), "Mod+Shift+A", "the same chord in two scopes is the design, not a conflict");
  const over = resolveKeymap("default", { settings: "Mod+Shift+Comma", inbox: "" });
  assert.equal(chordFor(over, "settings"), "Mod+Shift+Comma");
  assert.equal(over.bindings.find((b) => b.id === "settings").source, "override");
  assert.equal(chordFor(over, "inbox"), null, "an empty override unbinds");
  const bad = resolveKeymap("default", { settings: 7 });
  assert.equal(chordFor(bad, "settings"), "Mod+Comma");
  assert.equal(bad.warnings.length, 1);
  assert.equal(resolveKeymap("nonsense", null).preset, "default");
  assert.equal(chordFor(resolveKeymap("vscode", null), "git_panel"), "Ctrl+Shift+G");
  assert.equal(chordFor(resolveKeymap("vim", null), "omnibox"), "Mod+K", "vim keeps the app-level chords");
});

test("the Triggers occupant took its command with it: 86 named commands and the eight numbered tabs, and ⌘⇧Y is nobody's", () => {
  assert.equal(COMMANDS.length, 94, "86 named commands, and tab_1 … tab_8 from one row");
  assert.equal(COMMANDS.filter((c) => /^tab_[1-8]$/.test(c.id)).length, 8);
  assert.ok(!COMMANDS.some((c) => c.id === "panel_triggers"), "no Triggers panel to show");
  const km = resolveKeymap("default", null);
  assert.equal(chordFor(km, "panel_triggers"), null);
  assert.equal(commandForEvent(km, ev("y", { metaKey: true, shiftKey: true }), ["workbench"], true), null, "⌘⇧Y is free in a root");
  assert.equal(conflictFor(km, "panel_files", "Mod+Shift+Y"), null, "and free to rebind");
});

test("a conflicting override is refused: the later binding is dropped and named", () => {
  const km = resolveKeymap("default", { new_goal: "Mod+K" });
  assert.equal(chordFor(km, "omnibox"), "Mod+K", "the earlier command keeps it");
  assert.equal(chordFor(km, "new_goal"), null);
  assert.ok(km.warnings.some((w) => w.includes("omnibox") && w.includes("new_goal")), km.warnings.join("; "));
  const clean = resolveKeymap("default", null);
  assert.equal(conflictFor(clean, "new_goal", "Mod+K"), "omnibox");
  assert.equal(conflictFor(clean, "new_goal", "Mod+Shift+9"), null);
  assert.equal(conflictFor(clean, "attach_selection", "Mod+Shift+A"), "inbox", "a global chord collides with a scoped one");
});

test("the narrowest live scope wins for one chord", () => {
  const km = resolveKeymap("default", null);
  const e = ev("a", { metaKey: true, shiftKey: true });
  assert.equal(commandForEvent(km, e, [], true), "inbox");
  assert.equal(commandForEvent(km, e, ["workbench", "editor"], true), "attach_selection");
  assert.equal(commandForEvent(km, ev("m", { metaKey: true, shiftKey: true }), [], true), null, "workbench-only outside one");
  assert.equal(commandForEvent(km, ev("m", { metaKey: true, shiftKey: true }), ["workbench"], true), "panel_agents");
  assert.equal(commandForEvent(km, ev("j", { ctrlKey: true }), [], false), null, "a terminal needs a root");
  assert.equal(commandForEvent(km, ev("j", { ctrlKey: true }), ["workbench"], false), "show_terminal");
});

test("the reference page lists every command with both presets, one table per scope", () => {
  const md = keymapMarkdown();
  for (const c of COMMANDS) assert.ok(md.includes("`" + c.id + "`"), c.id);
  for (const w of WHENS) assert.ok(md.includes(`## ${w}`), w);
  assert.ok(md.startsWith("# Keymap"));
  assert.ok(!md.includes("just gen-keymap-docs"), "the generator is run with node; just is not a dependency");
});

test("every command's scope is a registered one, and the files, tabs and search commands are bound", () => {
  for (const c of COMMANDS) assert.ok(WHENS.includes(c.when), `${c.id}: ${c.when}`);
  const km = resolveKeymap("default", null);
  assert.deepEqual(km.warnings, []);
  assert.deepEqual(resolveKeymap("vscode", null).warnings, []);
  assert.equal(chordFor(km, "rename_entry"), "Enter", "Finder's rule: Enter renames; a click already opens");
  assert.equal(chordFor(resolveKeymap("vscode", null), "rename_entry"), "F2");
  assert.equal(chordFor(km, "delete_entry"), "Mod+Backspace");
  assert.equal(chordFor(km, "copy_entry"), "Mod+C");
  assert.equal(chordFor(km, "paste_entry"), "Mod+V");
  assert.equal(chordFor(km, "search_files"), "Mod+Shift+F");
  assert.equal(chordFor(km, "search_history"), "Mod+Shift+H");
  assert.equal(chordFor(km, "next_tab"), "Ctrl+Tab");
  assert.equal(chordFor(km, "import_project"), "Mod+Shift+O");
  assert.equal(bindingsIn(km, "files").length, 10);
  assert.equal(chordFor(km, "open_entry"), "Space");
  assert.equal(chordFor(resolveKeymap("vscode", null), "open_entry"), "Enter", "VS Code's hands: Enter opens, F2 renames");
  assert.equal(chordFromEvent(ev(" "), true), "Space");
  assert.ok(bindingsIn(km, "files").every((b) => b.when === "files"));
  assert.equal(filterBindings(km.bindings, "").length, km.bindings.length);
  assert.deepEqual(filterBindings(km.bindings, "rename").map((b) => b.id), ["rename", "rename_entry"]);
  assert.ok(filterBindings(km.bindings, "mod+shift+f").some((b) => b.id === "search_files"), "a chord is searchable too");
});

test("a files-scoped chord only fires with the tree focused, and it beats the workbench there", () => {
  const km = resolveKeymap("default", null);
  assert.equal(commandForEvent(km, ev("Enter"), ["workbench"], true), null, "Enter outside the tree is typing");
  assert.equal(commandForEvent(km, ev("Enter"), ["workbench", "files"], true), "rename_entry");
  assert.equal(commandForEvent(km, ev("c", { metaKey: true }), ["workbench", "files"], true), "copy_entry");
  assert.equal(commandForEvent(km, ev("c", { metaKey: true }), ["workbench"], true), null, "⌘C is the browser's outside the tree");
  assert.equal(commandForEvent(km, ev("Tab", { ctrlKey: true }), ["workbench", "tabs"], true), "next_tab");
  assert.equal(commandForEvent(km, ev("Tab", { ctrlKey: true, shiftKey: true }), ["workbench", "tabs"], true), "prev_tab");
  assert.equal(commandForEvent(km, ev("F2"), ["workbench", "files"], true), "rename", "F2 stays the rail's in the default preset");
  assert.equal(commandForEvent(resolveKeymap("vscode", null), ev("F2"), ["workbench", "files"], true), "rename_entry", "and the tree's in vscode's");
  assert.equal(conflictFor(km, "rename_entry", "F2"), null, "a files chord and a workbench chord may coincide: the narrower wins");
});

test("the tab and workstream chords are workbench-scoped and reach the reference page", () => {
  const km = resolveKeymap("default", null);
  assert.equal(chordFor(km, "close_tab"), "Mod+W");
  assert.equal(chordFor(km, "reopen_tab"), "Mod+Shift+T");
  assert.equal(chordFor(km, "new_workstream"), "Mod+Shift+W");
  assert.equal(commandForEvent(km, ev("w", { metaKey: true }), [], true), null, "workbench-only outside one");
  assert.equal(commandForEvent(km, ev("w", { metaKey: true }), ["workbench"], true), "close_tab");
  assert.equal(commandForEvent(km, ev("w", { metaKey: true, shiftKey: true }), ["workbench"], true), "new_workstream");
  const md = keymapMarkdown();
  for (const id of ["close_tab", "reopen_tab", "new_workstream"]) assert.ok(md.includes("`" + id + "`"), id);
});

test("⌘N is the workbench's untitled document, ⌘⌥N the tree's file", () => {
  const km = resolveKeymap("default", null);
  assert.equal(chordFor(km, "new_document"), "Mod+N");
  assert.equal(chordFor(km, "open_file"), "Mod+O");
  assert.equal(chordFor(km, "new_file"), "Mod+Alt+N");
  assert.equal(commandForEvent(km, ev("n", { metaKey: true }), [], true), null, "workbench-only outside one");
  assert.equal(commandForEvent(km, ev("n", { metaKey: true }), ["workbench"], true), "new_document");
  assert.equal(commandForEvent(km, ev("n", { metaKey: true }), ["workbench", "files"], true), "new_document", "the tree's chord needs Alt");
  assert.equal(commandForEvent(km, ev("n", { metaKey: true, altKey: true }), ["workbench", "files"], true), "new_file");
  assert.equal(commandForEvent(km, ev("o", { metaKey: true }), ["workbench"], true), "open_file");
  assert.ok(keymapMarkdown().includes("`new_document`"));
});

test("a focused shell keeps its keyboard: only chords a PTY cannot mean are intercepted", () => {
  const km = resolveKeymap("default", null);
  const b = (id) => km.bindings.find((x) => x.id === id);
  assert.ok(interceptsInTerminal(b("split_right"), true), "⌘\\ never reaches the PTY on macOS");
  assert.ok(interceptsInTerminal(b("split_down"), true));
  assert.ok(interceptsInTerminal(b("show_terminal"), true), "⌘J");
  assert.ok(interceptsInTerminal(b("panel_files"), true), "⌘⇧E");
  assert.ok(!interceptsInTerminal({ chord: "Mod+C" }, true), "copy stays with the terminal");
  assert.ok(!interceptsInTerminal({ chord: "Mod+V" }, true));
  assert.ok(!interceptsInTerminal({ chord: "Mod+F" }, true), "find is the terminal's own bar");
  assert.ok(!interceptsInTerminal({ chord: "Mod+R" }, true), "replace means nothing to a scrollback: the shell keeps ⌘R");
  assert.ok(!interceptsInTerminal(b("show_terminal"), false), "Ctrl+J is a shell key on Linux");
  assert.ok(interceptsInTerminal(b("panel_files"), false), "Ctrl+Shift+E is not");
  assert.ok(!interceptsInTerminal({ chord: "Ctrl+Shift+C" }, false), "the terminal's copy");
  assert.ok(interceptsInTerminal(b("rename"), false), "F2");
  assert.ok(interceptsInTerminal(b("rename"), true));
  assert.ok(interceptsInTerminal(b("new_terminal"), true), "Ctrl+` means nothing to a shell: a new terminal, on macOS too");
  assert.ok(interceptsInTerminal(b("new_terminal"), false) && interceptsInTerminal(b("new_browser"), false), "and off a Mac");
  assert.ok(interceptsInTerminal(b("next_tab"), true) && interceptsInTerminal(b("prev_tab"), false), "Ctrl+Tab cycles the strip from a shell");
  assert.ok(interceptsInTerminal(b("omnibox"), false) && interceptsInTerminal(b("quick_open"), false) && interceptsInTerminal(b("commands"), false), "the palette is the app's off a Mac too — never the PTY's as well");
  assert.ok(!interceptsInTerminal({ chord: "Ctrl+A" }, false), "a bare Ctrl letter is the shell's");
  assert.ok(!interceptsInTerminal(null, true) && !interceptsInTerminal({ chord: null }, true));
});

test("a terminal-scoped chord is the app's from inside a shell, whatever its keys", () => {
  const km = resolveKeymap("default", {});
  const left = km.bindings.find((b) => b.id === "pane_left");
  assert.ok(interceptsInTerminal(left, true));
  assert.ok(interceptsInTerminal(left, false));
  assert.equal(interceptsInTerminal({ when: "workbench", chord: "Alt+Left" }, true), false, "the same keys in another scope are the shell's");
});

test("find and replace are the document's — the editor or a rendering — on ⌘F and ⌘R in both presets", () => {
  const find = COMMANDS.find((c) => c.id === "find");
  const replace = COMMANDS.find((c) => c.id === "replace");
  assert.equal(find.when, "document");
  assert.equal(replace.when, "document");
  assert.deepEqual(replace.chords, { default: "Mod+R", vscode: "Mod+R" });
  assert.ok(WHENS.includes("document"));
  for (const preset of ["default", "vscode"]) {
    const km = resolveKeymap(preset, {});
    assert.equal(commandForEvent(km, { key: "r", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, ["workbench", "document"], true), "replace");
    assert.equal(commandForEvent(km, { key: "f", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, ["workbench", "document", "editor"], true), "find");
  }
});

test("a conflicted file's document answers the conflict keys — next and previous, keep mine, theirs or both, mark resolved — in both presets", () => {
  const ids = ["next_conflict", "previous_conflict", "keep_mine", "keep_theirs", "keep_both", "mark_resolved"];
  for (const id of ids) {
    const c = COMMANDS.find((x) => x.id === id);
    assert.ok(c, id);
    assert.equal(c.when, "document", id);
    assert.ok(c.chords.default && c.chords.vscode, id);
  }
  const km = resolveKeymap("default", {});
  assert.equal(commandForEvent(km, { key: "ArrowDown", metaKey: false, ctrlKey: false, altKey: true, shiftKey: false }, ["workbench", "document"], true), "next_conflict");
  assert.equal(commandForEvent(km, { key: "1", metaKey: true, ctrlKey: false, altKey: true, shiftKey: false }, ["workbench", "document"], true), "keep_mine");
  assert.equal(commandForEvent(km, { key: "Enter", metaKey: true, ctrlKey: false, altKey: true, shiftKey: false }, ["workbench", "document"], true), "mark_resolved");
  assert.equal(commandForEvent(resolveKeymap("vscode", {}), { key: "F8", metaKey: false, ctrlKey: false, altKey: false, shiftKey: false }, ["workbench", "document"], true), "next_conflict");
  assert.equal(conflictFor(km, "keep_mine", "Mod+Alt+1"), null, "the chords are free");
});

test("the browser scope holds the browser's chords — the address, back, forward, reload, a tab beside — narrowest like the editor's, and the page relays the same ids", () => {
  const km = resolveKeymap("default", null);
  assert.ok(WHENS.includes("browser"));
  assert.deepEqual(
    bindingsIn(km, "browser").map((b) => [b.id, b.chord]),
    [
      ["focus_address", "Mod+L"],
      ["browser_back", "Mod+BracketLeft"],
      ["browser_forward", "Mod+BracketRight"],
      ["browser_reload", "Mod+R"],
      ["new_browser_tab", "Mod+T"],
      ["close_browser_tab", "Mod+W"],
    ],
  );
  assert.equal(commandForEvent(km, { key: "[", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["browser"], true), "browser_back", "the bracket keys are spelt by name and pressed as themselves");
  assert.equal(commandForEvent(km, { key: "]", metaKey: false, ctrlKey: true, shiftKey: false, altKey: false }, ["browser"], false), "browser_forward");
  assert.equal(commandForEvent(km, { key: "w", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["workbench", "browser"], true), "close_browser_tab", "⌘W in a browser tab's body closes the browser tab, not the IDE's");
  assert.equal(commandForEvent(km, { key: "w", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["workbench"], true), "close_tab");
  assert.deepEqual(
    relayChords(km),
    [
      { key: "l", shift: false, alt: false, command: "focus_address" },
      { key: "[", shift: false, alt: false, command: "browser_back" },
      { key: "]", shift: false, alt: false, command: "browser_forward" },
      { key: "r", shift: false, alt: false, command: "browser_reload" },
      { key: "t", shift: false, alt: false, command: "new_browser_tab" },
      { key: "w", shift: false, alt: false, command: "close_browser_tab" },
    ],
    "what a page relays is read from the keymap",
  );
  const rebound = resolveKeymap("default", { browser_reload: "Mod+Shift+R" });
  assert.deepEqual(relayChords(rebound).find((r) => r.command === "browser_reload"), { key: "r", shift: true, alt: false, command: "browser_reload" }, "a rebinding rides into the relay");
  assert.ok(!relayChords(resolveKeymap("default", { browser_reload: "Ctrl+R" })).some((r) => r.command === "browser_reload"), "a chord with Ctrl spelt out is the main window's alone");
  assert.equal(commandForEvent(km, { key: "r", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["workbench", "browser"], true), "browser_reload", "⌘R in a browser tab reloads; in a document it is replace");
  assert.equal(commandForEvent(km, { key: "r", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["workbench", "document"], true), "replace");
  assert.equal(commandForEvent(km, { key: "l", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, ["browser"], true), "focus_address", "the pane's tab, off any workbench, still takes ⌘L");
  assert.equal(commandForEvent(km, { key: "l", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false }, [], true), null, "outside a browser tab ⌘L is nothing");
  assert.equal(conflictFor(km, "browser_reload", "Mod+R"), null, "two scopes may hold one chord; the narrowest live one wins");
  for (const id of ["focus_address", "browser_reload", "browser_back", "browser_forward", "new_browser_tab", "close_tab"]) {
    assert.ok(COMMANDS.some((c) => c.id === id), `the page relays ${id}, a command of the keymap`);
  }
});

test("the designer scope holds its panel's chords — the IDE's chords for the same gestures, on a screen the IDE is never on", () => {
  const keymap = resolveKeymap("default", {});
  const agent = { key: "m", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true };
  assert.equal(commandForEvent(keymap, agent, ["workbench"], true), "panel_agents");
  assert.equal(commandForEvent(keymap, agent, ["designer"], true), "designer_agent");
  assert.equal(commandForEvent(keymap, agent, [], true), null, "on neither screen the chord is nobody's");
  const properties = { key: "d", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true };
  assert.equal(commandForEvent(keymap, properties, ["workbench"], true), "panel_about");
  assert.equal(commandForEvent(keymap, properties, ["designer"], true), "designer_properties");
  const column = { key: "b", metaKey: true, ctrlKey: false, altKey: true, shiftKey: false };
  assert.equal(commandForEvent(keymap, column, ["workbench"], true), "toggle_right_panel");
  assert.equal(commandForEvent(keymap, column, ["designer"], true), "toggle_designer_panel");
  const runs = { key: "r", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true };
  assert.equal(commandForEvent(keymap, runs, ["workbench"], true), "run_project");
  assert.equal(commandForEvent(keymap, runs, ["designer"], true), "designer_runs");
  assert.deepEqual(
    bindingsIn(keymap, "designer").map((b) => b.id).sort(),
    ["designer_agent", "designer_properties", "designer_runs", "toggle_designer_panel"],
    "the scope holds the panel's four and nothing else",
  );
  assert.ok(keymapMarkdown().includes("## designer"), "the reference page has the scope's table");
});
