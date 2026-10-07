/**
 * The menu bar icon's facts (guide/the-desktop.md §The menu bar icon), where
 * `node --test` can read them: which state the platform is in and in what
 * words, how many things need the person, what closing the window means,
 * and the two machine settings behind it. The shell (`src-tauri/src/tray/`)
 * only paints what this file says — a dot in the state's colour, the count
 * beside the mark, the words in the menu — so every word lives here.
 *
 * The state is ranked by the cost of missing it: a node that cannot be
 * reached first, then what is owed, then work under way, then the two kinds
 * of stillness. The count is the Inbox's — rows that need you: an ask, a
 * gate, a harness at its prompt, a person waiting to be admitted
 * (`sidebarModel.inboxBadge(...).needs`) — never the unread, never a notice.
 * A harness waiting is one row, so it is never counted twice against the
 * sessions roster; the roster only says how many are *working*.
 */

import { workingCount as countWorking } from "./sessionCountsModel.mjs";
import { connected } from "../busModel.mjs";
import { boolOf } from "./settingsModel.mjs";
import { inboxBadge, needsWords } from "./sidebarModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The registry keys, spelt once. */
export const TRAY_KEYS = Object.freeze({
  closeKeepsRunning: "desktop.close_keeps_running",
  dockIcon: "desktop.dock_icon",
});

/** The registry's defaults: the window hides, the Dock icon shows. */
export const TRAY_DEFAULTS = Object.freeze({ closeKeepsRunning: true, dockIcon: true });

/**
 * The events between the shell and the webview, spelt once — `main.rs` and
 * `tray/mod.rs` spell the same strings (`scenarios/tray.test.mjs` holds them
 * equal). `close` and `quit` are the shell's two held doors; `go` and `dock`
 * are the menu's word to the webview.
 */
export const TRAY_EVENTS = Object.freeze({
  close: "bisa:close-requested",
  quit: "bisa:quit-requested",
  go: "tray:go",
  dock: "tray:dock",
});

/** Every state, in rank order — the first that holds is the icon's. */
export const TRAY_STATES = Object.freeze(["trouble", "connecting", "waiting", "working", "paused", "quiet"]);

/** Whether a `settings_changed` frame names one of the two keys. */
export function namesTrayKey(keys) {
  const wanted = new Set(Object.values(TRAY_KEYS));
  return Array.isArray(keys) && keys.some((k) => wanted.has(k));
}

/**
 * The two switches from a resolved settings list; anything unusable is the
 * default, so the close guard is never without an answer.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @returns {{closeKeepsRunning: boolean, dockIcon: boolean}}
 */
export function readTrayPrefs(resolved) {
  const rows = Array.isArray(resolved) ? resolved : [];
  return {
    closeKeepsRunning: boolOf(rows, TRAY_KEYS.closeKeepsRunning, TRAY_DEFAULTS.closeKeepsRunning),
    dockIcon: boolOf(rows, TRAY_KEYS.dockIcon, TRAY_DEFAULTS.dockIcon),
  };
}

/**
 * What the window's red button means: hide while the switch is on, else the
 * one close flow — ask, save, quit.
 * @param {{closeKeepsRunning?: boolean} | null | undefined} prefs
 * @returns {"hide" | "quit"}
 */
export function closeVerb(prefs) {
  return prefs?.closeKeepsRunning === true ? "hide" : "quit";
}

/**
 * How many agents are at work: the roster's running sessions and the scopes
 * with an agent mid-turn (`useWorkspace().working`).
 * @param {readonly import("../types").SessionRow[] | null | undefined} sessions
 * @param {Readonly<Record<string, readonly string[]>> | null | undefined} working
 */
export function workingCount(sessions, working) {
  return countWorking(sessions, working);
}

/**
 * The state, by rank. Before the node has ever answered, anything but open
 * is *connecting*; once it has, anything but open is *trouble* — the node
 * was there and is not. A `lagged` pulse is an open stream
 * (`busModel.connected`), never trouble.
 * @param {{conn: string, everOpen?: boolean, paused?: boolean, needs?: number, working?: number}} facts
 * @returns {"trouble" | "connecting" | "waiting" | "working" | "paused" | "quiet"}
 */
export function trayState({ conn, everOpen = false, paused = false, needs = 0, working = 0 }) {
  const away = !connected(conn);
  const holds = { trouble: away && everOpen, connecting: away, waiting: needs > 0, working: working > 0, paused: paused === true, quiet: true };
  // The first that holds, in the rank the list states.
  return TRAY_STATES.find((state) => holds[state]) ?? "quiet";
}

/**
 * What the platform is doing, in one sentence — the menu's first line and
 * the tooltip's. While something is owed and agents still work, the work is
 * the sentence; the count has its own line.
 * @param {string} state
 * @param {number} working
 */
export function statusWords(state, working = 0) {
  const agents = t("shell-tray-working-agents", { working });
  switch (state) {
    case "connecting":
      return t("shell-node-stat-connecting-node");
    case "trouble":
      return t("shell-tray-node-unreachable");
    case "waiting":
      return working > 0 ? agents : t("shell-tray-waiting");
    case "working":
      return agents;
    case "paused":
      return t("shell-tray-paused-nothing-starts");
    default:
      return t("shell-tray-quiet-nothing-running");
  }
}

/**
 * The whole report the shell paints: the state, the count, and the two
 * sentences for them.
 * @param {{conn: string, everOpen?: boolean, paused?: boolean, inbox?: readonly import("../types").InboxRow[] | null, sessions?: readonly import("../types").SessionRow[] | null, working?: Readonly<Record<string, readonly string[]>> | null}} facts
 * @returns {{state: string, needs: number, status: string, needs_words: string}}
 */
export function trayReport({ conn, everOpen = false, paused = false, inbox = [], sessions = [], working = {} }) {
  const needs = inboxBadge(inbox).needs;
  const busy = workingCount(sessions, working);
  const state = trayState({ conn, everOpen, paused, needs, working: busy });
  return { state, needs, status: statusWords(state, busy), needs_words: needsWords(needs) };
}
