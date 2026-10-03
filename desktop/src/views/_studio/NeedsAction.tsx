/**
 * The one thing the Inbox exists for: something is waiting on a human.
 *
 * This file is chrome. The header line, the accent-tinted card and the glyph are
 * what a pending ask wears; every control inside it comes from
 * {@link AskControls}. The goal page's *Your move* band mounts this same card
 * — there used to be a second chrome, `GateBar`, drawing the same
 * two shapes, and the copy is why the escape hatch out of a question could be
 * broken in one place and fixed in neither.
 *
 * The gate's glyph comes from `GATE_ICON`, so an approval gate here wears the
 * same symbol it wears on the Goals screen. That matters more than it looks: this
 * card is where you meet a gate you have never seen before, and learning the
 * symbol here is what makes it recognisable there.
 */

import { isAnswerAsk } from "../../askModel.mjs";
import type { InputDef, NeedsAction } from "../../types";
import { Chip, GATE_ICON, ICON, Markdown, Tooltip } from "../../ui";
import { ASK_ATTR, AskControls, FOCUS_ATTR } from "./PendingAsk";
import { askHint, askTitle, askVerbs } from "./inboxModel.mjs";
import { ProposalCard } from "./ProposalCard";
import { t } from "../../i18n/l10n.mjs";

export { ASK_ATTR, FOCUS_ATTR };

/**
 * The one ask card: the Inbox's row body and the goal page's *Your move*
 * band both render it, so answering is one act wherever it happens.
 */
export function NeedsActionCard({
  action,
  inputs,
  onResolved,
  autoFocus = false,
}: {
  /** The ask — decided through its own home, the goal or the run of the workspace. */
  action: NeedsAction;
  /** For an adoption: the workflow's inputs, asked for beside Approve. */
  inputs?: InputDef[];
  onResolved: () => void;
  autoFocus?: boolean;
}) {
  const isQuestion = isAnswerAsk(action.expects);
  // An adoption carries the workflow it proposes: the card shows
  // the design and its three doors, wherever the card is mounted. Only a
  // goal owes one, so its goal is the home's.
  const proposal = action.proposal ?? null;
  const proposalGoal = action.home.home === "goal" ? action.home.goal : null;

  return (
    <div className="rounded-card border border-accent/40 bg-accent-soft/40 p-3">
      <div className="mb-2 flex items-center gap-2">
        {/* `question` is its own glyph rather than a clock or an at-sign: a
            question is a person waiting on an answer, not a duration and not
            an address. The gate keeps its own kind's symbol. */}
        <Chip tone="accent" icon={isQuestion ? ICON.question : GATE_ICON[action.gate_kind]}>
          {askTitle(action, isQuestion)}
        </Chip>
        {action.durable && (
          <Tooltip label={t("studio-needs-action-reconstructed-from-durable-record-process-asked")}>
            <span className="cursor-default text-2xs text-text-dim">{t("studio-needs-action-waiting-since-previous-session")}</span>
          </Tooltip>
        )}
      </div>

      {/* `Markdown`, as on the goal screen. The same question was rendered
          as pre-wrapped plain text here and as markdown there, so an agent
          that asked with a list got a list in one place and a wall of
          asterisks in the other — and the Inbox is where the question is
          usually met first. */}
      {proposal && proposalGoal ? (
        <ProposalCard goal={proposalGoal} action={action} proposal={proposal} onResolved={onResolved} autoFocus={autoFocus} />
      ) : (
        <>
          <Markdown text={action.question} className="mb-3 text-text" />
          <AskControls
            action={action}
            inputs={inputs}
            onResolved={onResolved}
            autoFocus={autoFocus}
            labels={askVerbs(action)}
          />
          {askHint(action) && <p className="mt-2 text-2xs text-text-dim">{askHint(action)}</p>}
        </>
      )}
    </div>
  );
}
