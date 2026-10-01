/**
 * The project's run command (ide/18), as facts: the Terminal caret's one
 * item for it, which door ⌘⇧R opens — a server up, the approved command,
 * else the folder picker — and the words an unapproved command is held
 * with. The node resolves the command and this machine approves it
 * (About › Settings › Workstream scripts); this only orders the words. The
 * Browser's own items are `serversModel.mjs`'s. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { serverLabel } from "./serversModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Where the run command is set and approved: the Workstream scripts card. */
export const APPROVAL_PLACE = t("workbench-run-command-about-settings-workstream-scripts");

/** What an unapproved command is held with — the toast, and the card it opens. @param {string} command */
export function approvalWords(command) {
  return t("workbench-run-command-not-approved-machine-review-approve-under", { command, APPROVAL_PLACE });
}

/**
 * ⌘⇧R's door: open the newest server when one is up (never a second one
 * for the same folder), else run the project's command when it is set
 * **and approved**, else *From folder…* — the picker, on the folder served
 * last. A door, not a button: whichever menu holds the item that is it — the
 * Browser's for a server or the picker, the Terminal's for the run command —
 * marks it with the chord. An
 * unapproved command is no door: the chord never opens a settings card.
 * @param {{run: {command: string, trusted: boolean} | null, servers: readonly {id: string, url: string, owner: object, port: number}[]}} facts
 * @returns {{id: string, hint: string}}
 */
export function runDoor({ run, servers }) {
  const newest = (servers ?? []).at(-1) ?? null;
  if (newest) return { id: `open:${newest.id}`, hint: t("workbench-run-command-open-browser", { newest: serverLabel(newest) }) };
  if (run?.command && run.trusted) return { id: "run", hint: t("workbench-run-command-run-terminal-here", { command: run.command }) };
  return { id: "serve-folder", hint: t("workbench-run-command-choose-root-folder-checkout-serve-port") };
}

/**
 * The Terminal caret's one item for the run command, or `null` when the
 * project sets none — no *Set…* item anywhere: the setting is the card's.
 * *Run `npm run dev`* when this machine has approved it, held by `busy`;
 * *Run `npm run dev` — not approved here* when it has not — a press opens
 * the card, which no start in flight holds. The chord rides the approved
 * item when it is ⌘⇧R's door.
 * @param {{command: string, trusted: boolean} | null} run
 * @param {{door: string | null, busy: boolean}} facts `door` — ⌘⇧R's target id; `busy` — no checkout on disk yet
 */
export function runCommandItem(run, { door, busy }) {
  if (!run?.command) return null;
  if (!run.trusted) {
    return { id: "approve-run", label: t("workbench-run-command-run-not-approved-here", { command: run.command }), hint: approvalWords(run.command), icon: "play", disabled: false, separatorBefore: true };
  }
  const item = { id: "run", label: t("workbench-run-command-run", { command: run.command }), hint: t("workbench-run-command-terminal-here-through-login-shell"), icon: "play", disabled: busy, separatorBefore: true };
  if (door === "run") item.command = "run_project";
  return item;
}
