/**
 * The Resolve card's facts (ide/04 §Conflicts, continued): what git has
 * left half-done in a checkout — a merge, a rebase, a cherry-pick, a revert
 * — named by branch and commit from the node's facts, where it stands, the
 * checklist of conflicted files with their kinds, and the three verbs that
 * end it: **Continue** once every conflicted path is settled, **Skip** to
 * leave the stopped commit out (never for a merge, which has nothing to
 * skip), **Abort** to go back to where the tree was. One card above every
 * Git view, one door for each verb; the sync bar and the Branches view point
 * at it and hold no abort of their own. Pure, so `node --test` reads it;
 * `ResolveCard.tsx` draws. The sides' words are `conflictSidesModel`'s.
 */

import { VERB } from "./gitWords.mjs";
import { IN_PROGRESS_LABEL } from "./syncModel.mjs";
import { kindWords, sidesOf } from "./conflictSidesModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The word for the operation — *merge*, *rebase*, *cherry-pick*, *revert*. */
export function operationWord(inProgress) {
  return IN_PROGRESS_LABEL[inProgress] ?? "operation";
}

/** The paths git still lists as unmerged, from the file rows. */
export function conflictedPaths(files) {
  return (Array.isArray(files) ? files : []).filter((f) => f && f.conflicted && typeof f.path === "string").map((f) => f.path);
}

function count(n, noun) {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

/**
 * The card's title, its line, and the checklist. The title and the sides
 * are the node's facts (`sidesOf`) — a rebase begun in a terminal is named
 * as well as one begun here; `operation` is what this app run started, and
 * the 409 that stopped it named the paths, so the card can count what is
 * settled of them. `files` are the rows the panel reads; a conflicted one
 * is a checklist row with its kind, a settled one is ticked.
 * @param {"merge" | "rebase" | "cherry_pick" | "revert"} inProgress
 * @param {import("../../types").GitOperationFacts | null | undefined} facts
 * @param {{kind: string, paths: string[]} | null | undefined} operation
 * @param {readonly import("../../types").GitFileRow[] | null | undefined} files
 */
export function operationWords(inProgress, facts, operation, files) {
  const sides = sidesOf(inProgress, facts);
  const op = operation && operation.kind === inProgress ? operation : null;
  const rows = Array.isArray(files) ? files : [];
  const conflicted = rows.filter((f) => f && f.conflicted && typeof f.path === "string");
  const known = op ? op.paths : [];
  const settledPaths = known.filter((p) => !conflicted.some((f) => f.path === p));
  const checklist = [
    ...conflicted.map((f) => ({ path: f.path, kind: f.conflict ?? null, short: kindWords(f.conflict ?? null, sides).short, settled: false })),
    ...settledPaths.map((p) => ({ path: p, kind: null, short: "settled", settled: true })),
  ];
  const total = known.length > 0 ? known.length : conflicted.length;
  const settled = known.length > 0 ? settledPaths.length : 0;
  const line =
    conflicted.length === 0
      ? t("work-operation-every-file-settled-continue-when-ready")
      : t("work-operation-still-conflicted-settled-conflicted", { file: count(conflicted.length, "file"), settled, total, flag: (known.length > 0) ? "yes" : "no" });
  return { title: sides.title, explain: sides.explain, step: sides.step, line, sides, files: conflicted.map((f) => f.path), checklist, settled, total };
}

/**
 * The three verbs, each with whether it is off and why: Continue waits for
 * the files, Skip is not a merge's, everything waits while another
 * operation runs.
 * @param {"merge" | "rebase" | "cherry_pick" | "revert"} inProgress
 * @param {readonly string[]} conflicted
 * @param {boolean} busy
 */
export function operationVerbs(inProgress, conflicted, busy) {
  const running = busy ? t("work-branch-actions-another-operation-running") : null;
  const n = conflicted.length;
  const unmerged = n > 0 ? t("work-operation-still-conflicted-settle-them-first", { file: count(n, "file"), n }) : null;
  const word = operationWord(inProgress);
  return {
    continue: { label: VERB.continue, disabled: running !== null || unmerged !== null, reason: running ?? unmerged },
    skip: inProgress === "merge" ? null : { label: t("work-operation-commit", { skip: VERB.skip }), disabled: running !== null, reason: running },
    abort: { label: t("work-operation-words", { abort: VERB.abort, word }), disabled: running !== null, reason: running },
  };
}

/**
 * The confirmation before Continue: what the next step makes — for a merge
 * the commit git makes, named from the facts.
 * @param {"merge" | "rebase" | "cherry_pick" | "revert"} inProgress
 * @param {import("../../types").GitOperationFacts | null | undefined} facts
 */
export function continueConsent(inProgress, facts) {
  const word = operationWord(inProgress);
  const source = facts && facts.kind === "merge" ? facts.theirs?.name ?? null : null;
  const body =
    inProgress === "merge"
      ? t("work-operation-merge-commit-made-git-s-prepared", { source, flag: (source) ? "yes" : "no" })
      : inProgress === "rebase"
        ? t("work-operation-commit-made-next-ones-replayed-another")
        : inProgress === "cherry_pick"
          ? t("work-operation-commit-made-next-one-picked-another")
          : t("work-operation-revert-commit-made-next-one-follows");
  return { title: t("work-operation-words-2", { continue: VERB.continue, word }), body, confirm: VERB.continue, danger: false, kind: "tree" };
}

/** The confirmation before Skip: the stopped commit is left out. */
export function skipConsent(inProgress) {
  const word = operationWord(inProgress);
  return { title: t("work-operation-commit-2", { skip: VERB.skip }), body: t("work-operation-commit-stopped-left-out-goes-rest", { word }), confirm: VERB.skip, danger: true, kind: "tree" };
}

/** The confirmation before Abort: the tree goes back. */
export function abortConsent(inProgress) {
  const word = operationWord(inProgress);
  return { title: t("work-operation-words-3", { abort: VERB.abort, word }), body: t("work-operation-branch-goes-back-exactly-where-before", { word }), confirm: VERB.abort, danger: true, kind: "tree" };
}

/** The toasts. */
export function continuedWords(inProgress, stillIn) {
  const word = operationWord(inProgress);
  return stillIn ? t("work-operation-went-stopped-again", { word }) : t("work-operation-continued-done", { word });
}
export function skippedWords(inProgress, stillIn) {
  const word = operationWord(inProgress);
  return stillIn ? t("work-operation-skipped-commit-stopped-again", { word }) : t("work-operation-skipped-commit-done", { word });
}

/**
 * The next conflicted path to settle after `settled` — the one after it in
 * the list, else the first still there, else none.
 * @param {readonly string[]} conflicted the paths still unmerged, in list order
 * @param {string | null} settled the path just settled
 */
export function nextConflict(conflicted, settled) {
  const rest = conflicted.filter((p) => p !== settled);
  if (rest.length === 0) return null;
  const at = settled === null ? -1 : conflicted.indexOf(settled);
  return conflicted.slice(at + 1).find((p) => p !== settled) ?? rest[0];
}
