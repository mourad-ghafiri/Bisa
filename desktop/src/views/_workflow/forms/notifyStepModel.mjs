/**
 * The notify step's scope as the form lists it — and a boundary's post's,
 * which shares the field. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../../i18n/l10n.mjs";

/**
 * Whether the scope is one the select carries: none, a channel the
 * workspace has, a goal or a workstream the lists hold, or a text input the
 * workflow declares. A scope the list does not carry — an id gone from the
 * workspace, an input no longer declared — is shown as it is rather than as
 * the first option.
 * @param {string | null | undefined} scope
 * @param {readonly string[]} channelIds
 * @param {readonly string[]} textInputNames
 * @param {readonly string[]} [goalIds]
 * @param {readonly string[]} [workstreamIds]
 */
export function scopeListed(scope, channelIds, textInputNames, goalIds = [], workstreamIds = []) {
  const s = scope ?? "";
  return (
    s === "" ||
    (channelIds ?? []).includes(s) ||
    (goalIds ?? []).includes(s) ||
    (workstreamIds ?? []).includes(s) ||
    (textInputNames ?? []).some((name) => `{inputs.${name}}` === s)
  );
}

/** A goal's name as an option reads it: its title, else its statement, cut short. */
function shortTitle(text) {
  const s = String(text ?? "").trim();
  return s.length > 48 ? `${s.slice(0, 47)}…` : s;
}

/**
 * The options the scope select offers: the run's own conversation first,
 * then the channels, the goals (the open ones the workspace lists), the
 * workstreams (a nameless one by its id's tail, under its project's name)
 * and the text inputs read at start — each `{ value, label }`, the label in
 * words.
 * @param {{
 *   channels: readonly { id: string, name: string }[],
 *   goals: readonly { id: string, title?: string | null, statement?: string }[],
 *   workstreams: readonly { project_name?: string | null, workstream: { id: string, name?: string | null } }[],
 *   textInputs: readonly { name: string }[],
 * }} lists
 */
export function scopeOptions({ channels, goals, workstreams, textInputs }) {
  return {
    first: t("workflow-notify-step-form-goal-s-conversation"),
    channels: (channels ?? []).map((c) => ({ value: c.id, label: t("workflow-notify-step-form-channel", { channel: c.name }) })),
    goals: (goals ?? []).map((g) => ({ value: g.id, label: t("workflow-notify-step-form-goal", { goal: shortTitle(g.title || g.statement || g.id) }) })),
    workstreams: (workstreams ?? []).map((w) => ({
      value: w.workstream.id,
      label: t("workflow-notify-step-form-workstream", { project: w.project_name ?? "", workstream: w.workstream.name || w.workstream.id.slice(-6) }),
    })),
    inputs: (textInputs ?? []).map((i) => ({ value: `{inputs.${i.name}}`, label: t("workflow-connector-step-form-input", { i: i.name }) })),
  };
}
