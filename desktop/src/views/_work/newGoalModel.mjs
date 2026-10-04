/**
 * The New Goal dialog's decisions, as a module `node --test` can import.
 *
 * What the dialog can get wrong in a way a person notices: the body it
 * sends (the statement and the mode, and only what was set beside them),
 * whether *Capture* may be pressed (not while a document is still uploading
 * — the goal would be captured without it), and what the toast says after.
 * Which workflow it runs is not asked here: the Workflow Agent designs it,
 * or a manual goal's person does on the Workflow tab. Who carries it is
 * optional: the agents and teams picked become the goal's assignees, and the
 * Workflow Agent staffs every step from them alone — from the whole enabled
 * staff when none is picked (`staffHint`, `staffNotOffered`).
 */

import { t } from "../../i18n/l10n.mjs";
export { captureToast } from "../_goal/goalMode.mjs";

/**
 * What is said when projects handed over with a capture could not be
 * attached to the goal it made — the goal stands, each project was tried on
 * its own, and the first reason is the sentence's. `null` when every one
 * was attached.
 * @param {readonly {project: string, reason: string}[]} refused
 */
export function attachRefusedWords(refused) {
  if (!refused || refused.length === 0) return null;
  return t("work-new-goal-projects-not-attached", { n: refused.length, reason: refused[0].reason });
}

/**
 * The `POST /goals` body for what the dialog holds. Only what was set
 * travels: an empty list is absence on the wire. `assignees` is who carries
 * the goal — the agents and teams the person picked, in the wire's word.
 *
 * @param {{
 *   statement: string,
 *   mode: import("../../types").GoalMode,
 *   assignees: string[],
 *   tags: string[],
 *   documents: import("../../types").AttachmentRef[],
 * }} form
 */
export function goalBody(form) {
  /** @type {Record<string, unknown>} */
  const body = { statement: form.statement.trim(), mode: form.mode };
  if (form.assignees.length > 0) body.assignees = form.assignees;
  if (form.tags.length > 0) body.tags = form.tags;
  if (form.documents.length > 0) body.documents = form.documents.map(attachmentRef);
  return body;
}

/**
 * A document as the wire takes it — the four keys of an `AttachmentRef` and
 * nothing a row carried beside them (its selection, a preview): the node
 * refuses a key it does not know.
 * @param {{ sha256: string, name: string, mime: string, size: number }} doc
 */
function attachmentRef(doc) {
  return { sha256: doc.sha256, name: doc.name, mime: doc.mime, size: doc.size };
}

/**
 * May *Capture* be pressed? Not without a statement, not while a request is
 * out, and not while a document is still uploading: a goal captured before
 * its context arrived would run without it.
 *
 * @param {{ statement: string, busy: boolean, uploading: boolean }} s
 */
export function canSubmit(s) {
  return s.statement.trim().length > 0 && !s.busy && !s.uploading;
}

/**
 * The label on the button: what pressing it does, or why it waits.
 *
 * @param {{ busy: boolean, uploading: boolean }} s
 */
export function captureLabel(s) {
  if (s.busy) return t("work-new-goal-capturing");
  if (s.uploading) return t("work-new-goal-uploading");
  return t("work-new-goal-capture");
}

/**
 * The hint under the Documents section: what is held, or what the section is for.
 *
 * @param {number} count
 */
export function documentsHint(count) {
  if (count === 0) return t("work-new-goal-optional-brief-spec-screenshot-spreadsheet-kept");
  return count === 1 ? t("work-new-goal-1-document-goes-goal") : t("work-new-goal-documents-go-goal", { count });
}

/**
 * The agents *Who carries it* never offers: the platform's own — they run
 * the platform and are never given a step — and the disabled ones, which no
 * design may name. Teams stood down are the picker's own rule.
 * @param {readonly {id: string, origin: unknown, enabled: boolean}[]} agents
 * @returns {string[]}
 */
export function staffNotOffered(agents) {
  return (agents ?? []).filter((a) => a.origin === "core" || !a.enabled).map((a) => a.id);
}

/**
 * What *Who carries it* says under itself: that it is optional and what
 * nobody picked means, or what the picked ones are for.
 * @param {number} count how many agents and teams are picked
 */
export function staffHint(count) {
  return count === 0 ? t("work-new-goal-dialog-staff-hint-none") : t("work-new-goal-dialog-staff-hint-some");
}
