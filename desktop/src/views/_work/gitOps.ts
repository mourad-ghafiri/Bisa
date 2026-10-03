/**
 * The Git tab's mutations, run against the store rather than a component
 * (ide/04 §What survives a switch). Each takes the checkout's scope and id,
 * guards on the scope's `busy`, calls the node, and writes what happened —
 * the fresh rows, Suggest's note, the pull banner, the publish report, the
 * failure to retry — into `gitPanelStore`; a toast goes through the module
 * `toaster`. Nothing here closes over a component, so the panel that pressed
 * the button may be gone by the time the answer lands and the answer still
 * lands. Mutations are never aborted on unmount: the node has already begun,
 * and dropping the client's record of it is the bug this file removes.
 */

import { ApiError, api } from "../../api";
import type { Disposal, GitDone, GitFileRow, GitInProgress, GitMergeMode, GitRebasePlan, GitResolution, GitStash, GitStashPush, MergeStrategy, PrOutcome, PullMode, PushOutcome, WorkstreamStaged } from "../../types";
import { failureText, toaster } from "../../ui";
import { conflictWords } from "./commitActionsModel.mjs";
import { deletedWords, discardedWords, shortRef } from "./gitDiscardModel.mjs";
import { suggestionOutcome } from "./gitFiles.mjs";
import { applied, failureOf } from "./gitPanelModel.mjs";
import type { FailureKind, GitBusy } from "./gitPanelModel.mjs";
import { patchSession, sessionFor, setMessage } from "./gitPanelStore";
import { PLACE } from "./gitWords.mjs";
import { continuedWords, skippedWords } from "./operationModel.mjs";
import { tookWords } from "./conflictSidesModel.mjs";
import type { Sides } from "./conflictSidesModel.mjs";
import type { GitOperation } from "./operationModel.mjs";
import type { PrFormValues } from "./PrForm";
import { refused } from "./PublishOutcomeBanner";
import { appliedWords, droppedWords, poppedWords, stashRefusal, stashedWords } from "./stashModel.mjs";
import { IN_PROGRESS_LABEL, afterPull } from "./syncModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const words = (e: unknown) => (failureText("work", "git-ops-failed", e));

/** A write's answer: the rows shown, the selection kept, mounted readers told. */
function landed(scope: string, files: readonly GitFileRow[]): void {
  patchSession(scope, (s) => applied(s, files));
}

/** Something moved that no answer described: mounted readers reload. */
function moved(scope: string): void {
  patchSession(scope, (s) => ({ ...s, stale: s.stale + 1 }));
}

/**
 * One operation at a time per checkout: the guard and the busy flag, around
 * whatever the operation does. Answers whether it ran.
 */
async function guarded(scope: string, busy: GitBusy, run: () => Promise<void>): Promise<boolean> {
  if (sessionFor(scope).busy) return false;
  patchSession(scope, { busy });
  try {
    await run();
    return true;
  } finally {
    patchSession(scope, { busy: null });
  }
}

function failed(scope: string, kind: FailureKind, e: unknown, paths: readonly string[] = []): void {
  const error = words(e);
  toaster.error(error);
  patchSession(scope, { failure: failureOf(kind, error, paths) });
}

/**
 * Rewrite the last commit with the draft and what is staged — consented,
 * the old commit pinned first. The draft empties and the switch goes off
 * when it lands; a refusal stays on screen like a commit's.
 */
export function amend(scope: string, wid: string, message: string): Promise<boolean> {
  return guarded(scope, "amend", async () => {
    try {
      const r = await api.gitAmend(wid, message.trim());
      toaster.ok(t("work-git-ops-amended-now-what-here-saved", { short: r.short, ref_name: shortRef(r.recovery.ref_name) }));
      setMessage(scope, "");
      patchSession(scope, { note: null, amend: false });
      landed(scope, r.files);
    } catch (e) {
      failed(scope, "amend", e);
    }
  });
}

/** Ask the agent for a commit message; the box and the note read the answer. */
export function suggest(scope: string, wid: string): Promise<boolean> {
  return guarded(scope, "suggest", async () => {
    patchSession(scope, { note: null });
    try {
      const outcome = suggestionOutcome(await api.gitSuggestMessage(wid));
      // `message: null` means leave the box alone. Writing "" here would
      // clear what somebody had already typed in order to report a failure.
      if (outcome.message !== null) setMessage(scope, outcome.message);
      patchSession(scope, { note: outcome.note });
    } catch (e) {
      // The route is always 200, so reaching here means the node itself is
      // unreachable — which is a different sentence from "no agent answered".
      patchSession(scope, { note: words(e) });
    }
  });
}

/** Commit what is staged, with the draft; the draft empties when it lands. */
export function commit(scope: string, wid: string, message: string): Promise<boolean> {
  return guarded(scope, "commit", async () => {
    try {
      // No `paths`: the route reads that as "commit what is already staged",
      // never as "commit everything".
      const r = await api.gitCommit(wid, message.trim());
      toaster.ok(t("work-git-ops-committed", { short: r.short }));
      setMessage(scope, "");
      patchSession(scope, { note: null });
      landed(scope, r.files);
    } catch (e) {
      // 409 is the ordinary "nothing to do" — a clean tree or an empty
      // selection — rather than a fault, and it says so in its own words.
      failed(scope, "commit", e);
    }
  });
}

export function stage(scope: string, wid: string, paths: string[]): Promise<boolean> {
  if (paths.length === 0) return Promise.resolve(false);
  return guarded(scope, "stage", async () => {
    try {
      landed(scope, (await api.gitStage(wid, paths)).files);
    } catch (e) {
      failed(scope, "stage", e, paths);
    }
  });
}

export function unstage(scope: string, wid: string, paths: string[]): Promise<boolean> {
  if (paths.length === 0) return Promise.resolve(false);
  return guarded(scope, "unstage", async () => {
    try {
      landed(scope, (await api.gitUnstage(wid, paths)).files);
    } catch (e) {
      failed(scope, "unstage", e, paths);
    }
  });
}

/** Throw working-tree changes away — consented, the recovery ref named in the toast. */
export function discard(scope: string, wid: string, paths: string[]): Promise<boolean> {
  if (paths.length === 0) return Promise.resolve(false);
  return guarded(scope, "discard", async () => {
    try {
      const r = await api.gitDiscard(wid, { paths });
      toaster.ok(discardedWords(paths, r.recovery.ref_name));
      landed(scope, r.files);
    } catch (e) {
      failed(scope, "discard", e, paths);
    }
  });
}

/**
 * Delete files git has never seen — one, a folder's untracked ones, or
 * every one in the checkout — through the IDE's disposal, one after
 * another, stopping at the first
 * refusal: what was deleted is deleted, what was not is the failure's to
 * retry. The rows are re-read either way.
 */
export function remove(scope: string, wid: string, paths: readonly string[], disposal: Disposal | null): Promise<boolean> {
  if (paths.length === 0) return Promise.resolve(false);
  return guarded(scope, "delete", async () => {
    const done: string[] = [];
    let how: Disposal | null = null;
    try {
      for (const path of paths) {
        const r = await api.ideDelete("workstream", wid, path, false);
        how = r.disposal ?? how;
        done.push(path);
      }
      patchSession(scope, { failure: null });
    } catch (e) {
      failed(scope, "delete", e, paths.slice(done.length));
    }
    if (done.length > 0) {
      toaster.ok(deletedWords(done, how ?? disposal ?? "unlink"));
      patchSession(scope, (s) => ({ ...s, selection: s.selection && done.includes(s.selection.path) ? null : s.selection }));
    }
    moved(scope);
  });
}

/** Park the tree's changes; answers whether the entry was made. */
export function stashPush(scope: string, wid: string, body: GitStashPush): Promise<boolean> {
  let made = false;
  return guarded(scope, "stash", async () => {
    try {
      const r = await api.gitStashPush(wid, body);
      toaster.ok(stashedWords(r.stash, r.recovery.ref_name));
      landed(scope, r.files);
      made = true;
    } catch (e) {
      const refusal = stashRefusal(e instanceof ApiError ? { code: e.code, status: e.status, message: e.message, detail: e.detail } : { message: words(e) }, "push");
      toaster.error(refusal.sentence);
    }
  }).then((ran) => ran && made);
}

/**
 * Apply, pop or drop one entry — named by sha and index together so a list
 * that moved is refused rather than acted on. A moved list and a conflict
 * both mark the reads stale, so the Stashes view and the Conflicted section
 * re-read; a stash conflict has nothing to abort.
 */
export function stashAct(scope: string, wid: string, kind: "apply" | "pop" | "drop", entry: GitStash): Promise<boolean> {
  return guarded(scope, "stash", async () => {
    try {
      const call = kind === "apply" ? api.gitStashApply : kind === "pop" ? api.gitStashPop : api.gitStashDrop;
      const r = await call(wid, entry.commit, entry.index);
      const say = kind === "apply" ? appliedWords : kind === "pop" ? poppedWords : droppedWords;
      toaster.ok(say(entry, r.recovery.ref_name));
      landed(scope, r.files);
    } catch (e) {
      const refusal = stashRefusal(e instanceof ApiError ? { code: e.code, status: e.status, message: e.message, detail: e.detail } : { message: words(e) }, kind);
      toaster.error(refusal.sentence);
      if (refusal.kind === "error") patchSession(scope, { failure: failureOf(`stash_${kind}`, refusal.sentence) });
      moved(scope);
    }
  });
}

// ---------------------------------------------------------------------------
// The sync bar
// ---------------------------------------------------------------------------

export function fetch(scope: string, wid: string): Promise<boolean> {
  return guarded(scope, "fetch", async () => {
    try {
      const r = await api.gitFetch(wid);
      toaster.ok(r.status.behind > 0 ? t("work-git-ops-fetched-commit-commits-pull", { behind: r.status.behind }) : t("work-git-ops-fetched-nothing-new"));
    } catch (e) {
      toaster.error(words(e));
    } finally {
      moved(scope);
    }
  });
}

export function pull(scope: string, wid: string, mode: PullMode): Promise<boolean> {
  return guarded(scope, "pull", async () => {
    patchSession(scope, { pullBanner: null });
    try {
      const r = await api.gitPull(wid, mode);
      patchSession(scope, { pullBanner: afterPull({ ok: r.pull }) });
      if (r.pull.moved) toaster.ok(t("work-git-ops-pulled-what-here-saved", { ref_name: shortRef(r.recovery.ref_name) }));
    } catch (e) {
      const message = words(e);
      patchSession(scope, {
        pullBanner: afterPull({ err: e instanceof ApiError ? { status: e.status, code: e.code, message, detail: e.detail } : { status: 0, message } }),
      });
    } finally {
      moved(scope);
    }
  });
}

export function push(scope: string, wid: string): Promise<boolean> {
  return guarded(scope, "push", async () => {
    patchSession(scope, { publish: { kind: "none" } });
    try {
      const r: PushOutcome = await api.pushWorkstream(wid);
      patchSession(scope, { publish: r.status === "awaiting_publish_gate" ? { kind: "gate", gate: r.gate, what: "push" } : { kind: "pushed" } });
    } catch (e) {
      patchSession(scope, { publish: refused(e) });
    } finally {
      moved(scope);
    }
  });
}

export function abort(scope: string, wid: string, what: GitInProgress): Promise<boolean> {
  return guarded(scope, "abort", async () => {
    try {
      const r = await api.gitAbort(wid, what);
      toaster.ok(t("work-git-ops-aborted-what-here-saved", { what: IN_PROGRESS_LABEL[what], ref_name: shortRef(r.recovery.ref_name) }));
      patchSession(scope, { pullBanner: null, operation: null });
    } catch (e) {
      toaster.error(words(e));
    } finally {
      moved(scope);
    }
  });
}

// ---------------------------------------------------------------------------
// The Workstreams panel's lifecycle
// ---------------------------------------------------------------------------

export function openPr(scope: string, wid: string, values: PrFormValues): Promise<boolean> {
  return guarded(scope, "pr", async () => {
    patchSession(scope, { prPublish: { kind: "none" } });
    try {
      const r: PrOutcome = await api.openPr(wid, values);
      patchSession(scope, {
        prPublish: r.status === "awaiting_publish_gate" ? { kind: "gate", gate: r.gate, what: "pr" } : { kind: "pr", number: r.pr.number, url: r.pr.url },
      });
    } catch (e) {
      patchSession(scope, { prPublish: refused(e) });
    } finally {
      // The node may have pushed before the code host refused: the record moved either way.
      patchSession(scope, { prOpen: false });
      moved(scope);
    }
  });
}

export function mergePr(scope: string, wid: string, strategy: MergeStrategy, deleteBranch: boolean, host: string): Promise<boolean> {
  return guarded(scope, "merge", async () => {
    try {
      const r = await api.workstreamPrMerge(wid, strategy, deleteBranch);
      if (r.merged) {
        const branchNote = r.remote_branch_deleted === true ? ` ${t("work-git-ops-branch-deleted", { host })}` : r.remote_branch_deleted === false ? ` ${t("work-git-ops-branch-could-not-deleted", { host })}` : "";
        toaster.ok(`${r.sha ? t("work-git-ops-merged-as", { sha: r.sha.slice(0, 7) }) : t("work-git-ops-merged")}${branchNote}`);
      } else if (r.status === "awaiting_publish_gate") {
        patchSession(scope, { prPublish: { kind: "gate", gate: typeof r.gate === "string" ? r.gate : null, what: "pr" } });
      } else toaster.error(r.message ?? t("work-git-ops-not-merged"));
    } catch (e) {
      patchSession(scope, { prPublish: refused(e) });
    } finally {
      moved(scope);
    }
  });
}

// ---------------------------------------------------------------------------
// The Branches view and the graph: every act on a branch or a commit (ide/04
// §The Branches view). One helper runs a consented call — the guard, the
// landing, the toast naming the recovery ref — and reads a 409 `conflict` as
// the operation stopping: what was started is written to the session so the
// Resolve card can name it, and the rows it answered with are landed.
// ---------------------------------------------------------------------------

/** What a branch act says it started, for the card when it stops on conflicts. */
export type Started = Omit<GitOperation, "paths">;

async function consentedAct(scope: string, busy: GitBusy, run: () => Promise<GitDone>, did: (recovery: string) => string, started: Started | null = null): Promise<boolean> {
  return guarded(scope, busy, async () => {
    try {
      const r = await run();
      toaster.ok(t("work-git-ops-what-here-saved", { ref_name: did(shortRef(r.recovery.ref_name)), ref_name2: shortRef(r.recovery.ref_name) }));
      patchSession(scope, { operation: null, pullBanner: null, failure: null });
      landed(scope, r.files);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.code === "conflict") {
        const detail = e.detail && typeof e.detail === "object" ? (e.detail as { paths?: unknown; in_progress?: unknown }) : {};
        const paths = Array.isArray(detail.paths) ? detail.paths.map(String) : [];
        const kind = typeof detail.in_progress === "string" ? (detail.in_progress as GitInProgress) : started?.kind ?? null;
        patchSession(scope, (s) => ({
          ...s,
          operation: started && kind ? { ...started, kind, paths } : s.operation,
          failure: null,
          stale: s.stale + 1,
        }));
        toaster.error(conflictWords({ status: e.status, code: e.code, detail: e.detail }, e.message));
      } else {
        toaster.error(words(e));
        moved(scope);
      }
    }
  });
}

export function checkout(scope: string, wid: string, target: string, did: string): Promise<boolean> {
  return consentedAct(scope, "checkout", () => api.gitCheckout(wid, target), () => did);
}

/** Create a branch; with `switch`, the consented half moves the tree too. */
export function branchCreate(scope: string, wid: string, body: { name: string; start?: string | null; track?: boolean; switch?: boolean }, did: string): Promise<boolean> {
  if (body.switch) return consentedAct(scope, "checkout", () => api.gitBranchCreate(wid, body) as Promise<GitDone>, () => did);
  return guarded(scope, "branch", async () => {
    try {
      await api.gitBranchCreate(wid, body);
      toaster.ok(did);
      patchSession(scope, { failure: null });
    } catch (e) {
      failed(scope, "branch", e);
    } finally {
      moved(scope);
    }
  });
}

export function branchDelete(scope: string, wid: string, name: string, did: string): Promise<boolean> {
  return consentedAct(scope, "branch", () => api.gitBranchDelete(wid, name), () => did);
}

export function branchRename(scope: string, wid: string, from: string, to: string, did: string): Promise<boolean> {
  return consentedAct(scope, "branch", () => api.gitBranchRename(wid, from, to), () => did);
}

/** Point a branch at an upstream, or at none — safe, so no recovery is named. */
export function setUpstream(scope: string, wid: string, name: string, upstream: string | null, did: string): Promise<boolean> {
  return guarded(scope, "upstream", async () => {
    try {
      await api.gitSetUpstream(wid, name, upstream);
      toaster.ok(did);
    } catch (e) {
      toaster.error(words(e));
    } finally {
      moved(scope);
    }
  });
}

export function mergeBranch(scope: string, wid: string, body: { source: string; mode: GitMergeMode; message?: string | null }, into: string, did: string): Promise<boolean> {
  return consentedAct(scope, "merge_branch", () => api.gitMerge(wid, body), () => did, { kind: "merge", from: body.source, to: into, commits: [] });
}

export function rebase(scope: string, wid: string, body: { upstream: string; onto?: string | null; autostash?: boolean }, current: string, did: string): Promise<boolean> {
  return consentedAct(scope, "rebase", () => api.gitRebase(wid, body), () => did, { kind: "rebase", from: body.onto ?? body.upstream, to: current, commits: [] });
}

export function rebasePlan(scope: string, wid: string, plan: GitRebasePlan, current: string, did: string): Promise<boolean> {
  return consentedAct(scope, "rebase", () => api.gitRebasePlan(wid, plan), () => did, { kind: "rebase", from: plan.onto ?? plan.upstream, to: current, commits: plan.steps.map((s) => s.commit) });
}

export function cherryPick(scope: string, wid: string, body: { commits: string[]; record_origin?: boolean; no_commit?: boolean; mainline?: number | null }, onto: string, did: string): Promise<boolean> {
  return consentedAct(scope, "cherry_pick", () => api.gitCherryPick(wid, body), () => did, { kind: "cherry_pick", from: "", to: onto, commits: body.commits });
}

export function revert(scope: string, wid: string, body: { commits: string[]; no_commit?: boolean; mainline?: number | null }, on: string, did: string): Promise<boolean> {
  return consentedAct(scope, "revert", () => api.gitRevert(wid, body), () => did, { kind: "revert", from: "", to: on, commits: body.commits });
}

/** Go on with the operation half-done; a stop on the next commit keeps the card's operation. */
export function continueOp(scope: string, wid: string, what: GitInProgress): Promise<boolean> {
  return guarded(scope, "continue", async () => {
    try {
      const r = await api.gitContinue(wid, what);
      const stillIn = r.files.some((f) => f.conflicted);
      toaster.ok(continuedWords(what, stillIn));
      if (!stillIn) patchSession(scope, { operation: null, pullBanner: null });
      landed(scope, r.files);
    } catch (e) {
      toaster.error(e instanceof ApiError && e.status === 409 ? conflictWords({ status: e.status, code: e.code, detail: e.detail }, e.message) : words(e));
      moved(scope);
    }
  });
}

/** Leave out the commit the operation stopped on and go on. */
export function skipOp(scope: string, wid: string, what: GitInProgress): Promise<boolean> {
  return guarded(scope, "skip", async () => {
    try {
      const r = await api.gitSkip(wid, what);
      const stillIn = r.files.some((f) => f.conflicted);
      toaster.ok(skippedWords(what, stillIn));
      if (!stillIn) patchSession(scope, { operation: null, pullBanner: null });
      landed(scope, r.files);
    } catch (e) {
      toaster.error(e instanceof ApiError && e.status === 409 ? conflictWords({ status: e.status, code: e.code, detail: e.detail }, e.message) : words(e));
      moved(scope);
    }
  });
}

/**
 * Settle a conflicted path whole — a side kept, or the path removed —
 * consented; the rows land. The toast names the side in the person's words
 * (`sides`), never git's.
 */
export function resolve(scope: string, wid: string, path: string, take: GitResolution, sides: Sides): Promise<boolean> {
  return guarded(scope, "resolve", async () => {
    try {
      const r = (await api.gitResolve(wid, path, take)) as GitDone;
      toaster.ok(tookWords(path, take, sides));
      landed(scope, r.files);
    } catch (e) {
      toaster.error(words(e));
      moved(scope);
    }
  });
}

/**
 * *Mark resolved*: the composed text saved over the file — a compare-and-swap
 * on the hash it was read at, so a file that moved on disk since is never
 * written over — then the path staged. Index only after the save; the rows
 * land. Answers whether it ran, so the caller moves on to the next conflict.
 */
export function markResolved(scope: string, wid: string, path: string, text: string, hash: string | null): Promise<boolean> {
  return guarded(scope, "resolve", async () => {
    try {
      const w = await api.ideWriteFile("workstream", wid, path, text, hash ?? undefined);
      const r = (await api.gitResolve(wid, path)) as WorkstreamStaged;
      toaster.ok(t("work-git-ops-resolved-staged", { path, hash: w.hash.slice(0, 7) }));
      landed(scope, r.files);
    } catch (e) {
      toaster.error(e instanceof ApiError && e.status === 409 ? t("work-git-ops-changed-disk-since-read-panel-reloads", { path }) : words(e));
      moved(scope);
    }
  });
}

export function tagCreate(scope: string, wid: string, name: string, target: string | null, message: string | null, did: string): Promise<boolean> {
  return consentedAct(scope, "tag", () => api.gitTagCreate(wid, name, target, message), () => did);
}

export function tagDelete(scope: string, wid: string, name: string, did: string): Promise<boolean> {
  return consentedAct(scope, "tag", () => api.gitTagDelete(wid, name), () => did);
}

export function restore(scope: string, wid: string, ref: string, did: string): Promise<boolean> {
  return consentedAct(scope, "restore", () => api.gitRestore(wid, ref), () => did);
}

/**
 * Delete a branch on a remote — through the Publish gate like a push: a gate
 * that opened is the session's `publish` report, as a push's is.
 */
export function deleteRemoteBranch(scope: string, wid: string, remote: string, branch: string, did: string): Promise<boolean> {
  return guarded(scope, "delete_remote", async () => {
    try {
      const r = await api.gitRemoteBranchDelete(wid, remote, branch);
      if ("recovery" in r) {
        toaster.ok(t("work-git-ops-what-here-saved-2", { did, ref_name: shortRef(r.recovery.ref_name) }));
        landed(scope, r.files);
      } else {
        patchSession(scope, { publish: { kind: "gate", gate: r.gate, what: "push" } });
        toaster.info(t("work-git-ops-publish-gate-open-goes-from-once", { branch, remote, inbox: PLACE.inbox }));
        moved(scope);
      }
    } catch (e) {
      patchSession(scope, { publish: refused(e) });
      moved(scope);
    }
  });
}

/** A rewritten branch pushed with a lease — through the Publish gate, consented. */
export function pushWithLease(scope: string, wid: string, branch: string, did: string): Promise<boolean> {
  return guarded(scope, "push_lease", async () => {
    try {
      const r = await api.workstreamPushWithLease(wid);
      if (r.pushed) toaster.ok(r.recovery ? t("work-git-ops-what-here-saved-2", { did, ref_name: shortRef(r.recovery.ref_name) }) : did);
      else {
        patchSession(scope, { publish: { kind: "gate", gate: typeof r.gate === "string" ? r.gate : null, what: "push" } });
        toaster.info(t("work-git-ops-publish-gate-open-goes-out-once", { branch, inbox: PLACE.inbox }));
      }
    } catch (e) {
      patchSession(scope, { publish: refused(e) });
    } finally {
      moved(scope);
    }
  });
}
