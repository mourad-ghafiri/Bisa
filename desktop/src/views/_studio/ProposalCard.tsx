/**
 * The proposal, reviewable: what the Workflow Agent proposes for a
 * goal, drawn from the node's `NeedsAction.proposal` — the description, what
 * it starts on (by hand, or the events the goal will listen for), every
 * step with its one-line summary, who does it, where it branches or loops,
 * the inputs a run needs — and three doors: **Adopt and start** (with the
 * inputs, through the same `AskControls` every gate uses), **Request
 * changes…** (words to the Workflow Agent in the goal's thread; it proposes
 * again and the new proposal replaces this one), and **Decline**. A design
 * that begins on an event is adopted by listening — **Adopt and listen** —
 * and asks only what its events do not supply (`adoptionInputs`). The Inbox
 * and the goal page's *Your move* both mount it through `NeedsActionCard`.
 */
import { useState } from "react";
import { api } from "../../api";
import { t, tx } from "../../i18n/l10n.mjs";
import type { NeedsAction, ProposalView } from "../../types";
import { WORKFLOW_AGENT_ID } from "../../types";
import { Button, Chip, ICON, Markdown, STEP_KIND_ICON, TextArea, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { AskControls } from "./PendingAsk";
import { familyInk } from "../_workflow/familyInk";
import { adoptionInputs, changeRequest, flowSentence, inputsNeeded, listens, loopSentences, proposalHeadline, startsOn, stepLines } from "./proposalModel.mjs";

export function ProposalCard({
  goal,
  action,
  proposal,
  onResolved,
  onEdit,
  hero = false,
  autoFocus,
}: {
  goal: string;
  action: NeedsAction;
  proposal: ProposalView;
  onResolved: () => void;
  /** Open the canvas to edit the steps before adopting; omitted in the Inbox. */
  onEdit?: () => void;
  /** The goal's landing card: add a one-line lede that says nothing runs yet. */
  hero?: boolean;
  autoFocus?: boolean;
}) {
  const toast = useToast();
  const [changing, setChanging] = useState(false);
  const [request, setRequest] = useState("");
  const [busy, setBusy] = useState(false);
  const lines = stepLines(proposal);
  const loops = loopSentences(proposal);
  const needed = inputsNeeded(proposal);
  const starts = startsOn(proposal);
  const asked = adoptionInputs(proposal);
  const adoptWords = listens(proposal) ? t("studio-proposal-card-adopt-listen") : t("studio-proposal-card-adopt-start");

  const sendChanges = async () => {
    const text = changeRequest(request);
    if (!text || busy) return;
    setBusy(true);
    const ok = await attempt(
      () =>
        api.postGoalMessage(goal, {
          content: t("studio-proposal-card-please-change-proposed-workflow", { proposal: proposal.name, text }),
          mentions: [WORKFLOW_AGENT_ID],
        }),
      toast.error,
    );
    setBusy(false);
    if (ok) {
      toast.ok(t("studio-proposal-card-asked-workflow-agent-changes-proposes-again"));
      setChanging(false);
      setRequest("");
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <ICON.workflow size={14} aria-hidden className="text-text-dim" />
        <span className="text-xs font-semibold">{proposalHeadline(proposal)}</span>
        {needed.required.length > 0 && <Chip tone="quiet">{t("studio-proposal-card-inputs-to-fill", { n: needed.required.length })}</Chip>}
      </div>
      {hero && <p className="max-w-measure text-xs leading-relaxed text-text-dim">{t("studio-proposal-card-workflow-agent-designed-plan-review-adopt")}</p>}
      {proposal.description && <Markdown text={proposal.description} className="text-xs text-text" />}
      {/* What adopting it starts on — by hand, or the events its goal will listen for. */}
      {starts.length > 0 && (
        <p className="flex items-start gap-1.5 text-2xs text-text">
          <ICON.signal size={11} aria-hidden className="mt-0.5 shrink-0 text-text-dim" />
          <span>{t("studio-proposal-card-starts-on", { starts: starts.map((s) => tx(s)).join(" · ") })}</span>
        </p>
      )}
      <p className="text-2xs text-text-dim">{flowSentence(proposal)}</p>

      {/* The steps on the surface's own ground, rows divided by a hairline: this list sits inside the ask's card, so it draws no edge of its own. */}
      <ol className="flex flex-col divide-y divide-hairline rounded-control bg-surface">
        {lines.map((l) => {
          const Icon = STEP_KIND_ICON[l.kind as keyof typeof STEP_KIND_ICON] ?? ICON.workflow;
          return (
            <li key={l.id} className="flex items-start gap-2 px-2.5 py-2">
              <span className="tnum w-4 shrink-0 pt-0.5 text-right text-2xs text-text-dim">{l.index}</span>
              <Icon size={13} aria-hidden className={`mt-0.5 shrink-0 ${familyInk(l.kind)}`} />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5">
                  <span className="text-xs font-medium">{l.name}</span>
                  <span className="text-2xs text-text-dim">{l.kind}</span>
                  {l.assignee && <Chip tone="neutral">{l.assignee}</Chip>}
                </div>
                {l.summary && <p className="text-2xs text-text-dim">{tx(l.summary)}</p>}
                {l.notes.length > 0 && <p className="text-2xs text-text-dim italic">{l.notes.join(" · ")}</p>}
              </div>
            </li>
          );
        })}
      </ol>
      {loops.length > 0 && <p className="text-2xs text-text-dim">{loops.join("; ")}.</p>}

      {changing ? (
        <div className="flex flex-col gap-2 rounded-control bg-surface/60 p-2">
          <TextArea
            rows={3}
            autoFocus
            value={request}
            disabled={busy}
            onChange={(e) => setRequest(e.target.value)}
            placeholder={t("studio-proposal-card-what-should-change-step-add-drop")}
          />
          <div className="flex gap-2">
            <Button size="sm" variant="primary" disabled={busy || !changeRequest(request)} onClick={() => void sendChanges()}>{t("studio-proposal-card-send-workflow-agent")}</Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => setChanging(false)}>{t("studio-conversation-rows-cancel")}</Button>
          </div>
        </div>
      ) : (
        <>
          <AskControls
            action={action}
            inputs={asked}
            onResolved={onResolved}
            autoFocus={autoFocus}
            labels={{ approve: adoptWords, decline: t("studio-pending-ask-decline") }}
            extra={
              <>
                <Button size="sm" variant="ghost" onClick={() => setChanging(true)}>
                  <ICON.workflow size={12} aria-hidden />{t("studio-proposal-card-request-changes")}</Button>
                {onEdit && (
                  <Button size="sm" variant="ghost" onClick={onEdit}>
                    <ICON.edit size={12} aria-hidden />{t("studio-proposal-card-edit-steps")}</Button>
                )}
              </>
            }
          />
        </>
      )}
    </div>
  );
}
