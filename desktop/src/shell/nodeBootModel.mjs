/**
 * The node's boot and its failures, as the shell says them — no React.
 *
 * The desktop shell starts the node and waits for its first answer while it
 * reports what it is doing: opening the workspace, rebuilding the index so
 * far, starting the engine. Each word reaches the webview as `node:boot`; a
 * node that ended, fell silent past its budget, could not be started, or
 * found somebody else's node holding the workspace reaches it as
 * `node:failed`, with when the next try comes; a node that answers after a
 * restart as `node:restarted` (`src-tauri/src/sidecar.rs`). This module
 * folds those into one state and reads the one line the chrome shows while
 * the node is away — the sidebar's footer, the rail's tooltip, Settings ›
 * Node and the root crash card all say the same thing — and the doors
 * beside it. Before this a node booting for a minute read as *waiting for
 * the node…*, and a node that could not start said nothing at all.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/** The three events the shell emits about its node — spelt the same in `sidecar.rs`. */
export const NODE_EVENTS = Object.freeze({
  boot: "node:boot",
  failed: "node:failed",
  restarted: "node:restarted",
});

/**
 * Before anything was heard: nothing known, so nothing said — the plain
 * offline words stand until the shell speaks.
 */
export const NODE_BOOT_INITIAL = Object.freeze({
  /** The boot's phase, `ready` once the node answers; `null` while failed or unknown. */
  phase: null,
  done: null,
  of: null,
  /** Why there is no node, while there is none. */
  failure: null,
  /** Unasked-for restarts in a row, as the shell counts them. */
  attempt: 0,
  /** When the shell tries again, in seconds, while a failure stands. */
  nextInSecs: null,
  /** The node answers. */
  ready: false,
  /** The shell has said something: a seed or an event. */
  known: false,
});

const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);
const str = (v) => (typeof v === "string" ? v : null);
const strings = (v) => (Array.isArray(v) ? v.filter((s) => typeof s === "string") : []);

/**
 * One event folded in. An event of another name, or one without its words,
 * changes nothing.
 * @param {NodeBootState} state
 * @param {string} name
 * @param {unknown} payload
 * @returns {NodeBootState}
 */
export function reduceNodeEvent(state, name, payload) {
  const p = payload && typeof payload === "object" ? /** @type {Record<string, unknown>} */ (payload) : {};
  switch (name) {
    case NODE_EVENTS.boot: {
      const phase = str(p.phase);
      if (!phase) return state;
      if (phase === "ready") return { ...state, phase, done: null, of: null, failure: null, nextInSecs: null, ready: true, known: true };
      return { ...state, phase, done: num(p.done), of: num(p.of), failure: null, nextInSecs: null, ready: false, known: true };
    }
    case NODE_EVENTS.failed: {
      const failure = {
        kind: str(p.kind) ?? "exited",
        how: str(p.how),
        said: strings(p.said),
        secs: num(p.secs),
        pid: num(p.pid),
        tried: strings(p.tried),
      };
      return { ...state, phase: null, done: null, of: null, failure, attempt: num(p.attempt) ?? state.attempt, nextInSecs: num(p.next_in_secs), ready: false, known: true };
    }
    case NODE_EVENTS.restarted:
      return { ...state, phase: "ready", done: null, of: null, failure: null, nextInSecs: null, attempt: num(p.attempt) ?? state.attempt, ready: true, known: true };
    default:
      return state;
  }
}

/**
 * The state at the webview's own start, from what the shell knows of its
 * node (`node_status`): a child still booting, a failure standing, or a
 * node up. Applied only while nothing was heard yet — an event that came
 * first is newer than the answer.
 * @param {NodeBootState} state
 * @param {{ running: boolean, external: boolean, booting?: boolean, restarts: number, failure?: Record<string, unknown> | null } | null} status
 */
export function seedFromStatus(state, status) {
  if (!status || state.known) return state;
  if (status.failure) return reduceNodeEvent(state, NODE_EVENTS.failed, { ...status.failure, attempt: status.restarts, next_in_secs: null });
  if (status.booting) return { ...state, phase: "starting", done: null, of: null, failure: null, ready: false, known: true, attempt: status.restarts };
  if (status.running || status.external) return { ...state, phase: "ready", failure: null, ready: true, known: true, attempt: status.restarts };
  return state;
}

/**
 * A boot phase in the person's words; `null` for one this build does not
 * know, so an older or newer shell's word costs nothing.
 * @param {string | null} phase @param {number | null} done @param {number | null} of
 */
export function phaseWords(phase, done, of) {
  switch (phase) {
    case "starting":
      return t("shell-node-boot-starting");
    case "opening_workspace":
      return t("shell-node-boot-opening-workspace");
    case "rebuilding_index":
      return done != null && of != null && of > 0 ? t("shell-node-boot-rebuilding-index-progress", { done, of }) : t("shell-node-boot-rebuilding-index");
    case "starting_engine":
      return t("shell-node-boot-starting-engine");
    default:
      return null;
  }
}

/**
 * A failure in the person's words: the line, and the detail a fold shows —
 * what the node said on its stderr before it died, or what each binary
 * answered. The shell's own `how` (*exited with code 1*) is carried as it
 * is: it is the shell's diagnostic word, framed by the catalog's sentence.
 * @param {NodeFailure | null | undefined} failure
 * @returns {{ line: string, details: string | null } | null}
 */
export function failureWords(failure) {
  if (!failure) return null;
  const said = failure.said?.length ? t("shell-node-failed-said", { said: failure.said.join(" · ") }) : null;
  switch (failure.kind) {
    case "held_by_other":
      return { line: t("shell-node-failed-held-by-other", { pid: failure.pid ?? 0 }), details: null };
    case "timed_out":
      return { line: t("shell-node-failed-timed-out", { secs: failure.secs ?? 0 }), details: said };
    case "no_binary":
      return { line: t("shell-node-failed-no-binary"), details: failure.tried?.length ? failure.tried.join("\n") : null };
    default:
      return { line: t("shell-node-failed-exited", { how: failure.how ?? "" }), details: said };
  }
}

/**
 * The one line while the node is not there: its failure, with when the
 * next try comes; else the phase it is in. `null` when the node answers or
 * the shell has said nothing — the plain offline words stand then.
 * @param {NodeBootState | null | undefined} state
 * @returns {string | null}
 */
export function bootLine(state) {
  if (!state || state.ready) return null;
  if (state.failure) {
    const words = failureWords(state.failure);
    if (!words) return null;
    return state.nextInSecs != null ? t("shell-node-failed-retrying-in", { line: words.line, secs: state.nextInSecs }) : words.line;
  }
  return phaseWords(state.phase, state.done, state.of);
}

/** Every door beside a node that is away, in the order they are drawn. */
export const NODE_DOORS = Object.freeze(["restart_now", "reveal_log", "open_data_folder", "quit"]);

/**
 * The doors beside a node that is away. A workspace held by somebody else's
 * node offers no restart: that node is not this desktop's to restart, and
 * the shell starts its own the moment it goes.
 * @param {NodeBootState | null | undefined} state
 * @returns {string[]}
 */
export function nodeDoors(state) {
  return state?.failure?.kind === "held_by_other" ? NODE_DOORS.filter((d) => d !== "restart_now") : [...NODE_DOORS];
}

/**
 * A door's label.
 * @param {string} door
 */
export function doorWords(door) {
  switch (door) {
    case "restart_now":
      return t("shell-node-doors-restart-now");
    case "reveal_log":
      return t("shell-node-doors-reveal-log");
    case "open_data_folder":
      return t("shell-node-doors-open-data-folder");
    case "quit":
      return t("shell-node-doors-quit");
    default:
      return door;
  }
}

/**
 * The line under a door that did not work: which, and why.
 * @param {string} door @param {unknown} reason
 */
export function doorFailedWords(door, reason) {
  return t("shell-node-doors-door-failed", { door: doorWords(door), reason: reason instanceof Error ? reason.message : String(reason) });
}

/**
 * @typedef {{ kind: string, how: string | null, said: string[], secs: number | null, pid: number | null, tried: string[] }} NodeFailure
 * @typedef {{ phase: string | null, done: number | null, of: number | null, failure: NodeFailure | null, attempt: number, nextInSecs: number | null, ready: boolean, known: boolean }} NodeBootState
 */
