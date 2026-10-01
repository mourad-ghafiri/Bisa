/**
 * Whether a close asks first, and what it asks (ide/06, ide/13): the facts
 * behind the one close guard, where `node --test` can read them.
 *
 * Three switches, machine scope, on by default — closing a live shell,
 * terminating a running harness, quitting the app — spelt here once. A
 * close that is not switched on, and a tab that has already exited, never
 * asks. The words a dialog says are here too, so the guard draws and never
 * decides — and every question says where its switch is (`CONFIRM_DOORS`),
 * so the dialog is the door to turning it off.
 */

import { boolOf } from "./settingsModel.mjs";
import { harnessOf } from "./terminalsModel.mjs";
import { t } from "../i18n/l10n.mjs";
import { settingsPath } from "../views/_settings/settingsLink.mjs";

/** The registry keys, spelt once. */
export const CONFIRM_KEYS = Object.freeze({
  shell: "terminal.confirm_close",
  harness: "terminal.confirm_terminate",
  quit: "desktop.confirm_quit",
});

/** The registry's defaults: every confirmation on. */
export const CONFIRM_DEFAULTS = Object.freeze({ shell: true, harness: true, quit: true });

const TERMINAL_PATH = settingsPath("terminal");

/** Where each switch lives, for the question to say: the Settings tab, the lead-in, the path in words, and the switch's label as the registry says it. */
export const CONFIRM_DOORS = Object.freeze({
  shell: Object.freeze({ tab: "terminal", lead: t("shell-close-guard-switch-question-off-under"), path: TERMINAL_PATH, switch: t("shell-close-guard-confirm-before-closing-live-shell") }),
  harness: Object.freeze({ tab: "terminal", lead: t("shell-close-guard-switch-question-off-under"), path: TERMINAL_PATH, switch: t("shell-close-guard-confirm-before-terminating-running-harness") }),
  others: Object.freeze({ tab: "terminal", lead: t("shell-close-guard-switch-these-questions-off-under"), path: TERMINAL_PATH, switch: null }),
  quit: Object.freeze({ tab: "desktop", lead: t("shell-close-guard-switch-question-off-under"), path: settingsPath("desktop"), switch: t("shell-close-guard-confirm-before-quitting") }),
});

/** Whether a `settings_changed` frame names one of the three. */
export function namesConfirmKey(keys) {
  const wanted = new Set(Object.values(CONFIRM_KEYS));
  return Array.isArray(keys) && keys.some((k) => wanted.has(k));
}

/**
 * The three switches from a resolved settings list; anything unusable is
 * the default, so the guard is never left without an answer.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @returns {{shell: boolean, harness: boolean, quit: boolean}}
 */
export function readConfirmPrefs(resolved) {
  const rows = Array.isArray(resolved) ? resolved : [];
  return {
    shell: boolOf(rows, CONFIRM_KEYS.shell, CONFIRM_DEFAULTS.shell),
    harness: boolOf(rows, CONFIRM_KEYS.harness, CONFIRM_DEFAULTS.harness),
    quit: boolOf(rows, CONFIRM_KEYS.quit, CONFIRM_DEFAULTS.quit),
  };
}

/**
 * Whether closing this session asks first: only a live one, and only when
 * its kind's switch is on — a harness by `harness`, a plain shell by `shell`.
 * The kind is what the tab is about (`harnessOf`): a harness typed into a
 * shell is a harness here too.
 * @param {{liveness?: {status?: string}, harness?: string | null, running?: string | null} | null | undefined} session
 * @param {{shell: boolean, harness: boolean}} prefs
 */
export function confirmsClose(session, prefs) {
  if (!session || session.liveness?.status !== "live") return false;
  return harnessOf(session) ? prefs.harness === true : prefs.shell === true;
}

/** What a live session is, in words: a harness has an id, a plain shell does not. */
function noun(harness) {
  return harness ? t("shell-close-guard-running-harness") : t("shell-close-guard-live-shell");
}

/**
 * The question before one tab closes, with the door to its switch.
 * @returns {{title: string, body: string, confirmLabel: string, door: typeof CONFIRM_DOORS.shell}}
 */
export function closeQuestion(session) {
  const harness = Boolean(harnessOf(session));
  return {
    title: harness ? t("shell-close-guard-terminate-harness") : t("shell-close-guard-close-terminal"),
    body: t("shell-close-guard-closing-ends-process", { noun: noun(harness) }),
    confirmLabel: harness ? t("shell-close-guard-terminate") : t("shell-close-guard-close"),
    door: harness ? CONFIRM_DOORS.harness : CONFIRM_DOORS.shell,
  };
}

/**
 * The question before the other tabs rooted here close, over the ones that ask.
 * @param {number} live how many of them are live and switched on
 */
export function othersQuestion(live) {
  return {
    title: t("shell-close-guard-close-other-terminals-here"),
    body: t("shell-close-guard-still-running", { live }),
    confirmLabel: t("shell-close-guard-close-them"),
    door: CONFIRM_DOORS.others,
  };
}

/**
 * The question before the app goes — ⌘Q, the menu bar's Quit Bisa, or the
 * window closing while *Closing the window keeps Bisa running* is off —
 * naming what is running here and what is unsaved. Documents are saved
 * after the confirmation, never before: a cancelled quit must not have
 * written files.
 * @param {{shells: number, harnesses: number, dirty: number}} at
 */
export function quitQuestion({ shells, harnesses, dirty }) {
  const running = [];
  if (harnesses > 0) running.push(t("shell-close-guard-harnesses", { n: harnesses }));
  if (shells > 0) running.push(t("shell-close-guard-shells", { n: shells }));
  const parts = [];
  if (running.length > 0) parts.push(t("shell-close-guard-running-here", { running: running.length === 2 ? t("shell-close-guard-and", { a: running[0], b: running[1] }) : running[0], n: harnesses + shells }));
  if (dirty > 0) parts.push(t("shell-close-guard-unsaved", { n: dirty }));
  if (parts.length === 0) parts.push(t("shell-close-guard-nothing-running-nothing-unsaved"));
  return {
    title: t("shell-close-guard-quit-bisa"),
    body: parts.join(" "),
    confirmLabel: t("shell-close-guard-quit"),
    door: CONFIRM_DOORS.quit,
  };
}
