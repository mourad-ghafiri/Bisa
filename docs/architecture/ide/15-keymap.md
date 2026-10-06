# 15 — Keymap

Keyboard-first means every action has a chord, the chords are discoverable, and a person who lives
in another editor can bring their hands with them.

---

## Presets and overrides

`keymap.preset` is `default` | `vscode` | `vim`, at machine scope. `keymap.overrides` is a map from
command id to chord, applied over the preset. Both are settings ([13](13-settings.md)), so they are
one file a person can copy to another machine.

`desktop/src/shell/keymapModel.mjs` owns the **command registry** — every command's id, label,
scope and chord per preset — and resolution: overrides over preset over nothing. It is the one source
for the shortcut handler (`shell/shortcuts.ts`), the palette's rows, the Settings table, and the
generated [`docs/reference/keymap.md`](../../reference/keymap.md).

**Conflicts are refused, not stored.** Recording a chord already held by another command in the
same scope names that command and offers to move it. The model checks, so the same rule holds for a
hand-edited `machine.json`: a conflicting file loads with the later binding dropped and a warning,
never with two commands on one chord.

`Ctrl` works everywhere `⌘` does: chords are spelled `Mod+…`, and `Mod` is ⌘ on macOS and Ctrl
elsewhere.

**Where a Mac's hands differ, a command says so.** A command may carry `mac` — its chords on a Mac,
per preset, in place of `chords` — and `resolveKeymap(preset, overrides, mac)` takes them there (the
window listener and Settings pass `isMac`; the two-argument call is the rest's, as it always was).
Terminal pane focus is the case that needs it: a Mac's ⌥← and ⌥→ move a word, so `pane_left` …
`pane_down` are `⌘⌥←→↑↓` on a Mac and `Alt+←→↑↓` elsewhere — VS Code's own split. An override is the
person's on every platform, and a chord they set wins over a later command's in the same scope as
any conflict does. The reference page writes a Mac's chord beside the other (`macOS: …`), or alone
where only a Mac has one.

**A `typed` command is a terminal's own text editing.** The six of the `terminal` scope —
`line_start`, `line_end`, `word_left`, `word_right`, `delete_to_line_start`, `delete_word_right` —
are bound on a Mac only (`⌘←`, `⌘→`, `⌥←`, `⌥→`, `⌘⌫`, `⌥⌦`), and a focused terminal types each as
the key a shell's line editor reads (`terminal/typedKeysModel.mjs`: Ctrl+A, Ctrl+E, Alt+B, Alt+F,
Ctrl+U, Alt+D — [06](06-terminals.md)). They are commands so that a person sees, rebinds and unbinds
them in Settings › Keymap like any other; `interceptsInTerminal` never claims one, since the
keystroke is the shell's.

---

## The command registry

Every command carries a `when` — the context it is live in: `global` always; `workbench` only with
a workbench root open; `designer` while the Workflow Designer is on screen (`[data-designer-screen]`; never beside
`workbench`, so the two share `⌘⇧D`, `⌘⇧M` and `⌘⌥B` — a pane of a right panel, and the panel itself,
on either screen);
`tabs` while a root's tab strip is on screen; `files` with a Files tree focused;
`editor` only with an editor focused inside a root; `terminal` with a terminal focused; `document`
with any document — an editor or a rendering — holding the focus; `browser` with a browser tab's
body focused, in the IDE's centre or the Browser pane ([18](18-browser-and-servers.md): `⌘L` the
address, `⌘[` `⌘]` back and forward, `⌘R` reload, `⌘T` a tab beside — relayed from inside the page
too, where no key reaches the main window). Scoped
bindings are how one chord can mean two things without ever meaning both at once: `Mod+Shift+A` is
*Go to Inbox* everywhere and *Attach the selection to the Agent tab* when an editor has focus;
`Mod+C` copies a file in the tree and is the browser's everywhere else. When two live scopes hold one
chord the narrowest wins: `document`, `editor`, `terminal`, `files` and `browser`, then `tabs`, then
`workbench`, then `global`. The Settings table groups commands by scope so it is not a surprise. Three commands are
`always` live — the palette chords that must work from inside a composer, where every other chord
yields to typing.

A chord normally needs a modifier: a plain key is typing. `Enter`, `Space`, `Delete`, `Backspace`,
`Tab` and the function keys are the exception — a tree or a strip means something by them — so they can be
recorded bare. `Escape` is never recorded; it cancels the recording.

The ids, in the model's order (a test keeps this list equal to `COMMANDS`):

```text
omnibox
quick_open
commands
quit
new_goal
new_channel
new_message
inbox
toggle_addons
toggle_draw
attach_selection
ask_agent
edit_agent
go_to_line
toggle_word_wrap
settings
show_terminal
panel_agents
panel_files
panel_about
panel_workstreams
toggle_right_panel
toggle_rail
toggle_ide_mode
designer_properties
designer_agent
designer_runs
toggle_designer_panel
board
rename
split_right
split_down
pane_left
pane_right
pane_up
pane_down
line_start
line_end
word_left
word_right
delete_to_line_start
delete_word_right
new_terminal
new_browser
open_browser
run_project
focus_address
browser_back
browser_forward
browser_reload
new_browser_tab
close_browser_tab
git_panel
close_tab
reopen_tab
new_workstream
new_document
open_file
search_files
search_history
reveal_in_files
reveal_in_finder
import_project
next_tab
prev_tab
close_others
keep_tab
close_saved
tab_1
tab_2
tab_3
tab_4
tab_5
tab_6
tab_7
tab_8
tab_9
rename_entry
open_entry
delete_entry
copy_entry
cut_entry
paste_entry
duplicate_entry
new_file
new_folder
select_all_entries
save
find
replace
next_conflict
previous_conflict
keep_mine
keep_theirs
keep_both
mark_resolved
cycle_conversation_mode
review_next_change
review_previous_change
review_keep_change
review_undo_change
```

**The chords are not listed here.** [`docs/reference/keymap.md`](../../reference/keymap.md) is
generated from the registry by `node scripts/gen-keymap-docs.mjs`, checked in CI, and is the one
list of every chord — so the reference cannot describe a binding the app does not have. A page
that quotes a chord beside the gesture it describes — the Review table below, the guides — quotes
the `default` preset's, and the reference is what it is held to.

---

### Review

Five commands over a checkout's conversation ([20 — Reviewing agent changes](20-reviewing-agent-changes.md)):
cycling the mode, and stepping through, keeping and undoing a pending change. Every preset binds
them alike:

| Command | Chord | Where it answers |
|---|---|---|
| `cycle_conversation_mode` | `Shift+Tab` | the composer of a conversation about a checkout, and only there: the chord is every text field's own elsewhere, so the composer catches it itself rather than the workbench's dispatcher |
| `review_next_change` | `Alt+F5` | the focused document, while its review lens shows — never an arrow chord: `Alt+Left` and `Alt+Right` move the cursor by a word, and a document command is taken from every document |
| `review_previous_change` | `Alt+Shift+F5` | the same |
| `review_keep_change` | `Alt+K` | the change under the lens, or the whole file when it has no hunks |
| `review_undo_change` | `Alt+Shift+K` | the same — refused while the buffer is unsaved, since an Undo is made against the disk |

---

## Dispatch

`shell/shortcuts.ts` resolves the keymap once per settings change and dispatches. **The scopes a
key is live in are one reading** (`shell/keyContexts.ts`, `contextsOf(target, root)`, which reads
the DOM with the selectors `shell/keyContextsModel.mjs` names and leaves the decision to its
`scopesOf` — every scope it can answer is one the keymap ranks, and the reverse): `workbench`
while the IDE is on a root, `tabs` while a root's strip is on screen, `editor` inside Monaco,
`document` inside the editor, a rendering, a conflict document, or a document's own root — its bar,
its mode control (`[data-document]`, so ⌘F from the mode control finds) — **on any route** (a
rendering in the Details pane finds too), `terminal`, `browser`, `files` by their frames' markers — read once
per key by the window listener and by a terminal's key reservation alike, so the two never
disagree. The typing guard lets every non-`always` chord through to a focused input; `Mod+P` is
always `preventDefault`ed because it is Print in a webview, and `Mod+R` is prevented where it
resolves — a document's replace, a browser tab's reload — and falls through elsewhere; `find` /
`replace` open Monaco's own widgets in the editor and the rendering's bar (`DOC_FIND`, answered by
the document that holds the focus — a rendering takes the keyboard when it is shown, and takes it
back when its bar closes, never from a field, a shell or the Files tree: `docFocus.takeKeyboard`)
otherwise, and the terminal's own find bar opens on the same
`find` chord (`chordFor`, so a rebinding follows; the editor's `save` resolves through
`commandForEvent` the same way); a focused terminal types a `typed` command's key before anything
else may take the keystroke (`typedFor(currentKeymap(), …)`, then the app's reserved chords, then
`find`); `Esc` closes the palette, else a pane, else nothing — and never a
terminal while focus is inside it. The workbench commands act on the current root: `show_terminal`
focuses the most recent live terminal there or opens one — and, pressed while that terminal is the
active tab, goes back to the document that was showing; `toggle_right_panel`, `panel_agents`,
`panel_files`, `panel_about`, `panel_workstreams` and `git_panel` drive
`rightPanelStore` (every occupant — on the rail or in the header — has a chord, shown in its tooltip; the header shows no hint); `toggle_rail` the project rail (`projectRailStore`, the header's left toggle);
`rename` the selected row; `search_files` opens the Files occupant with its search box focused and
`search_history` the Git occupant's History with its box focused; `reveal_in_files` and
`reveal_in_finder` act on the active document; `import_project` opens the rail's import dialog.
The Git views take their keys **locally**, inside the element that has focus, and register no
command: the Changes file lists and the commit graph are one tab stop each (↑↓ and Home/End move,
Enter selects or opens, Space stages or picks a line, Delete discards or deletes after asking,
Shift+F10 opens the row's menu), a hunk's lines are one stop with ↑↓ and Space, and the commit
composer and a note's composer take ⌘Enter — each shown as a `KeyHint` beside its button.
The `tabs` commands: `next_tab` / `prev_tab` cycle, `close_others` and `close_saved` close, `keep_tab`
keeps the active preview, `tab_1` … `tab_9` go to the strip's nth tab (nine is the last). The
`editor` commands: `go_to_line` opens the omnibox in its `line` mode, `toggle_word_wrap` flips
`editor.word_wrap` at machine scope, `ask_agent` / `edit_agent` open the in-editor toolbar (with no
selection they do nothing, rather than fall through to another command spelling the same keys). The
`terminal` commands `pane_left` … `pane_down` move focus between terminal panes and are the app's
even from inside a shell (`interceptsInTerminal`: a terminal-scoped chord is never the PTY's; nor is
a palette chord — `always` — on any platform, nor an explicit `Ctrl` on `Tab` or `Backquote`, so
Ctrl+Tab cycles the strip and Ctrl+` opens a terminal from a focused shell; copy, paste, find and
a bare `Ctrl+letter` stay the shell's — `quit` on Ctrl+Q, an `always` chord, excepted).

**The native menu on macOS** (`src-tauri/src/main.rs`, `edit_menu.rs`) is built by hand, since a
menu key equivalent fires before the webview ever sees the key: no *Close Window* under ⌘W, and the
Edit menu's Cut · Copy · Paste · Select All are the shell's own items, not muda's predefined ones —
a predefined Paste would claim ⌘V, send `paste:` down the responder chain (nothing, on a Files tree)
and produce no `keydown`, which is how the tree once pasted from its menu alone. When one fires the
shell performs the verb natively (`NSApplication.sendAction`: a field, Monaco, xterm paste as they
always did) and then says the verb (`edit:verb`); the webview **replays** it as its chord on the
element that has the focus (`shell/editMenu.ts` over `editMenuModel.mjs`, whose ids and words mirror
the shell's), so a tree's `paste_entry` · `copy_entry` · `cut_entry` · `select_all_entries` fire and
a field, already pasted, is left alone by the typing guard. **Select All in a code editor is handed
over, not replayed**: Monaco takes Cut, Copy and Paste from the DOM's clipboard events, which the
native verb raises, but Select All raises none — the native one lands on Monaco's hidden input,
which holds a slice of the text, and a synthetic key carries no key code Monaco reads. So the
focused editor is asked first (`ui/monaco.selectAllInFocusedEditor`: the code editor, either side of
a diff, a lens — whichever Monaco holds the focus selects its model's whole range) and the chord is
replayed only when none took it (`editMenuModel.replaysChord`), which is how ⌘A selects a file's
whole text in the editor, a tree's rows in Files, and a field's text in a field. Undo, redo and Quit stay predefined — Quit's ⌘Q is AppKit's `terminate:`, which
`src-tauri/src/quit.rs` holds for the close guard's question, so a `quit` override to ⌘Q on a Mac is
the menu's first and never the keymap's; off macOS
no menu is built at all, since WebView2 and WebKitGTK take Ctrl+C/V/X/A themselves and a menubar
accelerator would only shadow the keymap — and `quit` on Ctrl+Q is the keymap's there, from a
composer and a focused shell alike. `shell/shortcuts.test.mjs` keeps every declared chord
clear of the menu's key equivalents.

**The browser chords** are the `browser` scope's six — `focus_address`, `browser_back`,
`browser_forward`, `browser_reload`, `new_browser_tab`, `close_browser_tab` (⌘W in a browser tab's
body closes the browser tab, in the IDE and in the Browser pane alike; `close_tab` stays the
workbench's) — fired through the `BROWSER_COMMAND` door, at which the tab's bar stands with
`onDoor` (a listener added by hand is one `fire` never dispatches to). Inside a page the chords are
relayed by the page's program, which is written with the keymap's browser bindings as the tab opens
(`keymapModel.relayChords` → `pageInspector.browserScript`): a rebinding rides into every tab opened
after it, and a chord spelt with `Ctrl` is the main window's alone ([18](18-browser-and-servers.md)).

The `files` scope is live while the key's target is inside a tree's frame — the element carrying
`data-files-tree`, which `ui/FileTree.tsx` stamps — so the same chord is the tree's there and the
browser's elsewhere. The `files` commands are dispatched to the focused tree through `ui/explorerStore.ts`
(`sendExplorerCommand(command)`, to the tree that has the focus), so the tree owns what a rename or
a paste does and the keymap owns only which chord asks for it ([03](03-files-and-editing.md)); on
macOS `⌘C ⌘X ⌘V ⌘A` reach it through the Edit menu's verbs, replayed (above). The `tabs` commands go to
the strip that has focus the same way, through the workbench's tab events.

`Ctrl` spelled explicitly in a chord (`Ctrl+Backquote`, `Ctrl+Tab`) means Ctrl on every platform;
off a Mac that is the same key as `Mod`, and the matcher treats the two as one chord there.

---

## The `vscode` and `vim` presets

`vscode` maps what a VS Code user's hands expect where the two differ; chords VS Code uses for
things this app does not have are left unbound, not aliased to something similar. `vim` is a
choice with no chords of its own yet: it reads as `default`, and the editor has no modal mode
([feature status](../../feature-status.md)).

---

## Discoverability

- Every menu item and tooltip shows its chord from the model, so a chord is learned where the action
  is.
- The palette shows the chord beside each command row.
- The Settings keymap panel is the registry rendered as one table per scope, with a filter box over
  labels, ids and chords; every row records, unbinds or resets.
- Context menus show each item's chord from the model (`MenuItem.shortcut`).

---

## What the keymap refuses to do

- close a document without the dirty guard;
- bind one chord to two commands that can both be live;
- describe a chord anywhere but the generated reference.
