/**
 * The Changes toolbar's `▾` beside *Stage all* (ide/04), as facts — no
 * React: the other stage scopes, then, apart, the two throw-aways —
 * *Discard all changes…* over every working-tree change and unmerged path,
 * and *Delete all untracked files…* over every file git has never seen,
 * through the IDE's disposal. Each item names its count and is off at
 * zero; the toolbar binds the ids to the panel's acts, never their places,
 * and the panel asks before either throw-away. The scopes are
 * `gitFiles.stageScopes`'s, one source for every bulk verb.
 */

import { VERB, bulkLabel } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * @param {import("./gitFiles.mjs").StageScopes} scopes
 * @returns {{id: "stage_tracked" | "stage_untracked" | "unstage_all" | "discard_all" | "delete_untracked", label: string, paths: string[], disabled: boolean, separatorBefore?: boolean, danger?: boolean}[]}
 */
export function bulkMenu(scopes) {
  const { tracked, untracked, staged, discardable } = scopes;
  return [
    { id: "stage_tracked", label: t("work-changes-bulk-tracked", { stage: VERB.stage, tracked: tracked.count }), paths: tracked.paths, disabled: tracked.count === 0 },
    { id: "stage_untracked", label: t("work-changes-bulk-untracked", { stage: VERB.stage, untracked: untracked.count }), paths: untracked.paths, disabled: untracked.count === 0 },
    { id: "unstage_all", label: `${bulkLabel("unstage")} (${staged.count})`, paths: staged.paths, disabled: staged.count === 0, separatorBefore: true },
    { id: "discard_all", label: t("work-changes-bulk-all-changes", { discard: VERB.discard, discardable: discardable.count }), paths: discardable.paths, disabled: discardable.count === 0, separatorBefore: true, danger: true },
    { id: "delete_untracked", label: t("work-changes-bulk-all-untracked-files", { delete: VERB.delete, untracked: untracked.count }), paths: untracked.paths, disabled: untracked.count === 0, danger: true },
  ];
}
