/**
 * The one place the terminal panel's state lives, and the only thing a screen
 * has to import to open a shell.
 *
 * A module-level store rather than a context, because the surfaces that open a
 * terminal — a project, a workstream, a work item, a goal's files, the
 * workbench header — sit at different depths under different screens, and
 * threading a provider to all of them would put "how many can be open, and who
 * owns them" in the hands of whoever wires the next one. Here there is one
 * variable. The rules it applies are in `terminalsModel.mjs`, where they have
 * tests.
 *
 * **`snapshot()` returns the state and nothing else.** React 19 throws *"The
 * result of getSnapshot should be cached to avoid an infinite loop"* the moment
 * it returns a freshly-derived object, and every derivation here is tempting:
 * the active session, the live count, the tabs for one place. All of them
 * belong in a `useMemo` in the component that wants them.
 *
 * The import of `terminalAvailable` reaches past `../terminal` to
 * `../terminal/session` on purpose. The index re-exports `<Terminal />` and its
 * stylesheet, and this module is imported by every screen with a terminal
 * button on it — screens that only want to know whether the button should exist
 * at all. `TerminalPanel.tsx` is still the single file that imports the
 * component, which is the property the index's own doc is protecting.
 */

import { errorFields, log } from "../log";
import { jsonPref, readPref, webStorage, writePref } from "./storedPrefModel.mjs";
import { useSyncExternalStore } from "react";
import { terminalAvailable, writeTerminal } from "../terminal/session";
import type { TerminalScope } from "../terminal/session";
import { dropScrollback } from "../terminal/Terminal";
import { onSessionTransition } from "./sessionsStore";
import {
  closeExitedTerminals,
  closeOtherTerminals,
  closePane,
  closeTerminal,
  closeTerminals,
  focusNeighborPane,
  focusPane,
  focusTerminal,
  moveTabToPane,
  noteExit,
  noteLive,
  noteUnverifiable,
  openTerminal,
  reorderTerminal,
  reorderTerminalSession,
  restartTerminal,
  restoreTerminals,
  revealTerminal,
  serializeTerminals,
  setPaneRatio,
  splitPane,
  noteRunning,
} from "./terminalsModel.mjs";
import type { TerminalTarget, TerminalsState } from "./terminalsModel.mjs";
import type { Direction } from "./paneTreeModel.mjs";

/**
 * Where a harness has already run.
 *
 * The one part of this store that persists, and the only part that has to:
 * everything else here describes shells that die with the window, while this
 * answers "is there a session to continue" — a question whose whole point is
 * that it outlives the run that created it.
 */
const LAUNCHED_KEY = "bisa.terminal.launched";

// Denied, or somebody hand-edited it into nonsense: an empty list is not a
// failure — every harness simply opens fresh the first time again.
function storedLaunched(): string[] {
  return readPref(
    webStorage(),
    LAUNCHED_KEY,
    (raw) => {
      const v = jsonPref(raw);
      return Array.isArray(v) ? v.filter((k): k is string => typeof k === "string") : [];
    },
    [],
  );
}

/**
 * The tabs and panes themselves (ide/06 §Scrollback restore). Persisted so a
 * restart brings every tab back *in its pane*, as `unverifiable` until its
 * shell is respawned, with its checkpoint replayed first. Only in the desktop
 * shell: a browser session has no PTY to respawn and nothing to restore into.
 */
const SESSIONS_KEY = "bisa.terminal.sessions.v3";

function storedSessions(): unknown {
  return readPref(webStorage(), SESSIONS_KEY, jsonPref, null);
}

let state: TerminalsState = terminalAvailable()
  ? restoreTerminals(storedSessions(), storedLaunched())
  : restoreTerminals(null, storedLaunched());
const listeners = new Set<() => void>();

/**
 * The persisted shape of `state.sessions`, last written. Liveness, the PTY id
 * and the roster session move `state.sessions` without changing what is worth
 * keeping, and `serializeTerminals` omits them — so gating the write on the
 * serialised string keeps those flips out of `localStorage` while still
 * re-rendering.
 */
let lastSessionsWritten: string | null = null;

/**
 * A harness a person opened in a terminal is a roster session; aborting it
 * from a roster row ends that session, and the tab behind it closes here —
 * live or already exited, since the tab is the row and its close is what
 * forgets it. Armed with the store, for the life of the app.
 *
 * A retirement aborts every session on a goal in one burst, one frame each:
 * the tabs are gathered and closed as one move on the next microtask, so
 * the store moves once and the panel renders once for N unmounts.
 */
function watchAborts(): void {
  let gathered: string[] = [];
  let queued = false;
  onSessionTransition((_prev, next) => {
    if (next.state.state !== "aborted") return;
    const tab = state.sessions.find((s) => s.sessionId === next.id);
    if (!tab) return;
    gathered.push(tab.key);
    if (queued) return;
    queued = true;
    queueMicrotask(() => {
      queued = false;
      const keys = gathered;
      gathered = [];
      closeTerminalTabs(keys);
    });
  });
}

/**
 * Identity is the change signal, which is why every rule in
 * `terminalsModel.mjs` returns the *same object* when nothing moved: a store
 * that notified on an equal-but-new state would re-render the panel, and a
 * re-render that produced a new mount key would end a shell.
 */
function set(next: TerminalsState) {
  if (next === state) return;
  // Storage denied: resuming degrades to opening fresh, which is the
  // behaviour before any of this existed rather than a broken one.
  if (next.launched !== state.launched) writePref(webStorage(), LAUNCHED_KEY, next.launched);
  const structural =
    next.sessions !== state.sessions ||
    next.panes !== state.panes ||
    next.active !== state.active ||
    next.focusedPane !== state.focusedPane;
  state = next;
  if (structural) {
    // Persist only when the *serialised* shape changed: a liveness or
    // session-id flip re-renders but is not persisted, since
    // `serializeTerminals` omits them.
    const serialized = JSON.stringify(serializeTerminals(next));
    if (serialized !== lastSessionsWritten) {
      lastSessionsWritten = serialized;
      // Storage denied: tabs die with the window, as they did before.
      writePref(webStorage(), SESSIONS_KEY, serialized);
    }
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): TerminalsState {
  return state;
}

/** Every open shell, and which one is showing. Re-renders when it changes. */
export function useTerminals(): TerminalsState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/**
 * Whether this build can open a shell at all.
 *
 * Every surface asks before it renders its button. A `npm run dev` browser
 * session has no Tauri IPC, and a button that opens a panel saying "not here"
 * teaches whoever is doing UI work that the terminal is broken — so outside the
 * desktop shell there is no button, rather than a dead one.
 */
export function canOpenTerminal(): boolean {
  return terminalAvailable();
}

/** Always a new shell, even if one is already rooted here. */
export function openTerminalIn(target: TerminalTarget): void {
  set(openTerminal(state, target));
}

/**
 * Show a shell for this place: the live one that is already there, or a new
 * one.
 *
 * What a surface's terminal button does. Deliberately never closes anything —
 * the old single-shell drawer's button was a toggle, and a toggle here would be
 * one misclick away from ending somebody's build.
 */
export function revealTerminalIn(target: TerminalTarget): void {
  set(revealTerminal(state, target));
}

export function focusTerminalTab(key: string): void {
  set(focusTerminal(state, key));
}

/**
 * Type into a live tab — what a Device document's Hot reload, Hot restart
 * and Stop send to the `flutter run` it opened (ide/19). A tab that is not
 * live has no PTY to write to, and says so.
 */
export function writeToTerminalTab(key: string, data: string): Promise<void> {
  const session = state.sessions.find((s) => s.key === key);
  if (!session || session.terminalId === null) {
    return Promise.reject(new Error("that terminal is not live"));
  }
  return writeTerminal(session.terminalId, data);
}

/**
 * Close one tab — which unmounts its terminal, which ends the shell — and
 * forget its checkpoint: a tab you closed is not one to bring back.
 */
export function closeTerminalTab(key: string): void {
  if (state.sessions.some((s) => s.key === key)) void dropScrollback(key).catch((e: unknown) => log.warn("terminals", "a closed tab's scrollback could not be dropped", { key, ...errorFields(e) }));
  set(closeTerminal(state, key));
}

/**
 * Close several tabs as one move: one store update, one render, one commit
 * of N unmounts — the shape a retirement's burst takes. A key not open is
 * ignored.
 */
function closeTerminalTabs(keys: readonly string[]): void {
  const open = new Set(state.sessions.map((s) => s.key));
  const closing = keys.filter((k) => open.has(k));
  if (closing.length === 0) return;
  for (const key of closing) void dropScrollback(key).catch((e: unknown) => log.warn("terminals", "a closed tab's scrollback could not be dropped", { key, ...errorFields(e) }));
  set(closeTerminals(state, closing));
}

/**
 * Close every tab rooted at these workstreams — and at this goal, when one
 * is named — without asking: the retirement that put them away or forgot
 * them was the question, and a shell in a folder that is gone or put away
 * has nothing left to stand in. One move for all of them.
 */
export function closeTerminalsRootedAt(workstreams: readonly string[], goal: string | null = null): void {
  if (workstreams.length === 0 && goal === null) return;
  const gone = new Set(workstreams);
  const keys = state.sessions
    .filter((s) => (s.scope === "workstream" && gone.has(s.id)) || (goal !== null && s.scope === "goal" && s.id === goal))
    .map((s) => s.key);
  closeTerminalTabs(keys);
}

/** Spawn a fresh shell in the same tab. */
export function restartTerminalTab(key: string): void {
  set(restartTerminal(state, key));
}

/** The current sessions — a read for close guards and the like (not for render). */
export function terminalSessions(): TerminalsState["sessions"] {
  return state.sessions;
}

/** Record a shell that exited on its own; ignored when it is not the live one. */
export function terminalExited(key: string, generation: number, code: number | null): void {
  set(noteExit(state, key, generation, code));
}

/**
 * The owning host confirmed the shell is running, with the live PTY's id and
 * — for a harness the node registered — the roster session it reports as.
 */
export function terminalLive(key: string, generation: number, terminalId: string | null = null, sessionId: string | null = null): void {
  set(noteLive(state, key, generation, terminalId, sessionId));
}

/** Contact lost without a verdict — never recorded as an exit. */
export function terminalUnverifiable(key: string, generation: number, reason: string): void {
  set(noteUnverifiable(state, key, generation, reason));
}

/** The shell said which harness runs under this tab's shell right now, or that none does. */
export function terminalRunning(key: string, generation: number, harness: string | null): void {
  set(noteRunning(state, key, generation, harness));
}

/** Split the focused pane and open a shell in the new half. */
export function splitTerminalPane(dir: "row" | "col", target?: TerminalTarget | null): void {
  set(splitPane(state, dir, target ?? null));
}

/** Close a pane; its tabs move next door. */
export function closeTerminalPane(leafId: string): void {
  set(closePane(state, leafId));
}

export function moveTerminalToPane(key: string, leafId: string): void {
  set(moveTabToPane(state, key, leafId));
}

/** A drag along a split pane's strip — reorders that pane's leaf. */
export function reorderTerminalTab(key: string, index: number): void {
  set(reorderTerminal(state, key, index));
}

/** A drag along the centre strip or a rail's harness rows — reorders session order. */
export function reorderTerminalSessionTab(key: string, index: number): void {
  set(reorderTerminalSession(state, key, index));
}

/** Close every other tab rooted where this one is; each forgets its checkpoint. */
export function closeOtherTerminalTabs(key: string): void {
  const me = state.sessions.find((s) => s.key === key);
  if (!me) return;
  for (const s of state.sessions) {
    if (s.key !== key && s.scope === me.scope && s.id === me.id) void dropScrollback(s.key).catch((e: unknown) => log.warn("terminals", "a closed tab's scrollback could not be dropped", { key: s.key, ...errorFields(e) }));
  }
  set(closeOtherTerminals(state, key));
}

/** Close the exited tabs rooted in one place. */
export function closeExitedTerminalTabs(scope: TerminalScope, id: string): void {
  for (const s of state.sessions) {
    if (s.scope === scope && s.id === id && s.liveness.status === "exited") void dropScrollback(s.key).catch((e: unknown) => log.warn("terminals", "a closed tab's scrollback could not be dropped", { key: s.key, ...errorFields(e) }));
  }
  set(closeExitedTerminals(state, scope, id));
}

export function focusTerminalPane(leafId: string): void {
  set(focusPane(state, leafId));
}

export function focusNeighborTerminalPane(direction: Direction): void {
  set(focusNeighborPane(state, direction));
}

export function setTerminalPaneRatio(splitId: string, ratio: number): void {
  set(setPaneRatio(state, splitId, ratio));
}

watchAborts();
