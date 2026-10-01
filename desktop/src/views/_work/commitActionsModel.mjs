/**
 * What can be done to one commit in the graph (ide/05), said once. Every
 * door reads this list — the row's `⋮` and its right-click menu the sections,
 * the commit document's toolbar every action with its label, a ref chip its
 * own menu — so an action has one label, one glyph, one hint, one rule for
 * when it is off and one reason why — and the words of every confirmation,
 * every toast and every conflict are here too, where a test can read them.
 * Pure: nothing here runs git; `useCommitActions.ts` does.
 *
 * Two tiers, git's own. A **consented** action moves the tree — checkout,
 * cherry-pick, revert, switching to a branch, deleting a tag — so it asks
 * first and a recovery ref is written before anything (ide/04). A branch or
 * a tag *created* is a ref: nothing moves, and it is asked for by name only.
 */

import { t } from "../../i18n/l10n.mjs";

/** The actions on a commit, in the commit document's toolbar order. */
export const COMMIT_ACTIONS = Object.freeze([
  Object.freeze({
    id: "cherry_pick",
    label: t("work-commit-actions-cherry-pick"),
    menuLabel: t("work-commit-actions-cherry-pick-onto-current-branch"),
    icon: "cherryPick",
    consented: true,
    hint: t("work-commit-actions-apply-commit-top-current-branch-new"),
  }),
  Object.freeze({
    id: "revert",
    label: t("work-commit-actions-revert"),
    menuLabel: t("work-commit-actions-revert-2"),
    icon: "revert",
    consented: true,
    hint: t("work-commit-actions-undo-what-commit-did-new-commit"),
  }),
  Object.freeze({
    id: "checkout",
    label: t("work-commit-actions-checkout"),
    menuLabel: t("work-commit-actions-checkout-detached"),
    icon: "checkout",
    consented: true,
    hint: t("work-commit-actions-detach-head-commit-asked-first-what"),
  }),
  Object.freeze({
    id: "branch",
    label: t("work-commit-actions-branch-here"),
    menuLabel: t("work-commit-actions-create-branch-here"),
    icon: "branchHere",
    consented: false,
    hint: t("work-commit-actions-start-branch-commit-nothing-moves"),
  }),
  Object.freeze({
    id: "tag",
    label: t("work-commit-actions-tag-here"),
    menuLabel: t("work-commit-actions-create-tag-here"),
    icon: "tagHere",
    consented: false,
    hint: t("work-commit-actions-tag-commit-name-message-makes-annotated"),
  }),
  Object.freeze({
    id: "inspect",
    label: t("work-new-workstream-dialog-open"),
    menuLabel: t("work-commit-actions-open-commit"),
    icon: "inspect",
    consented: false,
    hint: t("work-commit-actions-message-files-touched-diff-document-centre"),
  }),
  Object.freeze({
    id: "copy_sha",
    label: t("work-commit-actions-copy-sha"),
    menuLabel: t("work-commit-actions-copy-sha"),
    icon: "copy",
    consented: false,
    hint: t("work-commit-actions-full-id-clipboard"),
  }),
  Object.freeze({
    id: "copy_short",
    label: t("work-commit-actions-copy-short-sha"),
    menuLabel: t("work-commit-actions-copy-short-sha"),
    icon: "copy",
    consented: false,
    hint: t("work-commit-actions-short-id-clipboard"),
  }),
  Object.freeze({
    id: "attach",
    label: t("work-commit-actions-attach-agent-tab"),
    menuLabel: t("work-commit-actions-attach-agent-tab"),
    icon: "agent",
    consented: false,
    hint: t("work-commit-actions-hand-commit-agent-talking-chip-message"),
  }),
]);

/** The sections the row's `⋮` and its context menu draw, in order; a rule is drawn between sections. */
export const MENU_SECTIONS = Object.freeze([
  Object.freeze(["copy_sha", "copy_short"]),
  Object.freeze(["checkout", "cherry_pick", "revert"]),
  Object.freeze(["branch", "tag"]),
  Object.freeze(["inspect", "attach"]),
]);

/** The words for an operation left half-done, as the status names it. */
const IN_PROGRESS_WORDS = Object.freeze({
  merge: "a merge",
  rebase: "a rebase",
  cherry_pick: "a cherry-pick",
  revert: "a revert",
});

function halfDone(op) {
  return t("work-commit-actions-half-done-resolve-card-above-finishes", { op: IN_PROGRESS_WORDS[op] ?? t("work-commit-actions-operation") });
}

/**
 * Why an action on `row` is off right now, or `null` when it is on.
 * @param {string} id
 * @param {{id: string}} row
 * @param {{busy?: boolean, headId?: string | null, inProgress?: string | null}} ctx
 */
export function disabledReason(id, row, { busy = false, headId = null, inProgress = null } = {}) {
  const def = COMMIT_ACTIONS.find((a) => a.id === id);
  if (!def) return t("work-commit-actions-not-action");
  if (!def.consented) return null;
  if (busy) return t("work-branch-actions-another-operation-running");
  if (inProgress) return halfDone(inProgress);
  if (id === "checkout" && headId === row.id) return t("work-commit-actions-head-already-here");
  if (id === "cherry_pick" && headId === row.id) return t("work-commit-actions-already-tip-current-branch");
  return null;
}

/**
 * Every action on one commit, in toolbar order, each with whether it is off
 * and why.
 * @param {{id: string}} row
 * @param {{busy?: boolean, headId?: string | null, inProgress?: string | null}} [ctx]
 */
export function commitActions(row, ctx = {}) {
  return COMMIT_ACTIONS.map((a) => {
    const reason = disabledReason(a.id, row, ctx);
    return { ...a, disabled: reason !== null, reason };
  });
}

/**
 * The context menu: `MENU_SECTIONS` flattened, the first item of every
 * section after the first marked `separatorBefore`; the consented ones and
 * the two that ask for a name keep their ellipsis in `label`.
 */
export function menuActions(row, ctx = {}) {
  const all = commitActions(row, ctx);
  const out = [];
  MENU_SECTIONS.forEach((section, s) => {
    section.forEach((id, i) => {
      const a = all.find((x) => x.id === id);
      if (a) out.push({ ...a, label: a.menuLabel, separatorBefore: s > 0 && i === 0 });
    });
  });
  return out;
}

/**
 * What a ref chip offers. A local branch can be switched to (consented — the
 * tree moves) and its name copied; a remote branch's name copied; a tag's
 * name copied and the tag deleted (consented — a ref is gone, so it asks and
 * is pinned in Safety first); `HEAD` is a fact, not a handle.
 * @param {{name: string, kind: string}} ref
 * @param {{busy?: boolean, inProgress?: string | null, currentBranch?: string | null}} [ctx]
 */
export function refActions(ref, { busy = false, inProgress = null, currentBranch = null } = {}) {
  const moving = busy ? t("work-branch-actions-another-operation-running") : inProgress ? halfDone(inProgress) : null;
  const copy = { id: "copy_ref", label: t("work-commit-actions-copy-name"), icon: "copy", consented: false, danger: false, disabled: false, reason: null };
  switch (ref.kind) {
    case "branch": {
      const here = currentBranch !== null && currentBranch === ref.name ? t("work-commit-actions-words") : null;
      const reason = moving ?? here;
      return [
        { id: "switch", label: t("work-commit-actions-switch-3", { ref: ref.name }), icon: "switchBranch", consented: true, danger: false, disabled: reason !== null, reason },
        { id: "open_workstream", label: t("work-commit-actions-open-workstream", { ref: ref.name }), icon: "workstream", consented: false, danger: false, disabled: false, reason: null },
        copy,
      ];
    }
    case "remote":
      return [copy];
    case "tag":
      return [
        { id: "open_workstream", label: t("work-commit-actions-open-workstream-2", { ref: ref.name }), icon: "workstream", consented: false, danger: false, disabled: false, reason: null },
        copy,
        { id: "delete_tag", label: t("work-commit-actions-delete-tag-3", { ref: ref.name }), icon: "delete", consented: true, danger: true, disabled: moving !== null, reason: moving },
      ];
    default:
      return [];
  }
}

/** The commit `HEAD` sits on, from the rows loaded so far; `null` until that row is in. */
export function headOf(rows) {
  for (const r of rows) if (r && Array.isArray(r.refs) && r.refs.some((ref) => ref.kind === "head")) return r.id;
  return null;
}

/**
 * Why a name cannot be a branch or a tag — git's `check-ref-format` rules a
 * person actually hits, each as a sentence — or `null` when git would take
 * it. The node checks again; this is the inline word before the round trip.
 * @param {string} name
 */
export function refNameProblem(name) {
  const n = String(name ?? "");
  if (n.trim() === "") return t("work-commit-actions-name-needed");
  if (/\s/.test(n)) return t("work-commit-actions-no-spaces-git-refuses-them");
  if (/[~^:?*[\\]/.test(n)) return t("work-commit-actions-none-of-git-refuses-them-ref-name");
  if (/[\x00-\x1f\x7f]/.test(n)) return t("work-commit-actions-no-control-characters");
  if (n.startsWith("-")) return t("work-commit-actions-name-cannot-start-dash-git-would");
  if (n.startsWith("/") || n.endsWith("/")) return t("work-commit-actions-name-cannot-start-end-slash");
  if (n.includes("//")) return t("work-commit-actions-no-empty-segment-one-slash-between");
  if (n.includes("..")) return t("work-commit-actions-no-two-dots-row");
  if (n.includes("@{")) return t("work-commit-actions-no-git-reads-reflog-selector");
  if (n === "@") return t("work-commit-actions-alone-head-not-name");
  if (n.endsWith(".")) return t("work-commit-actions-name-cannot-end-dot");
  if (n.endsWith(".lock") || n.split("/").some((s) => s.endsWith(".lock"))) return t("work-commit-actions-part-cannot-end-lock-git-keeps");
  if (n.split("/").some((s) => s.startsWith("."))) return t("work-commit-actions-part-cannot-start-dot");
  return null;
}

/** `refs/bisa/safety/<x>` → `<x>`, for a toast. */
export function shortRecovery(ref) {
  return String(ref ?? "").replace(/^refs\/bisa\/safety\//, "");
}

/**
 * The confirmation for a consented action: a title, a body that says what
 * moves and what a conflict leaves, the button's word, and whether it is
 * red. A merge commit's revert or pick says which parent it is read against.
 * @param {"checkout" | "cherry_pick" | "revert" | "switch" | "delete_tag"} kind
 * @param {{short: string, name?: string, parents?: number}} target
 */
export function consentWords(kind, { short, name = "", parents = 1 }) {
  const mainline = parents > 1 ? ` ${t("work-commit-actions-merge-commit-first-parent", { short })}` : "";
  switch (kind) {
    case "checkout":
      return {
        title: t("work-commit-actions-detach-head-commit"),
        body: t("work-commit-actions-head-moves-detached-from-any-branch", { short }),
        confirm: t("work-commit-actions-checkout"),
        danger: false,
      };
    case "cherry_pick":
      return {
        title: t("work-commit-actions-cherry-pick-commit"),
        body: t("work-commit-actions-applied-new-commit-current-branch-conflict", { short, mainline }),
        confirm: "Cherry-pick",
        danger: false,
      };
    case "revert":
      return {
        title: t("work-commit-actions-revert-commit"),
        body: t("work-commit-actions-new-commit-undoes-what-did-history", { short, mainline }),
        confirm: t("work-commit-actions-revert"),
        danger: false,
      };
    case "switch":
      return {
        title: t("work-commit-actions-switch", { name }),
        body: t("work-commit-actions-head-moves-branch-git-refuses-if", { name }),
        confirm: t("work-commit-actions-switch-2"),
        danger: false,
      };
    case "delete_tag":
      return {
        title: t("work-commit-actions-delete-tag", { name }),
        body: t("work-commit-actions-tag-removed-from-repository-commit-pointed", { name, short }),
        confirm: t("work-commit-actions-delete-tag-2"),
        danger: true,
      };
    default:
      return { title: t("work-commit-actions-continue"), body: "", confirm: t("work-commit-action-dialogs-continue"), danger: false };
  }
}

/**
 * The toast once an action landed.
 * @param {"checkout" | "cherry_pick" | "revert" | "switch" | "delete_tag" | "branch" | "tag"} kind
 * @param {{short: string, name?: string, recovery?: string | null}} facts
 */
export function doneWords(kind, { short, name = "", recovery = null }) {
  const saved = recovery ? ` ${t("work-commit-actions-what-here-saved", { recovery: shortRecovery(recovery) })}` : "";
  switch (kind) {
    case "checkout":
      return t("work-commit-actions-head-now-detached", { short, saved });
    case "cherry_pick":
      return t("work-commit-actions-cherry-picked", { short, saved });
    case "revert":
      return t("work-commit-actions-reverted", { short, saved });
    case "switch":
      return t("work-commit-actions-now", { name, saved });
    case "delete_tag":
      return t("work-commit-actions-deleted-tag", { name, saved });
    case "branch":
      return t("work-commit-actions-branch-created", { name, short });
    case "tag":
      return t("work-commit-actions-tagged", { short, name, saved });
    default:
      return t("work-branch-actions-done");
  }
}

/**
 * A 409 from a consented operation, read by the node's `code` — never the
 * status alone: a `conflict` names the files and the way out; another 409
 * says what is half-done. Anything else is the fallback sentence.
 * @param {{status?: number, code?: string | null, detail?: unknown} | null | undefined} err
 * @param {string} fallback
 */
export function conflictWords(err, fallback) {
  if (!err || err.status !== 409) return fallback;
  const detail = err.detail && typeof err.detail === "object" ? err.detail : {};
  const paths = Array.isArray(detail.paths) ? detail.paths.map(String) : [];
  const op = typeof detail.in_progress === "string" ? detail.in_progress : null;
  if (err.code === "conflict") {
    const n = paths.length;
    return t("work-commit-actions-stopped-conflicted-file-files-resolve-card", { n, paths: paths.join(", "), flag: (n) ? "yes" : "no", op: op ? op.replace("_", "-") : "", flag2: op ? "yes" : "no" });
  }
  return t("work-commit-actions-resolve-card-under-git-finishes-aborts", { fallback });
}
