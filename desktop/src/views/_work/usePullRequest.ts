/**
 * Everything the Workstreams panel's lifecycle reads about one branch, on
 * top of the checkout the panel already read (`useWorkstream`, handed in as
 * `base` so the checkout is read once): the code host's capabilities, and —
 * once a pull request exists — the pull request, its checks and its reviews.
 *
 * One hook so the lifecycle has one `loading`, one `error` and one `reload`
 * instead of ten. It polls the code host on `git.fetch_interval_secs` while a
 * pull request exists and the panel is on screen — a read, on the
 * visibility-gated clock, as `clock.ts` asks; `0` turns the polling off. The
 * code host has no events: checks finish and reviews land on their own clock.
 *
 * **Following an agent.** While the lifecycle's agent run is live
 * (`following`) the cadence is `FOLLOW_MS` whatever the fetch interval says —
 * the person asked for it by asking the agent — and each tick reads the
 * checkout's status too (`base.reloadStatus`), so a fix's commits move the
 * Commit and Push rows within seconds. `readAt` says when the code host was
 * last read, for the line under the spine.
 */

import { useEffect, useMemo, useRef } from "react";
import { api } from "../../api";
import { useClock } from "../../shell/clock";
import { numberOf } from "../../shell/settingsModel.mjs";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import type { CheckRun, CodeHostCapabilities, PullRequest, RepoRef, ResolvedSetting, ReviewSummary, ReviewThread } from "../../types";
import { useAsync } from "./useAsync";
import type { WorkstreamData } from "./useWorkstream";

/** With polling off, the clock still ticks — once an hour, to nobody. */
const IDLE_MS = 3_600_000;
/** While an agent works: above the engine's ten-second code host cache, below a person's patience. */
export const FOLLOW_MS = 15_000;

export interface PullRequestData extends WorkstreamData {
  caps: CodeHostCapabilities | null;
  /** The code host behind origin, by name; `null` when it is not one this build knows. */
  codeHost: string | null;
  repo: RepoRef | null;
  /** The code host's pull request, once the record names one; `null` before, or while the code host is asked. */
  pr: PullRequest | null;
  checks: CheckRun[] | null;
  reviews: { reviews: ReviewSummary[]; threads: ReviewThread[] } | null;
  /** The project's resolved settings, for the merge strategy and what follows a merge. */
  settings: ResolvedSetting[] | null;
  /**
   * The connected account's login on the code host, or null while unknown —
   * what decides which verdicts the person may give: a pull request this
   * account opened takes a comment from it and nothing more.
   */
  viewer: string | null;
  /** The code host's side again — after a review, a merge, a thread resolved. */
  reloadPr: () => void;
  /** Unix seconds the code host's side was last read — the latest of the pull request, its checks and its reviews; `null` before any. */
  readAt: number | null;
}

export function usePullRequest(base: WorkstreamData, { following = false }: { following?: boolean } = {}): PullRequestData {
  const { projectId } = base;
  const wid = base.wid;
  const codeHost = useAsync((s) => (projectId ? api.codeHostCapabilities(projectId, s) : Promise.resolve(null)), [projectId]);
  const caps = codeHost.data?.capabilities ?? null;
  const { resolved: settings } = useResolvedSettings(projectId);

  // The pull request exists once the record says so — open, or merged and
  // kept — and only then is the code host asked about it.
  const word = base.detail?.workstream.state.state;
  const hasPr = word === "pr_open" || word === "merged";
  const pr = useAsync((s) => (hasPr ? api.workstreamPr(wid, s) : Promise.resolve(null)), [wid, hasPr]);
  const checks = useAsync((s) => (hasPr && caps?.check_runs ? api.workstreamPrChecks(wid, s) : Promise.resolve(null)), [wid, hasPr, caps?.check_runs]);
  const reviews = useAsync((s) => (hasPr ? api.workstreamPrReviews(wid, s) : Promise.resolve(null)), [wid, hasPr]);
  // Whose account this checkout speaks to the code host as — the same facts the
  // Connection card under About › Settings shows (`codehost.account` resolved in the
  // checkout: a profile's, a pin's, the default), read once a pull request
  // exists. Local reads, no request to the code host; not polled: an account
  // does not change while the panel is open.
  const connection = useAsync((s) => (hasPr ? api.gitConnection(wid, s) : Promise.resolve(null)), [wid, hasPr]);
  const viewer = connection.data?.account.login ?? null;

  const reloadPr = () => {
    pr.reload();
    checks.reload();
    reviews.reload();
  };
  const reload = () => {
    base.reload();
    reloadPr();
  };

  const secs = numberOf(settings, "git.fetch_interval_secs", 300, { min: 0, max: 3600 });
  const polling = hasPr && secs > 0;
  const followed = hasPr && following;
  const tick = useClock(followed ? FOLLOW_MS : polling ? secs * 1000 : IDLE_MS);
  const firstTick = useRef(tick);
  useEffect(() => {
    if ((!polling && !followed) || tick === firstTick.current) return;
    reloadPr();
    if (followed) base.reloadStatus();
    // `reloadPr` and `base` are remade every render; the poll follows the tick and whether it should run.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tick, polling, followed]);

  const readAt = [pr.at, checks.at, reviews.at].reduce<number | null>((latest, t) => (t !== null && (latest === null || t > latest) ? t : latest), null);

  return useMemo(
    () => ({
      ...base,
      caps,
      codeHost: codeHost.data?.code_host ?? null,
      repo: codeHost.data?.repo ?? null,
      pr: pr.data?.pr ?? null,
      checks: checks.data?.checks ?? null,
      reviews: reviews.data ? { reviews: reviews.data.reviews, threads: reviews.data.threads } : null,
      settings,
      viewer,
      reload,
      reloadPr,
      readAt,
    }),
    // `reload` and `reloadPr` are remade every render over stable doors; the
    // object is rebuilt on the facts that change what it says.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [base, caps, codeHost.data, pr.data, checks.data, reviews.data, settings, viewer, readAt],
  );
}
