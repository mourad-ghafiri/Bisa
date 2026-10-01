/**
 * The words around throwing a change away in Git › Changes (ide/04): what the
 * confirmation says before a discard or a delete, and what the toast says
 * after. Pure, so `node --test` reads them.
 *
 * Two verbs, two fates. **Discard** is the consented tier's
 * `POST …/git/discard {paths}`: the working tree goes back to the index, the
 * index is left alone, and a recovery ref under `refs/bisa/safety/` is
 * written first — the toast names it. **Delete** is the IDE's disposal for a
 * file git has never seen: the Trash when the root says so, else gone; the
 * file tree's own sentence, reused here so the two doors agree.
 */

import { deleteCopy as treeDeleteCopy } from "../../ui/fileTreeMutations.mjs";
import { confirmLabel } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

const SAFETY = "refs/bisa/safety/";

/** `refs/bisa/safety/<name>` → `<name>`. */
export function shortRef(ref) {
  return String(ref ?? "").startsWith(SAFETY) ? String(ref).slice(SAFETY.length) : String(ref ?? "");
}

function names(paths) {
  const list = [...paths];
  if (list.length <= 3) return list.join(", ");
  return t("work-git-discard-more", { list: list.slice(0, 2).join(", "), list2: list.length - 2 });
}

const count = (n) => t("work-git-discard-file-files", { n });

/**
 * The confirmation before a discard — of one file, of several, or of every
 * file under a folder (`under`), which the title names.
 * @param {readonly string[]} paths
 * @param {{under?: string | null}} [of]
 */
export function discardCopy(paths, { under = null } = {}) {
  const n = paths.length;
  const title = under ? t("work-git-discard-discard-changes-under", { under }) : n === 1 ? t("work-git-discard-discard-changes", { paths: paths[0] }) : t("work-git-discard-discard-changes-files", { n });
  // Two sentences; the recovery promise is the dialog's `SafetyNote`, not a third.
  const what = under ? t("work-git-discard-the-under", { count: count(n), under }) : n === 1 ? t("work-git-discard-file") : names(paths);
  const body = [t("work-git-discard-working-tree-goes-back", { what }), t("work-git-discard-staged-change-kept-unstage-first-if")].join(" ");
  return { title, body, confirm: confirmLabel("discard", { count: n }), danger: true };
}

/**
 * The confirmation before untracked files are deleted — the file tree's own
 * words for a git root, so the two doors say the same thing; a folder's
 * delete names the folder in the title and takes its untracked files only;
 * the toolbar's (`all`) says it is every untracked file in the checkout.
 * @param {readonly string[]} paths
 * @param {"trash" | "unlink" | null} disposal
 * @param {{under?: string | null, all?: boolean}} [of]
 */
export function deleteCopy(paths, disposal, { under = null, all = false } = {}) {
  const copy = treeDeleteCopy({ targets: paths.map((path) => ({ path, dir: false })), counts: {}, disposal, gitRoot: true });
  if (under) return { ...copy, title: t("work-git-discard-delete-under", { paths: count(paths.length), under }) };
  if (all) return { ...copy, title: paths.length === 1 ? t("work-git-discard-delete-one-untracked-file", { paths: paths[0] }) : t("work-git-discard-delete-every-untracked-file", { paths: paths.length }) };
  return copy;
}

/**
 * What a Safety row says its recovery ref holds — one word per
 * `bisa_vcs::RecoveryKind`, which the test reads from the Rust source.
 * @param {"commit" | "tree" | "stash"} kind
 */
export function recoveryWords(kind) {
  switch (kind) {
    case "commit":
      return t("work-git-discard-commit-pinned");
    case "tree":
      return t("work-git-discard-index-tree-saved");
    case "stash":
      return t("work-git-discard-stash-entry-saved");
    default:
      return String(kind);
  }
}

/**
 * The confirmation before *Restore*: what comes back depends on the kind.
 * @param {{ ref_name: string; kind: "commit" | "tree" | "stash"; branch?: string | null }} rec
 */
export function restoreWords(rec) {
  const short = shortRef(rec.ref_name);
  switch (rec.kind) {
    case "stash":
      return t("work-git-discard-put-stash-entry-saved-back-stash", { short });
    case "tree":
      return t("work-git-discard-put-back-what-saved-head-then", { short, branch: rec.branch ?? t("work-git-discard-commit") });
    default:
      return t("work-git-discard-put-back-what-saved-head-restoring", { short, branch: rec.branch ?? t("work-git-discard-commit") });
  }
}

/**
 * The toast after a discard landed.
 * @param {readonly string[]} paths
 * @param {string} ref the recovery ref the node wrote
 */
export function discardedWords(paths, ref) {
  const n = paths.length;
  const what = n === 1 ? t("work-git-discard-discarded-changes", { paths: paths[0] }) : t("work-git-discard-discarded-changes-files", { n });
  return t("work-git-discard-what-there-saved", { what, ref: shortRef(ref) });
}

/**
 * The toast after a delete landed: the file by name, or how many.
 * @param {readonly string[]} paths
 * @param {"trash" | "unlink"} disposal
 */
export function deletedWords(paths, disposal) {
  const what = paths.length === 1 ? paths[0] : count(paths.length);
  return disposal === "trash" ? t("work-git-discard-moved-trash", { what }) : t("work-git-discard-deleted", { what });
}
