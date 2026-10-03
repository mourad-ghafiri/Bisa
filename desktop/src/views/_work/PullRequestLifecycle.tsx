/**
 * One branch on its way to its base — the lifecycle in the Workstreams panel
 * (ide/07, ide/08), under the checkout's header, on a branch beside the
 * primary.
 *
 * One spine — where the branch is (`LifecycleStepper` over
 * `prLifecycleModel`) — and under each step the surface that belongs to it, so
 * it reads in the order the work happens: the pull request card under *Open
 * pull request*, the check runs — a failed one handed to an agent — under
 * *Checks*, the review — an agent's, yours, the comments to reply on, resolve
 * or hand to an agent each — under *Review*, the merge control under *Merge*,
 * the clean-up — which terminates what still stands in the checkout — under
 * *Clean up*. The **one** act that is legal sits under **its own step** — the
 * model names it (`cta.step`) and makes it the step in hand, so the merge
 * control is never met above the review — with the outcome of the last one
 * beside it; the Review step is never that act —
 * it is optional, its surface drawn under its row from the moment the branch
 * exists (the branch against its base before a pull request, the pull
 * request after), and the merge is offered whenever the pull request is
 * open. Comments still open and a standing request for changes are the
 * cautions the merge names. Commit, push, pull and
 * fetch are Git › Changes' verbs; the checkout's name, sessions, path and
 * closing are the panel's, above and below this. This is where a pull request
 * starts, is read, is reviewed, is merged — and nothing else.
 *
 * The publish outcomes are the node's, never a guess:
 *
 * | what happened | HTTP | banner |
 * |---|---|---|
 * | `gated` (default): a gate is open; **nothing has left this machine** | 202 | *Decide it in the inbox* |
 * | `auto`: the branch is on the remote / the pull request exists | 200 | done |
 * | refused | 409 + `code` | one banner per code (`publishOutcome.mjs`) |
 *
 * All the reading is one hook (`usePullRequest`) over the checkout the panel
 * already read: the code host's capabilities, the connected account and —
 * once a pull request exists — the pull request, its checks and its reviews,
 * refreshed on the bus and on `git.fetch_interval_secs`. **The agent run is
 * the lifecycle's** (`useReviewRun`, handed to the Review and Checks steps to
 * draw — one at a time in the checkout, so two agents never edit one tree)
 * because the run moves what the lifecycle reads: while it is live the code
 * host and the checkout status are read every `FOLLOW_MS`; when the agent
 * replies, again; when it ends, everything at once — so every row settles
 * together. A line under the spine says when the code host was last read,
 * with the door to read it now.
 */

import { errorFields, log } from "../../log";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { hostLabel, prNoun } from "./codeHostWords.mjs";
import type { MergeStrategy, PullMode } from "../../types";
import { Button, ErrorNote, ICON, SkeletonRows, Tooltip } from "../../ui";
import { useClock } from "../../shell/clock";
import { useSessions } from "../../shell/sessionsStore";
import { useTerminals } from "../../shell/useTerminals";
import { boolOf, choiceOf } from "../../shell/settingsModel.mjs";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { workstreamActivity } from "../_workbench/workstreamActivityModel.mjs";
import * as ops from "./gitOps";
import { patchSession, useGitSession } from "./gitPanelStore";
import { AfterMergeDialog } from "./AfterMergeDialog";
import { AFTER_MERGE, CLEANUP } from "./afterMergeModel.mjs";
import { ChecksStep } from "./ChecksStep";
import { terminationCounts } from "./closeWorkstreamModel.mjs";
import { LifecycleStepper } from "./LifecycleStepper";
import { MergeControl } from "./MergeControl";
import { PrForm, type PrFormValues } from "./PrForm";
import { lifecycle, prTitleFrom, readWords, type LifecycleStep } from "./prLifecycleModel.mjs";
import { PublishBanner } from "./PublishOutcomeBanner";
import { publishFailure } from "./publishOutcome.mjs";
import { PullRequestCard } from "./PullRequestCard";
import { ReviewStep } from "./ReviewStep";
import { reviewFacts } from "./reviewStepModel.mjs";
import { PULL_MODES, pullChoice } from "./syncModel.mjs";
import { useAsync } from "./useAsync";
import { usePullRequest } from "./usePullRequest";
import { useReviewRun, type RunMove } from "./useReviewRun";
import type { WorkstreamData } from "./useWorkstream";
import { t } from "../../i18n/l10n.mjs";

/** The read line re-reads its *ago* on this beat. */
const READ_LINE_TICK_MS = 5_000;

export function PullRequestLifecycle({
  base,
  onChanged,
  onOpenAbout,
  onOpenChanges,
  onCheckoutGone,
}: {
  /** The checkout, as the panel read it — read once, shared. */
  base: WorkstreamData;
  /** The workstream record changed — a caller's list is stale. */
  onChanged?: () => void;
  /** Open About on a view — Settings, where the project's publishing policy is set. */
  onOpenAbout?: (view: "checkout" | "settings") => void;
  /** Switch to Git › Changes — where a commit or a push is made. */
  onOpenChanges?: () => void;
  /** The clean-up deleted the checkout: this root is gone. */
  onCheckoutGone?: () => void;
}) {
  const wid = base.wid;
  // Whether an agent run is live decides the read cadence; the run itself
  // needs the reviews read, so the flag follows the run one render behind.
  const [following, setFollowing] = useState(false);
  const data = usePullRequest(base, { following });
  // The lifecycle's report, its form and its clean-up dialog live in the
  // checkout's session (`gitPanelStore`): a merge that opened the Publish gate
  // is still reported after a tab switch, and a pull request opened while you
  // were elsewhere shows its number when you come back.
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const publish = session.prPublish;
  const busy = session.busy === "pr" || session.busy === "merge" ? session.busy : null;
  const prOpen = session.prOpen;
  const afterMerge = session.afterMerge;
  const setPublish = (p: typeof publish) => patchSession(scope, { prPublish: p });
  const setPrOpen = (open: boolean) => patchSession(scope, { prOpen: open });
  const setAfterMerge = (a: typeof afterMerge) => patchSession(scope, { afterMerge: a });

  // The policies behind the merge and what follows it.
  const preferredStrategy = choiceOf(data.settings, "git.merge_strategy", ["merge", "squash", "rebase"], "merge");
  const deleteBranchDefault = boolOf(data.settings, "git.delete_branch_after_merge", true);
  const cleanupPolicy = choiceOf(data.settings, "workstreams.cleanup", CLEANUP, "ask");
  const afterMergePolicy = choiceOf(data.settings, "workstreams.after_merge", AFTER_MERGE, "ask");
  const pullMode: PullMode = pullChoice(choiceOf(data.settings, "git.pull", PULL_MODES, "ff_only"));

  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();

  // The branch tip's subject titles a new pull request; read only when the form is about to open.
  const branches = useAsync((s) => (prOpen ? api.gitBranches(wid, s) : Promise.resolve(null)), [wid, prOpen]);

  const reloadAll = () => {
    data.reload();
    onChanged?.();
  };

  // What a person approved at the gate and did not go out (ide/08): a 202 was
  // the last word the click heard, so the act says its own end on the bus.
  // This checkout's frame is the banner under the step that still offers the
  // act — the record did not move, so the rows are read again all the same.
  useEngineEvents((e) => {
    const failed = publishFailure(e.payload, wid);
    if (!failed) return;
    setPublish(failed);
    reloadAll();
  });

  // Who has reviewed — the agent, you, others — and where that leaves the step.
  const review = reviewFacts({ reviews: data.reviews?.reviews, viewer: data.viewer, comments: data.reviews?.threads, caps: data.caps });
  const aheadOfBase = data.status?.ahead_of_base ?? data.detail?.ahead_of_base ?? null;
  // The agent run: the lifecycle's, because it moves what the lifecycle reads.
  const moved = (why: RunMove) => {
    if (why === "ended") data.reload();
    else {
      data.reloadPr();
      base.reloadStatus();
    }
  };
  const run = useReviewRun({ wid, pid: data.detail?.workstream.project ?? null, scope, facts: data.reviews ? review : null, aheadOfBase, onMoved: moved });
  useEffect(() => setFollowing(run.busy), [run.busy]);
  const readTick = useClock(READ_LINE_TICK_MS);

  if (data.error || !data.detail) return <ErrorNote error={data.error ?? t("work-pull-request-lifecycle-workstream-not-found")} retry={data.reload} />;

  const d = data.detail;
  const w = d.workstream;
  const s = data.status;
  const isGit = w.kind.kind !== "copy";
  const life = lifecycle({
    isGit,
    state: w.state.state,
    aheadOfBase: s?.ahead_of_base ?? d.ahead_of_base,
    upstream: s?.upstream ?? d.status.upstream,
    ahead: s?.ahead ?? d.status.ahead,
    base: s?.base ?? d.base,
    pr: data.pr,
    checks: data.checks,
    review,
    caps: data.caps,
    noun: prNoun(data.codeHost),
  });
  const needsPush = life.cta?.id === "open_pr" && life.cta.pushes === true;
  const tipSubject = branches.data?.branches.find((b) => b.current)?.subject ?? null;
  const prExpected = w.state.state === "pr_open" || w.state.state === "merged";

  const openPr = (values: PrFormValues) => void ops.openPr(scope, wid, values).then((ran) => ran && reloadAll());
  const merge = (strategy: MergeStrategy, deleteBranch: boolean) => void ops.mergePr(scope, wid, strategy, deleteBranch, hostLabel(data.codeHost)).then((ran) => ran && reloadAll());

  /** Clean up: the after-merge dialog needs to know whether anything runs on the primary first. */
  const cleanUp = async () => {
    if (!data.projectId) return;
    let onPrimary: number | null = 0;
    try {
      onPrimary = (await api.workstreamStatus(data.projectId)).status.running_agents;
    } catch (e) {
      // Unknown is not idle: the dialog holds the pull and says the primary could not be read.
      log.warn("workstreams", "the primary's status could not be read before the after-merge dialog", errorFields(e));
      onPrimary = null;
    }
    const local = workstreamActivity(sessions, terminals, data.projectId).counts.working;
    setAfterMerge({ primaryBusy: onPrimary === null ? null : onPrimary + local });
  };

  /** The one act, under its own step — `life.current` is that step by the model's word. */
  const action = (() => {
    const cta = life.cta;
    if (!cta) return null;
    if (cta.id === "merge" && data.pr) {
      return (
        <MergeControl
          pr={data.pr}
          caps={data.caps}
          preferredStrategy={preferredStrategy}
          deleteBranchDefault={deleteBranchDefault}
          blocked={cta.blocked ?? null}
          cautions={cta.cautions ?? []}
          busy={busy === "merge"}
          onMerge={merge}
        />
      );
    }
    const go = () => {
      if (cta.id === "open_pr") setPrOpen(true);
      else if (cta.id === "cleanup") void cleanUp();
      else onOpenChanges?.();
    };
    return (
      <div className="flex items-center gap-2">
        <Button size="sm" variant={cta.secondary ? "ghost" : "primary"} disabled={busy !== null || cta.blocked !== undefined} onClick={go}>
          {busy === "pr" && cta.id === "open_pr" ? t("work-after-merge-dialog-working") : cta.label}
          {cta.secondary && <ICON.forward size={11} aria-hidden />}
        </Button>
      </div>
    );
  })();

  /** Each step's own surface, under its row — whatever the step's status. */
  const surface = (step: LifecycleStep) => {
    switch (step.id) {
      case "open_pr":
        if (data.pr) return <PullRequestCard pr={data.pr} checks={data.checks} caps={data.caps} codeHost={data.codeHost} />;
        return prExpected ? <SkeletonRows rows={2} /> : null;
      case "checks":
        // The checks' own home: each run, and a failed one handed to an agent.
        if (data.pr) {
          return (
            <ChecksStep
              scope={scope}
              pid={data.projectId ?? null}
              pr={data.pr}
              checks={data.checks}
              caps={data.caps}
              noun={prNoun(data.codeHost)}
              aheadOfBase={s?.ahead_of_base ?? d.ahead_of_base}
              run={run}
              onOpenChanges={onOpenChanges}
            />
          );
        }
        return prExpected && data.caps?.check_runs ? <SkeletonRows rows={1} /> : null;
      case "review": {
        // One surface from the moment the branch exists: the branch against
        // its base until a pull request, the pull request after.
        const base = s?.base ?? d.base;
        if (!data.pr && !prExpected && !base) return null;
        const target = data.pr ? { kind: "pr" as const, pr: data.pr, noun: prNoun(data.codeHost) } : { kind: "branch" as const, base: base ?? t("work-new-workstream-dialog-base") };
        if (!data.pr && prExpected) return <SkeletonRows rows={2} />;
        return (
          <ReviewStep
            wid={wid}
            scope={scope}
            pid={data.projectId ?? null}
            target={target}
            pr={data.pr}
            aheadOfBase={s?.ahead_of_base ?? d.ahead_of_base}
            caps={data.caps}
            viewer={data.viewer}
            facts={review}
            data={data.reviews}
            run={run}
            onChanged={data.reloadPr}
            onOpenChanges={onOpenChanges}
          />
        );
      }
      default:
        return null;
    }
  };

  // When the code host was last read, and the door to read it now — once there is a pull request to read.
  const readLine = (() => {
    if (!data.pr) return null;
    const words = readWords(data.readAt, run.busy, run.run?.agent ?? null, readTick / 1000);
    if (!words) return null;
    return (
      <div className="flex items-center gap-1 text-2xs text-text-dim" role="status">
        <span className="min-w-0 truncate">{words}</span>
        <Tooltip label={t("work-pull-request-lifecycle-read-code-host-checkout-now")}>
          <button type="button" aria-label={t("work-commit-graph-refresh")} className="anim flex h-5 w-5 items-center justify-center rounded text-text-dim hover:bg-surface-2 hover:text-text" onClick={data.reload}>
            <ICON.refresh size={11} aria-hidden />
          </button>
        </Tooltip>
      </div>
    );
  })();

  return (
    <>
      <LifecycleStepper steps={life.steps} current={life.current} note={life.note} action={action} render={surface} footer={readLine}>
        <PublishBanner publish={publish} goal={w.goal} onDismiss={() => setPublish({ kind: "none" })} onOpenAbout={onOpenAbout} />
      </LifecycleStepper>

      <PrForm
        open={prOpen}
        onClose={() => setPrOpen(false)}
        caps={data.caps}
        repo={data.repo}
        codeHost={data.codeHost}
        hostKnown={data.codeHost !== null}
        needsPush={needsPush}
        initialTitle={prTitleFrom(tipSubject, d.branch)}
        busy={busy === "pr"}
        onSubmit={(values) => void openPr(values)}
        suggest={(signal) => api.suggestPr(wid, signal)}
      />

      {d.project && afterMerge && (
        <AfterMergeDialog
          open
          onClose={() => setAfterMerge(null)}
          pid={d.project.id}
          wid={wid}
          branch={d.branch}
          defaultBranch={s?.default_branch ?? null}
          dirty={!(s?.clean ?? d.status.clean)}
          cleanup={cleanupPolicy}
          afterMerge={afterMergePolicy}
          pullMode={pullMode}
          primaryBusy={afterMerge.primaryBusy}
          terminated={terminationCounts(sessions, terminals, wid)}
          onDone={(outcomes) => {
            setAfterMerge(null);
            reloadAll();
            if (outcomes.delete === "done") onCheckoutGone?.();
          }}
        />
      )}
    </>
  );
}
