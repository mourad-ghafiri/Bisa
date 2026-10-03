/**
 * The two sides of a conflict in the person's words (ide/04 §Conflicts,
 * continued): **mine** — what they have — and **theirs** — what is coming
 * in — named by branch and commit from the node's operation facts
 * (`GET …/git/operation`), so a merge reads *main — your branch* against
 * *feature/login — incoming* and a rebase *your commit "Add login"* against
 * *main — already there*. This is the one place git's `ours` and `theirs`
 * are turned around: under a rebase git's `ours` is the branch rebased onto
 * and `theirs` the commit replayed — the person's own work — so *mine* is
 * git's `theirs` there, and every take, every block choice and every toast
 * reads the mapping from here. Also here: what each kind of conflict is,
 * the two explicit choices a deleted-or-added-by-one-side path offers, and
 * the three sentences that say what a conflict is. Pure, so `node --test`
 * reads it.
 */

import { IN_PROGRESS_LABEL } from "./syncModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The glyph each side's name wears, everywhere. A side is an identity, not a
 * state and not a summons, so both are drawn in neutral ink and told apart by
 * this mark and their names — the accent stays for what waits on the person,
 * and `ok` for what is done.
 */
export const SIDE_ICON = Object.freeze({ mine: "person", theirs: "branch" });

const short = (sha) => (typeof sha === "string" && sha.length >= 7 ? sha.slice(0, 7) : null);

/** git's word for a side of the person's — the swap, once. */
export function gitSideOf(inProgress, side) {
  const swapped = inProgress === "rebase";
  if (side === "mine") return swapped ? "theirs" : "ours";
  return swapped ? "ours" : "theirs";
}

/** Whether *mine* is git's `theirs` — true under a rebase. */
export function isSwapped(inProgress) {
  return inProgress === "rebase";
}

/**
 * The name a side reads under, from the facts: its branch, else its
 * commit's subject in quotes, else its short sha, else `fallback`.
 * @param {{name?: string | null, commit?: string | null, subject?: string | null} | null | undefined} ref
 * @param {string} fallback
 */
function nameOf(ref, fallback) {
  if (ref?.name) return ref.name;
  if (ref?.subject) return `"${ref.subject}"`;
  return short(ref?.commit) ?? fallback;
}

/**
 * The two sides, the sentence that says what is happening, and the step.
 * `facts` is the node's, or null for an operation it could not describe —
 * the words then say *your branch* and *the other side*.
 * @param {"merge" | "rebase" | "cherry_pick" | "revert" | null | undefined} inProgress
 * @param {import("../../types").GitOperationFacts | null | undefined} facts
 * @returns {{mine: Side, theirs: Side, explain: string, step: string | null, title: string}}
 * @typedef {{key: "mine" | "theirs", git: "ours" | "theirs", name: string, role: string, icon: "person" | "branch"}} Side
 */
export function sidesOf(inProgress, facts) {
  // The operation in progress names the kind — and so the swap (ide/04): facts
  // for another operation are stale and not read, but the swap still holds.
  const kind = inProgress ?? facts?.kind ?? "merge";
  const f = facts && facts.kind === kind ? facts : null;
  const mine = { key: "mine", git: gitSideOf(kind, "mine"), icon: SIDE_ICON.mine };
  const theirs = { key: "theirs", git: gitSideOf(kind, "theirs"), icon: SIDE_ICON.theirs };
  const branch = f?.branch ?? null;
  const stepWords = f?.step ? t("work-conflict-sides-commit", { done: f.step.done, total: f.step.total }) : null;
  switch (kind) {
    case "rebase": {
      const onto = nameOf(f?.ours, t("work-conflict-sides-branch-rebasing-onto"));
      const commit = f?.theirs?.subject ? t("work-conflict-sides-commit-2", { subject: f.theirs.subject }) : short(f?.theirs?.commit) ? t("work-conflict-sides-commit-3", { commit: short(f.theirs.commit) }) : t("work-conflict-sides-commit-4");
      return {
        mine: { ...mine, name: commit, role: branch ? t("work-conflict-sides-from-work-being-replayed", { branch }) : t("work-conflict-sides-work-being-replayed") },
        theirs: { ...theirs, name: onto, role: t("work-conflict-sides-already-there-what-commits-land") },
        explain: t("work-conflict-sides-rebasing-onto-git-replays-commits-one", { branch: branch ?? t("work-conflict-sides-branch"), onto, stepWords, flag: (stepWords) ? "yes" : "no" }),
        step: stepWords,
        title: t("work-conflict-sides-rebasing-onto", { branch: branch ?? t("work-conflict-sides-branch"), onto }),
      };
    }
    case "cherry_pick": {
      const picked = f?.theirs?.subject ? t("work-conflict-sides-commit-5", { subject: f.theirs.subject }) : short(f?.theirs?.commit) ? t("work-conflict-sides-commit-short", { commit: short(f.theirs.commit) }) : t("work-conflict-sides-commit-being-picked");
      return {
        mine: { ...mine, name: branch ?? t("work-conflict-sides-branch"), role: t("work-conflict-sides-branch-2") },
        theirs: { ...theirs, name: picked, role: t("work-conflict-sides-being-picked-onto") },
        explain: t("work-conflict-sides-cherry-picking-onto-where-commit-branch", { picked, branch: branch ?? t("work-conflict-sides-branch"), stepWords, flag: (stepWords) ? "yes" : "no" }),
        step: stepWords,
        title: t("work-conflict-sides-cherry-picking-onto", { picked, branch: branch ?? t("work-conflict-sides-branch") }),
      };
    }
    case "revert": {
      const undone = f?.theirs?.subject ? `"${f.theirs.subject}"` : short(f?.theirs?.commit) ? t("work-conflict-sides-commit-short", { commit: short(f.theirs.commit) }) : t("work-conflict-sides-commit-6");
      return {
        mine: { ...mine, name: branch ?? t("work-conflict-sides-branch"), role: t("work-conflict-sides-branch-2") },
        theirs: { ...theirs, name: `undoing ${undone}`, role: t("work-conflict-sides-what-revert-would-make") },
        explain: t("work-conflict-sides-reverting-new-commit-undoes-where-later", { undone, branch: branch ?? t("work-conflict-sides-branch"), stepWords, flag: (stepWords) ? "yes" : "no" }),
        step: stepWords,
        title: t("work-conflict-sides-reverting", { undone, branch: branch ?? t("work-conflict-sides-branch") }),
      };
    }
    default: {
      const source = nameOf(f?.theirs, t("work-conflict-sides-other-side"));
      const remote = f?.theirs?.role === "upstream";
      return {
        mine: { ...mine, name: branch ?? t("work-conflict-sides-branch"), role: t("work-conflict-sides-branch-what-have-here") },
        theirs: { ...theirs, name: source, role: remote ? t("work-conflict-sides-remote-incoming") : t("work-conflict-sides-incoming-branch-merged") },
        explain: t("work-conflict-sides-merging-into-everything-changed-only-one", { source, branch: branch ?? t("work-conflict-sides-branch") }),
        step: null,
        title: t("work-conflict-sides-merging-into", { source, branch: branch ?? t("work-conflict-sides-branch") }),
      };
    }
  }
}

/** The side the person's word names, from the pair. */
function sideNamed(sides, key) {
  return key === "mine" ? sides.mine : sides.theirs;
}

/** The side git's word names, from the pair. */
export function sideByGit(sides, git) {
  return sides.mine.git === git ? sides.mine : sides.theirs;
}

/**
 * What kind of conflict a path is, in words: the chip's short word and the
 * sentence under it, naming the sides.
 * @param {import("../../types").GitConflictKind | null | undefined} kind
 * @param {{mine: Side, theirs: Side}} sides
 * @returns {{short: string, sentence: string}}
 */
export function kindWords(kind, sides) {
  const ours = sideByGit(sides, "ours").name;
  const theirs = sideByGit(sides, "theirs").name;
  switch (kind) {
    case "both_added":
      return { short: t("work-conflict-sides-added-both"), sentence: t("work-conflict-sides-each-added-file-differently", { ours, theirs }) };
    case "both_deleted":
      return { short: t("work-conflict-sides-deleted-both"), sentence: t("work-conflict-sides-both-sides-deleted-file-git-still") };
    case "deleted_by_us":
      return { short: t("work-conflict-sides-deleted", { ours }), sentence: t("work-conflict-sides-deleted-file-changed", { ours, theirs }) };
    case "deleted_by_them":
      return { short: t("work-conflict-sides-deleted-2", { theirs }), sentence: t("work-conflict-sides-deleted-file-changed-2", { theirs, ours }) };
    case "added_by_us":
      return { short: t("work-conflict-sides-added", { ours }), sentence: t("work-conflict-sides-added-file-where-has-something-else", { ours, theirs }) };
    case "added_by_them":
      return { short: t("work-conflict-sides-added-2", { theirs }), sentence: t("work-conflict-sides-added-file-where-has-something-else-2", { theirs, ours }) };
    default:
      return { short: t("work-conflict-sides-both-changed"), sentence: t("work-conflict-sides-both-changed-file", { ours, theirs }) };
  }
}

/** A kind settled by a choice between two whole outcomes, never block by block. */
export function isWholeFileKind(kind) {
  return kind === "both_deleted" || kind === "deleted_by_us" || kind === "deleted_by_them";
}

/**
 * The explicit choices a path that cannot be merged block by block offers
 * — a side deleted it, a binary file, a file added on both sides — each
 * naming what it does and the resolution git makes of it (`take`).
 * @param {import("../../types").GitConflictKind | null | undefined} kind
 * @param {{mine: Side, theirs: Side}} sides
 * @returns {{id: string, label: string, hint: string, take: "ours" | "theirs" | "delete", danger: boolean}[]}
 */
export function kindChoices(kind, sides) {
  const ours = sideByGit(sides, "ours");
  const theirs = sideByGit(sides, "theirs");
  const keep = (side) => ({ id: `keep_${side.key}`, label: t("work-conflict-sides-keep-mine-theirs", { flag: (side.key === "mine") ? "yes" : "no", side: side.name }), hint: t("work-conflict-sides-file-becomes-s-version-whole-staged", { side: side.name }), take: side.git, danger: false });
  const remove = (side) => ({ id: "delete", label: t("work-conflict-sides-delete-did", { side: side.name }), hint: t("work-conflict-sides-file-removed-from-tree-index-safety"), take: "delete", danger: true });
  switch (kind) {
    case "deleted_by_us":
      return [keep(theirs), remove(ours)];
    case "deleted_by_them":
      return [keep(ours), remove(theirs)];
    case "both_deleted":
      return [remove(sideNamed(sides, "theirs"))];
    default:
      return [keep(sideNamed(sides, "mine")), keep(sideNamed(sides, "theirs"))];
  }
}

/**
 * What a conflict is, for the person who has never met one: three
 * sentences and the promise.
 * @param {"merge" | "rebase" | "cherry_pick" | "revert" | null | undefined} inProgress
 * @returns {string[]}
 */
export function whatIsAConflict(inProgress) {
  const word = IN_PROGRESS_LABEL[inProgress] ?? "operation";
  const second =
    inProgress === "rebase"
      ? t("work-conflict-sides-everything-else-commit-already-landed-branch")
      : t("work-conflict-sides-everything-changed-only-one-side-already");
  return [
    t("work-conflict-sides-both-sides-changed-same-lines"),
    second,
    t("work-conflict-sides-nothing-lost-abort-puts-branch-back", { word }),
  ];
}

/** The toast after a side is taken whole. */
export function tookWords(path, take, sides) {
  if (take === "delete") return t("work-conflict-sides-deleted-settled", { path });
  const side = sideByGit(sides, take);
  return t("work-conflict-sides-kept-staged", { path, key: side.key, side: side.name });
}

/**
 * The question drafted for an agent about one conflict, and the text the
 * chip carries: the block with both sides named.
 * @param {string} path
 * @param {{ours: string, theirs: string, base: string | null}} conflict
 * @param {{mine: Side, theirs: Side, explain: string}} sides
 * @param {{at: number, of: number}} where
 */
export function agentQuestion(path, conflict, sides, where) {
  const mine = sides.mine.git === "ours" ? conflict.ours : conflict.theirs;
  const theirs = sides.mine.git === "ours" ? conflict.theirs : conflict.ours;
  const text = [t("work-conflict-sides-conflict-of-in", { at: where.at, of: where.of, path }), sides.explain, "", `--- mine: ${sides.mine.name} (${sides.mine.role})`, mine.replace(/\n$/, ""), `--- theirs: ${sides.theirs.name} (${sides.theirs.role})`, theirs.replace(/\n$/, "")];
  if (conflict.base !== null) text.push("--- base: what both started from", conflict.base.replace(/\n$/, ""));
  return {
    text: text.join("\n"),
    question: t("work-conflict-sides-explain-conflict-what-each-side-changed", { path }),
  };
}
