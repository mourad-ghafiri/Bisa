import type { TerminalScope } from "../terminal/session";
import type { Direction, PaneNode } from "./paneTreeModel.mjs";

/**
 * Three values, not two (ide/06). Loss of contact is never
 * evidence of process death: only the owning host's positive exit is `exited`.
 */
export type Liveness =
  | { status: "live" }
  | { status: "exited"; code: number | null }
  | { status: "unverifiable"; reason: string };

/** One shell, running or exited, with a tab of its own. */
export interface TerminalSessionState {
  /** Tab identity. Minted from a counter, never reused, stable across a restart. */
  readonly key: string;
  readonly scope: TerminalScope;
  readonly id: string;
  /** A harness id from `GET /harnesses`, or `null` for the login shell. */
  readonly harness: string | null;
  /** What the surface that opened it calls the place. */
  readonly label: string | null;
  /** The code host sign-in this tab runs, when it is one — the shell's argv, not a program the tab named. */
  readonly login: TerminalLogin | null;
  /**
   * Whether this shell continued the harness's latest session in that place
   * rather than starting a new one.
   *
   * On the session and not derived at spawn time, so a restart repeats what
   * this tab did rather than what the place has become since.
   */
  readonly resume: boolean;
  /** The project's run command runs in it (ide/18): labelled *run*, never resumed. */
  readonly run: boolean;
  /** The checkout's Flutter app runs in it on that device (ide/19): labelled *flutter*. */
  readonly mobileDevelopment: TerminalMobileDevelopment | null;
  /** Bumped by a restart, so React remounts. Only ever increases. */
  readonly generation: number;
  readonly liveness: Liveness;
  /** Restored from storage: the mount replays its checkpoint before it spawns. */
  readonly restoring: boolean;
  /**
   * The live PTY's id (the shell's `terminal_open` handle), when the mount has
   * reported it — the key the port scanner traces a terminal-rooted port
   * through. `null` before it goes live, after an exit, and across a restart;
   * never persisted.
   */
  readonly terminalId: string | null;
  /** Unix seconds the shell opened, for "open 12m"; `null` until its PTY is live. Never persisted. */
  readonly openedAt: number | null;
  /** Unix seconds the shell exited, for "ran 5m 20s"; `null` while live or unknown. Never persisted. */
  readonly exitedAt: number | null;
  /**
   * The roster session the harness in this shell reports as (`GET /sessions`),
   * when the node registered one at open — what turns the tab into an agent
   * row with a real state and sub-agents. `null` for a plain shell, a harness
   * that cannot report, before the PTY is live, and across a restart; never
   * persisted (a restored tab registers again when it respawns).
   */
  readonly sessionId: string | null;
  /**
   * The harness the process table shows running under this shell right now —
   * a `claude` typed into a plain shell — told by the host as it changes.
   * `null` for a shell running nothing the catalog names, after an exit, and
   * across a restart; never persisted. Read through `harnessOf`, which lets
   * the launch's own `harness` win.
   */
  readonly running: string | null;
}

export interface TerminalsState {
  readonly sessions: TerminalSessionState[];
  /** A session key, or `null` — which holds if and only if `sessions` is empty. */
  readonly active: string | null;
  /** Monotonic. The only source of key uniqueness. */
  readonly seq: number;
  /**
   * Where a harness has been started before, most recent first — the answer to
   * "is there a session here to continue". See {@link rememberLaunch}.
   */
  readonly launched: readonly string[];
  /** The pane tree over the tabs; every session key is in exactly one leaf. */
  readonly panes: PaneNode;
  /** Monotonic; mints pane and split ids. */
  readonly paneSeq: number;
  readonly focusedPane: string;
}

/** A code host sign-in a terminal is opened for: which kind, at which host. */
export interface TerminalLogin {
  kind: string;
  host: string;
}

/** A device a terminal runs the checkout's Flutter app on (ide/19). */
export interface TerminalMobileDevelopment {
  device: string;
}

export interface TerminalTarget {
  scope: TerminalScope;
  id: string;
  harness?: string | null;
  label?: string | null;
  /** Run the CLI's browser sign-in for this code host instead of a shell. */
  login?: TerminalLogin | null;
  /**
   * Force a fresh session (`false`) or a resumed one (`true`). Left out, the
   * answer is whether this harness has run in this place before.
   */
  resume?: boolean;
  /** Run the project's run command (ide/18) instead of a shell — the node's answer for this checkout. */
  run?: boolean;
  /** Run the checkout's Flutter app on that device (ide/19) — the node's line for it. */
  mobileDevelopment?: TerminalMobileDevelopment | null;
  /** The pane to open in; the focused one when absent or gone. */
  pane?: string | null;
}

export declare const LIVE: { status: "live" };
export declare const RESTORED_REASON: string;
export declare const SESSIONS_VERSION: number;
export declare function unverifiable(reason: string): Liveness;
export declare function exited(code: number | null): Liveness;

export declare const TERMINAL_SCOPES: readonly TerminalScope[];
export declare const LAUNCH_MEMORY: number;

/** "This harness, in this place", or `null` for a plain shell. */
export declare function launchKey(
  harness: string | null,
  scope: TerminalScope,
  id: string,
): string | null;
export declare function hasLaunched(launched: readonly string[], key: string | null): boolean;
export declare function rememberLaunch(
  launched: readonly string[],
  key: string | null,
  cap?: number,
): readonly string[];

export declare function emptyTerminals(): TerminalsState;
export declare function mountKey(session: TerminalSessionState | null | undefined): string;
export declare function isRootedAt(
  session: TerminalSessionState | null | undefined,
  scope: TerminalScope,
  id: string,
): boolean;
export declare function isLive(session: TerminalSessionState | null | undefined): boolean;
export declare function isExited(session: TerminalSessionState | null | undefined): boolean;
export declare function settledByTab<S extends { state: unknown; since: number; children?: readonly unknown[] }>(session: S, tab: TerminalSessionState | null | undefined): S;
/** The roster as the tabs say it — every session through `settledByTab` with the tab that claims it; the same array when no tab changes a row. */
export declare function settledRoster<S extends { id: string; state: unknown; since: number; children?: readonly unknown[] }>(
  sessions: readonly S[],
  terminals: readonly { sessionId?: string | null }[],
): readonly S[];
/**
 * Whether what was just typed into a tab answers the dialog its harness has up: the roster says the session
 * or a sub-agent waits, the tab is live, the bytes are an answering key, and this wait was not said already.
 * The `since` of the wait answered — to remember as `said` — or `null`.
 */
export declare function answerDue(
  tab: TerminalSessionState | null | undefined,
  row: { state: unknown; since: number; children?: readonly { state: unknown; since: number }[] } | null | undefined,
  data: string,
  said: number | null | undefined,
): number | null;

export declare function nowSecs(): number;
export declare function loginOf(login: unknown): TerminalLogin | null;
export declare function mobileDevelopmentOf(mobileDevelopment: unknown): TerminalMobileDevelopment | null;
export declare function mobileDevelopmentRunSession(sessions: readonly TerminalSessionState[], wid: string, device: string): TerminalSessionState | null;
export declare function mobileDevelopmentRunSessions(sessions: readonly TerminalSessionState[], wid: string): TerminalSessionState[];
export declare function openTerminal(state: TerminalsState, target: TerminalTarget, at?: number): TerminalsState;
export declare function revealTerminal(state: TerminalsState, target: TerminalTarget): TerminalsState;
export declare function focusTerminal(state: TerminalsState, key: string): TerminalsState;
export declare function closeTerminal(state: TerminalsState, key: string): TerminalsState;
export declare function closeTerminals(state: TerminalsState, keys: readonly string[]): TerminalsState;
export declare function restartTerminal(state: TerminalsState, key: string, at?: number): TerminalsState;
export declare function noteExit(
  state: TerminalsState,
  key: string,
  generation: number,
  code: number | null,
  at?: number,
): TerminalsState;
export declare function noteLive(state: TerminalsState, key: string, generation: number, terminalId?: string | null, sessionId?: string | null, at?: number): TerminalsState;
export declare function noteRunning(state: TerminalsState, key: string, generation: number, harness: string | null): TerminalsState;
/** The harness a tab is about: the one it was opened with, else the one running in it. */
export declare function harnessOf(session: { harness?: string | null; running?: string | null } | null | undefined): string | null;
export declare function noteUnverifiable(
  state: TerminalsState,
  key: string,
  generation: number,
  reason: string,
): TerminalsState;
export declare function splitPane(
  state: TerminalsState,
  dir: "row" | "col",
  target?: TerminalTarget | null,
): TerminalsState;
export declare function closePane(state: TerminalsState, leafId: string): TerminalsState;
export declare function moveTabToPane(state: TerminalsState, key: string, leafId: string): TerminalsState;
export declare function reorderTerminal(state: TerminalsState, key: string, index: number): TerminalsState;
export declare function reorderTerminalSession(state: TerminalsState, key: string, index: number): TerminalsState;
export declare function closeOtherTerminals(state: TerminalsState, key: string): TerminalsState;
export declare function closeExitedTerminals(state: TerminalsState, scope: TerminalScope, id: string): TerminalsState;
export declare function focusPane(state: TerminalsState, leafId: string): TerminalsState;
export declare function focusNeighborPane(state: TerminalsState, direction: Direction): TerminalsState;
export declare function setPaneRatio(state: TerminalsState, splitId: string, ratio: number): TerminalsState;
export declare function serializeTerminals(state: TerminalsState): unknown;
export declare function restoreTerminals(json: unknown, launched?: readonly string[]): TerminalsState;
export declare function tabOfSession<T extends { key: string; sessionId?: string | null }>(terminals: readonly T[], sessionId: string): T | null;
export declare function sessionsRootedAt(
  sessions: readonly TerminalSessionState[],
  scope: TerminalScope,
  id: string,
): TerminalSessionState[];

export declare function terminalTitle(session: TerminalSessionState | null | undefined): string;
export declare function terminalTabLabel(
  session: TerminalSessionState | null | undefined,
  harnessLabels?: Record<string, string>,
): string;
export declare function exitNote(session: TerminalSessionState | null | undefined): string | null;
/** A tab's liveness as its word — open · exited · exited (n) · unverifiable — the one rule every surface reads. */
export declare function livenessWord(liveness: Liveness | null | undefined): string;
/** What runs in a tab, as its word: the harness's label, the run command, the device run, else a plain shell. */
export declare function runningWord(session: TerminalSessionState | null | undefined, harnessLabels?: Readonly<Record<string, string>>): string;
/** The word for a plain shell. */
export declare function shellWord(): string;
/** *ran 12 s* — how long an exited tab's shell lived. */
export declare function ranWords(duration: string): string;
export declare function livenessTone(session: TerminalSessionState | null | undefined): "danger" | "dim";
export declare function livenessReason(session: TerminalSessionState | null | undefined): string | null;
