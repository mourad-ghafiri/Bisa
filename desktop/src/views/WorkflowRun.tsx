/**
 * One run of the workspace, by its id (`#/runs/<rid>`; 03-workflows §Runs of
 * the workspace) — where every door to such a run lands: the Runs pane's
 * *Open*, *Run…*, the Inbox's ask, the roster. Its header: the workflow and
 * the run's number, its status and who it waits on, when it started and who
 * started it, with *Stop* while it goes, *Restart* and the workflow's
 * designer. *Your move* — what it owes a person, decided through the run's
 * own home (`YourMoveBand`, the goal page's band). **Progress** — its steps
 * read downward (`RunSteps`, the goal page's rows, every verb addressed to
 * the run). **Canvas** — the frozen workflow, read-only, wearing the run.
 *
 * A goal's run opened by its id is its goal's to show: the page hands it to
 * the goal's Workflow tab on that run. The bus refreshes the page whenever
 * its own run moves — a step, a boundary, a gate or a question on it — and
 * when its workflow is deleted, which takes the run with it (`movesRun`).
 * What the page says is `workflowRunsModel.runPageFacts`'s; this is paint.
 *
 * The page is keyed by its run (`App.tsx`) and keeps how it stood under the
 * run's place (`shell/viewMemoryStore`): the tab, the step picked on the
 * canvas and where the canvas looked, the rows opened, where Progress was
 * scrolled. The last answer is drawn at once and read again behind it; a
 * run that is gone is left for the library (`useGonePlace`).
 */

import { useCallback, useEffect, useMemo, useRef } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { navigate, replace } from "../router";
import { useGonePlace } from "../shell/useGonePlace";
import { useViewScroll } from "../shell/useViewScroll";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { idValue, wordOf } from "../shell/viewValuesModel.mjs";
import { Button, Chip, ErrorNote, ICON, PageHeader, RUN_STATUS_ICON, RelativeTime, SkeletonRows, Tabs } from "../ui";
import { RunSteps } from "./_goal/RunSteps";
import { YourMoveBand, scrollToYourMove } from "./_goal/YourMoveBand";
import { readKey } from "./_work/keptReadsModel.mjs";
import { forgetRead } from "./_work/keptReadsStore";
import { useAsync } from "./_work/useAsync";
import { Designer } from "./_workflow/Designer";
import { keptSelection } from "./_workflow/designerMemoryModel.mjs";
import { canvasViewportAt, rememberViewportAt } from "./_workflow/designerMemoryStore";
import { useDesignerSettings } from "./_workflow/useDesignerSettings";
import { useRunVerbs } from "./_workflow/useRunVerbs";
import { movesRun, readStanding, runPageFacts } from "./_workflow/workflowRunsModel.mjs";
import { t } from "../i18n/l10n.mjs";

const RUN_TABS = ["progress", "canvas"] as const;
type RunTab = (typeof RUN_TABS)[number];
const TAB_DEFS = [
  { id: "progress", label: t("screens-workflow-run-progress") },
  { id: "canvas", label: t("screens-workflow-run-canvas") },
];
/** The tab the memory gives back: one of the two, else Progress. */
const tabValue = wordOf(RUN_TABS);

export default function WorkflowRun({ id }: { id: string }) {
  const settings = useDesignerSettings();
  const place = placeOf({ name: "run", id });
  const runRead = readKey("run", id);
  const { data, error, loading, reload, missing } = useAsync((s) => api.run(id, s), [id], { keep: runRead });
  // The node says there is no such run — it went with its workflow while
  // the app was closed, or the link was wrong: what the window kept of it is
  // no longer its answer, and a place the person had been on is left for
  // the library.
  useEffect(() => {
    if (missing) forgetRead(runRead);
  }, [missing, runRead]);
  useGonePlace(missing, { name: "run", id });
  const [tab, setTab] = useViewState<RunTab>(place, "tab", "progress", tabValue);
  const [picked, setSelected] = useViewState<string | null>(place, "canvas:step", null, idValue);
  // Where the canvas looked when it was left; the canvas reads it once, when it opens.
  const startViewport = useMemo(() => canvasViewportAt(place), [place]);
  const keepViewport = useCallback((v: { x: number; y: number; zoom: number }) => rememberViewportAt(place, v), [place]);
  const workflow = data?.run.workflow.id ?? null;
  useEngineEvents((e) => {
    if (movesRun(e, id, workflow)) reload();
  });
  // What a restarted node resumed, failed or withdrew reaches the page by no
  // frame: the read is made again when the bus comes back (`useAsync`).
  // Stop and Restart, one at a time: a restart lands on the new run's page.
  const runVerbs = useRunVerbs(reload);
  // What the page says, decided before it draws.
  const facts = useMemo(() => runPageFacts(data), [data]);
  // A goal's run is its goal's: the goal's Workflow tab draws it, on this run.
  const goal = facts?.handsTo?.route.id ?? null;
  useEffect(() => {
    if (goal) replace({ name: "goal", id: goal }, { tab: "workflow", run: id });
  }, [goal, id]);
  // Progress comes back where it was scrolled. The page is drawn once the
  // run is read: what is on screen changes then, and the scroll is put back then.
  const root = useRef<HTMLDivElement>(null);
  // A re-read that failed keeps the run on screen and says why; gone, or never read, the note stands alone.
  const read = readStanding({ data, loading, error, missing }, t("workflow-runs-what-run"));
  const drawn = (read.standing === "ready" || read.standing === "stale") && !goal;
  useViewScroll(root, `${place}#${drawn ? tab : "reading"}`, place);

  if (read.standing === "failed" || read.standing === "gone") {
    return (
      <div className="p-6">
        <ErrorNote error={read.line ?? t("screens-goal-detail-not-found")} retry={reload} />
      </div>
    );
  }
  if (!data || !facts || goal) {
    return (
      <div className="p-6">
        <SkeletonRows rows={6} className="max-w-2xl" />
      </div>
    );
  }

  const { run, summary, needs_actions } = data;
  const { words, verbs } = facts;
  const StatusIcon = RUN_STATUS_ICON[summary.status] ?? ICON.run;
  // A remembered step the run's workflow does not have is no pick.
  const selected = keptSelection(picked, run.workflow.steps);
  const busy = runVerbs.busy(run.id);

  return (
    <div ref={root} className="flex h-full min-h-0 flex-col">
      <PageHeader
        className="shrink-0 border-b border-border"
        icon={ICON.run}
        back={{ label: t("screens-workflow-run-back-to-workflow"), onClick: () => navigate({ name: "workflow", id: run.workflow.id }, { panel: "runs" }) }}
        title={facts.title}
        subtitle={run.workflow.description || undefined}
        meta={
          <>
            <Chip tone={words.tone} icon={StatusIcon}>
              {words.word}
            </Chip>
            <Chip tone="quiet">{facts.holder}</Chip>
            {words.at != null && (
              <span className="text-2xs text-text-dim">
                {words.atWord} <RelativeTime at={words.at} />
              </span>
            )}
            <span className="text-2xs text-text-dim">{facts.startedBy}</span>
            <Chip tone="quiet">{facts.revision}</Chip>
          </>
        }
        actions={
          <div className="flex items-center gap-1">
            {verbs.stop && (
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => runVerbs.stop(run.id)}>
                <ICON.stop size={12} aria-hidden />
                {t("workflow-runs-pane-stop")}
              </Button>
            )}
            {verbs.restart && (
              <Button size="sm" variant="ghost" disabled={busy} onClick={() => runVerbs.restart(run.id)}>
                <ICON.restart size={12} aria-hidden />
                {t("workflow-runs-pane-restart")}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => navigate({ name: "workflow", id: run.workflow.id })}>
              <ICON.workflow size={12} aria-hidden />
              {t("screens-workflow-run-open-workflow")}
            </Button>
          </div>
        }
      />
      {read.standing === "stale" && (
        <p role="status" className="shrink-0 border-b border-border px-4 py-1 text-2xs text-danger">
          {read.line}
        </p>
      )}
      <Tabs className="shrink-0 border-b border-border px-4" tabs={TAB_DEFS} active={tab} onChange={(next) => setTab(next as RunTab)} />
      <div data-scroll-keep={tab === "progress" ? "tab:progress" : undefined} className={tab === "progress" ? "relative min-h-0 flex-1 overflow-y-auto" : "relative flex min-h-0 flex-1 flex-col overflow-hidden"}>
        <YourMoveBand actions={needs_actions} adoptInputs={null} onResolved={reload} />
        {tab === "progress" ? (
          <div className="mx-auto flex w-full max-w-4xl flex-col gap-2 px-4 py-3">
            <RunSteps
              run={run}
              strip={{ current: facts.live, steps: [] }}
              place={place}
              onChanged={reload}
              onOpenItem={(item) => navigate({ name: "workbench", scope: "work_item", id: item })}
              onDecide={scrollToYourMove}
            />
            {facts.ended && <p className="px-3 py-2 text-2xs text-text-dim">{facts.ended}</p>}
          </div>
        ) : (
          <main className="relative min-h-0 flex-1">
            <Designer value={run.workflow} problems={[]} run={run} selected={selected} onSelect={setSelected} onChange={() => {}} readOnly settings={settings} startViewport={startViewport} onViewport={keepViewport} />
          </main>
        )}
      </div>
    </div>
  );
}
