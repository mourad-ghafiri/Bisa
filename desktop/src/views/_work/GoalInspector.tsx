/**
 * Everything structural about a goal, beside its conversation.
 *
 * Details (statement, the run's shape and progress, spend, assignees), Work
 * (the items the run's agent steps became), Projects and Files. Which panel
 * is showing lives in the URL (`?insp=`), so Back leaves the panel rather
 * than the goal, a link can land straight on Projects, and a reload puts the
 * reader back where they were.
 *
 * **Read-only while a run goes** (`frozen`, from `runControl.mjs`): the
 * assignees, the documents and the files wait for the run to finish — a
 * banner says so, and the one door that stays open is the
 * Workflow tab's amendment. **A project is made only by an agent's
 * `create_project`** — a step that reads or answers runs in the goal's
 * scratch and makes nothing — so the Projects panel lists and never creates
 * or attaches.
 *
 * Each panel comes back where it was scrolled, kept under the goal's place
 * (`shell/useViewScroll`), and the Projects panel draws what it last read
 * at once and reads again behind it.
 */

import { useRef, useState } from "react";
import { api } from "../../api";
import { errorFields, log } from "../../log";
import { href, setSearch, useSearchValue } from "../../router";
import { useViewScroll } from "../../shell/useViewScroll";
import { placeOf, useViewState } from "../../shell/viewMemoryStore";
import { wordsValue } from "../../shell/viewValuesModel.mjs";
import type { GoalView, ProjectRow, WorkItemSpec, Workstream } from "../../types";
import {
  Chip,
  CountBadge,
  EmptyState,
  ErrorNote,
  FileTreeView,
  ICON,
  Markdown,
  SectionHeader,
  SkeletonRows,
  Tabs,
  Tooltip,
  WorkItemStateChip,
  type TabDef,
  useToast,
} from "../../ui";
import { TerminalLauncher } from "../../shell/TerminalLauncher";
import { AssigneePicker, AssigneeTag, AssigneeTags, useAssigneeOptions } from "./AssigneePicker";
import { DocumentsCard } from "./DocumentsCard";
import { ProjectDot, projectIsGit } from "./ProjectDetail";
import { madeByGoal, stepOf } from "./projectOriginModel.mjs";
import { designLine, originChips, panelOf, projectsWords, runCardWords, spendWords, workstreamWord, type Panel } from "./goalInspectorModel.mjs";
import { headline } from "./workItemRowModel.mjs";
import { stateLabel as workstreamStateLabel } from "./workstreamCardModel.mjs";
import { assigneeWire } from "./types";
import { attempt, useAsync } from "./useAsync";
import { readKey } from "./keptReadsModel.mjs";
import { progress } from "../_workflow/runView.mjs";
import { StepChipStrip } from "../_goals/StepChipStrip";
import { FROZEN_HINT } from "../_goal/runControl.mjs";
import { MODE_ICON, MODE_MEANING, designs, modeOf } from "../_goal/goalMode.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";
import { percent } from "../../i18n/format.mjs";

/**
 * Something is happening in this goal right now.
 *
 * The Files tab polls only while this is true. `review` counts: a run that
 * finished can still be writing its result, and a tree that stops following
 * one row too early is the case a reader never notices is wrong.
 */
function isRunning(w: WorkItemSpec): boolean {
  const s = w.state.state;
  return s === "claimed" || s === "in_progress" || s === "review";
}

/** A project this goal can see, with the checkouts open against it. */
interface ProjectWithSites {
  row: ProjectRow;
  workstreams: Workstream[];
}

function ProjectsPanel({ goal }: { goal: string }) {
  const { data, error, loading, reload } = useAsync(
    async (s) => {
      const { projects } = await api.projects(goal, s);
      return Promise.all(
        projects.map(async (row) => ({
          row,
          workstreams: await api
            .projectWorkstreams(row.project.id, s)
            .then((r) => r.workstreams.filter((w) => w.kind.kind !== "primary"))
            .catch((e: unknown) => {
              // The project still has its row; that its checkouts could not be read is the log's to keep.
              if (!s.aborted) log.warn("goal", "a project's workstreams could not be read; its row lists none", { project: row.project.id, ...errorFields(e) });
              return [] as Workstream[];
            }),
        })),
      );
    },
    [goal],
    { keep: readKey("goal-projects-sites", goal) },
  );

  const rows: ProjectWithSites[] = data ?? [];

  if (loading && rows.length === 0) return <SkeletonRows rows={3} />;
  if (error) return <ErrorNote error={error} retry={reload} />;

  if (rows.length === 0) {
    return (
      <EmptyState
        icon={ICON.project}
        title={t("work-goal-inspector-no-projects-yet")}
        hint={t("work-goal-inspector-goal-s-projects-born-run-step")}
        action={null}
      />
    );
  }

  return (
    <>
      {/* What this goal made first — its own run's projects, whichever door
 — then the rest it can see. A link opens the IDE on the
          project; the rail follows the route and shows the row. */}
      <ul className="flex flex-col gap-2">
        {[...rows].sort((a, b) => Number(madeByGoal(b.row.project.origin, goal)) - Number(madeByGoal(a.row.project.origin, goal))).map(({ row, workstreams }) => (
          <li key={row.project.id} className="rounded-control border border-border p-2">
            <a
              href={href({ name: "workbench", scope: "workstream", id: row.project.id })}
              title={t("work-goal-inspector-open-project-ide")}
              className="anim flex min-w-0 items-center gap-1.5 underline-offset-2 hover:underline"
            >
              <ProjectDot exists={row.exists} />
              <span className="min-w-0 flex-1 truncate text-xs font-medium">{row.project.name}</span>
              {madeByGoal(row.project.origin, goal) && (
                <Chip tone="quiet" title={stepOf(row.project.origin) ? t("work-goal-inspector-made-step-goal-s-own-design", { step: stepOf(row.project.origin)?.step }) : t("work-goal-inspector-made-from-goal")}>
                  {stepOf(row.project.origin) ? t("work-goal-inspector-made-here-step", { step: stepOf(row.project.origin)?.step }) : t("work-goal-inspector-made-here")}
                </Chip>
              )}
              {row.project.origin.origin === "step" && (
                <Chip tone="quiet" title={t("work-goal-inspector-made-step-library-workflow-running-here", { step: stepOf(row.project.origin)?.step })}>{t("work-goal-inspector-workflow-step")}</Chip>
              )}
              {projectIsGit(row.project) ? <Chip tone="neutral">{t("work-goal-inspector-git")}</Chip> : <Chip tone="quiet">{t("work-goal-inspector-plain")}</Chip>}
            </a>
            <Tooltip label={row.path} side="left">
              <p className="mt-0.5 truncate font-mono text-2xs text-text-dim">{row.path}</p>
            </Tooltip>
            {workstreams.length > 0 && (
              <ul className="mt-1.5 flex flex-col gap-0.5">
                {workstreams.map((w) => (
                  <li key={w.id}>
                    <a
                      href={href({ name: "workbench", scope: "workstream", id: w.id })}
                      className="anim flex items-center gap-1.5 rounded-control px-1 py-0.5 hover:bg-surface-2"
                    >
                      <ICON.workstream size={11} aria-hidden className="shrink-0 text-text-dim" />
                      <span className="min-w-0 flex-1 truncate font-mono text-2xs">
                        {workstreamWord(w)}
                      </span>
                      <Chip tone={w.state.state === "closed" ? "quiet" : "neutral"}>{workstreamStateLabel(w.state)}</Chip>
                    </a>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}

/**
 * The goal's items: what the run's agent steps became, who is running them.
 *
 * One dense row per item — the state, the first line of the instructions
 * (`headline`, the item's name: an agent's instructions open with what to
 * do), the step at the right — and a second line only when somebody was
 * asked or an agent is running it. The rows used to be framed cards with a
 * two-line clamp of the instructions; in a 380 px pane, once a theme faded
 * the border, that read as text floating with big gaps between items. The
 * whole body is one click away, in the item's panel.
 *
 * `assignees` and `agent` answer two different questions, so a row shows them
 * as two different things. The request is who the step was delegated to; the
 * runner is the agent the engine picked once assignment resolved. There is no
 * "add" here: an item is born of a step, and a step is added on the Workflow
 * tab.
 */
function WorkPanel({ items, onOpenItem }: { items: WorkItemSpec[]; onOpenItem: (item: WorkItemSpec) => void }) {
  const { describe } = useAssigneeOptions();

  if (items.length === 0) {
    return (
      <EmptyState
        icon={ICON.workItem}
        title={t("work-goal-inspector-no-work-items")}
        hint={t("work-goal-inspector-each-agent-step-run-becomes-one")}
        action={null}
      />
    );
  }

  return (
    <ul className="flex flex-col gap-0.5">
      {items.map((w) => {
        const assignees = w.assignees ?? [];
        const name = headline(w.instructions);
        return (
          <li key={w.id}>
            <button
              type="button"
              onClick={() => onOpenItem(w)}
              title={name}
              className="anim flex w-full flex-col gap-0.5 rounded-control px-1.5 py-1 text-left hover:bg-surface-2"
            >
              <span className="flex w-full min-w-0 items-center gap-1.5">
                <WorkItemStateChip state={w.state} />
                <span className="min-w-0 flex-1 truncate text-xs">{name}</span>
                {w.step && <span className="shrink-0 text-2xs text-text-dim">{t("work-goal-inspector-step", { step: w.step })}</span>}
              </span>
              {(assignees.length > 0 || w.agent) && (
                <span className="flex w-full flex-wrap items-center gap-1">
                  {assignees.map((a) => (
                    <AssigneeTag key={assigneeWire(a)} option={describe(assigneeWire(a))} />
                  ))}
                  {w.agent && (
                    <Tooltip label={t("work-goal-inspector-agent-running-picked-engine-not-asked")}>
                      <span>
                        <Chip tone="neutral" icon={ICON.agent}>
                          {w.agent}
                        </Chip>
                      </span>
                    </Tooltip>
                  )}
                </span>
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

/** The Details panel's project card: a count, and the way into the tab. */
function ProjectsCard({ goal, onSeeAll }: { goal: string; onSeeAll: () => void }) {
  const { data } = useAsync((s) => api.projects(goal, s), [goal], { keep: readKey("goal-projects", goal) });
  const words = projectsWords(data?.projects);
  return (
    <section>
      <SectionHeader flush title={t("work-goal-inspector-projects")} />
      {words === null ? (
        // Nothing is created or attached here: a goal's projects are born of its run.
        <p className="text-2xs text-text-dim">{t("work-goal-inspector-none-yet-see-projects")}</p>
      ) : (
        <button type="button" onClick={onSeeAll} className="anim w-full rounded-control border border-border p-2 text-left hover:bg-surface-2">
          <p className="truncate text-2xs">{words.names}</p>
          <p className="mt-0.5 text-2xs text-text-dim">{words.counts}</p>
        </button>
      )}
    </section>
  );
}

/** The run, step by step: the frozen workflow's order with each step's record. */
function RunCard({ view }: { view: GoalView }) {
  const { goal, run, runs } = view;
  if (!run) {
    const door = (words: string) => (
      <button type="button" onClick={() => setSearch({ tab: "workflow" })} className="text-accent-ink underline underline-offset-2">
        {words}
      </button>
    );
    return (
      <section>
        <SectionHeader flush title={t("work-goal-inspector-run")} />
        <p className="text-2xs text-text-dim">{goal.workflow ? rich("work-goal-inspector-not-started-open", { door }) : rich("work-goal-inspector-no-workflow-yet-open", { door })}</p>
      </section>
    );
  }
  const pct = Math.round(progress(run) * 100);
  return (
    <section>
      <SectionHeader flush title={t("work-goal-inspector-run")} />
      <button type="button" onClick={() => setSearch({ tab: "workflow" })} className="anim mb-1.5 w-full rounded-control border border-border p-2 text-left hover:bg-surface-2">
        <p className="flex items-center gap-1.5 text-2xs">
          <ICON.run size={12} aria-hidden className="text-text-dim" />
          <span className="min-w-0 flex-1 truncate font-medium">{run.workflow.name}</span>
          <span className="tnum text-text-dim">{percent(pct / 100)}</span>
        </p>
        {/* Progress is a fact, not a summons: the kit Meter's quiet fill. */}
        <div className="mt-1 h-1 overflow-hidden rounded-full bg-border/70">
          <div className="h-full bg-text-dim" style={{ width: `${pct}%` }} />
        </div>
        <p className="mt-1 text-2xs text-text-dim">{runCardWords(run, runs)}</p>
      </button>
      <StepChipStrip strip={view.strip} onOpenStep={(st) => setSearch({ tab: "workflow", step: st })} />
    </section>
  );
}

/** A tree nobody unfolded: one list, so a goal with nothing kept starts from the same beginning every render. */
const NO_FOLDERS: string[] = [];

export function GoalInspector({
  view,
  frozen,
  onOpenItem,
  onWorkChanged,
}: {
  view: GoalView;
  /** A run is going: every control is read-only until it finishes (`runControl.mjs`). */
  frozen: boolean;
  onOpenItem: (item: WorkItemSpec) => void;
  /** Kept for the screen that mounts us; the run's changes arrive on the bus. */
  onWorkChanged?: () => void;
}) {
  const toast = useToast();
  const [savingAssignees, setSavingAssignees] = useState(false);
  const saveAssignees = async (next: string[]) => {
    setSavingAssignees(true);
    await attempt(() => api.setAssignees(view.goal.id, next), toast.error, () => {
      toast.ok(next.length > 0 ? t("work-goal-inspector-assignment-saved") : t("work-goal-inspector-assignment-cleared"));
      onWorkChanged?.();
    });
    setSavingAssignees(false);
  };
  const [param, setParam] = useSearchValue("insp");
  const panel: Panel = panelOf(param);
  // `details` is the default, so it is written as the *absence* of the key:
  // a URL that says nothing and a URL that says "details" would otherwise be
  // two history entries for one place.
  const setPanel = (p: Panel) => setParam(p === "details" ? null : p);
  const { goal, run, work_items, spent, guidance } = view;
  const mode = modeOf(goal);
  // One scrollport shows four panels: each is kept under its own name, and
  // put back when it is the one shown.
  const root = useRef<HTMLDivElement>(null);
  const place = placeOf({ name: "goal", id: goal.id });
  useViewScroll(root, `${place}#insp:${panel}`, place);
  // The folders the Files tree was left unfolded with, under the goal's place.
  const [openFolders, setOpenFolders] = useViewState(place, "files.open", NO_FOLDERS, wordsValue);

  const tabs: TabDef[] = [
    { id: "details", label: t("work-goal-inspector-details") },
    { id: "work", label: t("work-goal-inspector-work"), badge: <CountBadge count={work_items.length} tone="neutral" /> },
    { id: "projects", label: t("work-goal-inspector-projects") },
    { id: "files", label: t("work-goal-inspector-files") },
  ];

  return (
    <div ref={root} className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 overflow-x-auto">
        <Tabs tabs={tabs} active={panel} onChange={(id) => setPanel(id as Panel)} />
      </div>

      <div data-scroll-keep={`insp:${panel}`} className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {frozen && (
          <p className="mb-4 flex items-start gap-1.5 rounded-control border border-border bg-surface-2/70 px-3 py-2 text-2xs text-text">
            <ICON.run size={12} aria-hidden className="mt-px shrink-0 text-text-dim" />
            {FROZEN_HINT}
          </p>
        )}
        {panel === "details" && (
          <div className="flex flex-col gap-6">
            <section>
              <SectionHeader flush title={t("work-goal-inspector-statement")} />
              <Markdown text={goal.statement} />
            </section>

            {designs(mode) && designLine(mode, guidance) !== null && (
              <p className="flex items-start gap-1.5 rounded-control border border-border bg-surface-2 px-2 py-1.5 text-2xs text-text-dim">
                <ICON.coreAgent size={12} aria-hidden className="mt-px shrink-0" />
                {designLine(mode, guidance)}
              </p>
            )}

            <RunCard view={view} />

            <ProjectsCard goal={goal.id} onSeeAll={() => setPanel("projects")} />

            <DocumentsCard goal={goal.id} readOnly={frozen} onOpenFiles={() => setPanel("files")} />

            <section>
              <SectionHeader flush title={t("work-goal-inspector-spend")} />
              <p className="tnum text-2xs text-text-dim">{spendWords(spent)}</p>
            </section>

            <section>
              <SectionHeader flush title={t("work-goal-inspector-assignees")} />
              <p className="mb-1.5 text-2xs text-text-dim">{t("work-goal-inspector-names-agent-every-step")}</p>
              {frozen ? (
                <AssigneeTags value={(goal.assignees ?? []).map(assigneeWire)} />
              ) : (
                <AssigneePicker
                  value={(goal.assignees ?? []).map(assigneeWire)}
                  onChange={(next) => void saveAssignees(next)}
                  disabled={savingAssignees}
                />
              )}
            </section>

            <section>
              <SectionHeader flush title={t("work-goal-inspector-origin")} />
              <ul className="flex flex-wrap gap-1.5 text-2xs text-text-dim">
                {originChips(goal, run, mode).map((chip) => {
                  const drawn = (
                    <Chip tone="quiet" icon={chip.icon === "mode" ? ICON[MODE_ICON[mode]] : ICON[chip.icon]} title={chip.icon === "mode" ? MODE_MEANING[mode] : chip.title}>
                      {chip.label}
                    </Chip>
                  );
                  return <li key={chip.id}>{chip.route ? <a href={href(chip.route)}>{drawn}</a> : drawn}</li>;
                })}
              </ul>
            </section>
          </div>
        )}

        {panel === "projects" && <ProjectsPanel goal={goal.id} />}

        {panel === "files" && (
          <>
            {!frozen && (
              <div className="mb-2 flex justify-end">
                <TerminalLauncher scope="goal" id={goal.id} label={goal.title ?? t("work-goal-inspector-goal", { goal: goal.id.slice(-6) })} />
              </div>
            )}
            <FileTreeView
              scope="goal"
              id={goal.id}
              goal={goal.id}
              live={work_items.some(isRunning)}
              showRoot
              openFolders={openFolders}
              onOpenFolders={setOpenFolders}
              emptyHint={t("work-goal-inspector-nothing-has-been-written-yet-agents")}
            />
          </>
        )}

        {panel === "work" && <WorkPanel items={work_items} onOpenItem={onOpenItem} />}
      </div>
    </div>
  );
}
