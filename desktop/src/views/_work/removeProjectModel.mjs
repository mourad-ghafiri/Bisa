/**
 * Removing a project, as facts: the acts on offer, the words each says, and
 * what is said once one is done. One vocabulary for every surface that asks —
 * the rail's menu and the project's own page — so the two cannot promise
 * different things about the same call.
 *
 * Three acts, from the mildest: **archive** (hidden from the rail, one move
 * back), **forget** (the records go, the disk is not touched) and **delete**
 * the folder. How a folder goes is `editor.delete.trash`'s word, the way a
 * file's is: the OS Trash, recoverable, or off disk for good — the node does
 * what the setting says, so the words follow it and never assume. An adopted
 * folder is never deleted from here: the node refuses it, and the act is
 * shown held, with that reason, rather than left out.
 */

import { t } from "../../i18n/l10n.mjs";

/** The acts, mildest first — the order they are offered in. */
export const REMOVE_ACTS = ["archive", "forget", "delete"];

/** `n checkouts`, in words a sentence can hold. @param {number} n */
export function checkoutWords(n) {
  return t("work-remove-project-checkouts", { n });
}

/**
 * The acts on offer for one project.
 * @param {{archived: boolean, adopted: boolean, trash: boolean, checkouts?: number}} facts
 *   `trash` — `editor.delete.trash` as it resolves here; `checkouts` — how many the folder holds, when known
 * @returns {{id: "archive" | "forget" | "delete", label: string, description: string, tone: "default" | "danger", disabled: string | null}[]}
 */
export function removeChoices({ archived, adopted, trash, checkouts }) {
  const choices = [];
  if (!archived) {
    choices.push({ id: "archive", label: t("work-remove-project-archive"), description: t("work-remove-project-hides-from-rail-stops-every-session"), tone: "default", disabled: null });
  }
  choices.push({
    id: "forget",
    label: t("work-remove-project-forget-keep-files"),
    description: adopted ? t("work-remove-project-drops-project-workstreams-from-bisa-folder") : t("work-remove-project-drops-project-workstreams-from-bisa-folder-2"),
    tone: "default",
    disabled: null,
  });
  const holds = ` ${typeof checkouts === "number" ? t("work-remove-project-counted-checkouts-workstreams-notes", { checkouts: checkoutWords(checkouts) }) : t("work-remove-project-checkouts-workstreams-notes")}`;
  choices.push({
    id: "delete",
    label: trash ? t("work-remove-project-move-folder-trash") : t("work-remove-project-delete-folder-good"),
    description: trash ? t("work-remove-project-forgets-project-moves-whole-folder-trash", { holds }) : t("work-remove-project-forgets-project-removes-whole-folder-from", { holds }),
    tone: "danger",
    disabled: adopted ? t("work-remove-project-folder-adopted-not-created-bisa-so") : null,
  });
  return choices;
}

/**
 * Whether an act asks once more before it runs: only a folder going for
 * good. The Trash is its own undo; archive and forget touch nothing on disk.
 * @param {string} act @param {boolean} trash
 */
export function asksAgain(act, trash) {
  return act === "delete" && !trash;
}

/**
 * The second question's words, for a folder going for good.
 * @param {{path?: string | null, checkouts?: number}} facts
 */
export function forGoodWords({ path, checkouts }) {
  const what = path ? `\`${path}\`` : t("work-remove-project-project-s-folder");
  const inside = typeof checkouts === "number" ? t("work-remove-project-including-counted", { checkouts: checkoutWords(checkouts) }) : t("work-remove-project-including-every-checkout");
  return {
    title: t("work-remove-project-delete-folder-from-disk"),
    body: t("work-remove-project-permanently-removes-everything-inside-deleting-trash", { what, inside }),
    confirmLabel: t("work-remove-project-delete-folder"),
  };
}

/**
 * What is said once an act is done. `answer` is the node's, when the act was
 * a delete: a folder that was asked to go and could not is still on disk
 * (`kept`), and saying it went would be the one lie a delete must not tell.
 * @param {string} act
 * @param {boolean} trash
 * @param {{removed_tree?: boolean, kept?: string | null, path?: string} | null} [answer]
 * @returns {{tone: "ok" | "warn", text: string}}
 */
export function removedSaid(act, trash, answer = null) {
  if (act === "archive") return { tone: "ok", text: t("work-remove-project-project-archived-every-session-stopped-nothing") };
  if (act === "forget") return { tone: "ok", text: t("work-remove-project-project-forgotten-files-still-there") };
  if (answer && answer.removed_tree === false) {
    const where = answer.path ? ` (${answer.path})` : "";
    const why = answer.kept ? `: ${answer.kept}` : "";
    return { tone: "warn", text: t("work-remove-project-project-forgotten-but-folder-could-not", { where, why }) };
  }
  return { tone: "ok", text: trash ? t("work-remove-project-project-forgotten-folder-trash") : t("work-remove-project-project-folder-deleted") };
}

/**
 * Whether the project's folder is one a person pointed at, not one the
 * workspace made (`ProjectRoot::External`): the platform never deletes it,
 * so the dialog offers no *Delete the folder* for it — and a record this
 * build cannot read is treated as adopted, since offering to delete a folder
 * on a guess is the one mistake this dialog exists to prevent.
 * @param {{root?: {type?: string} | null} | null | undefined} project
 */
export function isAdopted(project) {
  return project?.root?.type !== "managed";
}


/**
 * The roots a project stands on — its primary, which shares its id, and
 * every other workstream of it — as the workspace lists them: what a removal
 * that forgets the project takes with it.
 * @param {readonly {workstream: {id: string, project: string}}[] | null | undefined} workstreams the workspace's list
 * @param {string} pid
 * @returns {string[]} workstream ids, the primary first, none twice
 */
export function projectRoots(workstreams, pid) {
  const ids = [pid];
  for (const ref of workstreams ?? []) if (ref.workstream.project === pid && !ids.includes(ref.workstream.id)) ids.push(ref.workstream.id);
  return ids;
}

/**
 * Whether an act takes the project's roots away: a forgotten project and a
 * deleted one leave no workstream to stand in, so what this app holds on
 * them — a shell's tab, a browser tab, the open documents — goes with them.
 * An archived project is still there, hidden: one move brings it back, and
 * its shells with it.
 * @param {string} act
 */
export function rootsGo(act) {
  return act === "forget" || act === "delete";
}

/**
 * Whether the person stands on what was removed — the project's own root or
 * any checkout of it — and so is taken to the IDE's landing once the node
 * has answered, and never before: a removal the node refused moves nobody.
 * @param {{scope: string, id: string} | null | undefined} current the root the workbench is on
 * @param {readonly string[]} roots `projectRoots`
 */
export function standsOn(current, roots) {
  return current?.scope === "workstream" && roots.includes(current.id);
}
