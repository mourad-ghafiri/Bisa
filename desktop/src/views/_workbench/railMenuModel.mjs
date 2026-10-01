/**
 * The verbs a workstream row offers (ide/07), as facts — the rail's rows and
 * the Workstreams panel's rows are one list, so the two can never disagree.
 * The views map ids to actions; a harness item carries the harness it starts.
 * The verbs an agent row under it offers are here too (`agentRowMenuSpec`),
 * so *Abort* is offered by the one rule every Stop reads.
 */

import { isStoppable } from "../../ui/sessionState.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * @param {{
 *   primary: boolean,          the project's own root — it has no diff and cannot be closed
 *   exists: boolean,           the checkout is on disk — a shell needs it
 *   harnesses: readonly {id: string, label: string}[],  the installed, launchable harnesses
 *   rename: boolean,           the surface has a rename field to offer
 *   close: boolean,            the surface has a close confirmation to offer
 * }} ctx
 * @returns {{id: string, label: string, harness?: string, danger?: boolean, disabled?: boolean, separatorBefore?: boolean}[]}
 */
export function workstreamMenuSpec({ primary, exists, harnesses, rename, close }) {
  const items = [{ id: "open", label: t("workbench-rail-menu-open") }];
  if (rename) items.push({ id: "rename", label: t("workbench-project-rail-rename") });
  items.push({ id: "new-shell", label: t("workbench-rail-menu-new-shell-here"), disabled: !exists, separatorBefore: true });
  for (const h of harnesses) items.push({ id: `harness:${h.id}`, label: t("workbench-rail-menu-new-here", { h: h.label }), harness: h.id, disabled: !exists });
  if (!primary) {
    // A branch's own verbs: the diff against its base, and the end. Its pull
    // request needs no verb — *Open* lands on the Workstreams panel, whose
    // lifecycle it is.
    items.push({ id: "diff", label: t("workbench-right-panel-diff-against-base"), separatorBefore: true });
    if (close) items.push({ id: "close", label: t("workbench-rail-menu-close-workstream"), danger: true });
  }
  return items;
}

/**
 * The verbs a session row on the rail offers (ide/09). A harness in a
 * terminal is answered and ended there: its one verb is *Terminate*, and only
 * while something can be ended. An engine session opens in the Agent panel,
 * answers a gate in the Inbox, and can be aborted only while it runs or
 * waits on the person (`isStoppable`) — never idle between turns. A
 * sub-agent row offers nothing of its own.
 * @param {{parent: string | null, terminalKey: string | null, gateId: string | null, state: string | object}} row
 * @returns {{id: string, label: string, danger?: boolean, separatorBefore?: boolean}[]}
 */
export function agentRowMenuSpec(row) {
  if (row.parent !== null) return [];
  const stoppable = isStoppable(row.state);
  if (row.terminalKey) return stoppable ? [{ id: "terminate", label: t("workbench-project-rail-terminate"), danger: true }] : [];
  const items = [{ id: "show", label: t("workbench-rail-menu-show-agent-panel") }];
  if (row.gateId) items.push({ id: "answer", label: t("workbench-rail-menu-answer-inbox") });
  if (stoppable) items.push({ id: "abort", label: t("workbench-rail-menu-abort"), danger: true, separatorBefore: true });
  return items;
}
