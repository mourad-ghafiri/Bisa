/**
 * A resumed harness picks its work up (ide/06 §A resumed session starts).
 *
 * Resuming is one argv word the harness's own catalog names — `--continue`,
 * `resume --last` — and every one of them comes up with the conversation
 * loaded and **waits at its prompt**. Starting it means saying something,
 * and the one way to say something to every harness alike — a custom
 * descriptor included, with no per-harness prompt flag to invent — is the
 * harness's own input: the tab types a short nudge and Enter, once the
 * harness has drawn its prompt.
 *
 * "Has drawn its prompt" is read from the PTY: output has been seen, and then
 * none for a moment. Never before a beat after the first byte, never after a
 * generous wait (a harness that is still signing in is not nudged into a
 * login form), never twice, and never once the person has typed — their
 * words go first and the nudge is dropped. Whether to do it at all is the
 * `terminal.resume_start` switch, machine-scoped like the other terminal
 * switches. Pure, so every one of those rules is assertable without a PTY.
 */

import { boolOf } from "../shell/settingsModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The registry key, spelt once. */
export const RESUME_START_KEY = "terminal.resume_start";
/** The registry's default: a resumed harness starts. */
export const RESUME_START_DEFAULT = true;
/** What the tab says to a resumed harness — neutral, so the harness picks its own work up. */
export const RESUME_START_WORDS = t("terminal-resume-start-continue-where-left-off");

/** Output has gone quiet for this long: the prompt is drawn. */
export const START_QUIET_MS = 700;
/** Never before this much after the first byte — a TUI's first frame is not its prompt. */
export const START_MIN_MS = 300;
/** Never after this much from arming — a harness still signing in is not nudged. */
export const START_WITHIN_MS = 20_000;

/** Whether resumed harnesses start, from the resolved settings. */
export function readResumeStart(resolved) {
  return boolOf(Array.isArray(resolved) ? resolved : [], RESUME_START_KEY, RESUME_START_DEFAULT);
}

/**
 * @typedef {{armedAt: number, firstOutputAt: number | null, lastOutputAt: number | null, spent: boolean}} StartState
 */

/** A nudge armed now: nothing seen yet. */
export function armed(now) {
  return { armedAt: now, firstOutputAt: null, lastOutputAt: null, spent: false };
}

/** The PTY wrote something. */
export function onOutput(state, now) {
  if (state.spent) return state;
  return { ...state, firstOutputAt: state.firstOutputAt ?? now, lastOutputAt: now };
}

/** The person typed: their words go first, the nudge is dropped for good. */
export function onInput(state) {
  return state.spent ? state : { ...state, spent: true };
}

/** The shell exited: nothing to say to. */
export function onExit(state) {
  return state.spent ? state : { ...state, spent: true };
}

/** Whether the nudge is due now — the prompt drawn, inside the window, not yet said. */
export function due(state, now) {
  if (state.spent || state.firstOutputAt === null || state.lastOutputAt === null) return false;
  if (now - state.armedAt > START_WITHIN_MS) return false;
  if (now - state.firstOutputAt < START_MIN_MS) return false;
  return now - state.lastOutputAt >= START_QUIET_MS;
}

/** The nudge was said: never again. */
export function said(state) {
  return { ...state, spent: true };
}

/** Whether there is still anything to wait for — the tick can stop otherwise. */
export function pending(state, now) {
  return !state.spent && now - state.armedAt <= START_WITHIN_MS;
}

/** What the tab types: the words, then Enter. */
export function nudgeText() {
  return `${RESUME_START_WORDS}\r`;
}
