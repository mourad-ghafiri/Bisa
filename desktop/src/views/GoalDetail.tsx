/**
 * A goal *is* its run — and its conversation.
 *
 * Read downward: a header that says what the goal is and where it stands
 * (title, statement, who holds the ball, the run as a strip of step chips),
 * then **Your move** — everything owed to a person, as the same answerable
 * cards the Inbox shows, fed by the node's one builder so the two never
 * disagree — then three tabs. **Progress** is the default: the run's steps as
 * rows, the live ones open, with what each produced and the verbs it admits.
 * **Conversation** is the goal's thread. **Workflow** is the canvas.
 * Everything structural — the run's card, work, projects, files, assignees —
 * lives in the right-hand Details pane where it stays visible
 * while you read.
 *
 * `?tab=` carries the tab (an unknown value is Progress); the chips elsewhere
 * still land on `?tab=workflow&step=`.
 *
 * **Left as it stood.** The page is keyed by its goal (`App.tsx`) and keeps
 * what it showed under the goal's place (`shell/viewMemoryStore`): the last
 * answer is drawn at once and read again behind it, each tab comes back
 * where it was scrolled, and the Details pane stands as it was left — it
 * opens by itself only for a goal the person has never been on. A goal that
 * is gone is left for the list (`useGonePlace`).
 */

import { endedOf, saidAfter, stopTone, willStopWords } from "../shell/stopOutcomeModel.mjs";
import { useSessions } from "../shell/sessionsStore";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { navigate, setSearch, useSearchValue } from "../router";
import { useAux, AuxPortal } from "../shell/AuxPane";
import { placeWasKnown } from "../shell/placeMemoryStore";
import { useGonePlace } from "../shell/useGonePlace";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf } from "../shell/viewMemoryStore";
import type { WorkItemSpec } from "../types";
import { closeBody, closeWords, pageFacts, replacements, type CloseForm } from "./_goal/goalPageModel.mjs";
import { Button, ConfirmDialog, Dialog, ErrorNote, Field, GOAL_STATUS_ICON, ICON, Select, SkeletonRows, Tabs, TextArea, useToast, type MenuItem } from "../ui";
import { GoalHeader } from "./_goal/GoalHeader";
import { ProgressTab } from "./_goal/ProgressTab";
import { YourMoveBand, scrollToYourMove } from "./_goal/YourMoveBand";
import { adoptAction, bandActions } from "./_goal/proposalRouting.mjs";
import { designInProgress } from "./_goal/designStatus.mjs";
import { panelFrozen, runVerbs, liveSessionsOf, spawnedOpen } from "./_goal/runControl.mjs";
import { RunVerbDialogs, type RunVerb } from "./_goal/RunVerbDialogs";
import { DEFAULT_TAB, GOAL_TABS, GOAL_TAB_LABEL, tabOf } from "./_goal/goalTabs.mjs";
import { GoalConversationPane } from "./_goal/GoalConversationPane";
import { GoalInspector } from "./_work/GoalInspector";
import { WorkItemPanel } from "./_work/WorkItemPanel";
import { attempt, useAsync } from "./_work/useAsync";
import { readKey } from "./_work/keptReadsModel.mjs";
import { forgetRead } from "./_work/keptReadsStore";
import { retiredWords } from "./_work/retireModel.mjs";
import { RetireDialog } from "./_work/RetireDialog";
import { GoalWorkflowTab } from "./_workflow/GoalWorkflowTab";
import { WorkflowPicker } from "./_workflow/WorkflowPicker";
import { t as tr } from "../i18n/l10n.mjs";

/**
 * The frames that change what this screen shows. `guided` is not one of
 * them: `DesigningCard` applies that frame to its own state, and refetching
 * the whole goal on every status tick was what kept the Workflow tab
 * re-rendering — `workflow_proposed` / `workflow_changed` are the design
 * frames that change the page.
 */
const STRUCTURAL = new Set([
  "run_started",
  "run_queued",
  "run_finished",
  "run_cancelled",
  "step_changed",
  "goal_closed",
  "workflow_proposed",
  "workflow_changed",
  "workflow_archived",
  "goal_archived",
  "gate_opened",
  "gate_decided",
  "question_asked",
  "result_accepted",
  "execution_ended",
  // The goal's listening: armed, cleared or paused, and a start event of its
  // workflow that could not start a run.
  "listening_changed",
  "listener_failed",
  "guided",
  "attachment_changed",
  "document_added",
  "project_created",
]);

const TAB_DEFS = GOAL_TABS.map((id) => ({ id, label: GOAL_TAB_LABEL[id] }));

/** The close dialog as it opens: no reason given, no goal named. */
const NOTHING_SAID: CloseForm = { rationale: "", replacedBy: null };

export default function GoalDetail({ id }: { id: string }) {
  const toast = useToast();
  const ws = useWorkspace();
  const sessionRows = useSessions();
  const aux = useAux();
  const [tabParam, setTabParam] = useSearchValue("tab");
  const tab = tabOf(tabParam);
  const setTab = (t: string) => setTabParam(t === DEFAULT_TAB ? null : t);
  const [retiring, setRetiring] = useState<"archive" | "delete" | null>(null);
  const [confirmClose, setConfirmClose] = useState(false);
  // Why it is closed, or the goal that takes its place: a dialog's fields, empty every time it opens.
  const [closing, setClosing] = useState<CloseForm>(NOTHING_SAID);
  const [choosing, setChoosing] = useState(false);
  const [verb, setVerb] = useState<RunVerb | null>(null);

  const place = placeOf({ name: "goal", id });
  const goalRead = readKey("goal", id);
  const { data, error, loading, reload, missing } = useAsync((s) => api.goal(id, s), [id], { keep: goalRead });
  // The node says there is no such goal — it went while the app was closed,
  // or the link was wrong: what the window kept of it is no longer its
  // answer, and a place the person had been on is left for the list.
  useEffect(() => {
    if (missing) forgetRead(goalRead);
  }, [missing, goalRead]);
  useGonePlace(missing, { name: "goal", id });
  // The workflow the goal points at while no run carries a copy: its inputs
  // are what an adoption or a start asks for.
  const storedId = data && !data.run ? data.goal.workflow ?? null : null;
  const { data: stored } = useAsync(async (s) => (storedId ? (await api.workflow(storedId, s)).workflow : null), [storedId], { keep: readKey("goal-workflow", storedId) });
  // Each tab comes back where it was scrolled. The Conversation tab is left
  // out — a thread's place is the thread's own to keep — and the page is
  // drawn once the goal is read: what is on screen changes then, and the
  // scroll is put back then.
  const scrolls = useRef<HTMLDivElement>(null);
  useViewScroll(scrolls, `${place}#${data && !error ? tab : "reading"}`, place);
  // Stable per fetch: the dialogs that take these memoise on them. Both memos
  // sit above the early returns below — a hook after a conditional return is
  // a hook React sometimes does not see.
  /** The workflow a start would run: the stored one before a run, the run's frozen copy after. */
  const startable = data?.run?.workflow ?? stored ?? null;
  const refresh = useCallback(() => reload(), [reload]);
  const refreshAll = useCallback(() => {
    reload();
    ws.refresh();
  }, [reload, ws]);

  // The goal is gone — retired here, from the CLI, from another desktop:
  // the screen leaves once, whichever came first, the dialog's after-effect
  // or the bus.
  const left = useRef(false);
  const leave = useCallback(
    (words: string) => {
      if (left.current) return;
      left.current = true;
      toast.ok(words);
      navigate({ name: "goals" });
    },
    [toast],
  );
  useEngineEvents((e) => {
    if (e.payload.type === "goal_deleted") {
      leave(tr("screens-goal-detail-goal-deleted"));
      return;
    }
    if (STRUCTURAL.has(e.payload.type)) refresh();
  }, id);

  // The dialogs stand outside the loading and error gates below: a goal
  // that is closed and gone mid-confirm turns the screen into a note, and
  // the dialog must not go with it while its work is in flight.
  const dialogs = (
    <>
      <ConfirmDialog
        open={confirmClose}
        onClose={() => setConfirmClose(false)}
        title={tr("screens-goal-detail-close-goal")}
        confirmLabel={tr("screens-goal-detail-close")}
        body={
          <div className="flex flex-col gap-3">
            <p>{tr("screens-goal-detail-live-run-stopped-queued-runs-withdrawn")}</p>
            {/* What the close reaches beside the run: the sessions on it, the goals it spawned. */}
            {willStopWords({ sessions: liveSessionsOf(sessionRows, id), children: spawnedOpen(ws.goals, id), closing: true }) && <p>{willStopWords({ sessions: liveSessionsOf(sessionRows, id), children: spawnedOpen(ws.goals, id), closing: true })}</p>}
            <Field label={tr("screens-goal-detail-close-why")} hint={tr("screens-goal-detail-close-why-hint")}>
              <TextArea rows={2} value={closing.rationale ?? ""} disabled={!!closing.replacedBy} onChange={(e) => setClosing((f) => ({ ...f, rationale: e.target.value }))} />
            </Field>
            <Field label={tr("screens-goal-detail-close-replaced-by")} hint={tr("screens-goal-detail-close-replaced-by-hint")}>
              {/* The select reads `""` for the choice not made and the form holds `null`. */}
              <Select value={closing.replacedBy ?? ""} onChange={(e) => setClosing((f) => ({ ...f, replacedBy: e.target.value || null }))}>
                <option value="">{tr("screens-goal-detail-close-replaced-by-nothing")}</option>
                {replacements(ws.goals, id).map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.label}
                  </option>
                ))}
              </Select>
            </Field>
            <p className="text-2xs text-text-dim">{closeWords(closing, ws.goals, id).records}</p>
          </div>
        }
        onConfirm={() => {
          setConfirmClose(false);
          const said = closeWords(closing, ws.goals, id).closed;
          void attempt(() => api.closeGoal(id, closeBody(closing, id)), toast.error, (answer) => {
            const ended = endedOf(answer);
            toast[stopTone(ended)](saidAfter(said, ended, { closed: true }));
            refreshAll();
          });
        }}
      />
      {retiring && (
        <RetireDialog
          kind="goal"
          id={id}
          name={data?.goal.title ?? tr("screens-goal-detail-goal")}
          wanted={retiring}
          open
          onClose={() => setRetiring(null)}
          onRetired={(done, choices) => {
            const said = retiredWords("goal", choices.thing, done.terminated, done.ended);
            if (choices.thing === "delete") leave(said);
            else {
              toast.ok(said);
              refreshAll();
            }
          }}
        />
      )}
    </>
  );

  /** Land in the inspector's Projects tab — one item from the header menu. */
  const showProjects = () => setSearch({ aux: "inspector", auxId: null, insp: "projects" });
  const showDetails = () => setSearch({ aux: "inspector", auxId: null, insp: null });

  // The details pane is the goal's home state — opened on arrival at a goal
  // the person has never been on, and never left on a work-item panel a
  // link carried from elsewhere. A goal they come back to stands as they
  // left it: the pane open or closed, on the panel it showed.
  useEffect(() => {
    if (placeWasKnown(placeOf({ name: "goal", id }))) return;
    if (!aux.kind || aux.kind === "session") aux.open("inspector");
    // Only on id change: the reader may close the pane deliberately.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (loading && !data) {
    return (
      <>
        <div className="p-6">
          <SkeletonRows rows={6} className="max-w-2xl" />
        </div>
        {dialogs}
      </>
    );
  }
  if (error || !data) {
    return (
      <>
        <div className="p-6">
          <ErrorNote error={error ?? tr("screens-goal-detail-not-found")} retry={reload} />
        </div>
        {dialogs}
      </>
    );
  }

  const { goal, run, runs, guidance, pending_gates } = data;
  const working = (ws.working[id] ?? []).length > 0;
  // Where the run stands, whether a design waits, whose design it is, whether it begins on events (`goalPageModel`).
  const { closed, finished, unfinished, proposed, ownDesign, listens, manualEntry } = pageFacts({ goal, run, pendingGates: pending_gates, startable });
  // The proposed plan is the goal's landing: the Progress tab hosts
  // its card, so the band shows everything else and never a second copy.
  const proposal = adoptAction(guidance.open_questions);
  const verbs = runVerbs({ goal, run, runs, guidance, proposed: !!proposed, startable: !!startable, listens, manualEntry, liveSessions: liveSessionsOf(sessionRows, id) });

  const act = async (label: string, fn: () => Promise<unknown>) => {
    const ok = await attempt(fn, toast.error);
    if (ok) {
      toast.ok(label);
      refreshAll();
    }
  };

  // Choosing a workflow by hand is offered before the Workflow Agent starts
  // and after it stops — never while it is designing one for this goal.
  const designing = designInProgress(guidance, goal);
  // A design of the goal's own (`ownDesign`) may be copied out to the library at any
  // time; `startable` is that design before a run and the run's frozen copy after.
  const menu: MenuItem[] = [
    // The conversations about this goal — apart from its thread, which is
    // the Workflow Agent's: listed, searched, started and picked up on the
    // Conversation tab, whose list this unfolds.
    { label: tr("screens-goal-detail-conversations-about-goal"), icon: ICON.dm, onSelect: () => setSearch({ tab: "conversation", conversations: "1" }) },
    ...(!closed && !unfinished && !designing
      ? [{ label: tr("screens-goal-detail-choose-workflow-2"), icon: ICON.template, onSelect: () => setChoosing(true), separatorBefore: true }]
      : []),
    ...(ownDesign
      ? [
          {
            label: tr("screens-goal-detail-promote-library"),
            icon: ICON.workflow,
            onSelect: () => void act(tr("screens-goal-detail-promoted-copy-design-library"), () => api.promoteWorkflow(ownDesign.id)),
          },
        ]
      : []),
    { label: tr("screens-goal-detail-assign"), icon: ICON.assignee, onSelect: showDetails },
    { label: tr("screens-goal-detail-projects"), icon: ICON.project, onSelect: showProjects },
    ...(verbs.restart
      ? [{ label: tr("screens-goal-detail-restart"), icon: ICON.restart, separatorBefore: true, onSelect: () => setVerb("restart") }]
      : []),
    ...(!closed
      ? [
          {
            label: tr("screens-goal-detail-close-goal-2"),
            icon: GOAL_STATUS_ICON.closed,
            separatorBefore: !verbs.restart,
            onSelect: () => {
              setClosing(NOTHING_SAID);
              setConfirmClose(true);
            },
          },
        ]
      : []),
    ...(goal.archived
      ? [{ label: tr("screens-workflow-designer-unarchive-2"), icon: ICON.archive, separatorBefore: true, onSelect: () => void act(tr("screens-goal-detail-taken-back-out-still-closed"), () => api.archiveGoal(goal.id, false)) }]
      : [{ label: tr("screens-goal-detail-archive-goal"), icon: ICON.archive, separatorBefore: true, onSelect: () => setRetiring("archive") }]),
    {
      label: tr("screens-goal-detail-delete-goal"),
      icon: ICON.delete,
      danger: true,
      onSelect: () => setRetiring("delete"),
    },
  ];

  const openItem = (item: WorkItemSpec | string) => aux.open("session", typeof item === "string" ? item : item.id);

  // One rule for every verb the page offers (`runControl.mjs`): a start
  // queues behind a live run, never over the Workflow Agent at work; a stop
  // whenever a run is live or queued; a restart of a goal that ran. A design
  // that begins on events starts by listening, then runs by hand as *Run now*.
  const primary = verbs.start || verbs.stop ? (
    <>
      {verbs.start && (
        <Button size="sm" variant="primary" onClick={() => setVerb("start")}>
          {verbs.start.listen ? <ICON.signal size={12} aria-hidden /> : <ICON.run size={12} aria-hidden />}
          {verbs.start.label}
        </Button>
      )}
      {verbs.stop && (
        <Button size="sm" variant="danger" onClick={() => setVerb("stop")}>
          <ICON.stop size={12} aria-hidden />
          {verbs.stop.label}
        </Button>
      )}
    </>
  ) : undefined;

  return (
    <>
      <div className="flex h-full min-h-0 flex-col">
        <GoalHeader
          view={data}
          working={working}
          primary={primary}
          detailsOpen={aux.kind === "inspector"}
          onToggleDetails={() => aux.toggle("inspector")}
          menu={menu}
          onChanged={refreshAll}
        />
        {/* The strip draws its own hairline; the header above draws the edge. */}
        <Tabs className="shrink-0 px-4" tabs={TAB_DEFS} active={tab} onChange={setTab} />
        {/* The root the tabs' scrollports are kept from; none while the thread shows. */}
        <div ref={tab === "conversation" ? undefined : scrolls} className="flex min-h-0 flex-1 flex-col">
          <div data-scroll-keep={tab === "progress" ? "tab:progress" : undefined} className={tab === "progress" ? "relative min-h-0 flex-1 overflow-y-auto" : "relative flex min-h-0 flex-1 flex-col overflow-hidden"}>
            <YourMoveBand
              actions={bandActions(guidance.open_questions)}
              adoptInputs={null}
              onResolved={refreshAll}
            />
            {tab === "progress" && (
              <ProgressTab
                view={data}
                proposal={proposal}
                onEditPlan={() => setSearch({ tab: "workflow", edit: "1" })}
                onChanged={refreshAll}
                onOpenItem={openItem}
                onDecide={scrollToYourMove}
                onRestart={verbs.restart && finished ? () => setVerb("restart") : undefined}
                onNewRun={verbs.start && !verbs.start.adopt && finished ? () => setVerb("start") : undefined}
                newRunLabel={verbs.start?.label}
              />
            )}
            {tab === "conversation" && (
              <div className="min-h-0 flex-1">
                {/* The conversations about this goal list above its thread. */}
                <GoalConversationPane goal={goal} />
              </div>
            )}
            {tab === "workflow" && (
              <div className="min-h-0 flex-1">
                <GoalWorkflowTab view={data} onChanged={refreshAll} onOpenItem={openItem} onGoToGate={scrollToYourMove} />
              </div>
            )}
          </div>
        </div>
      </div>

      <AuxPortal>
        {aux.kind === "inspector" && (
          <GoalInspector view={data} frozen={panelFrozen(run, runs)} onOpenItem={openItem} onWorkChanged={refreshAll} />
        )}
        {aux.kind === "session" && aux.id && <WorkItemPanel itemId={aux.id} />}
      </AuxPortal>

      <RunVerbDialogs view={data} startable={startable} pending={verb} onClose={() => setVerb(null)} onDone={refreshAll} />

      {/* A pick takes effect as it is made, so there is nothing to confirm or
          take back: a plain dialog with one way out, never a Cancel that
          reverts nothing. */}
      <Dialog
        open={choosing}
        onClose={() => setChoosing(false)}
        title={tr("screens-goal-detail-choose-workflow")}
        description={tr("screens-goal-detail-next-run-uses-workflow")}
        footer={
          <Button variant="primary" onClick={() => setChoosing(false)}>
            {tr("screens-goal-detail-done")}
          </Button>
        }
      >
        <WorkflowPicker
          goal={goal.id}
          value={goal.workflow ?? null}
          allowNone
          onChange={(wf) =>
            void act(wf ? tr("screens-goal-detail-workflow-chosen") : tr("screens-goal-detail-workflow-cleared"), () => api.setGoalWorkflow(goal.id, { workflow: wf }))
          }
        />
      </Dialog>
      {dialogs}
    </>
  );
}
