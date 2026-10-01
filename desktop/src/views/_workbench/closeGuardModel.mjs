/**
 * The question a close asks when it would lose unsaved work (ide/03 §Tabs),
 * as words: what is being closed, and the three answers. The platform's own
 * wording for this question — *Save*, *Don't save*, *Cancel* — so nobody has
 * to work out whether *Discard* throws away the file or only the edits.
 *
 * Which tabs hold unsaved work is the buffers' fact (`docBuffersStore.ts`);
 * this only says it.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The answers, in the footer's order: the safe way out, the loss, the save. */
export const GUARD_VERBS = Object.freeze({ cancel: tr("workbench-capture-tray-cancel"), discard: tr("workbench-close-guard-don-t-save"), save: tr("workbench-editor-doc-save") });

/**
 * @param {readonly {label: string, untitled: boolean}[]} tabs the closing tabs that hold unsaved work
 * @returns {{title: string, description: string, note: string}}
 */
export function guardWords(tabs) {
  const list = tabs ?? [];
  const one = list.length === 1;
  const untitled = list.filter((t) => t.untitled).length;
  const title = one ? tr("workbench-close-guard-save-changes", { list: list[0].label }) : tr("workbench-close-guard-save-changes-documents", { list: list.length });
  const description = one ? tr("workbench-close-guard-has-changes-never-saved") : tr("workbench-close-guard-have-changes-never-saved", { t: list.map((t) => t.label).join(", ") });
  const naming = untitled === 0 ? "" : one ? ` ${tr("workbench-close-guard-has-no-name-yet-saving-asks")}` : untitled === 1 ? ` ${tr("workbench-close-guard-one-has-no-name-yet-saving")}` : ` ${tr("workbench-close-guard-have-no-name-yet-saving-asks", { untitled })}`;
  return {
    title,
    description,
    note: tr("workbench-close-guard-not-saving-drops-what-typed-not", { flag: (one) ? "yes" : "no", naming }),
  };
}
