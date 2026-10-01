/**
 * The Workstream scripts card's facts (ide/07 §Workstream scripts): what each
 * phase is, where it runs, what it is told, and what a person's edits become
 * on the wire. The card draws; this file decides — and, being plain
 * JavaScript, is tested with `node --test` and no DOM.
 *
 * Three phases, one setting each. The text is a **project** setting and syncs
 * with the project; whether *this machine* runs it is the node's answer
 * (`trusted`), never computed here — the desktop hashes nothing.
 */

import { t } from "../../i18n/l10n.mjs";

/** The phases, in the order a checkout meets them. */
export const PHASES = Object.freeze([
  Object.freeze({
    id: "pre_create",
    key: "workstreams.script.pre_create",
    label: t("work-workstream-scripts-pre-create"),
    when: t("work-workstream-scripts-before-workstream-created-project-root-non"),
    cwd: t("work-workstream-scripts-project-root"),
  }),
  Object.freeze({
    id: "post_create",
    key: "workstreams.script.post_create",
    label: t("work-workstream-scripts-post-create"),
    when: t("work-workstream-scripts-once-checkout-exists-checkout-install-dependencies"),
    cwd: t("work-workstream-scripts-new-checkout"),
  }),
  Object.freeze({
    id: "clean",
    key: "workstreams.script.clean",
    label: t("work-workstream-scripts-clean"),
    when: t("work-workstream-scripts-before-checkout-deleted-checkout-stop-containers"),
    cwd: t("work-workstream-scripts-checkout"),
  }),
  Object.freeze({
    id: "run",
    key: "workstreams.script.run",
    label: t("work-goal-inspector-run"),
    when: t("work-workstream-scripts-run-when"),
    cwd: t("work-workstream-scripts-checkout-terminal-watch"),
  }),
]);

export const TIMEOUT_KEY = "workstreams.script.timeout_secs";

/** The variables every script is handed; a script cannot override them. */
export const ENV_VARS = Object.freeze([
  Object.freeze(["BISA_SCRIPT_PHASE", t("work-workstream-scripts-pre-create-post-create-clean-run")]),
  Object.freeze(["BISA_PROJECT_PATH", t("work-workstream-scripts-project-s-root-checkout")]),
  Object.freeze(["BISA_PROJECT_SLUG", t("work-workstream-scripts-project-s-slug")]),
  Object.freeze(["BISA_WORKSTREAM_ID", t("work-workstream-scripts-workstream-s-id")]),
  Object.freeze(["BISA_WORKSTREAM_PATH", t("work-workstream-scripts-checkout-not-yet-disk-pre-create")]),
  Object.freeze(["BISA_BRANCH", t("work-workstream-scripts-branch-intended-name-pre-create-empty")]),
  Object.freeze(["BISA_BASE", t("work-workstream-scripts-base-branch-empty-copy")]),
]);

/** The one rule on a text: a bound, so a pasted file does not become a setting. */
export const MAX_SCRIPT_CHARS = 4096;

/**
 * The card's edits, seeded from the node's view: one text per phase and the
 * timeout.
 * @param {{ timeout_secs: number, scripts: readonly {phase: string, command: string, trusted: boolean}[] } | null | undefined} view
 * @returns {{ texts: Record<string, string>, timeout: number }}
 */
export function scriptEdits(view) {
  const texts = {};
  for (const p of PHASES) texts[p.id] = view?.scripts.find((s) => s.phase === p.id)?.command ?? "";
  return { texts, timeout: view?.timeout_secs ?? 300 };
}

/**
 * What is wrong with a script's text, or `null`.
 * @param {string} text
 */
export function validateScript(text) {
  if (text.length > MAX_SCRIPT_CHARS) return t("work-workstream-scripts-most-characters-put-longer-script-repository", { MAX_SCRIPT_CHARS });
  return null;
}

/**
 * Whether the edits differ from the view.
 * @param {ReturnType<typeof scriptEdits>} current
 * @param {ReturnType<typeof scriptEdits>} edits
 */
export function isDirty(current, edits) {
  if (current.timeout !== edits.timeout) return true;
  return PHASES.some((p) => (current.texts[p.id] ?? "") !== (edits.texts[p.id] ?? ""));
}

/**
 * The trust line under a script, as the node reports it.
 * @param {{command: string, trusted: boolean} | undefined} script
 * @returns {{ tone: "ok" | "warn" | "quiet", text: string } | null}
 */
export function trustLine(script) {
  if (!script || !script.command.trim()) return null;
  return script.trusted
    ? { tone: "ok", text: t("work-use-project-settings-draft-approved-machine") }
    : { tone: "warn", text: t("work-workstream-scripts-changed-since-anyone-here-approved-will") };
}

/**
 * Whether the view holds a script this machine has not approved — what makes
 * the standalone *Approve* button appear.
 * @param {{ scripts: readonly {command: string, trusted: boolean}[] } | null | undefined} view
 */
export function needsApproval(view) {
  return (view?.scripts ?? []).some((s) => s.command.trim() && !s.trusted);
}

/**
 * The words of a `script_failed` refusal, for a dialog: the phase sentence
 * and the tail the script printed.
 * @param {{ message: string, detail?: {phase?: unknown, output?: unknown} | null }} err
 */
export function scriptRefusal(err) {
  const phase = PHASES.find((p) => p.id === err.detail?.phase);
  const output = typeof err.detail?.output === "string" ? err.detail.output : "";
  return {
    title: phase ? t("work-workstream-scripts-script-refused", { phase: phase.label.toLowerCase() }) : t("work-workstream-scripts-workstream-script-refused"),
    message: err.message,
    output,
  };
}
