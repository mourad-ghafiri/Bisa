/**
 * The Checks step's surface (ide/08), under its row from the moment a pull
 * request exists: every check run the code host reports — its conclusion,
 * its name as the door to the log, the host's summary — and, on a run that
 * **failed**, *Fix with ▾*: the agents the checkout can reach, the last one
 * a check was handed to first (`draftKeys(scope).checkAgent`, starting on
 * the comments' fixer). Picking one hands the failure over
 * (`checkFixPrompt`: reproduce here, fix, commit, no push or merge, report)
 * and the run is followed under the list by the same `ReviewRunLine` a fix
 * has — what the agent is doing, *Stop*, then the outcome with the commits
 * it added and the door to Git › Changes.
 *
 * One agent at a time in the checkout: while any run is live — a review, a
 * comment fix, another check — every *Fix with* here gives way to who is
 * busy (`busyWords`). A code host that reports no check runs draws nothing:
 * the row already says so. The run is the lifecycle's (`useReviewRun`),
 * handed in, because it moves what the lifecycle reads.
 */

import type { CheckRun, CodeHostCapabilities, PullRequest } from "../../types";
import { ExternalLink, SkeletonRows, cn } from "../../ui";
import { DEFAULT_AGENT, FIX_MENU_LABEL, draftKeys } from "./agentReviewModel.mjs";
import { AgentMenu } from "./AgentMenu";
import { useSessionDraft } from "./gitPanelStore";
import { checkFixPrompt, failedCheck } from "./prReviewModel.mjs";
import { ReviewRunLine } from "./ReviewRunLine";
import { busyWords } from "./reviewStepModel.mjs";
import type { ReviewRunControl } from "./useReviewRun";
import { t } from "../../i18n/l10n.mjs";

export function ChecksStep({
  scope,
  pid,
  pr,
  checks,
  caps,
  noun,
  aheadOfBase,
  run,
  onOpenChanges,
}: {
  /** The checkout's session scope (`rootKey("workstream", wid)`): where the remembered agent and the run are kept. */
  scope: string;
  /** The project, for its default agent. */
  pid: string | null;
  pr: PullRequest;
  /** Check runs; `null` while unread. */
  checks: CheckRun[] | null;
  caps: CodeHostCapabilities | null;
  /** The host's word — *pull request*, *merge request*. */
  noun: string;
  /** Commits beyond the base — a fix's outcome counts what it added. */
  aheadOfBase: number | null;
  /** The agent run — the lifecycle's, one at a time. */
  run: ReviewRunControl;
  /** Git › Changes — where a fix's commits are kept or discarded. */
  onOpenChanges?: () => void;
}) {
  const keys = draftKeys(scope);
  const [reviewer] = useSessionDraft<string>(keys.agent, DEFAULT_AGENT);
  const [fixer] = useSessionDraft<string>(keys.fixAgent, reviewer);
  const [remembered, setRemembered] = useSessionDraft<string>(keys.checkAgent, fixer);
  if (!caps?.check_runs) return null;
  const busy = busyWords(run.state);
  const canAsk = pr.state === "open" && !run.busy && !run.asking;
  const fixing = run.busy && run.state?.run.kind === "check" ? run.state.run.check : null;
  const checkLine =
    run.state && run.state.run.kind === "check" ? (
      <ReviewRunLine scope={scope} state={run.state} reply={run.reply} aheadOfBase={aheadOfBase} onStop={run.stop} onDismiss={run.dismiss} onOpenChanges={onOpenChanges} />
    ) : null;
  const hand = (check: CheckRun, agent: string) => {
    if (!canAsk) return;
    setRemembered(agent);
    void run.ask("check", agent, checkFixPrompt(pr, check, noun), { check: check.name });
  };

  if (checks === null) return <SkeletonRows rows={2} />;
  return (
    <section aria-label={t("work-checks-step-checks")} className="flex flex-col gap-1.5 text-2xs">
      <ul className="flex flex-col gap-0.5">
        {checks.map((c) => {
          const failed = failedCheck(c);
          return (
            <li key={c.name} className="group flex items-center gap-2">
              <span className={cn("w-16 shrink-0", c.conclusion === "success" ? "text-ok" : failed ? "text-danger" : "text-text-dim")}>{c.conclusion ?? c.status}</span>
              {c.url ? (
                <ExternalLink href={c.url} className="min-w-0 truncate text-text underline-offset-2 hover:underline">
                  {c.name}
                </ExternalLink>
              ) : (
                <span className="min-w-0 truncate text-text">{c.name}</span>
              )}
              {c.summary && (
                <span className="min-w-0 flex-1 truncate text-text-dim" title={c.summary}>
                  {c.summary}
                </span>
              )}
              {!c.summary && <span className="flex-1" />}
              {failed && fixing === c.name && <span className="shrink-0 text-accent-ink">{t("work-checks-step-fixing")}</span>}
              {failed && fixing !== c.name && busy && <span className="shrink-0 truncate text-text-dim">{busy}</span>}
              {failed && !busy && pr.state === "open" && (
                <span className="row-actions anim shrink-0">
                  <AgentMenu
                    label={FIX_MENU_LABEL}
                    remembered={remembered}
                    pid={pid}
                    hint={t("work-checks-step-hand-failed-check-agent-first-another", { remembered })}
                    disabled={!canAsk}
                    onPick={(agent) => hand(c, agent)}
                  />
                </span>
              )}
            </li>
          );
        })}
        {checks.length === 0 && <li className="text-text-dim">{t("work-checks-step-no-checks-ran", { noun })}</li>}
      </ul>
      {checkLine}
    </section>
  );
}
