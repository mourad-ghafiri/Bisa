/**
 * The app-level keyboard (ide/15), driven by `keymapModel.mjs`: the preset
 * and the overrides from Settings decide which chord fires which command,
 * and the command decides what happens. A chord shown anywhere in the app is
 * read from the same model, so it is a promise the handler keeps.
 *
 * Contexts narrow a chord's meaning: `⌘⇧A` attaches the editor's selection
 * inside a project workbench and opens the Inbox everywhere else. The three
 * palette chords are checked before the typing guard on purpose — the guard
 * exists so a bare `j`/`k` does not move an inbox row while somebody writes,
 * but the palette is reached for most from inside a composer.
 *
 * A command whose work belongs to a screen goes through a *door*: `fire`
 * opens it and the screen stands at it through `onDoor`, never a raw window
 * listener — `fire` dispatches only to a listener it knows of, and keeps a
 * request whose listener has not mounted yet (`doorModel.mjs`).
 */

import { useEffect } from "react";
import { toggleAddonLayer } from "../addons/addonsStore";
import { toggleDraw } from "../draw/drawStore";
import { api } from "../api";
import { errorFields, log } from "../log";
import { navigate, type WorkbenchScope } from "../router";
import { isMac } from "../ui/KeyHint";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { toggleProjectRail } from "../views/_workbench/projectRailStore";
import { openPanelView, showRightPanel, toggleRightPanel, useRightPanel } from "../views/_workbench/rightPanelStore";
import { isExplorerCommand, openExplorerSearch, sendExplorerCommand } from "../ui/explorerStore";
import { provideChords } from "../ui/keymapHints";
import { selectionChip } from "../views/_workbench/contextChips.mjs";
import { activeEditor } from "../views/_workbench/editorRegistry";
import { looseTargetRoot } from "../views/_workbench/dropModel.mjs";
import { workbenchRootKeys } from "../views/_workbench/workbenchStore";
import { chordFor, commandForEvent, interceptsInTerminal, resolveKeymap } from "./keymapModel.mjs";
import type { Keymap } from "./keymapModel.mjs";
import { contextsOf, designerOnScreen } from "./keyContexts";
import { paneOfCommand } from "../views/_workflow/designerPanelModel.mjs";
import { showDesignerPane, toggleDesignerPanel } from "../views/_workflow/designerPanelStore";
import { focusNeighborTerminalPane, focusTerminalTab, openTerminalIn, splitTerminalPane, useTerminals } from "./useTerminals";
import { browserState } from "./useBrowsers";
import { boolOf, choiceOf, settingOf } from "./settingsModel.mjs";
import { emptyDoors, pendDoor, takeDoor } from "./doorModel.mjs";
import type { Doors } from "./doorModel.mjs";
import { BOARD_DEFAULTS, BOARD_ENABLED_KEY } from "../views/_board/boardSettings.mjs";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, MODES } from "../views/_workbench/ideModeModel.mjs";
import { setIdeMode, toggleIdeMode } from "../views/_workbench/ideModeStore";
import { lastRoot } from "../views/_workbench/lastRootStore";
import { useResolvedSettings } from "./useResolvedSettings";
import { isLive, sessionsRootedAt } from "./terminalsModel.mjs";
import type { TerminalScope } from "../terminal/session";

export const OPEN_PLACES = "bisa:open-places";

export function openPlaces(): void {
  window.dispatchEvent(new CustomEvent(OPEN_PLACES));
}

export const NEW_GOAL = "bisa:new-goal";
export const NEW_CHANNEL = "bisa:new-channel";
export const NEW_MESSAGE = "bisa:new-message";
/** The rail listens: rename whatever row is selected. */
export const RENAME_SELECTION = "bisa:rename-selection";
/** The workbench listens: show this terminal tab in the centre. */
export const SHOW_TERMINAL = "bisa:show-terminal";
/** The workbench listens: close the active tab (a document asks when dirty; a terminal closes). */
export const CLOSE_ACTIVE_TAB = "bisa:close-active-tab";
/** The workbench listens: reopen the last document closed in this root. */
export const REOPEN_TAB = "bisa:reopen-tab";
/**
 * What `NEW_WORKSTREAM` carries: the project to open a workstream on — the
 * current root's when absent — and, from a door on a ref (a branch row, a
 * tag, a chip), the source it presets.
 */
export interface NewWorkstreamRequest {
  pid?: string;
  preset?: import("../views/_work/workstreamCreation.mjs").SourcePreset;
}
/** The workbench listens: open a workstream — the dialog is the workbench's, so every door works with the rail hidden. */
export const NEW_WORKSTREAM = "bisa:new-workstream";
/** Run or serve the project the IDE is on (ide/18) — the Browser launcher listens. */
export const RUN_PROJECT = "bisa:run-project";
/** Show or hide the Browser pane (ide/18). */
export const OPEN_BROWSER = "bisa:open-browser";
/** The browser layer listens: a blank tab at home in the IDE's root — `{home}` rides in `detail` — shown where that root's browser is (ide/18 §The browser tab). */
export const NEW_BROWSER_HERE = "bisa:new-browser-here";
/** The workbench listens: open a new untitled document (⌘N); its path is asked when it is saved. */
export const NEW_DOCUMENT = "bisa:new-document";
/** The workbench listens: the OS's open dialog (⌘O), each chosen file a loose document (ide/03 §Loose files). */
export const OPEN_FILE = "bisa:open-file";
/** The workbench listens: open these absolute paths as loose documents — `{paths: string[]}` rides in `detail`. */
export const OPEN_LOOSE = "bisa:open-loose";
/** Git › Stashes listens: open the *Stash changes…* dialog for the checkout it shows. */
export const GIT_STASH = "bisa:git-stash";
/** The rail listens: import a folder or clone a repository into the tab it shows. */
export const IMPORT_PROJECT = "bisa:import-project";
/** The workbench listens: show the active document in the Files tree. */
export const REVEAL_ACTIVE_IN_FILES = "bisa:reveal-active-in-files";
/** The workbench listens: show the active document in the OS file manager. */
export const REVEAL_ACTIVE_IN_FINDER = "bisa:reveal-active-in-finder";
/** Find, or find and replace, in the rendered document that has the focus: `{replace: boolean}`. */
export const DOC_FIND = "bisa:doc-find";
/** The event a conflict key sends to the conflicted file's document: `{command}`. */
export const CONFLICT_COMMAND = "bisa:conflict-command";
/**
 * The event a review-lens chord sends to the document showing it (ide/09,
 * ide/03): `{command}`. The focused document's own review lens answers;
 * anywhere else the chord does nothing, the same rule `CONFLICT_COMMAND` follows.
 */
export const REVIEW_COMMAND = "bisa:review-command";
/**
 * The event a browser chord sends to a browser tab's bar (ide/18):
 * `{command, key}` — the keymap's id and the tab it is for, the active tab
 * when typed in the main window, the page's own when relayed from inside
 * it. The bar of the host that draws the tab answers.
 */
export const BROWSER_COMMAND = "bisa:browser-command";
/** The browser scope's ids — the keymap's, and what a page relays. */
export type BrowserCommand = "focus_address" | "browser_back" | "browser_forward" | "browser_reload" | "new_browser_tab" | "close_browser_tab";
export interface BrowserCommandRequest {
  command: BrowserCommand;
  key: string | null;
}
/** The workbench listens: the strip's next / previous tab, and close the others. */
export const NEXT_TAB = "bisa:next-tab";
export const PREV_TAB = "bisa:prev-tab";
export const CLOSE_OTHER_TABS = "bisa:close-other-tabs";
/** The workbench listens: keep the active preview tab open; close every saved, unpinned tab. */
export const KEEP_TAB = "bisa:keep-tab";
export const CLOSE_SAVED_TABS = "bisa:close-saved-tabs";
/** The workbench listens: activate the nth tab of the strip (`detail`: 1–9, 9 being the last). */
export const SELECT_TAB_AT = "bisa:select-tab-at";
/** The Git occupant's History listens: focus its search box. */
export const SEARCH_HISTORY = "bisa:search-history";

/** How many listeners each door has right now, and the requests waiting for one (`doorModel.mjs`). */
const listeners = new Map<string, number>();
let doors: Doors = emptyDoors();

/**
 * Open a door. With a listener standing at it (`onDoor`) the request is
 * dispatched once the route has settled; without one — the hosting screen
 * is a lazy chunk still on its way, or another screen entirely — it waits in
 * `doorModel` for the listener to mount, and the creation doors navigate to
 * the screen that hosts theirs. `detail` rides along for a listener that
 * takes one — the New workstream dialog's preset, the strip's tab number,
 * the terminal to show. A listener added to the window by hand is one this
 * function cannot see, and its door stays shut.
 */
export function fire(event: string, detail?: unknown): void {
  if (event === NEW_GOAL) navigate({ name: "goals" });
  else if (event === NEW_CHANNEL) navigate({ name: "channels" });
  else if (event === NEW_MESSAGE) navigate({ name: "messages" });
  else if (event === NEW_WORKSTREAM && !(listeners.get(event) ?? 0)) navigate({ name: "projects" });
  if (listeners.get(event) ?? 0) {
    setTimeout(() => window.dispatchEvent(new CustomEvent(event, { detail })), 0);
    return;
  }
  doors = pendDoor(doors, event, detail, Date.now());
}

/**
 * Stand at a door. The listener is counted so `fire` dispatches to it, and a
 * request that arrived before it mounted is handed over now, once, if it is
 * still fresh. Every listener at a door comes through here.
 */
export function onDoor(event: string, handler: (detail?: unknown) => void): () => void {
  const fn = (e: Event) => handler((e as CustomEvent<unknown>).detail);
  window.addEventListener(event, fn);
  listeners.set(event, (listeners.get(event) ?? 0) + 1);
  const taken = takeDoor(doors, event, Date.now());
  doors = taken.doors;
  if (taken.request) {
    const { detail } = taken.request;
    setTimeout(() => handler(detail), 0);
  }
  return () => {
    window.removeEventListener(event, fn);
    listeners.set(event, Math.max(0, (listeners.get(event) ?? 1) - 1));
  };
}

function isTypingTarget(el: EventTarget | null): boolean {
  const node = el as HTMLElement | null;
  if (!node) return false;
  const tag = node.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    node.isContentEditable === true
  );
}

// ---------------------------------------------------------------------------
// The live keymap: preset + overrides from Machine settings, refreshed when
// Settings saves them. Module-level so KeyHint and the palette read it too.
// ---------------------------------------------------------------------------

let keymap: Keymap = resolveKeymap("default", null);
const keymapListeners = new Set<() => void>();
// The kit's menus show chords from here without importing the shell.
provideChords((id) => chordFor(keymap, id));

export function currentKeymap(): Keymap {
  return keymap;
}

export function onKeymapChange(l: () => void): () => void {
  keymapListeners.add(l);
  return () => {
    keymapListeners.delete(l);
  };
}

/** Re-read `keymap.preset` and `keymap.overrides`. Best effort; the default stands otherwise. */
export async function reloadKeymap(): Promise<Keymap> {
  try {
    const r = await api.settingsResolved(null);
    const get = (k: string) => r.settings.find((x) => x.key === k)?.value;
    keymap = resolveKeymap(get("keymap.preset"), get("keymap.overrides"));
  } catch {
    keymap = resolveKeymap("default", null);
  }
  provideChords((id) => chordFor(keymap, id));
  for (const l of keymapListeners) l();
  return keymap;
}

/** The root the Project IDE is on, from the address — `null` on any other screen. */
export function workbenchRoot(): { scope: TerminalScope; id: string } | null {
  const m = /^#\/projects\/(goal|workstream|work_item)\/([^/?#]+)/.exec(window.location.hash);
  return m ? { scope: m[1] as TerminalScope, id: decodeURIComponent(m[2]) } : null;
}

/**
 * The root the IDE is on as a place a tab or a document is at home in — a
 * terminal's `machine` scope is no such place. The one filter every door
 * reads, so "on a root" means the same at each.
 */
export function ideRoot(): { scope: Exclude<TerminalScope, "machine">; id: string } | null {
  const root = workbenchRoot();
  return root && root.scope !== "machine" ? { scope: root.scope, id: root.id } : null;
}

/** The right panel's key for the current root — what it remembers a strip tab under. */
function rootKeyNow(): string | null {
  const root = workbenchRoot();
  return root ? `${root.scope}:${root.id}` : null;
}

/**
 * Open files from this machine as loose documents, from anywhere: in the
 * workbench on screen, else in the root used most recently — navigating
 * there first — else a word, since a document needs a workbench to sit in.
 * Returns whether a workbench took them.
 */
export function openLooseFiles(paths: string[]): boolean {
  const here = ideRoot();
  const onScreen = here ? { name: "workbench" as const, scope: here.scope as WorkbenchScope, id: here.id } : null;
  const target = looseTargetRoot(onScreen, workbenchRootKeys());
  if (!target) return false;
  const at = target.indexOf(":");
  const scope = target.slice(0, at) as WorkbenchScope;
  const id = target.slice(at + 1);
  if (!here || here.scope !== scope || here.id !== id) navigate({ name: "workbench", scope, id });
  fire(OPEN_LOOSE, { paths });
  return true;
}

/**
 * Board Mode, from anywhere (ide/16): the workstream you are in shows the
 * Board in its centre; from a goal's or a work item's root, or from another
 * screen, the workstream you were last in does, and `#/projects` takes you
 * back to it.
 */
export function openBoard(): void {
  const root = workbenchRoot();
  if (root?.scope === "workstream") {
    setIdeMode(`workstream:${root.id}`, "board");
    return;
  }
  const last = lastRoot();
  if (last) setIdeMode(`workstream:${last}`, "board");
  navigate({ name: "projects" });
}

/** The scopes live for a key — `keyContexts.contextsOf`, told whether the IDE is on a root. */
export function contexts(target: EventTarget | null): string[] {
  return contextsOf(target, workbenchRoot() !== null);
}

interface ShortcutHandlers {
  /** ⌘K, ⌘P for the workbench switcher, ⌘⇧P for the commands, ⌘G for a line. */
  onOmnibox: (mode: "all" | "places" | "commands" | "line") => void;
  onEscape: () => void;
}

export function useGlobalShortcuts({ onOmnibox, onEscape }: ShortcutHandlers): void {
  // Keep the store subscribed so toggles reflect in the same tick.
  useRightPanel();
  const terminals = useTerminals();
  // `toggle_word_wrap` flips a machine setting; the editor follows the frame.
  const { resolved } = useResolvedSettings(null);
  useEffect(() => {
    void reloadKeymap();
  }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Escape works even from a field — it is how you get out of one.
      if (e.key === "Escape") {
        onEscape();
        return;
      }
      const ctx = contexts(e.target);
      const cmd = commandForEvent(keymap, e, ctx, isMac);
      if (!cmd) return;
      const binding = keymap.bindings.find((b) => b.id === cmd);
      // Palette chords work from inside a composer; everything else defers
      // to typing. A focused shell is a typing target too (xterm's helper
      // textarea), but a chord a PTY cannot mean is the app's from there
      // (`interceptsInTerminal`). ⌘P is always prevented: it is Print in a webview.
      const fromShell = ctx.includes("terminal") && interceptsInTerminal(binding, isMac);
      if (!binding?.always && isTypingTarget(e.target) && !ctx.includes("editor") && !fromShell) return;
      // The tree's verbs: the focused tree answers (`explorerStore`), and a
      // chord no tree took falls through to the browser.
      if (isExplorerCommand(cmd)) {
        if (sendExplorerCommand(cmd)) e.preventDefault();
        return;
      }
      // ⌘1 … ⌘9: the strip's nth tab, the ninth being the last.
      const nth = /^tab_([1-9])$/.exec(cmd);
      if (nth) {
        e.preventDefault();
        fire(SELECT_TAB_AT, Number(nth[1]));
        return;
      }
      switch (cmd) {
        case "omnibox":
          e.preventDefault();
          onOmnibox("all");
          return;
        case "quick_open":
          e.preventDefault();
          onOmnibox("places");
          return;
        case "commands":
          e.preventDefault();
          onOmnibox("commands");
          return;
        case "new_message":
          e.preventDefault();
          fire(NEW_MESSAGE);
          return;
        case "new_channel":
          e.preventDefault();
          fire(NEW_CHANNEL);
          return;
        case "new_goal":
          e.preventDefault();
          fire(NEW_GOAL);
          return;
        case "toggle_addons":
          e.preventDefault();
          toggleAddonLayer();
          return;
        case "toggle_draw":
          e.preventDefault();
          toggleDraw();
          return;
        case "inbox":
          e.preventDefault();
          navigate({ name: "inbox" });
          return;
        case "attach_selection": {
          e.preventDefault();
          const ed = activeEditor();
          const sel = ed?.selection?.();
          if (ed?.path && sel && sel.text.trim()) attachContext(selectionChip(ed.path, sel.start, sel.end, sel.text));
          else navigate({ name: "inbox" });
          return;
        }
        case "ask_agent":
        case "edit_agent": {
          // Open the in-editor agent toolbar straight into Ask/Edit, when
          // there is a selection to hand over; the active EditorDoc answers.
          // Without one the chord is still the editor's — it does nothing
          // rather than fall through to whatever else spells the same keys.
          e.preventDefault();
          const ed = activeEditor();
          const sel = ed?.selection?.();
          if (ed?.path && sel && sel.text.trim()) {
            window.dispatchEvent(new CustomEvent("bisa:ide-agent", { detail: { mode: cmd === "edit_agent" ? "edit" : "ask" } }));
          }
          return;
        }
        case "find":
        case "replace": {
          // ⌘R is Reload in a browser: prevented here, always. In the editor
          // Monaco's own widget opens; in a rendering the active document
          // answers with its bar (`DOC_FIND`), replacing into its buffer.
          e.preventDefault();
          const replacing = cmd === "replace";
          if (ctx.includes("editor")) {
            activeEditor()?.trigger?.(replacing ? "editor.action.startFindReplaceAction" : "actions.find");
            return;
          }
          window.dispatchEvent(new CustomEvent(DOC_FIND, { detail: { replace: replacing } }));
          return;
        }
        case "go_to_line":
          e.preventDefault();
          onOmnibox("line");
          return;
        case "next_conflict":
        case "previous_conflict":
        case "keep_mine":
        case "keep_theirs":
        case "keep_both":
        case "mark_resolved":
          // A conflicted file's document answers (`ConflictView`); anywhere
          // else the chord is the document's and does nothing.
          e.preventDefault();
          window.dispatchEvent(new CustomEvent(CONFLICT_COMMAND, { detail: { command: cmd } }));
          return;
        case "review_next_change":
        case "review_previous_change":
        case "review_keep_change":
        case "review_undo_change":
          // The focused document's own review lens answers, when it shows one.
          e.preventDefault();
          window.dispatchEvent(new CustomEvent(REVIEW_COMMAND, { detail: { command: cmd } }));
          return;
        case "toggle_word_wrap": {
          e.preventDefault();
          const wrapped = settingOf(resolved, "editor.word_wrap", "off") !== "off";
          void api.setSettings("machine", { "editor.word_wrap": wrapped ? "off" : "on" }).catch((e: unknown) => {
            // The editor keeps what it had; Settings › Editor says why when asked.
            log.debug("settings", "the word wrap setting could not be saved; the editor keeps what it had", { key: "editor.word_wrap", ...errorFields(e) });
          });
          return;
        }
        case "pane_left":
        case "pane_right":
        case "pane_up":
        case "pane_down":
          e.preventDefault();
          focusNeighborTerminalPane(cmd.slice(5) as "left" | "right" | "up" | "down");
          return;
        case "settings":
          e.preventDefault();
          navigate({ name: "settings" });
          return;
        case "show_terminal": {
          // The most recent live shell rooted here, else a new one.
          const root = workbenchRoot();
          if (!root) return;
          e.preventDefault();
          const here = sessionsRootedAt(terminals.sessions, root.scope, root.id).filter(isLive);
          const last = here[here.length - 1];
          if (last) {
            focusTerminalTab(last.key);
            fire(SHOW_TERMINAL, last.key);
          } else {
            openTerminalIn({ scope: root.scope, id: root.id });
          }
          return;
        }
        case "panel_agents":
          e.preventDefault();
          showRightPanel("agents", rootKeyNow());
          return;
        case "panel_files":
          e.preventDefault();
          showRightPanel("files", rootKeyNow());
          return;
        case "panel_about":
          e.preventDefault();
          showRightPanel("about", rootKeyNow());
          return;
        case "panel_workstreams":
          e.preventDefault();
          showRightPanel("workstreams", rootKeyNow());
          return;
        case "toggle_right_panel":
          e.preventDefault();
          toggleRightPanel();
          return;
        case "board": {
          // Off by the setting: the chord does nothing, the way a hidden mode is no mode.
          if (!boolOf(resolved, BOARD_ENABLED_KEY, BOARD_DEFAULTS.enabled)) return;
          e.preventDefault();
          openBoard();
          return;
        }
        case "toggle_ide_mode": {
          // Project, Agent, Board, round — the Board skipped while the setting hides it.
          e.preventDefault();
          const root = rootKeyNow();
          if (root) toggleIdeMode(root, choiceOf(resolved, DEFAULT_MODE_KEY, MODES, DEFAULT_MODE), boolOf(resolved, BOARD_ENABLED_KEY, BOARD_DEFAULTS.enabled));
          return;
        }
        // The Workflow Designer's panel: a pane shown, or the column shown or hidden.
        case "designer_properties":
        case "designer_agent":
        case "designer_runs": {
          if (!designerOnScreen()) return;
          // Which pane a command shows is the panel model's to say.
          const pane = paneOfCommand(cmd);
          if (!pane) return;
          e.preventDefault();
          showDesignerPane(pane);
          return;
        }
        case "toggle_designer_panel": {
          if (!designerOnScreen()) return;
          e.preventDefault();
          toggleDesignerPanel();
          return;
        }
        case "close_tab":
          e.preventDefault();
          fire(CLOSE_ACTIVE_TAB);
          return;
        case "reopen_tab":
          e.preventDefault();
          fire(REOPEN_TAB);
          return;
        case "new_workstream":
          e.preventDefault();
          fire(NEW_WORKSTREAM);
          return;
        case "new_document":
          e.preventDefault();
          fire(NEW_DOCUMENT);
          return;
        case "open_file":
          e.preventDefault();
          fire(OPEN_FILE);
          return;
        case "toggle_rail":
          e.preventDefault();
          toggleProjectRail();
          return;
        case "rename":
          e.preventDefault();
          fire(RENAME_SELECTION);
          return;
        case "split_right":
          e.preventDefault();
          splitTerminalPane("row");
          return;
        case "split_down":
          e.preventDefault();
          splitTerminalPane("col");
          return;
        case "new_terminal": {
          const root = workbenchRoot();
          if (root) {
            e.preventDefault();
            openTerminalIn({ scope: root.scope, id: root.id });
          }
          return;
        }
        case "new_browser": {
          // A blank browser tab here (ide/18), the URL field ready — shown
          // where this root's browser is: the strip, or the pane while the
          // centre is the conversation or the Board.
          const root = ideRoot();
          if (root) {
            e.preventDefault();
            fire(NEW_BROWSER_HERE, { home: root });
          }
          return;
        }
        case "open_browser":
          // The Details pane's Browser occupant beside any screen (ide/18) — toggled.
          e.preventDefault();
          fire(OPEN_BROWSER);
          return;
        case "focus_address":
        case "browser_back":
        case "browser_forward":
        case "browser_reload":
        case "new_browser_tab":
        case "close_browser_tab": {
          // A browser chord in a browser tab's body: the active tab's bar answers (ide/18).
          e.preventDefault();
          const request: BrowserCommandRequest = { command: cmd, key: browserState().active };
          fire(BROWSER_COMMAND, request);
          return;
        }
        case "run_project":
          e.preventDefault();
          fire(RUN_PROJECT);
          return;
        case "git_panel":
          e.preventDefault();
          showRightPanel("git", rootKeyNow());
          return;
        case "search_files": {
          const k = rootKeyNow();
          if (!k) return;
          e.preventDefault();
          showRightPanel("files", k);
          openExplorerSearch(k);
          return;
        }
        case "search_history": {
          const k = rootKeyNow();
          if (!k) return;
          e.preventDefault();
          openPanelView("git", "history", k);
          fire(SEARCH_HISTORY);
          return;
        }
        case "reveal_in_files":
          e.preventDefault();
          fire(REVEAL_ACTIVE_IN_FILES);
          return;
        case "reveal_in_finder":
          e.preventDefault();
          fire(REVEAL_ACTIVE_IN_FINDER);
          return;
        case "import_project":
          e.preventDefault();
          fire(IMPORT_PROJECT);
          return;
        case "next_tab":
          e.preventDefault();
          fire(NEXT_TAB);
          return;
        case "prev_tab":
          e.preventDefault();
          fire(PREV_TAB);
          return;
        case "close_others":
          e.preventDefault();
          fire(CLOSE_OTHER_TABS);
          return;
        case "keep_tab":
          e.preventDefault();
          fire(KEEP_TAB);
          return;
        case "close_saved":
          e.preventDefault();
          fire(CLOSE_SAVED_TABS);
          return;
        default:
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onOmnibox, onEscape, terminals.sessions, resolved]);
}
