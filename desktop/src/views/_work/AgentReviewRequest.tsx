/**
 * Ask an agent to review — the Review step's request control (ide/08), for a
 * pull request or for a branch against its base. An agent from a plain
 * `Select`, an optional line of words that lead the request, one button; the
 * request goes into the checkout's conversation as a mention through the step's
 * `onAsk`, and the step follows the run right under this control
 * (`ReviewRunLine`) — nothing here leaves the step. A pull request's review,
 * once given, shows who, when and the words folded, with *Review again*.
 *
 * The agent and the words live in the checkout's session (`useSessionDraft`,
 * `draftKeys`), so a tab switch keeps them; the run is `useReviewRun`'s.
 */

import { useState } from "react";
import { Button, ICON, TextArea } from "../../ui";
import { DEFAULT_AGENT, askContent, askLabel, draftKeys, landsWords } from "./agentReviewModel.mjs";
import type { ReviewTarget } from "./agentReviewModel.mjs";
import { AgentSelect } from "./AgentSelect";
import { useSessionDraft } from "./gitPanelStore";
import { ReasonLine } from "./ReasonLine";
import { ReviewGiven } from "./ReviewGiven";
import { reviewKey, reviewWords, verdictWords } from "./reviewStepModel.mjs";
import type { ReviewFacts, RunKind } from "./reviewStepModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function AgentReviewRequest({
  scope,
  target,
  facts = null,
  disabled = false,
  busy = false,
  reason = null,
  onAsk,
}: {
  /** The checkout's session scope (`rootKey("workstream", wid)`): where the drafts are kept. */
  scope: string;
  target: ReviewTarget;
  /** Who has reviewed the pull request, when there is one — the agent's review, once given, is drawn here. */
  facts?: ReviewFacts | null;
  /** The request cannot be made now — the pull request closed, nothing to review. */
  disabled?: boolean;
  /** A run is starting or live: one at a time. */
  busy?: boolean;
  /** Why the request is off, as a line under the button. */
  reason?: string | null;
  /** Post the request and record the run — the step's. Resolves to whether it went. */
  onAsk: (kind: RunKind, agent: string, content: string) => Promise<boolean>;
}) {
  const keys = draftKeys(scope);
  const [agent, setAgent] = useSessionDraft<string>(keys.agent, DEFAULT_AGENT);
  const [message, setMessage] = useSessionDraft<string>(keys.message, "");
  const [asking, setAsking] = useState(false);
  const [again, setAgain] = useState(false);
  const given = target.kind === "pr" && facts?.agent && !again ? facts.agent : null;
  const off = disabled || busy || asking;

  const ask = async () => {
    if (off) return;
    setAsking(true);
    try {
      const went = await onAsk(target.kind === "pr" ? "review" : "branch", agent, askContent(target, message));
      if (went) {
        setMessage("");
        setAgain(false);
      }
    } finally {
      setAsking(false);
    }
  };

  if (given) {
    return (
      <>
        <div className="flex items-center gap-2">
          <span className="min-w-0 truncate font-mono text-text">{given.agent}</span>
          <span className="text-text-dim">{t("work-agent-review-request-reviewed")}</span>
          <span className="flex-1" />
          {!disabled && (
            <Button size="sm" variant="ghost" disabled={busy || asking} onClick={() => setAgain(true)}>{t("work-agent-review-request-review-again")}</Button>
          )}
        </div>
        <ReviewGiven review={given.review} words={reviewWords(given.review)} verdict={verdictWords(given.review)} storeKey={`ide.pr.review.${reviewKey(given.review, 0)}`} />
      </>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <AgentSelect value={agent} onChange={setAgent} disabled={off} />
      <TextArea value={message} rows={2} disabled={off} placeholder={t("work-agent-review-request-anything-agent-should-look-optional")} onChange={(e) => setMessage(e.target.value)} />
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={off} onClick={() => void ask()}>
          {asking ? t("work-agent-review-request-asking") : askLabel(target, agent)}
          <ICON.forward size={11} aria-hidden />
        </Button>
        {again && (
          <Button size="sm" variant="ghost" onClick={() => setAgain(false)}>{t("work-agent-review-request-keep-review")}</Button>
        )}
      </div>
      {reason ? <ReasonLine>{reason}</ReasonLine> : <p className="text-3xs text-text-dim">{landsWords(target)}</p>}
    </div>
  );
}
