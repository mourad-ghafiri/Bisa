/**
 * What a work item actually did — as a pane, not a modal.
 *
 * It sits in the right-hand slot beside the conversation, so you can read the
 * transcript while the discussion about it is still on screen. The transcript
 * is `SessionTranscript`'s tail — by byte offset while the item is live, so a
 * long run streams in rather than refetching from the top.
 */

import { api } from "../../api";
import type { WorkItemSpec } from "../../types";
import {
  Button,
  Chip,
  ErrorNote,
  ICON,
  LinkedText,
  Markdown,
  SectionHeader,
  Skeleton,
  SkeletonRows,
  WorkItemStateChip,
} from "../../ui";
import { navigate } from "../../router";
import { AssigneeTags } from "./AssigneePicker";
import { SessionTranscript } from "./SessionTranscript";
import { assigneeWire } from "./types";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

function isLive(item: WorkItemSpec): boolean {
  const s = item.state.state;
  return s === "claimed" || s === "in_progress" || s === "review";
}

export function WorkItemPanel({
  itemId,
  inWorkbench = false,
}: {
  /** The item — addressed by its id alone, whichever home it is filed at. */
  itemId: string;
  /**
   * This *is* the workbench's About document, so it does not offer a way to
   * get here. Set false everywhere else — chiefly the goal's aux pane, which
   * is 380px wide and has nowhere to put the item's files or a terminal.
   */
  inWorkbench?: boolean;
}) {
  const { data, error, loading, reload } = useAsync(
    (s) => api.workItem(itemId, s),
    [itemId],
  );

  if (loading && !data) {
    return (
      <div className="flex flex-col gap-3 p-3" aria-busy>
        <Skeleton className="h-5 w-40" />
        <SkeletonRows rows={4} />
      </div>
    );
  }
  if (error || !data) {
    return (
      <div className="p-3">
        <ErrorNote error={error ?? t("work-work-item-panel-not-found")} retry={reload} />
      </div>
    );
  }

  const item = data.item;

  return (
    <div className="flex flex-col gap-4 p-3">
      <div className="flex flex-wrap items-center gap-1.5">
        <WorkItemStateChip state={item.state} />
        {item.harness_candidates.length > 0 && (
          <Chip tone="quiet" icon={ICON.harness}>
            {item.harness_candidates.join(" → ")}
          </Chip>
        )}
        {/* Where its files are, and where a terminal can be opened on them —
            neither of which fits in a 380px pane. */}
        {!inWorkbench && (
          <Button
            size="sm"
            className="ml-auto"
            onClick={() => navigate({ name: "workbench", scope: "work_item", id: item.id })}
          >{t("work-work-item-panel-open-workbench")}</Button>
        )}
      </div>

      {/*
        Two rows because these are two facts, and the panel used to show one
        chip that quietly conflated them.

        *Requested* is who the item was given to — written by whoever added it,
        and the only half a person controls. *Running it* is the agent the
        engine picked once assignment resolved, and it is deliberately not
        something a caller can set: an accepted value would be a way to borrow
        another agent's signing keys.

        So an item delegated to a team, before anybody has claimed it, has a
        request and no runner. That is the ordinary state of freshly delegated
        work, not a gap, and both lines say so in words rather than leaving an
        empty space for the reader to interpret.
      */}
      <section className="grid gap-2 sm:grid-cols-2">
        <div>
          <SectionHeader title={t("work-work-item-panel-requested")} />
          <AssigneeTags
            value={(item.assignees ?? []).map(assigneeWire)}
            empty={t("work-work-item-panel-nobody-particular-follows-goal-s-own")}
          />
        </div>
        <div>
          <SectionHeader title={t("work-work-item-panel-running")} />
          {item.agent ? (
            <Chip tone="accent" icon={ICON.agent}>
              {item.agent}
            </Chip>
          ) : (
            <p className="text-2xs text-text-dim">{t("work-work-item-panel-nobody-yet-engine-records-when-agent")}</p>
          )}
        </div>
      </section>

      {/* Agents write these in markdown — lists of steps, file paths in
          backticks, the occasional table — and a monospace-free wall of it was
          the last place in the app still showing the source. */}
      <section>
        <SectionHeader title={t("work-work-item-panel-instructions")} />
        <div className="rounded-control border border-border bg-surface-2 p-2">
          <Markdown text={item.instructions} />
        </div>
      </section>

      {item.state.state === "blocked" && (
        <p className="rounded-control bg-danger-soft px-2 py-1.5 text-2xs text-danger">
          {t("work-work-item-panel-reason-running-goal-again-retries", { reason: String(item.state.reason) })}
        </p>
      )}
      {item.state.state === "rejected" && Array.isArray(item.state.evidence) && (
        <div className="rounded-control bg-danger-soft px-2 py-1.5 text-2xs text-danger">
          {(item.state.evidence as string[]).map((e, i) => (
            <p key={i}>{e}</p>
          ))}
        </div>
      )}

      <section>
        <SectionHeader title={t("work-work-item-panel-result")} />
        {data.result ? (
          <LinkedText
            as="pre"
            className="max-h-56 overflow-auto rounded-control border border-border bg-surface-2 p-2 font-mono text-2xs"
            text={JSON.stringify(data.result, null, 2)}
          />
        ) : (
          <p className="text-2xs text-text-dim">{t("work-work-item-panel-nothing-yielded-yet")}</p>
        )}
        {data.has_result && (
          <a
            className="anim mt-2 inline-flex items-center gap-1.5 text-2xs text-accent-ink underline underline-offset-2"
            href={api.resultUrl(item.id)}
            download={`${item.id}.patch`}
          >
            <ICON.install size={12} aria-hidden />{t("work-work-item-panel-download-captured-patch")}</a>
        )}
      </section>

      <section>
        <SectionHeader title={t("work-work-item-panel-session-transcript")} />
        <SessionTranscript session={item.id} live={isLive(item)} />
      </section>
    </div>
  );
}
