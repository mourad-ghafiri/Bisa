/**
 * The sync bar's facts: what a checkout can do against its remote
 * right now, the words for it, and what a pull's answer or refusal means.
 *
 * A pull is a safe fetch and a consented merge or rebase (ide/04); the node
 * names every refusal with a `code`, and this reads the code — never the
 * status, never the sentence — so the bar can offer the next step: the other
 * pull modes when a fast-forward is impossible, the files when a merge
 * stopped, *Abort* when something is half-done.
 */

import { VERB } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The pull modes, in the menu's order; `git.pull` picks the one-click default. */
export const PULL_MODES = Object.freeze(["ff_only", "rebase", "merge"]);

export const PULL_LABEL = Object.freeze({
  ff_only: t("work-git-words-pull"),
  rebase: t("work-sync-pull-rebase"),
  merge: t("work-sync-pull-merge"),
});

export const PULL_MEANING = Object.freeze({
  ff_only: t("work-sync-fast-forward-only-moves-branch-when"),
  rebase: t("work-sync-replays-local-commits-top-upstream-linear"),
  merge: t("work-sync-merge-commit-when-both-sides-moved"),
});

/** The pull mode a setting names, or fast-forward when it names nothing usable. */
export function pullChoice(setting) {
  return typeof setting === "string" && PULL_MODES.includes(setting) ? setting : "ff_only";
}

/** The words for an operation git has left half-done. */
export const IN_PROGRESS_LABEL = Object.freeze({
  rebase: "rebase",
  merge: "merge",
  cherry_pick: "cherry-pick",
  revert: "revert",
});

/**
 * What the bar can do for a checkout, from its git status.
 * @param {import("../../types").GitStatusInfo | null | undefined} status
 */
export function syncState(status) {
  if (!status || !status.exists || !status.git) return { kind: "not_git" };
  if (status.in_progress) return { kind: "in_progress", op: status.in_progress };
  if (!status.remote) return { kind: "no_remote" };
  if (!status.branch) return { kind: "detached" };
  if (!status.upstream) return { kind: "no_upstream", branch: status.branch };
  return {
    kind: "ready",
    branch: status.branch,
    upstream: status.upstream,
    ahead: status.ahead,
    behind: status.behind,
  };
}

/** One line under the branch: where it stands against its upstream. */
export function syncLine(state) {
  switch (state.kind) {
    case "not_git":
      return t("work-stash-not-repository");
    case "no_remote":
      return t("work-sync-no-remote-nothing-pull-from-push");
    case "detached":
      return t("work-sync-head-detached-switch-branch-sync");
    case "no_upstream":
      return t("work-sync-has-no-upstream-yet-pushing-branch", { branch: state.branch });
    case "in_progress":
      return t("work-sync-progress-resolve-card-above-finishes-aborts", { op: IN_PROGRESS_LABEL[state.op] ?? state.op });
    case "ready": {
      const parts = [];
      if (state.ahead > 0) parts.push(`↑${state.ahead}`);
      if (state.behind > 0) parts.push(`↓${state.behind}`);
      return parts.length === 0 ? t("work-sync-up-date", { upstream: state.upstream }) : `${state.upstream} · ${parts.join(" ")}`;
    }
    default:
      return "";
  }
}

/**
 * Which controls the bar shows for a state. Fetch and pull need an upstream;
 * push needs a branch and a remote (it publishes the branch when there is no
 * upstream yet); nothing while an operation is half-done — the Operation
 * card above holds its verbs.
 */
/**
 * Whether the branch may overwrite its upstream with a lease (ide/04 §Force
 * pushes), and why not: only a branch that is ready — an upstream, nothing
 * half-done — and never the project's default branch. The push itself is
 * `--force-with-lease`, consented, through the Publish gate; this only says
 * when the door is open.
 * @param {import("./syncModel.d.mts").SyncState} state
 * @param {string | null | undefined} defaultBranch
 * @returns {{on: boolean, reason: string | null}}
 */
export function forcePushRule(state, defaultBranch) {
  switch (state.kind) {
    case "in_progress":
      return { on: false, reason: t("work-sync-operation-half-done-settle-abort-first") };
    case "no_remote":
      return { on: false, reason: t("work-sync-no-remote-yet") };
    case "detached":
      return { on: false, reason: t("work-branch-actions-no-branch-checked-out") };
    case "no_upstream":
      return { on: false, reason: t("work-sync-branch-has-no-upstream-yet-push") };
    case "ready":
      return defaultBranch != null && state.branch === defaultBranch ? { on: false, reason: t("work-sync-never-project-s-default-branch") } : { on: true, reason: null };
    default:
      return { on: false, reason: t("work-sync-not-here") };
  }
}

/**
 * The sync bar's `⋮` (ide/04 §The sync bar): Refresh first — the read that
 * opens the menu — then, under a rule, the three pulls — the default mode
 * first, by its label, the other two after — then, under a rule, Fetch, and
 * last, under a rule of its own, the force push (with a lease, `forcePushRule`).
 * Each item carries whether the state allows it and why not; the bar binds
 * the ids to its acts, never their places, and confirms every pull and the
 * force push.
 * @param {import("./syncModel.d.mts").SyncState} state
 * @param {"ff_only" | "rebase" | "merge"} defaultMode
 * @param {string | null} [defaultBranch] the project's default branch, which is never force-pushed
 * @returns {{id: string, label: string, hint: string | null, icon: string | null, disabled: boolean, reason: string | null, separatorBefore?: boolean, danger?: boolean}[]}
 */
export function syncMenu(state, defaultMode, defaultBranch = null) {
  const controls = syncControls(state);
  const why = (on) => (on ? null : state.kind === "in_progress" ? t("work-sync-operation-half-done-settle-abort-first") : state.kind === "no_remote" ? t("work-sync-no-remote-yet") : state.kind === "no_upstream" ? t("work-sync-branch-has-no-upstream-yet") : t("work-sync-not-here"));
  const items = [{ id: "refresh", label: t("work-commit-graph-refresh"), hint: t("work-sync-read-tree-again"), icon: "refresh", disabled: false, reason: null }];
  const modes = [defaultMode, ...PULL_MODES.filter((m) => m !== defaultMode)];
  modes.forEach((mode, i) => {
    items.push({
      id: `pull:${mode}`,
      label: PULL_LABEL[mode],
      hint: PULL_MEANING[mode],
      icon: i === 0 ? "install" : null,
      disabled: !controls.pull,
      reason: why(controls.pull),
      separatorBefore: i === 0,
    });
  });
  items.push({ id: "fetch", label: t("work-new-workstream-dialog-fetch"), hint: t("work-sync-bring-remote-s-news-nothing-tree"), icon: "refresh", disabled: !controls.fetch, reason: why(controls.fetch), separatorBefore: true });
  const force = forcePushRule(state, defaultBranch);
  items.push({
    id: "force_push",
    label: t("work-branch-actions-lease", { forcePush: VERB.forcePush }),
    hint: t("work-sync-overwrite-upstream-branch-only-if-still"),
    icon: "send",
    disabled: !force.on,
    reason: force.reason,
    separatorBefore: true,
    danger: true,
  });
  return items;
}

export function syncControls(state) {
  switch (state.kind) {
    case "ready":
      return { fetch: true, pull: true, push: true, setOrigin: false };
    case "no_upstream":
      return { fetch: true, pull: false, push: true, setOrigin: false };
    case "no_remote":
      return { fetch: false, pull: false, push: false, setOrigin: true };
    default:
      return { fetch: false, pull: false, push: false, setOrigin: false };
  }
}

/**
 * What a pull's answer or refusal means for the person.
 * @param {{ok: import("../../types").PullOutcome} | {err: {status: number, code?: string | null, message: string, detail?: Record<string, unknown> | null}}} result
 */
export function afterPull(result) {
  if ("ok" in result) {
    const o = result.ok;
    return o.moved
      ? { kind: "moved", upstream: o.upstream, from: o.from, to: o.to, mode: o.mode }
      : { kind: "current", upstream: o.upstream };
  }
  const { code, message, detail } = result.err;
  const d = detail ?? {};
  switch (code) {
    case "not_fast_forward":
      return { kind: "not_fast_forward", ahead: Number(d.ahead ?? 0), behind: Number(d.behind ?? 0) };
    case "conflict":
      return {
        kind: "conflict",
        paths: Array.isArray(d.paths) ? d.paths.map(String) : [],
        in_progress: typeof d.in_progress === "string" ? d.in_progress : null,
        detail: message,
      };
    case "in_progress":
      return { kind: "in_progress", op: typeof d.in_progress === "string" ? d.in_progress : null };
    default:
      return { kind: "error", detail: message };
  }
}

/**
 * A pull banner's sentences, one per outcome (`afterPull`): the title, and
 * the line under it when there is one. The buttons are the bar's.
 * @param {ReturnType<typeof afterPull>} banner
 * @returns {{title: string, body: string | null}}
 */
export function pullBannerWords(banner) {
  switch (banner.kind) {
    case "moved":
      return { title: t("work-sync-model-pulled-from", { upstream: banner.upstream, from: String(banner.from).slice(0, 7), to: String(banner.to).slice(0, 7) }), body: null };
    case "current":
      return { title: t("work-sync-bar-already-up-date", { upstream: banner.upstream }), body: null };
    case "not_fast_forward":
      return { title: t("work-sync-bar-not-fast-forward"), body: t("work-sync-model-branch-has-commits-own-upstream-has", { ahead: banner.ahead, behind: banner.behind }) };
    case "conflict": {
      const what = banner.in_progress ? IN_PROGRESS_LABEL[banner.in_progress] : t("work-sync-model-pull");
      return { title: t("work-sync-model-stopped-on-files", { what, files: banner.paths.length }), body: t("work-sync-model-first-in-list-resolve-card", { what }) };
    }
    case "in_progress":
      return { title: banner.op ? t("work-sync-model-something-already-progress-here", { op: IN_PROGRESS_LABEL[banner.op] }) : t("work-sync-model-something-already-progress-here-unnamed"), body: t("work-sync-bar-settle-files-below-then-continue-abort") };
    default:
      return { title: banner.detail, body: null };
  }
}
