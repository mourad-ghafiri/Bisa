/**
 * One goal as one card, in three compact lines. The first says
 * what and where: the status glyph, the title, who holds the ball, whether
 * it listens for its workflow's start events (*listening*, *paused*),
 * whether the Workflow Agent is designing, where it came from, its tags,
 * when it last moved. The second is the statement, dimmed. The third is the run: every
 * step as a chip — folded to one line when the workflow is long, the current
 * step never hidden — the progress count and the current step named, the
 * workflow's name, who is assigned, how many projects it carries, how many
 * things wait on you, *Act* when the move is yours, and a `⋮` on every card
 * (`cardMenu`) — *Open*; the run's verbs — *New run…*, *Restart*, *Stop* —
 * drawn from the row alone and acted on through the same dialogs the goal
 * page mounts, the view fetched only when a verb is picked; and *Delete…*,
 * the goal page's retirement dialog reached from the list. The facts are
 * `goalCardModel.mjs`'s and `goalStripModel.mjs`'s; this file paints.
 */
import { useEffect, useState } from "react";
import { api } from "../../api";
import { href, navigate } from "../../router";
import type { GoalRow, GoalView, InboxRow, ProjectRow } from "../../types";
import { Avatar, Button, Chip, GOAL_STATUS_ICON, ICON, MoreMenu, RelativeTime, TagChips, Tooltip, WorkingDot, useToast, type MenuItem } from "../../ui";
import { progressCount } from "../_goal/progressModel.mjs";
import { RunVerbDialogs, type RunVerb } from "../_goal/RunVerbDialogs";
import { CurrentStepPill } from "./CurrentStepPill";
import { GoalActPopover } from "./GoalActPopover";
import { assigneeSummary, cardMenu, compactChips, listeningChip, needsYouCount, projectsOf, queuedChip, rowVerbs, secondLine, statusTone, statusWord, workflowWord, designingRow } from "./goalCardModel.mjs";
import { HolderBadge } from "./HolderBadge";
import { RetireDialog } from "../_work/RetireDialog";
import { retiredWords } from "../_work/retireModel.mjs";
import { StepChipStrip } from "./StepChipStrip";
import { MODE_ICON, MODE_LABEL, MODE_MEANING, designs, modeOf } from "../_goal/goalMode.mjs";
import { t } from "../../i18n/l10n.mjs";

export function GoalCard({
  row,
  working,
  projects,
  inbox,
  onChanged,
}: {
  row: GoalRow;
  working: boolean;
  projects: readonly ProjectRow[];
  inbox: readonly InboxRow[];
  onChanged: () => void;
}) {
  const toast = useToast();
  const [acting, setActing] = useState(false);
  // A run verb picked on the row: the goal's view is fetched once, then the
  // page's own dialogs run it.
  const [verb, setVerb] = useState<RunVerb | null>(null);
  const [view, setView] = useState<GoalView | null>(null);
  /** *Delete…* picked: the goal page's retirement dialog, over this card. */
  const [deleting, setDeleting] = useState(false);
  useEffect(() => {
    if (!verb) return;
    const ctl = new AbortController();
    void api
      .goal(row.id, ctl.signal)
      .then(setView)
      .catch((e: unknown) => {
        // A card that left gave the read up itself: there is nobody to tell.
        if (ctl.signal.aborted) return;
        toast.error(e instanceof Error ? e.message : String(e));
        setVerb(null);
      });
    return () => ctl.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fetched once per verb picked
  }, [verb, row.id]);
  const verbs = rowVerbs(row);
  // The menu's shape is the model's, the workflow card's too; the card binds each id to its door or dialog.
  const menu: MenuItem[] = cardMenu(verbs).map((item) => {
    if (item.id === "open") return { label: item.label, icon: ICON.forward, onSelect: () => navigate({ name: "goal", id: row.id }) };
    if (item.id === "delete") return { label: item.label, icon: ICON.delete, danger: true, separatorBefore: true, onSelect: () => setDeleting(true) };
    const icon = item.id === "start" ? ICON.run : item.id === "restart" ? ICON.restart : ICON.stop;
    const picked: RunVerb = item.id;
    return { label: item.label, icon, danger: item.danger, separatorBefore: item.separatorBefore, onSelect: () => setVerb(picked) };
  });
  const queued = queuedChip(row);
  const listening = listeningChip(row);
  const label = row.title ?? (row.statement || t("goals-goal-card-goal", { row: row.id.slice(-6) }));
  const second = secondLine(row);
  const StatusIcon = GOAL_STATUS_ICON[row.status] ?? ICON.goal;
  const tone = statusTone(row.status);
  const folded = compactChips(row.strip.steps, row.strip.current, 12);
  const progress = progressCount(row.strip);
  const workflow = workflowWord(row.strip);
  const attached = projectsOf(row.id, projects);
  const owed = needsYouCount(row.id, inbox);
  const people = assigneeSummary(row.assignees, 3);
  const mode = modeOf(row);
  const designing = designingRow(row, designs(mode));

  return (
    <div className="anim flex min-w-0 flex-col gap-1 rounded-card px-3 py-2 hover:bg-surface-2">
      <div className="flex min-w-0 items-center gap-2">
        <Tooltip label={statusWord(row.status)}>
          <span className="inline-flex">
            <StatusIcon size={14} aria-hidden className="shrink-0" style={{ color: `var(--color-${tone})` }} />
          </span>
        </Tooltip>
        <a href={href({ name: "goal", id: row.id })} className="min-w-0 flex-1 truncate text-sm font-medium text-text" title={row.statement}>
          {label}
        </a>
        <HolderBadge holder={row.holder} strip={row.strip} />
        {row.run_unreadable && (
          <Tooltip label={t("goals-goal-card-goal-names-run-node-cannot-read")}>
            <span className="inline-flex">
              <Chip tone="warn" icon={ICON.warn}>{t("goals-goal-card-run-unreadable")}</Chip>
            </span>
          </Tooltip>
        )}
        {working && <WorkingDot />}
        {queued && (
          <Tooltip label={t("goals-goal-card-run-runs-waits-wait-behind-live", { queued: row.queued })}>
            <span className="inline-flex">
              <Chip tone="quiet" icon={ICON.queued}>{queued}</Chip>
            </span>
          </Tooltip>
        )}
        {listening && (
          <span className="inline-flex" data-goal-listening={listening.paused ? "paused" : "on"}>
            <Chip tone={listening.tone} icon={ICON.signal}>{listening.words}</Chip>
          </span>
        )}
        {designing ? (
          <Tooltip label={t("goals-goal-card-workflow-agent-designing-goal-s-workflow")}>
            <span className="hidden md:inline">
              <Chip tone="quiet" icon={ICON.coreAgent}>{t("goals-goal-card-designing")}</Chip>
            </span>
          </Tooltip>
        ) : (
          <Tooltip label={MODE_MEANING[mode]}>
            <span className="hidden md:inline">
              <Chip tone="quiet" icon={ICON[MODE_ICON[mode]]}>{MODE_LABEL[mode]}</Chip>
            </span>
          </Tooltip>
        )}
        <TagChips tags={row.tags ?? []} max={3} />
        <span className="tnum hidden w-16 shrink-0 text-right text-2xs text-text-dim sm:inline">
          <RelativeTime at={row.last_activity_at} />
        </span>
      </div>
      {second && <p className="truncate pl-6 text-2xs text-text-dim">{second}</p>}
      <div className="flex min-w-0 items-center gap-3 pl-6">
        <div className="flex min-w-0 shrink-0 items-center gap-1.5">
          <StepChipStrip
            strip={{ ...row.strip, steps: folded.shown }}
            onOpenStep={(step) => navigate({ name: "goal", id: row.id }, { tab: "workflow", step })}
          />
          {folded.hidden > 0 && (
            <Tooltip label={t("goals-goal-card-more-step-steps", { hidden: folded.hidden })}>
              <span className="inline-flex">
                <Chip tone="quiet">+{folded.hidden}</Chip>
              </span>
            </Tooltip>
          )}
        </div>
        <span className="tnum shrink-0 text-2xs text-text-dim">{progress.label}</span>
        <CurrentStepPill row={row} withHolder={false} />
        <span className="min-w-0 flex-1" />
        {workflow && (
          <span className="hidden min-w-0 max-w-40 truncate text-2xs text-text-dim lg:inline" title={t("goals-goal-card-workflow", { workflow })}>
            <ICON.workflow size={11} aria-hidden className="mr-1 inline" />
            {workflow}
          </span>
        )}
        {people.shown.length > 0 && (
          <span className="hidden items-center gap-1 md:inline-flex" title={t("goals-goal-card-assigned", { people: [...people.shown.map((p) => p.word), ...(people.more > 0 ? [`+${people.more}`] : [])].join(", ") })}>
            {people.shown.map((p) => (
              <Avatar key={`${p.kind}:${p.word}`} id={p.word} name={p.word} size={16} />
            ))}
            {people.more > 0 && <span className="tnum text-2xs text-text-dim">+{people.more}</span>}
          </span>
        )}
        {attached.length > 0 && (
          <Tooltip label={attached.map((p) => p.project.name).join(", ")}>
            <span className="hidden items-center gap-1 text-2xs text-text-dim md:inline-flex">
              <ICON.project size={11} aria-hidden />
              <span className="tnum">{attached.length}</span>
            </span>
          </Tooltip>
        )}
        {owed > 0 && (
          <Tooltip label={t("goals-goal-card-thing-things-waits-wait", { owed })}>
            <span className="inline-flex">
              <Chip tone="warn" icon={ICON.question}>{owed}</Chip>
            </span>
          </Tooltip>
        )}
        {row.holder === "you" && (
          <GoalActPopover goal={row.id} open={acting} onClose={() => setActing(false)} onChanged={onChanged}>
            <Button size="sm" variant="primary" onClick={() => setActing(true)} aria-label={t("goals-goal-card-act-2", { label })}>{t("goals-goal-card-act")}</Button>
          </GoalActPopover>
        )}
        <MoreMenu vertical label={t("goals-goal-card-menu", { label })} items={menu} />
      </div>
      {verb && view && (
        <RunVerbDialogs
          view={view}
          pending={verb}
          onClose={() => {
            setVerb(null);
            setView(null);
          }}
          onDone={onChanged}
        />
      )}
      {deleting && (
        <RetireDialog
          kind="goal"
          id={row.id}
          name={label}
          wanted="delete"
          open
          onClose={() => setDeleting(false)}
          onRetired={(done, choices) => {
            toast.ok(retiredWords("goal", choices.thing, done.terminated));
            onChanged();
          }}
        />
      )}
    </div>
  );
}
