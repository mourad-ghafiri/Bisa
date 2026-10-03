/**
 * One agent run, followed in the lifecycle from the ask to its outcome
 * (ide/08) — drawn under **Agent** for a review and under the **Comments**
 * header for a comment fix by `ReviewStep`, under the checks' list for a
 * check fix by `ChecksStep`; the run itself is `useReviewRun`'s, one at a
 * time in the checkout.
 *
 * Three moments. **Starting** — the mark and *general-agent is reviewing…*
 * before the session shows, or *getting ready…* while it is up and idle
 * before its first turn; nothing to stop yet, so no *Stop*. **Live** — the
 * mark in the session's state, what was asked, how long, and under it what
 * the agent is doing this moment (*running Bash · cargo test*, *thinking…*,
 * *2 sub-agents*), with *Stop* — offered only while the session runs or
 * waits on the person (`state.live`) — and a quiet door to the Agent panel;
 * a session waiting on the person says so in the caution's colour, with
 * *Answer in Agents* — the one time the panel is named, since answering
 * happens there. **Ended** — the
 * outcome in a sentence (`outcomeWords`), the agent's reply folded under it,
 * the door that follows (Git › Changes after a fix), and × to dismiss.
 * Nothing here navigates on its own.
 */

import type { MessageRow } from "../../types";
import { Button, FoldedText, ICON, LiveDuration, SessionMark, Tooltip, cn } from "../../ui";
import { stateOf } from "../../ui/sessionState.mjs";
import { showRightPanel } from "../_workbench/rightPanelStore";
import { activityWords, endWord, isFix, outcomeWords, runWords } from "./reviewStepModel.mjs";
import type { RunState } from "./reviewStepModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function ReviewRunLine({
  scope,
  state,
  reply,
  aheadOfBase,
  onStop,
  onDismiss,
  onOpenChanges,
}: {
  /** The root the Agent panel opens on, when the person asks for it. */
  scope: string;
  state: RunState;
  /** The agent's latest reply after the ask, once it has said anything. */
  reply: MessageRow | null;
  /** Commits beyond the base now — a fix's outcome counts what it added. */
  aheadOfBase: number | null;
  onStop: () => void;
  onDismiss: () => void;
  /** Git › Changes — where a fix's commits are kept or discarded. */
  onOpenChanges?: () => void;
}) {
  const { run, session } = state;
  const openAgents = () => showRightPanel("agents", scope);

  if (!state.done) {
    const waiting = session !== null && stateOf(session.state) === "waiting";
    const doing = activityWords(session);
    return (
      <div role="status" aria-label={runWords(run)} className="flex flex-col gap-1 rounded-control border border-border px-2 py-1.5 text-2xs">
        <div className="flex items-center gap-2">
          <SessionMark state={session?.state ?? "starting"} title={doing} />
          <span className="min-w-0 flex-1 truncate text-text">{runWords(run)}</span>
          {session && <LiveDuration since={session.started} className="shrink-0 text-text-dim" />}
          {state.live && (
            <Tooltip label={t("work-review-run-line-stop-agent-action-not-word-chat")}>
              <Button size="sm" variant="ghost" aria-label={t("work-review-run-line-stop-agent")} onClick={onStop}>
                <ICON.terminate size={11} aria-hidden />
                <span className="ml-1">{t("work-review-run-line-stop")}</span>
              </Button>
            </Tooltip>
          )}
          <Tooltip label={t("work-review-run-line-open-agent-panel-whole-conversation")}>
            <Button size="sm" variant="ghost" aria-label={t("work-review-run-line-open-agents")} onClick={openAgents}>
              <ICON.agent size={11} aria-hidden />
            </Button>
          </Tooltip>
        </div>
        <div className={cn("flex items-center gap-2 pl-5", waiting ? "text-warn" : "text-text-dim")}>
          <span className="min-w-0 flex-1 truncate" title={doing}>
            {doing}
          </span>
          {waiting && (
            <Button size="sm" variant="ghost" onClick={openAgents}>{t("work-review-run-line-answer-agents")}<ICON.forward size={11} aria-hidden />
            </Button>
          )}
        </div>
      </div>
    );
  }

  const how = state.how ?? "done";
  const words = outcomeWords(run, how, aheadOfBase, state.reason);
  // A landed review has its row as the outcome; the run is on its way out.
  if (words === null) return null;
  return (
    <div role="status" aria-label={words} className="flex flex-col gap-1 rounded-control border border-border px-2 py-1.5 text-2xs">
      <div className="flex items-center gap-2">
        <SessionMark state={endWord(how)} title={words} />
        <span className="min-w-0 flex-1 text-text">{words}</span>
        <Tooltip label={t("work-review-run-line-dismiss-run-over-ask-again-any")}>
          <Button size="sm" variant="ghost" aria-label={t("work-publish-outcome-banner-dismiss")} onClick={onDismiss}>
            <ICON.close size={11} aria-hidden />
          </Button>
        </Tooltip>
      </div>
      {reply && <FoldedText text={reply.content} storeKey={`ide.pr.run.${run.at}`} className="pl-5 text-text-dim" />}
      {isFix(run) && onOpenChanges && (
        <button type="button" className="anim self-start pl-5 text-2xs text-accent-ink underline underline-offset-2 hover:text-text" onClick={onOpenChanges}>{t("work-new-workstream-dialog-open-git-changes")}</button>
      )}
    </div>
  );
}
