/**
 * The command registry and the keymap (ide/15): every command's id, label,
 * scope (`when`) and chord per preset; overrides over preset; conflicts
 * refused. One source for the shortcut handler, the palette's hints, the
 * Settings table and `docs/reference/keymap.md` — so a chord shown is a
 * chord that works.
 *
 * Chords are spelled the way `KeyHint` reads them: `Mod+Shift+P`, `Alt+Left`,
 * `Ctrl+Backquote`. `Mod` is ⌘ on macOS and Ctrl elsewhere. Where a Mac's
 * hands differ from the rest — ⌥← moves a word in a Mac terminal, so pane
 * focus is ⌘⌥← there, as in VS Code — a command carries `mac`, the chords a
 * Mac takes in place of `chords` (`resolveKeymap(…, mac)`).
 *
 * A `typed` command is a terminal's own text editing: its chord is typed into
 * the shell as the key the shell's line editor reads (`terminal/typedKeysModel.mjs`)
 * — never the app's, so a focused shell keeps it (`interceptsInTerminal`).
 */

import { t } from "../i18n/l10n.mjs";

export const PRESETS = Object.freeze(["default", "vscode", "vim"]);

/**
 * `when`: the context a binding is live in. `global` always; `workbench`
 * only with a workbench root open; `designer` while the Workflow Designer is
 * on screen (03-workflows §The designer) — never beside `workbench`, so the
 * two may share a chord; `tabs` while a root's
 * tab strip is on screen; `files` with a Files tree focused; `editor` only
 * with an editor focused inside a root; `document` with the editor or a
 * rendered document focused — the two ways a file is read; `terminal` with a
 * terminal focused; `browser` with a browser tab's body focused — in the
 * IDE's centre or the Browser pane (ide/18).
 */
export const WHENS = Object.freeze(["global", "workbench", "designer", "tabs", "files", "editor", "document", "terminal", "browser"]);

/** Narrowest first: two live scopes holding one chord resolve to the lower rank. */
const RANK = Object.freeze({ editor: 0, document: 0, terminal: 0, files: 0, browser: 0, tabs: 1, workbench: 2, designer: 2, global: 3 });

export const COMMANDS = Object.freeze([
  { id: "omnibox", label: t("shell-omnibox-search-jump"), when: "global", chords: { default: "Mod+K", vscode: "Mod+K" }, always: true },
  { id: "quick_open", label: t("shell-omnibox-quick-open"), when: "global", chords: { default: "Mod+P", vscode: "Mod+P" }, always: true },
  { id: "commands", label: t("shell-keymap-commands"), when: "global", chords: { default: "Mod+Shift+P", vscode: "Mod+Shift+P" }, always: true },
  { id: "new_goal", label: t("shell-omnibox-new-goal"), when: "global", chords: { default: "Mod+Shift+I", vscode: "Mod+Shift+I" } },
  { id: "new_channel", label: t("shell-omnibox-new-channel"), when: "global", chords: { default: "Mod+Shift+N", vscode: "Mod+Shift+N" } },
  { id: "new_message", label: t("shell-omnibox-new-message"), when: "global", chords: { default: "Mod+Shift+K", vscode: "Mod+Shift+K" } },
  { id: "inbox", label: t("shell-keymap-go-inbox"), when: "global", chords: { default: "Mod+Shift+A", vscode: "Mod+Shift+A" } },
  { id: "toggle_addons", label: t("shell-keymap-show-hide-addons"), when: "global", chords: { default: "Mod+Shift+X", vscode: "Mod+Shift+X" } },
  { id: "toggle_draw", label: t("shell-keymap-show-hide-draw-panel"), when: "global", chords: { default: "Mod+Alt+D", vscode: "Mod+Alt+D" } },
  { id: "attach_selection", label: t("shell-keymap-attach-selection-agent-panel"), when: "editor", chords: { default: "Mod+Shift+A", vscode: "Mod+Shift+A" } },
  { id: "ask_agent", label: t("shell-keymap-ask-agent-about-selection"), when: "editor", chords: { default: "Mod+I", vscode: "Mod+I" } },
  { id: "edit_agent", label: t("shell-keymap-ask-agent-edit-selection"), when: "editor", chords: { default: "Mod+Shift+I", vscode: "Mod+Shift+I" } },
  { id: "go_to_line", label: t("shell-keymap-go-line"), when: "editor", chords: { default: "Mod+G", vscode: "Ctrl+G" } },
  { id: "toggle_word_wrap", label: t("shell-keymap-wrap-long-lines-stop-wrapping-them"), when: "editor", chords: { default: "Alt+Z", vscode: "Alt+Z" } },
  { id: "settings", label: t("shell-keymap-settings"), when: "global", chords: { default: "Mod+Comma", vscode: "Mod+Comma" } },
  { id: "show_terminal", label: t("shell-keymap-show-terminal-here-go-back-document"), when: "workbench", chords: { default: "Mod+J", vscode: "Mod+J" } },
  { id: "panel_agents", label: t("shell-keymap-agent-panel"), when: "workbench", chords: { default: "Mod+Shift+M", vscode: "Mod+Shift+M" } },
  { id: "panel_files", label: t("shell-keymap-files-panel"), when: "workbench", chords: { default: "Mod+Shift+E", vscode: "Mod+Shift+E" } },
  { id: "panel_about", label: t("shell-keymap-about-panel"), when: "workbench", chords: { default: "Mod+Shift+D", vscode: "Mod+Shift+D" } },
  { id: "panel_workstreams", label: t("shell-keymap-workstreams-panel"), when: "workbench", chords: { default: "Mod+Shift+U", vscode: "Mod+Shift+U" } },
  { id: "toggle_right_panel", label: t("shell-keymap-show-hide-right-panel"), when: "workbench", chords: { default: "Mod+Alt+B", vscode: "Mod+Alt+B" } },
  { id: "toggle_rail", label: t("shell-keymap-show-hide-project-rail"), when: "workbench", chords: { default: "Mod+B", vscode: "Mod+B" } },
  { id: "toggle_ide_mode", label: t("shell-keymap-cycle-centre-s-mode-project-agent"), when: "workbench", chords: { default: "Mod+Alt+A", vscode: "Mod+Alt+A" } },
  // The Workflow Designer's panel — the IDE's chords for the same gestures, on a screen the IDE is never on.
  { id: "designer_properties", label: t("shell-keymap-designer-properties-pane"), when: "designer", chords: { default: "Mod+Shift+D", vscode: "Mod+Shift+D" } },
  { id: "designer_agent", label: t("shell-keymap-designer-agent-pane"), when: "designer", chords: { default: "Mod+Shift+M", vscode: "Mod+Shift+M" } },
  { id: "designer_runs", label: t("shell-keymap-designer-runs-pane"), when: "designer", chords: { default: "Mod+Shift+R", vscode: "Mod+Shift+R" } },
  { id: "toggle_designer_panel", label: t("shell-keymap-show-hide-designer-panel"), when: "designer", chords: { default: "Mod+Alt+B", vscode: "Mod+Alt+B" } },
  { id: "board", label: t("shell-keymap-board-mode-centre-every-workstream-s"), when: "global", chords: { default: "Mod+Shift+B", vscode: "Mod+Shift+B" } },
  { id: "rename", label: t("shell-keymap-rename-selected-project-workstream"), when: "workbench", chords: { default: "F2", vscode: "F2" } },
  { id: "split_right", label: t("shell-keymap-split-terminal-pane-right"), when: "terminal", chords: { default: "Mod+Backslash", vscode: "Mod+Backslash" } },
  { id: "split_down", label: t("shell-keymap-split-terminal-pane-downwards"), when: "terminal", chords: { default: "Mod+Shift+Backslash", vscode: "Mod+Shift+Backslash" } },
  // Pane focus is ⌥+arrows off a Mac and ⌘⌥+arrows on one — VS Code's split, since a Mac's ⌥← and ⌥→ move a word.
  { id: "pane_left", label: t("shell-keymap-focus-terminal-pane-left"), when: "terminal", chords: { default: "Alt+Left", vscode: "Alt+Left" }, mac: { default: "Mod+Alt+Left", vscode: "Mod+Alt+Left" } },
  { id: "pane_right", label: t("shell-keymap-focus-terminal-pane-right"), when: "terminal", chords: { default: "Alt+Right", vscode: "Alt+Right" }, mac: { default: "Mod+Alt+Right", vscode: "Mod+Alt+Right" } },
  { id: "pane_up", label: t("shell-keymap-focus-terminal-pane-above"), when: "terminal", chords: { default: "Alt+Up", vscode: "Alt+Up" }, mac: { default: "Mod+Alt+Up", vscode: "Mod+Alt+Up" } },
  { id: "pane_down", label: t("shell-keymap-focus-terminal-pane-below"), when: "terminal", chords: { default: "Alt+Down", vscode: "Alt+Down" }, mac: { default: "Mod+Alt+Down", vscode: "Mod+Alt+Down" } },
  // A Mac terminal's text editing, typed into the shell (`terminal/typedKeysModel.mjs`). Off a Mac the terminal's own Home, End and Ctrl+arrows already do it, so nothing is bound there.
  { id: "line_start", label: t("shell-keymap-move-start-line"), when: "terminal", typed: true, chords: {}, mac: { default: "Mod+Left", vscode: "Mod+Left" }, note: t("shell-keymap-types-key", { key: "Ctrl+A" }) },
  { id: "line_end", label: t("shell-keymap-move-end-line"), when: "terminal", typed: true, chords: {}, mac: { default: "Mod+Right", vscode: "Mod+Right" }, note: t("shell-keymap-types-key", { key: "Ctrl+E" }) },
  { id: "word_left", label: t("shell-keymap-move-back-word"), when: "terminal", typed: true, chords: {}, mac: { default: "Alt+Left", vscode: "Alt+Left" }, note: t("shell-keymap-types-key", { key: "Alt+B" }) },
  { id: "word_right", label: t("shell-keymap-move-forward-word"), when: "terminal", typed: true, chords: {}, mac: { default: "Alt+Right", vscode: "Alt+Right" }, note: t("shell-keymap-types-key", { key: "Alt+F" }) },
  { id: "delete_to_line_start", label: t("shell-keymap-delete-to-line-start"), when: "terminal", typed: true, chords: {}, mac: { default: "Mod+Backspace", vscode: "Mod+Backspace" }, note: t("shell-keymap-types-key", { key: "Ctrl+U" }) },
  { id: "delete_word_right", label: t("shell-keymap-delete-next-word"), when: "terminal", typed: true, chords: {}, mac: { default: "Alt+Delete", vscode: "Alt+Delete" }, note: t("shell-keymap-types-key", { key: "Alt+D" }) },
  { id: "new_terminal", label: t("shell-keymap-new-terminal-here"), when: "workbench", chords: { default: "Ctrl+Backquote", vscode: "Ctrl+Backquote" } },
  { id: "new_browser", label: t("shell-keymap-new-browser-tab-here"), when: "workbench", chords: { default: "Ctrl+Shift+Backquote", vscode: "Ctrl+Shift+Backquote" } },
  { id: "open_browser", label: t("shell-omnibox-browser-show-hide-browser-pane"), when: "global", chords: { default: "Mod+Shift+L", vscode: "Mod+Shift+L" } },
  { id: "run_project", label: t("shell-keymap-run-serve-project"), when: "workbench", chords: { default: "Mod+Shift+R", vscode: "Mod+Shift+R" } },
  { id: "focus_address", label: t("shell-keymap-focus-browser-tab-s-address-field"), when: "browser", chords: { default: "Mod+L", vscode: "Mod+L" }, note: t("shell-keymap-whole-address-selected-ready-replace") },
  { id: "browser_back", label: t("shell-keymap-back-browser-tab"), when: "browser", chords: { default: "Mod+BracketLeft", vscode: "Mod+BracketLeft" } },
  { id: "browser_forward", label: t("shell-keymap-forward-browser-tab"), when: "browser", chords: { default: "Mod+BracketRight", vscode: "Mod+BracketRight" } },
  { id: "browser_reload", label: t("shell-keymap-reload-browser-tab-s-page"), when: "browser", chords: { default: "Mod+R", vscode: "Mod+R" }, note: t("shell-keymap-stop-while-page-loads") },
  { id: "new_browser_tab", label: t("shell-keymap-new-browser-tab-beside-one"), when: "browser", chords: { default: "Mod+T", vscode: "Mod+T" }, note: t("shell-keymap-home-where-tab") },
  { id: "close_browser_tab", label: t("shell-keymap-close-browser-tab"), when: "browser", chords: { default: "Mod+W", vscode: "Mod+W" }, note: t("shell-keymap-tab-whose-body-has-focus-ide") },
  { id: "git_panel", label: t("shell-keymap-git-panel"), when: "workbench", chords: { default: "Mod+Shift+G", vscode: "Ctrl+Shift+G" } },
  { id: "close_tab", label: t("shell-keymap-close-active-tab"), when: "workbench", chords: { default: "Mod+W", vscode: "Mod+W" } },
  { id: "reopen_tab", label: t("shell-keymap-reopen-last-closed-tab"), when: "workbench", chords: { default: "Mod+Shift+T", vscode: "Mod+Shift+T" } },
  { id: "new_workstream", label: t("shell-keymap-new-workstream-project"), when: "workbench", chords: { default: "Mod+Shift+W", vscode: "Mod+Shift+W" } },
  { id: "new_document", label: t("shell-keymap-new-file"), when: "workbench", chords: { default: "Mod+N", vscode: "Mod+N" }, note: t("shell-keymap-untitled-document-path-asked-when-saved") },
  { id: "open_file", label: t("shell-omnibox-open-file-from-machine"), when: "workbench", chords: { default: "Mod+O", vscode: "Mod+O" }, note: t("shell-keymap-loose-document-under-project-saved-back") },
  { id: "search_files", label: t("shell-keymap-search-files-name-content"), when: "workbench", chords: { default: "Mod+Shift+F", vscode: "Mod+Shift+F" } },
  { id: "search_history", label: t("shell-keymap-search-commit-history"), when: "workbench", chords: { default: "Mod+Shift+H", vscode: "Mod+Shift+H" } },
  { id: "reveal_in_files", label: t("shell-keymap-reveal-active-document-files"), when: "workbench", chords: { default: "Alt+Shift+R", vscode: "Alt+Shift+R" } },
  { id: "reveal_in_finder", label: t("shell-keymap-reveal-active-document-file-manager"), when: "workbench", chords: { default: "Mod+Alt+R", vscode: "Mod+Alt+R" } },
  { id: "import_project", label: t("shell-keymap-import-folder-clone-repository"), when: "workbench", chords: { default: "Mod+Shift+O", vscode: "Mod+Shift+O" } },
  { id: "next_tab", label: t("shell-keymap-next-tab"), when: "tabs", chords: { default: "Ctrl+Tab", vscode: "Ctrl+Tab" } },
  { id: "prev_tab", label: t("shell-keymap-previous-tab"), when: "tabs", chords: { default: "Ctrl+Shift+Tab", vscode: "Ctrl+Shift+Tab" } },
  { id: "close_others", label: t("shell-keymap-close-other-tabs"), when: "tabs", chords: { default: "Mod+Alt+W", vscode: "Mod+Alt+W" } },
  { id: "keep_tab", label: t("shell-keymap-keep-preview-tab-open"), when: "tabs", chords: { default: "Mod+Alt+Enter", vscode: "Mod+Alt+Enter" } },
  { id: "close_saved", label: t("shell-keymap-close-saved-tabs"), when: "tabs", chords: { default: "Mod+Alt+Shift+W", vscode: "Mod+Alt+Shift+W" } },
  ...[1, 2, 3, 4, 5, 6, 7, 8].map((n) => ({ id: `tab_${n}`, label: t("shell-keymap-go-tab", { n }), when: "tabs", chords: { default: `Mod+${n}`, vscode: `Mod+${n}` } })),
  { id: "tab_9", label: t("shell-keymap-go-last-tab"), when: "tabs", chords: { default: "Mod+9", vscode: "Mod+9" } },
  { id: "rename_entry", label: t("shell-keymap-rename-selected-file-folder"), when: "files", chords: { default: "Enter", vscode: "F2" } },
  { id: "open_entry", label: t("shell-keymap-open-selected-file-toggle-folder"), when: "files", chords: { default: "Space", vscode: "Enter" } },
  { id: "delete_entry", label: t("shell-keymap-delete-selected-file-folder"), when: "files", chords: { default: "Mod+Backspace", vscode: "Delete" } },
  { id: "copy_entry", label: t("shell-keymap-copy-selected-file-folder"), when: "files", chords: { default: "Mod+C", vscode: "Mod+C" } },
  { id: "cut_entry", label: t("shell-keymap-cut-selected-file-folder"), when: "files", chords: { default: "Mod+X", vscode: "Mod+X" } },
  { id: "paste_entry", label: t("shell-keymap-paste-into-selected-folder"), when: "files", chords: { default: "Mod+V", vscode: "Mod+V" } },
  { id: "duplicate_entry", label: t("shell-keymap-duplicate-selected-file-folder"), when: "files", chords: { default: "Mod+D", vscode: "Mod+D" } },
  { id: "new_file", label: t("shell-keymap-new-file-selected-folder"), when: "files", chords: { default: "Mod+Alt+N", vscode: "Mod+Alt+N" } },
  { id: "new_folder", label: t("shell-keymap-new-folder-selected-folder"), when: "files", chords: { default: "Mod+Alt+Shift+N", vscode: "Mod+Alt+Shift+N" } },
  { id: "select_all_entries", label: t("shell-keymap-select-every-file-folder-screen"), when: "files", chords: { default: "Mod+A", vscode: "Mod+A" } },
  { id: "save", label: t("shell-keymap-save"), when: "editor", chords: { default: "Mod+S", vscode: "Mod+S" }, note: t("shell-keymap-handled-editor") },
  { id: "find", label: t("shell-keymap-find-document"), when: "document", chords: { default: "Mod+F", vscode: "Mod+F" }, note: t("shell-keymap-editor-s-widget-rendering-s-bar") },
  { id: "replace", label: t("shell-keymap-find-replace-document"), when: "document", chords: { default: "Mod+R", vscode: "Mod+R" }, note: t("shell-keymap-editor-s-widget-rendering-s-bar-2") },
  { id: "next_conflict", label: t("shell-keymap-next-conflict"), when: "document", chords: { default: "Alt+Down", vscode: "F8" }, note: t("shell-keymap-conflicted-file-s-document-next-block") },
  { id: "previous_conflict", label: t("shell-keymap-previous-conflict"), when: "document", chords: { default: "Alt+Up", vscode: "Shift+F8" }, note: t("shell-keymap-block-before") },
  { id: "keep_mine", label: t("shell-keymap-keep-mine-conflict"), when: "document", chords: { default: "Mod+Alt+1", vscode: "Mod+Alt+1" }, note: t("shell-keymap-side-block-under-cursor") },
  { id: "keep_theirs", label: t("shell-keymap-keep-theirs-conflict"), when: "document", chords: { default: "Mod+Alt+2", vscode: "Mod+Alt+2" }, note: t("shell-keymap-incoming-side-block-under-cursor") },
  { id: "keep_both", label: t("shell-keymap-keep-both-conflict"), when: "document", chords: { default: "Mod+Alt+3", vscode: "Mod+Alt+3" }, note: t("shell-keymap-mine-then-theirs") },
  { id: "mark_resolved", label: t("shell-keymap-mark-file-resolved"), when: "document", chords: { default: "Mod+Alt+Enter", vscode: "Mod+Alt+Enter" }, note: t("shell-keymap-once-every-conflict-settled-saves-result") },
  { id: "cycle_conversation_mode", label: t("shell-keymap-cycle-conversation-s-mode-manual-auto"), when: "workbench", chords: { default: "Shift+Tab", vscode: "Shift+Tab" }, note: t("shell-keymap-handled-composer-while-has-focus-only") },
  { id: "review_next_change", label: t("shell-keymap-next-change-review"), when: "document", chords: { default: "Alt+F5", vscode: "Alt+F5" }, note: t("shell-keymap-review-lens-over-file-pending-review") },
  { id: "review_previous_change", label: t("shell-keymap-previous-change-review"), when: "document", chords: { default: "Alt+Shift+F5", vscode: "Alt+Shift+F5" }, note: t("shell-keymap-review-lens-over-file-pending-review") },
  { id: "review_keep_change", label: t("shell-keymap-keep-change-under-cursor"), when: "document", chords: { default: "Alt+K", vscode: "Alt+K" }, note: t("shell-keymap-review-lens-whole-file-s-when") },
  { id: "review_undo_change", label: t("shell-keymap-undo-change-under-cursor"), when: "document", chords: { default: "Alt+Shift+K", vscode: "Alt+Shift+K" }, note: t("shell-keymap-review-lens-disabled-hint-while-buffer") },
]);

/** The keys a chord may name, normalised from `KeyboardEvent.key`. */
const KEY_ALIASES = { ",": "Comma", "\\": "Backslash", "`": "Backquote", "[": "BracketLeft", "]": "BracketRight", " ": "Space", arrowleft: "Left", arrowright: "Right", arrowup: "Up", arrowdown: "Down", escape: "Escape", enter: "Enter", delete: "Delete", backspace: "Backspace", tab: "Tab" };

/** The key a chord's name stands for, as `KeyboardEvent.key` spells it — the aliases read back, a letter lowercased. */
const KEY_OF_NAME = Object.freeze(Object.fromEntries(Object.entries(KEY_ALIASES).filter(([k]) => k.length === 1).map(([k, name]) => [name, k])));

/**
 * The keys a chord may name with no modifier at all: they are never typing,
 * so recording one is unambiguous. Everything else needs Mod, Ctrl or Alt.
 */
const BARE_KEYS = Object.freeze(["Enter", "Space", "Delete", "Backspace", "Tab"]);

/** @param {string} key */
function isBareKey(key) {
  return BARE_KEYS.includes(key) || /^F\d{1,2}$/.test(key);
}

/**
 * Parse `Mod+Shift+P` into its parts. Unknown modifiers are refused (null).
 * @param {string} chord
 * @returns {{mod: boolean, ctrl: boolean, shift: boolean, alt: boolean, key: string} | null}
 */
export function parseChord(chord) {
  const parts = String(chord).split("+").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return null;
  const out = { mod: false, ctrl: false, shift: false, alt: false, key: "" };
  for (const p of parts) {
    const l = p.toLowerCase();
    if (l === "mod" || l === "cmd" || l === "meta") out.mod = true;
    else if (l === "ctrl" || l === "control") out.ctrl = true;
    else if (l === "shift") out.shift = true;
    else if (l === "alt" || l === "option") out.alt = true;
    else if (out.key) return null;
    else out.key = normaliseKey(p);
  }
  return out.key ? out : null;
}

/** @param {string} key */
function normaliseKey(key) {
  const alias = KEY_ALIASES[key] ?? KEY_ALIASES[key.toLowerCase()];
  if (alias) return alias;
  return key.length === 1 ? key.toUpperCase() : key.charAt(0).toUpperCase() + key.slice(1);
}

/** Canonical spelling, so two chords compare as strings. */
export function canonicalChord(chord) {
  const p = parseChord(chord);
  if (!p) return null;
  return [p.mod && "Mod", p.ctrl && "Ctrl", p.alt && "Alt", p.shift && "Shift", p.key].filter(Boolean).join("+");
}

/**
 * Whether a keyboard event is this chord. `Mod` is ⌘ on a Mac and Ctrl
 * elsewhere; a chord that names `Ctrl` explicitly means Ctrl on both.
 * @param {{key: string, metaKey: boolean, ctrlKey: boolean, shiftKey: boolean, altKey: boolean}} e
 * @param {string} chord
 * @param {boolean} mac
 */
export function matchesEvent(e, chord, mac) {
  const p = parseChord(chord);
  if (!p) return false;
  // Off a Mac, Mod *is* Ctrl: a chord that spells `Ctrl` explicitly names the
  // same key, so the two are one chord there.
  if (!mac && p.ctrl) {
    p.mod = true;
    p.ctrl = false;
  }
  const modDown = mac ? e.metaKey : e.ctrlKey;
  if (p.mod !== modDown) return false;
  const ctrlDown = mac ? e.ctrlKey : p.mod ? false : e.ctrlKey;
  if (p.ctrl !== ctrlDown) return false;
  if (p.shift !== e.shiftKey || p.alt !== e.altKey) return false;
  return normaliseKey(e.key) === p.key;
}

/**
 * Chord an event as a string, for recording an override. Null for a bare
 * modifier, for Escape (it cancels a recording) and for a plain key that
 * would be typing — `Enter`, `Delete`, `Backspace`, `Tab` and the function
 * keys are the exception, since a tree or a strip means something by them.
 * @param {{key: string, metaKey: boolean, ctrlKey: boolean, shiftKey: boolean, altKey: boolean}} e
 * @param {boolean} mac
 */
export function chordFromEvent(e, mac) {
  if (["Meta", "Control", "Shift", "Alt", "Escape"].includes(e.key)) return null;
  const key = normaliseKey(e.key);
  const mod = mac ? e.metaKey : e.ctrlKey;
  const ctrl = mac ? e.ctrlKey : false;
  if (!mod && !ctrl && !e.altKey && !isBareKey(key)) return null;
  return [mod && "Mod", ctrl && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", key].filter(Boolean).join("+");
}

/**
 * The effective keymap: `{bindings: [{id, label, when, chord, source}], warnings: string[]}`.
 * Overrides (command id → chord, `""` to unbind) sit over the preset; the vim
 * preset uses the default app-level chords. On a Mac a command's `mac` chords
 * stand in for its `chords`; an override is the person's on every platform. A
 * chord held by two commands in the **same scope** is a conflict: the later
 * binding is dropped and named.
 * @param {string} preset
 * @param {Record<string, string> | null | undefined} overrides
 * @param {boolean} [mac] whether the keymap is a Mac's — false, the rest's, when not said
 */
export function resolveKeymap(preset, overrides, mac = false) {
  const pre = PRESETS.includes(preset) ? preset : "default";
  const chordsKey = pre === "vim" ? "default" : pre;
  const warnings = [];
  const bindings = [];
  const held = new Map(); // `${when}|${chord}` → id
  const ov = overrides && typeof overrides === "object" ? overrides : {};
  for (const c of COMMANDS) {
    const table = mac && c.mac ? c.mac : c.chords;
    let chord = table[chordsKey] ?? table.default ?? null;
    let source = "preset";
    if (Object.prototype.hasOwnProperty.call(ov, c.id)) {
      const raw = ov[c.id];
      if (raw === "" || raw === null) {
        chord = null;
        source = "override";
      } else if (typeof raw === "string" && canonicalChord(raw)) {
        chord = canonicalChord(raw);
        source = "override";
      } else {
        warnings.push(t("shell-keymap-override-not-a-chord-preset-stands", { id: c.id, raw: String(JSON.stringify(raw)) }));
      }
    } else if (chord) {
      chord = canonicalChord(chord);
    }
    if (chord) {
      const k = `${c.when}|${chord}`;
      const other = held.get(k);
      if (other) {
        warnings.push(t("shell-keymap-bound-both-left-unbound", { chord, other, c: c.id, when: c.when }));
        chord = null;
      } else {
        held.set(k, c.id);
      }
    }
    bindings.push({ id: c.id, label: c.label, when: c.when, chord, source, note: c.note ?? null, always: c.always === true, typed: c.typed === true });
  }
  return { preset: pre, bindings, warnings };
}

/**
 * Which command a chord would collide with if given to `id`, in that
 * command's scope — the sentence the recorder shows before refusing.
 * @param {ReturnType<typeof resolveKeymap>} keymap
 * @param {string} id
 * @param {string} chord
 */
export function conflictFor(keymap, id, chord) {
  const c = canonicalChord(chord);
  if (!c) return null;
  const me = keymap.bindings.find((b) => b.id === id);
  if (!me) return null;
  const other = keymap.bindings.find((b) => b.id !== id && b.chord === c && (b.when === me.when || b.when === "global" || me.when === "global"));
  return other ? other.id : null;
}

/**
 * Whether a chord is the app's even from inside a focused terminal — pressed
 * while xterm holds the focus, the app takes it and the shell never sees
 * it. A shell owns its keyboard, so only a chord that cannot mean anything to
 * a PTY is intercepted: on macOS every `Mod` (⌘) chord — the PTY never sees ⌘;
 * elsewhere `Ctrl+Shift+…` and function keys, since bare `Ctrl+x` is the
 * shell's own vocabulary; on every platform a palette chord (`always`) and an
 * explicit `Ctrl` on `Tab` or `Backquote` — the strip's cycle, a terminal or a
 * browser tab here — which no PTY means. Copy, paste and find stay with the
 * terminal everywhere: `Mod+C`, `Mod+V`, `Mod+F`, `Ctrl+Shift+C`, `Ctrl+Shift+V`.
 * A `typed` command is the shell's own text editing — the terminal types it
 * (`terminal/typedKeysModel.mjs`) — so it is never intercepted either.
 * @param {{chord: string | null, when?: string, always?: boolean, typed?: boolean} | null | undefined} binding
 * @param {boolean} mac
 */
export function interceptsInTerminal(binding, mac) {
  const parsed = binding?.chord ? parseChord(binding.chord) : null;
  if (!parsed) return false;
  // The shell's typing, keyed through the keymap: the terminal types it, the app never takes it.
  if (binding.typed) return false;
  // A command live only in a terminal is the app's there by definition — the
  // pane-focus arrows, a split — whatever its chord.
  if (binding.when === "terminal") return true;
  // The palette is the app's from anywhere, and never the PTY's as well.
  if (binding.always) return true;
  // Ctrl+Tab and Ctrl+` mean nothing to a shell: the strip's cycle, a new terminal or browser tab.
  if (parsed.ctrl && !parsed.mod && !parsed.alt && ["Tab", "Backquote"].includes(parsed.key)) return true;
  const key = parsed.key.toLowerCase();
  if (parsed.mod && !parsed.ctrl) {
    // Copy, paste and find stay with the terminal; replace means nothing to
    // a scrollback, so ⌘R is left to the shell too.
    if (["c", "v", "f", "r"].includes(key) && !parsed.alt) return false;
    // ⌘ never reaches the PTY. Elsewhere Mod is Ctrl, which is the shell's —
    // except Ctrl+Shift+<letter>, which no PTY means (copy and paste aside).
    return mac || (parsed.shift && !["c", "v"].includes(key));
  }
  if (!mac && parsed.ctrl && parsed.shift && !parsed.alt) {
    return !["c", "v"].includes(key);
  }
  return /^f\d{1,2}$/.test(key);
}

/** The chord for a command, or null. */
export function chordFor(keymap, id) {
  return keymap.bindings.find((b) => b.id === id)?.chord ?? null;
}

/**
 * Which command an event fires, given the active contexts (`global` is
 * always included). The narrowest scope wins when two match: `document`,
 * `editor`, `terminal`, `files` and `browser` over `tabs` over `workbench` over `global`.
 * @param {ReturnType<typeof resolveKeymap>} keymap
 * @param {{key: string, metaKey: boolean, ctrlKey: boolean, shiftKey: boolean, altKey: boolean}} e
 * @param {Set<string> | string[]} contexts
 * @param {boolean} mac
 */
export function commandForEvent(keymap, e, contexts, mac) {
  const live = new Set(["global", ...contexts]);
  const hits = keymap.bindings.filter((b) => b.chord && live.has(b.when) && matchesEvent(e, b.chord, mac));
  hits.sort((a, b) => RANK[a.when] - RANK[b.when]);
  return hits[0]?.id ?? null;
}

/** The bindings of one scope, in registry order — what a grouped table is made of. */
export function bindingsIn(keymap, when) {
  return keymap.bindings.filter((b) => b.when === when);
}

/**
 * The browser chords a page can relay from inside itself (ide/18): every
 * `browser`-scoped binding spelt `Mod` and one key — Shift or Alt too — as
 * the key `KeyboardEvent.key` reports (lowercased; a named key by its name)
 * with the flags and the command. A chord with `Ctrl` spelt out, or none,
 * is the main window's alone. Read when a tab opens, so a tab opened after
 * a rebinding carries the new chords.
 * @param {ReturnType<typeof resolveKeymap>} keymap
 * @returns {{key: string, shift: boolean, alt: boolean, command: string}[]}
 */
export function relayChords(keymap) {
  const out = [];
  for (const b of bindingsIn(keymap, "browser")) {
    const p = b.chord ? parseChord(b.chord) : null;
    if (!p || !p.mod || p.ctrl) continue;
    const key = KEY_OF_NAME[p.key] ?? p.key.toLowerCase();
    out.push({ key, shift: p.shift, alt: p.alt, command: b.id });
  }
  return out;
}

/**
 * The bindings whose label, id or chord contains `needle`, case-insensitively.
 * An empty needle keeps every binding.
 */
export function filterBindings(bindings, needle) {
  const q = String(needle ?? "").trim().toLowerCase();
  if (!q) return bindings;
  return bindings.filter((b) => b.label.toLowerCase().includes(q) || b.id.includes(q) || (b.chord ?? "").toLowerCase().includes(q));
}

// Read by `scripts/gen-keymap-docs.mjs`, which writes that page, and by `keymapModel.test.mjs`; the app never draws it.
/** The reference page, as Markdown — what `docs/reference/keymap.md` is: one table per scope. */
export function keymapMarkdown() {
  const lines = [
    "# Keymap", // content, never translated: the head of the generated reference page
    "",
    "Generated by `node scripts/gen-keymap-docs.mjs` from `desktop/src/shell/keymapModel.mjs` — do not edit by hand.",
    "",
    "`Mod` is ⌘ on macOS and Ctrl elsewhere. `keymap.preset` (`default` · `vscode` · `vim`) and",
    "`keymap.overrides` (command id → chord, `\"\"` to unbind) are Machine-scope settings; a chord held by",
    t("shell-keymap-two-commands-one-scope-refused-later"),
    t("shell-keymap-live-scope-wins-when-two-hold"),
    t("shell-keymap-workbench-designer-which-never-live-together"),
    "",
    t("shell-keymap-macos-chord-stands-on-a-mac"),
    "",
  ];
  // One preset's cell: the chord, and a Mac's beside it where a Mac's differs — alone where only a Mac has one.
  const cell = (c, preset) => {
    const rest = c.chords[preset] ? canonicalChord(c.chords[preset]) : null;
    const mac = c.mac?.[preset] ? canonicalChord(c.mac[preset]) : null;
    const onMac = mac && mac !== rest ? t("shell-keymap-on-macos", { chord: mac }) : null;
    return [rest, onMac].filter(Boolean).join(" · ") || "—";
  };
  for (const when of WHENS) {
    const rows = COMMANDS.filter((c) => c.when === when);
    if (rows.length === 0) continue;
    lines.push(`## ${when}`, "", t("shell-keymap-command-default-vscode"), "|---|---|---|");
    for (const c of rows) {
      lines.push(`| \`${c.id}\` — ${c.label}${c.note ? ` (${c.note})` : ""} | ${cell(c, "default")} | ${cell(c, "vscode")} |`);
    }
    lines.push("");
  }
  lines.push(t("shell-keymap-vim-preset-keeps-these-app-level"), "");
  return lines.join("\n");
}
