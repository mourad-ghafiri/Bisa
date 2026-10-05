/**
 * The eight Tauri commands behind an embedded terminal, typed.
 *
 * These mirror `src-tauri/src/terminal.rs`, and its module doc is where the
 * reasoning lives: a shell is a capability of *this machine*, not a fact about
 * the workspace, so it is reached over Tauri's IPC and never over the node's
 * HTTP API. Nothing here should ever move into `api.ts`.
 *
 * The one rule this file enforces on callers: **you name things, you never
 * name a command or a path.** The Rust side asks the node
 * `GET /placement/{scope}/{id}` for the directory and `GET /harnesses` for what
 * to run, and refuses anything it does not recognise. A signature that accepted
 * a `cwd`, or a `{program, args}`, would put those choices back in the webview
 * — which is the one place in this design that is not allowed to make them.
 * A path is *read back* by one command alone — `cwd()`, where the shell
 * stands, so a relative path it printed is read from there (ide/17) — and
 * never handed in.
 */

import type { WorkbenchScope } from "../routeModel.mjs";

/**
 * The rooted things the node will resolve a directory for — the generated
 * `FileScope`, because the names go on the wire verbatim. The webview names a
 * place and, at most, a harness; never a program.
 */
/**
 * Where a terminal is rooted: one of the IDE's scopes, or `machine` — the
 * person's home directory, where a code host sign-in runs.
 */
export type TerminalScope = WorkbenchScope | "machine";

/**
 * What a live session says. Ordered, because it arrives on a Tauri channel
 * rather than the event bus — output that reordered itself would not be a
 * terminal.
 *
 * There is no error case: everything that can fail does so before the session
 * exists, and comes back as a rejected `open`.
 */
export type TerminalEvent =
  | { kind: "output"; data: string }
  | { kind: "exit"; code: number | null }
  /** What runs in the shell changed: a harness the catalog names started under it, or the last one ended (ide/06 §What runs in a shell). */
  | { kind: "process"; harness: string | null };

/** A shell that is running right now. */
export interface TerminalSession {
  /** Opaque, unique for this run of the app. */
  readonly id: string;
  /**
   * The roster session the harness in this shell reports as, when the node
   * registered one — the same id `GET /sessions` lists. `null` for a plain
   * shell and for a harness that cannot report.
   */
  readonly session: string | null;
  /** Keystrokes, straight through — xterm has already encoded them. */
  write(data: string): Promise<void>;
  resize(rows: number, cols: number): Promise<void>;
  /**
   * Where the shell stands now, absolute: its process's current directory —
   * it follows a `cd` — or, once the process is gone, the directory it was
   * started in. Read at a click on a relative path the shell printed.
   */
  cwd(): Promise<string>;
  /** Ends the shell. Safe to call twice, and on one that has already exited. */
  close(): Promise<void>;
}

/**
 * Whether a shell can be opened at all.
 *
 * `npm run dev` in a plain browser has no Tauri IPC, and a panel that renders
 * an empty black box there teaches whoever is doing UI work that the terminal
 * is broken. Callers use this to say what is actually true instead.
 */
export function terminalAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Open a shell — or a harness — in whatever directory the node says this scope
 * and id live in.
 *
 * `onEvent` is called for every chunk and once at exit; it is unsubscribed by
 * the channel going out of scope, which happens when the returned session is
 * dropped.
 */
export async function openTerminal(
  scope: TerminalScope,
  id: string,
  /** A harness id from `GET /harnesses`, or `null` for the login shell. */
  harness: string | null,
  /**
   * Continue that harness's latest session in this directory rather than
   * starting a new one. Ignored for a plain shell, and for a harness the node
   * reports no way to resume — both open fresh.
   */
  resume: boolean,
  /**
   * A code host sign-in: the shell runs the CLI's browser login for this kind
   * at this host instead of a shell. The argv is the shell's own closed
   * table — nothing here names a program (ide/01).
   */
  login: { kind: string; host: string } | null,
  rows: number,
  cols: number,
  onEvent: (event: TerminalEvent) => void,
  /**
   * Run the project's run command (ide/18) instead of a shell: the node
   * answers what it is for this checkout, approved on this machine — nothing
   * here names a program.
   */
  run = false,
  /**
   * Run this checkout's Flutter app on a device (ide/19) instead of a shell:
   * the node answers the `flutter run` line for the device's id — nothing
   * here names a program.
   */
  mobileDevelopment: { device: string } | null = null,
): Promise<TerminalSession> {
  if (!terminalAvailable()) {
    throw new Error("a shell needs the desktop app — the browser has no terminal to open");
  }
  const { Channel, invoke } = await import("@tauri-apps/api/core");
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = onEvent;

  const opened = await invoke<{ terminal_id: string; session: string | null }>("terminal_open", {
    scope,
    id,
    harness,
    resume,
    rows,
    cols,
    login,
    run,
    mobileDevelopment,
    onOutput: channel,
  });
  const terminalId = opened.terminal_id;

  return {
    id: terminalId,
    session: opened.session ?? null,
    async write(data: string) {
      await invoke("terminal_write", { id: terminalId, data });
    },
    async resize(nextRows: number, nextCols: number) {
      await invoke("terminal_resize", { id: terminalId, rows: nextRows, cols: nextCols });
    },
    cwd() {
      return invoke<string>("terminal_cwd", { id: terminalId });
    },
    async close() {
      await invoke("terminal_close", { id: terminalId });
    },
  };
}

/**
 * Type into a live terminal by its PTY id — what a Device document's Hot
 * reload, Hot restart and Stop send to the `flutter run` it opened (`r`,
 * `R`, `q`). The same door the terminal's own keystrokes go through.
 */
export async function writeTerminal(terminalId: string, data: string): Promise<void> {
  if (!terminalAvailable()) {
    throw new Error("a shell needs the desktop app — there is no terminal to write to");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("terminal_write", { id: terminalId, data });
}

// ---------------------------------------------------------------------------
// Listening ports
// ---------------------------------------------------------------------------

/** Where a scanned port traces back to — a rail's shell, or a harness pid. */
export type PortRoot = { kind: "terminal"; terminal_id: string } | { kind: "pid"; pid: number };

/** One listening TCP port, as the shell's scanner reports it. */
export interface ListeningPort {
  port: number;
  pid: number;
  /** The process name the OS reports — `node`, `vite`, `python3`. */
  process: string;
  root: PortRoot;
}

/**
 * The listening ports the shell can trace to a rail's shell or to one of
 * `roots` — the harness pids the roster carries. A browser has no scanner and
 * answers with nothing.
 */
export async function listeningPorts(roots: readonly number[]): Promise<ListeningPort[]> {
  if (!terminalAvailable()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ListeningPort[]>("listening_ports", { roots });
}

/** `SIGTERM` the process holding a port — re-checked against the socket table first. */
export async function stopPort(pid: number, port: number): Promise<void> {
  if (!terminalAvailable()) throw new Error("stopping a port needs the desktop app");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("stop_port", { pid, port });
}

// ---------------------------------------------------------------------------
// Scrollback checkpoints (ide/06)
// ---------------------------------------------------------------------------
//
// The PTY does not survive a restart; what a person goes back to read does.
// The emulator's buffer is serialised to a replayable escape stream and kept
// by the desktop shell at `run/terminals/<key>.scrollback` in the workspace —
// local state, never synced. The shell validates the key and caps the size;
// the webview never names a path.

/** The last checkpoint for a tab, or `null` when there is none. */
export async function readScrollback(key: string): Promise<string | null> {
  if (!terminalAvailable()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string | null>("terminal_scrollback_read", { key });
}

/** Save a checkpoint. Replaces the previous one. */
export async function writeScrollback(key: string, data: string): Promise<void> {
  if (!terminalAvailable()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("terminal_scrollback_write", { key, data });
}

/** Drop a tab's checkpoint — on close, when it will never be restored. */
export async function forgetScrollback(key: string): Promise<void> {
  if (!terminalAvailable()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("terminal_scrollback_forget", { key });
}
