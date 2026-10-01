/**
 * What can be done to a branch, a remote branch, a tag or a Safety entry in
 * the Branches view (ide/04 §The Branches view), said once — the twin of
 * `commitActionsModel.mjs` for the graph. A row's hover verb, its `⋮` menu,
 * every confirmation and every toast read this list, so an act has one
 * label, one glyph, one rule for when it is off and one reason why, and
 * the words say the verbs `gitWords.mjs` fixes: *Switch*, *Merge*,
 * *Rebase*, *Rename*, *Delete*, *Restore*, *Force push with lease*. Every
 * consented act is off, with the one sentence, while an operation is
 * half-done — the Resolve card above is the way through. Pure: nothing
 * here runs git.
 */

import { VERB, confirmLabel } from "./gitWords.mjs";
import { restoreWords } from "./gitDiscardModel.mjs";
import { IN_PROGRESS_LABEL } from "./syncModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Why every consented act is off while something is half-done. */
export function halfDoneReason(inProgress) {
  return t("work-branch-actions-half-done-resolve-card-above-finishes", { inProgress: IN_PROGRESS_LABEL[inProgress] ?? "operation" });
}

function blocker({ busy, inProgress }) {
  if (busy) return t("work-branch-actions-another-operation-running");
  if (inProgress) return halfDoneReason(inProgress);
  return null;
}

const act = (id, label, icon, over = {}) => ({ id, label, icon, hover: false, consented: false, danger: false, disabled: false, reason: null, ...over });

/**
 * The actions on one branch row, in order: the one hover verb first, then
 * the `⋮` menu's. `ctx.current` is the checked-out branch, `ctx.defaultBranch`
 * the project's default, `ctx.busy` an operation in flight, `ctx.inProgress`
 * what git has half-done.
 * @param {{name: string, current: boolean, upstream?: string | null}} branch
 * @param {{current?: string | null, defaultBranch?: string | null, busy?: boolean, inProgress?: string | null}} [ctx]
 */
export function branchActions(branch, { current = null, defaultBranch = null, busy = false, inProgress = null } = {}) {
  const held = blocker({ busy, inProgress });
  const off = { disabled: held !== null, reason: held };
  const isCurrent = branch.current === true;
  const isDefault = defaultBranch !== null && branch.name === defaultBranch;
  const target = current ?? t("work-branch-actions-current-branch");
  const noneOut = current === null ? t("work-branch-actions-no-branch-checked-out") : null;
  const out = [];
  if (!isCurrent) {
    out.push(act("switch", t("work-branch-actions-words", { switch: VERB.switch, branch: branch.name }), "switchBranch", { hover: true, consented: true, ...off }));
    out.push(act("merge", t("work-branch-actions-into", { merge: VERB.merge, branch: branch.name, target }), "merge", { consented: true, separatorBefore: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }));
    out.push(act("rebase", t("work-branch-actions-onto", { rebase: VERB.rebase, target, branch: branch.name }), "rebase", { consented: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }));
    out.push(act("cherry_pick", t("work-branch-actions-cherry-pick-from", { branch: branch.name }), "cherryPick", { consented: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }));
  } else {
    if (!isDefault) {
      out.push(act("rebase_plan", t("work-branch-actions-interactively", { rebase: VERB.rebase }), "rebase", { consented: true, ...off }));
      out.push(act("push_lease", t("work-branch-actions-lease", { forcePush: VERB.forcePush }), "send", { consented: true, ...off }));
    }
  }
  out.push(act("upstream", branch.upstream ? t("work-branch-actions-upstream", { upstream: branch.upstream }) : t("work-branch-actions-set-upstream"), "link", { separatorBefore: true, ...off }));
  // A workstream on this branch: a door to the New workstream dialog, so it
  // moves nothing here and is never off.
  out.push(act("workstream", t("work-branch-actions-open-workstream", { branch: branch.name }), "workstream", { separatorBefore: true }));
  if (!isDefault) {
    out.push(act("rename", `${VERB.rename}…`, "edit", { separatorBefore: true, ...off }));
  }
  // The default branch is never deleted from here, checked out or not.
  if (!isCurrent && !isDefault) {
    out.push(act("delete", `${VERB.delete}…`, "delete", { consented: true, danger: true, ...off }));
  }
  return out;
}

/** The row's `⋮` menu — every action but the hover verb. */
export function menuActions(branch, ctx) {
  return branchActions(branch, ctx).filter((a) => !a.hover);
}

/**
 * The actions on one remote branch row: **Check out** on hover — a local
 * branch that tracks it — then what the local rows offer against it, the
 * fetch of that one branch, the workstream door, the delete on the remote
 * (consented, through the Publish gate), and the copies.
 * @param {{remote: string, name: string, tracked?: boolean}} branch
 * @param {{current?: string | null, defaultBranch?: string | null, busy?: boolean, inProgress?: string | null}} [ctx]
 */
export function remoteBranchActions(branch, { current = null, defaultBranch = null, busy = false, inProgress = null } = {}) {
  const full = `${branch.remote}/${branch.name}`;
  const held = blocker({ busy, inProgress });
  const off = { disabled: held !== null, reason: held };
  const target = current ?? t("work-branch-actions-current-branch");
  const noneOut = current === null ? t("work-branch-actions-no-branch-checked-out") : null;
  const isDefault = defaultBranch !== null && branch.name === defaultBranch;
  return [
    act("checkout", t("work-branch-actions-check-out", { full }), "switchBranch", { hover: true, consented: true, ...off }),
    act("merge", t("work-branch-actions-into-2", { merge: VERB.merge, full, target }), "merge", { consented: true, separatorBefore: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }),
    act("rebase", t("work-branch-actions-onto-2", { rebase: VERB.rebase, target, full }), "rebase", { consented: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }),
    act("cherry_pick", t("work-branch-actions-cherry-pick-from-2", { full }), "cherryPick", { consented: true, disabled: held !== null || noneOut !== null, reason: held ?? noneOut }),
    act("fetch", t("work-branch-actions-branch", { fetch: VERB.fetch }), "refresh", { separatorBefore: true, ...off }),
    act("workstream", t("work-branch-actions-open-workstream-2", { full }), "workstream", { separatorBefore: true }),
    act("delete_remote", t("work-branch-actions-words-2", { delete: VERB.delete, remote: branch.remote }), "delete", {
      consented: true,
      danger: true,
      separatorBefore: true,
      disabled: held !== null || isDefault,
      reason: held ?? (isDefault ? t("work-branch-actions-project-s-default-branch-not-deleted") : null),
    }),
    act("copy_name", t("work-branch-actions-copy-name"), "copy", { separatorBefore: true }),
    act("copy_full", t("work-branch-actions-copy", { full }), "copy"),
  ];
}

/**
 * The confirmation before a consented act — the question as the title, one
 * or two sentences as the body (the recovery promise is the dialog's
 * `SafetyNote`, never a third), the verb as the button.
 * @param {"switch" | "checkout_remote" | "delete_branch" | "delete_remote" | "delete_tag" | "restore" | "push_lease"} kind
 * @param {{name?: string, from?: string, remote?: string, current?: string | null, rec?: {ref_name: string, kind: "commit" | "tree" | "stash", branch?: string | null}}} facts
 */
export function consentWords(kind, { name = "", from = "", remote = "", current = null, rec = null } = {}) {
  switch (kind) {
    case "switch":
      return { title: t("work-branch-actions-words-3", { switch: VERB.switch, name }), body: t("work-branch-actions-head-working-tree-move-git-refuses"), confirm: VERB.switch, danger: false, kind: "tree" };
    case "checkout_remote":
      return { title: t("work-branch-actions-check-out-2", { from, name }), body: t("work-branch-actions-local-branch-made-tracking-head-working", { name, from }), confirm: t("work-remotes-section-check-out"), danger: false, kind: "tree" };
    case "delete_branch":
      return { title: t("work-branch-actions-branch-2", { delete: VERB.delete, name }), body: t("work-branch-actions-commits-stay-name-goes-tip-pinned"), confirm: confirmLabel("delete"), danger: true, kind: "commit" };
    case "delete_remote":
      return { title: t("work-branch-actions-words-4", { delete: VERB.delete, name, remote }), body: t("work-branch-actions-nothing-machine-changes-branch-goes-from", { remote }), confirm: confirmLabel("delete"), danger: true, kind: "commit" };
    case "delete_tag":
      return { title: t("work-branch-actions-tag", { delete: VERB.delete, name }), body: t("work-branch-actions-commit-pointed-stays-tag-goes-tag"), confirm: confirmLabel("delete"), danger: true, kind: "commit" };
    case "restore":
      return { title: t("work-branch-actions-what-here", { restore: VERB.restore }), body: rec ? restoreWords(rec) : "", confirm: VERB.restore, danger: false, kind: "tree" };
    case "push_lease":
      return {
        title: t("work-branch-actions-lease-2", { forcePush: VERB.forcePush, name }),
        body: t("work-branch-actions-remote-branch-overwritten-only-if-still"),
        confirm: t("work-branch-actions-lease-3", { forcePush: VERB.forcePush }),
        danger: true,
        kind: "commit",
      };
    default:
      return { title: `${current ?? ""}?`, body: "", confirm: VERB.continue, danger: false, kind: "tree" };
  }
}

/**
 * The toast once an act landed, the verb the button used.
 * @param {"switch" | "checkout_remote" | "merge" | "rebase" | "rebase_plan" | "cherry_pick" | "revert" | "delete_branch" | "delete_remote" | "delete_tag" | "restore" | "push_lease" | "rename" | "create" | "create_switch" | "tag" | "upstream" | "abort"} kind
 * @param {{name?: string, to?: string, op?: string, count?: number}} facts
 */
export function doneWords(kind, { name = "", to = "", op = "", count = 1 } = {}) {
  const commits = t("work-branch-actions-commit-commits", { count });
  switch (kind) {
    case "switch":
      return t("work-branch-actions-now", { name });
    case "checkout_remote":
      return t("work-branch-actions-now-tracking", { name, to });
    case "merge":
      return t("work-branch-actions-merged", { name });
    case "rebase":
      return t("work-branch-actions-rebased-onto", { name });
    case "rebase_plan":
      return t("work-branch-actions-rebased", { name, to });
    case "cherry_pick":
      return t("work-branch-actions-cherry-picked", { commits });
    case "revert":
      return t("work-branch-actions-reverted", { commits });
    case "delete_branch":
      return t("work-branch-actions-deleted-branch", { name });
    case "delete_remote":
      return t("work-branch-actions-deleted", { name, to });
    case "delete_tag":
      return t("work-branch-actions-deleted-tag", { name });
    case "restore":
      return t("work-branch-actions-restored", { name });
    case "push_lease":
      return t("work-branch-actions-pushed-lease", { name });
    case "rename":
      return t("work-branch-actions-renamed", { name, to });
    case "create":
      return t("work-branch-actions-created-nothing-moved", { name });
    case "create_switch":
      return t("work-branch-actions-created-switched", { name });
    case "tag":
      return t("work-branch-actions-tagged-head", { name });
    case "upstream":
      return to ? t("work-branch-actions-now-follows", { name, to }) : t("work-branch-actions-follows-no-upstream-now", { name });
    case "abort":
      return t("work-branch-actions-ed", { abort: VERB.abort, op: String(op).replace("_", "-") });
    default:
      return t("work-branch-actions-done");
  }
}

/** The rows whose name or upstream holds `text`, case-blind; every row for an empty text. */
export function branchFilter(rows, text) {
  const needle = String(text ?? "")
    .trim()
    .toLowerCase();
  if (needle === "") return [...rows];
  return rows.filter((b) => b.name.toLowerCase().includes(needle) || (b.upstream ?? "").toLowerCase().includes(needle));
}

/** The current branch first, the default second, the rest as listed (newest first). */
export function branchOrder(rows, defaultBranch) {
  const rank = (b) => (b.current ? 0 : defaultBranch !== null && b.name === defaultBranch ? 1 : 2);
  return [...rows].sort((a, b) => rank(a) - rank(b));
}

/** The local name a remote branch takes when checked out: its own when free, else with the remote's prefix. */
export function localNameFor(branch, locals) {
  const taken = new Set(locals.map((b) => b.name));
  if (!taken.has(branch.name)) return branch.name;
  return `${branch.remote}-${branch.name}`;
}

/** *↑2 ↓1* for a branch's standing against its upstream; empty when in step or without one. */
export function standingWords(branch) {
  const parts = [];
  if (branch.ahead > 0) parts.push(`↑${branch.ahead}`);
  if (branch.behind > 0) parts.push(`↓${branch.behind}`);
  return parts.join(" ");
}
