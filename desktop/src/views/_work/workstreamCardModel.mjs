/**
 * What a workstream card says, in order, as chips (ide/07): the
 * branch or label as its title; then its state, its pull request, the agents
 * in it, where it stands against its base and its upstream, what is staged,
 * modified and untracked, conflicts, and an operation left half-done. Each
 * fact is one chip, shown only when it is true — a clean, quiet checkout says
 * almost nothing. One model for the list row and the panel header, so "+3"
 * means the same thing everywhere.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The title a workstream goes by — **the one rule**, read by the Workstreams
 * panel's header, the project rail's row, the Board's card and the footer:
 * the person's label (trimmed; a blank one is no label), else the branch the
 * record stands on, else the branch the checkout is on now (the primary's,
 * which has none of its own), else the primary's word, else the copy's tail.
 * @param {import("../../types").Workstream | null | undefined} w
 * @param {{branch?: string | null} | null | undefined} s the live status, when read
 */
export function cardTitle(w, s) {
  const name = typeof w?.name === "string" ? w.name.trim() : "";
  if (name) return name;
  if (w?.kind?.kind === "worktree" && w.kind.branch) return w.kind.branch;
  if (s?.branch) return s.branch;
  if (w?.kind?.kind === "primary") return t("work-workstream-card-primary");
  return t("work-workstream-card-copy", { tail: String(w?.id ?? "").slice(-6) });
}

/**
 * What a rename writes (`PATCH /workstreams/{wid}`): the name trimmed, and
 * `null` for an empty one — the branch titles the workstream again. One rule
 * for the panel's field, the rail's inline rename and the Board's dialog.
 * @param {string | null | undefined} value what was typed
 * @returns {{name: string | null}}
 */
export function renameBody(value) {
  const name = String(value ?? "").trim();
  return { name: name === "" ? null : name };
}

/**
 * Whether what was typed changes the name at all: a field left as it was —
 * or blank over no name — writes nothing.
 * @param {string | null | undefined} name the record's name
 * @param {string | null | undefined} value what was typed
 */
export function renames(name, value) {
  return renameBody(name).name !== renameBody(value).name;
}

/** The toast once a rename landed: renamed, or named by its branch again. @param {{name: string | null}} body */
export function renamedWords(body) {
  return body.name === null ? t("work-rename-workstream-workstream-named-branch-again") : t("work-rename-workstream-workstream-renamed");
}

/** The record's states, each the word a person reads. */
const STATE_WORD = Object.freeze({
  open: t("work-workstream-card-state-open"),
  dirty: t("work-workstream-card-state-dirty"),
  committed: t("work-workstream-card-state-committed"),
  pushed: t("work-workstream-card-state-pushed"),
  pr_open: t("work-workstream-panel-pr-open"),
  merged: t("work-workstream-card-state-merged"),
  closed: t("work-workstream-card-state-closed"),
});

/** A state's word; a state this build does not know is said as the node spelled it, never dropped. @param {string | null | undefined} state */
export function stateWord(state) {
  return STATE_WORD[state] ?? String(state ?? "").replaceAll("_", " ");
}

/** The state word a chip shows: the pull request's number while one is open, else the record's word. */
export function stateLabel(state) {
  return state?.state === "pr_open" ? t("work-workstream-card-pr-number", { number: state.number }) : stateWord(state?.state);
}

const STATE_TONE = Object.freeze({
  open: "quiet",
  dirty: "warn",
  committed: "warn",
  pushed: "ok",
  pr_open: "accent",
  merged: "ok",
  closed: "quiet",
});

const IN_PROGRESS_WORD = Object.freeze({
  rebase: "rebase",
  merge: "merge",
  cherry_pick: "cherry-pick",
  revert: "revert",
});

/**
 * The chips, in order. `href` marks a chip that opens something (the pull
 * request on the code host).
 * @param {import("../../types").Workstream} w
 * @param {import("../../types").WorkstreamStatus | null | undefined} s
 * @returns {{id: string, text: string, tone: string, title: string, href?: string}[]}
 */
export function cardChips(w, s) {
  const out = [];
  const state = w?.state?.state ?? s?.state?.state;
  if (state && state !== "open") {
    out.push({
      id: "state",
      text: stateWord(state),
      tone: STATE_TONE[state] ?? "quiet",
      title: t("work-workstream-card-workstream", { state: stateWord(state) }),
    });
  }
  // The pull request, from the live status, else from the record — which keeps
  // it through the merge, so a merged branch still says where it landed.
  const pr = s?.pr ?? (w?.state?.state === "pr_open" || w?.state?.state === "merged" ? { number: w.state.number, url: w.state.url } : null);
  if (pr) {
    out.push({ id: "pr", text: `#${pr.number}`, tone: "accent", title: t("work-git-words-pull-request-code-host"), href: pr.url });
  }
  if (!s) return out;
  if (!s.exists) {
    out.push({ id: "missing", text: t("work-workstream-card-no-checkout"), tone: "warn", title: t("work-workstream-card-folder-not-disk") });
    return out;
  }
  // The harnesses standing here are shown live as their own rows with their
  // real state, not as a frozen count chip — `running_agents` is
  // still a real server field, used by the merge guard, but not drawn here.
  if (!s.git) return out;
  if (s.in_progress) {
    out.push({
      id: "in_progress",
      text: t("work-workstream-card-progress", { in_progress: IN_PROGRESS_WORD[s.in_progress] ?? s.in_progress }),
      tone: "warn",
      title: t("work-workstream-card-git-has-left-operation-half-done"),
    });
  }
  if (s.ahead_of_base !== null && s.ahead_of_base !== undefined && s.ahead_of_base > 0) {
    out.push({ id: "ahead_base", text: `+${s.ahead_of_base}`, tone: "neutral", title: t("work-workstream-card-commit-commits-beyond", { ahead_of_base: s.ahead_of_base, base: s.base ?? t("work-new-workstream-dialog-base") }) });
  }
  if (s.behind_base !== null && s.behind_base !== undefined && s.behind_base > 0) {
    out.push({ id: "behind_base", text: `−${s.behind_base}`, tone: "neutral", title: t("work-workstream-card-commit-commits-not-here", { behind_base: s.behind_base, base: s.base ?? t("work-new-workstream-dialog-base") }) });
  }
  if (s.upstream) {
    if (s.ahead > 0 || s.behind > 0) {
      out.push({ id: "upstream", text: `↑${s.ahead} ↓${s.behind}`, tone: "neutral", title: t("work-workstream-card-against", { upstream: s.upstream }) });
    }
  } else if (s.branch) {
    out.push({ id: "no_upstream", text: t("work-workstream-card-not-pushed"), tone: "quiet", title: t("work-workstream-card-no-upstream-branch-yet") });
  }
  if (s.staged > 0) out.push({ id: "staged", text: `S ${s.staged}`, tone: "warn", title: t("work-git-words-staged", { staged: s.staged }) });
  if (s.unstaged > 0) out.push({ id: "modified", text: `M ${s.unstaged}`, tone: "warn", title: t("work-workstream-card-modified-not-staged", { unstaged: s.unstaged }) });
  if (s.untracked > 0) out.push({ id: "untracked", text: `? ${s.untracked}`, tone: "quiet", title: t("work-changes-toolbar-untracked", { untracked: s.untracked }) });
  if (s.conflicted > 0) {
    out.push({ id: "conflicts", text: t("work-git-words-conflict-conflicts", { conflicted: s.conflicted }), tone: "warn", title: t("work-workstream-card-unmerged-paths-settle-them-git-tab") });
  }
  return out;
}
