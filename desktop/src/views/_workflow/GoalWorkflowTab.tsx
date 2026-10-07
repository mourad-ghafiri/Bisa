/**
 * The goal's Workflow tab: its workflow on the canvas, and — once it runs —
 * the run drawn over it with live step states.
 *
 * Four states, one screen:
 * - no workflow: the designing card — the Workflow Agent's standing, or the
 *   door to the designer on a manual goal — and a blank canvas once drawing;
 * - a workflow and no run: adopt it (when the Workflow Agent proposed it and
 *   the gate is open) or start it — both through `StartRunDialog`, which
 *   collects the inputs — or *Edit the steps*: the design draft, every step
 *   yours, validation following the cursor, *Save changes* refused while
 *   problems remain. A goal-scoped design wears a *Designed for this goal*
 *   banner; *Promote to library* is in the header's menu;
 * - a run: **read-only** canvas wearing the run; the selected step's actions
 *   beside it. Nothing here edits a running workflow — not an amendment, not
 *   a word to the Workflow Agent: an amendment is the agent's own move
 *   (applied on an auto goal, gated on a guided one) or the CLI's;
 * - a finished run: restart it, start a new run, or pick another workflow.
 *   A run made while one is live queues behind it.
 *
 * What the tab was left on is the goal's memory (`shell/viewMemoryStore`,
 * under the goal's place): the step picked, the run being viewed, where the
 * canvas looked and where its side columns were scrolled — read from the
 * memory on every render, so a switch of tab, a way out and back, and a
 * restart all land on the same picture. The drawing in progress is kept
 * beside them (`designDraftStore`). `?step=` and `?run=` are a link's to
 * hand over once: each is taken into the memory and off the address.
 */

import { useSessions } from "../../shell/sessionsStore";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api } from "../../api";
import { useEngineEvents } from "../../bus";
import { setSearch, useSearchValue } from "../../router";
import { placeOf, useViewState } from "../../shell/viewMemoryStore";
import { idValue } from "../../shell/viewValuesModel.mjs";
import type { GoalView, ListenerView, NewWorkflowBody, Problem, Workflow, WorkflowRun } from "../../types";
import { Button, Chip, ConfirmDialog, ErrorNote, ICON, LinkedText, SkeletonRows, failureText, cn, useToast } from "../../ui";
import { readKey } from "../_work/keptReadsModel.mjs";
import { attempt, useAsync } from "../_work/useAsync";
import { Designer } from "./Designer";
import { keptSelection } from "./designerMemoryModel.mjs";
import { canvasViewportAt, rememberViewportAt } from "./designerMemoryStore";
import { bodyOf, definitionBody, goalTabRemote, problemsFromErrorBody } from "./designerSession.mjs";
import { Inspector } from "./Inspector";
import { Palette } from "./Palette";
import { ProblemsList } from "./ProblemsList";
import { RunOverlay } from "./RunOverlay";
import { StartRunDialog } from "./StartRunDialog";
import { StepActions } from "./StepActions";
import { WorkflowPicker } from "./WorkflowPicker";
import { DesigningCard } from "../_goal/DesigningCard";
import { designs, modeOf } from "../_goal/goalMode.mjs";
import { listeningFacts, pageFacts } from "../_goal/goalPageModel.mjs";
import { liveSessionsOf, runVerbs } from "../_goal/runControl.mjs";
import { startInputs as askedAtStart } from "./forms/startForm.mjs";
import { blankWorkflow } from "./stepKinds.mjs";
import { useDesignDraft } from "./designDraftStore";
import { canRedo, canUndo, create, push, redo, undo } from "./history.mjs";
import { draftChanged } from "./designDraftModel.mjs";
import { addStep } from "./workflowGraph.mjs";
import { fitColumns } from "../_workbench/ideColumnsModel.mjs";
import { useDesignerSettings } from "./useDesignerSettings";
import { useLiveValidation } from "./useLiveValidation";
import { failedStep } from "./runView.mjs";
import { t } from "../../i18n/l10n.mjs";

// Module-scope empties: a fresh `[]` per render is a new identity, and the
// canvas and the inspector memoise on identity. This tab renders on every
// frame while the Workflow Agent designs, so a literal here was a loop.
const NO_PROBLEMS: readonly Problem[] = Object.freeze([]);
const NO_LISTENERS: readonly ListenerView[] = Object.freeze([]);
/** The palette's column with its words (`w-40`), and the inspector's width and its least. */
const PALETTE_WIDTH = 160;
const INSPECTOR_WIDTH = 320;
const INSPECTOR_MIN = 240;

export function GoalWorkflowTab({
  view,
  onChanged,
  onOpenItem,
  onGoToGate,
}: {
  view: GoalView;
  onChanged: () => void;
  onOpenItem: (item: string) => void;
  onGoToGate: () => void;
}) {
  const toast = useToast();
  const settings = useDesignerSettings();
  const { goal, run, runs, pending_gates, guidance } = view;
  // The palette, the canvas and the inspector share the row as the designer's
  // do (`ideColumnsModel.fitColumns`): the inspector gives way to its least,
  // then the palette folds to its glyphs; the canvas is the work.
  const [rowEl, setRowEl] = useState<HTMLDivElement | null>(null);
  const [rowWidth, setRowWidth] = useState(0);
  useEffect(() => {
    if (!rowEl) return;
    setRowWidth(rowEl.getBoundingClientRect().width);
    const ro = new ResizeObserver((entries) => setRowWidth(entries[0]?.contentRect.width ?? 0));
    ro.observe(rowEl);
    return () => ro.disconnect();
  }, [rowEl]);
  // What the tab was left on is kept under the goal's place.
  const place = placeOf({ name: "goal", id: goal.id });
  // A run-strip chip elsewhere lands here with `?step=<id>` — that step opens selected.
  const [stepParam] = useSearchValue("step");
  // The Runs list lands here with `?run=<id>` — the canvas wears that run.
  const [runParam, setRunParam] = useSearchValue("run");
  const [editParam, setEditParam] = useSearchValue("edit");
  const [picked, setSelected] = useViewState<string | null>(place, "workflow:step", null, idValue);
  useEffect(() => {
    if (!stepParam) return;
    setSelected(stepParam);
    // Replaced, never pushed: the step is the memory's from here on, and a
    // step picked by hand afterwards is not undone by the address.
    setSearch({ step: null }, { replace: true });
  }, [stepParam, setSelected]);
  const [starting, setStarting] = useState(false);
  const [choosing, setChoosing] = useState(false);
  // The draft outlives this tab, and the window: it is kept per goal in
  // `designDraftStore` so a tab switch, a remount or a restart does not
  // discard what was drawn.
  const [draft, setDraft] = useDesignDraft(goal.id);
  const [serverProblems, setServerProblems] = useState<Problem[]>([]);
  // *Discard changes* asks first once the drawing holds an edit.
  const [discarding, setDiscarding] = useState(false);
  // Leaving the window with edits to undo asks first: the drawing as it
  // stands is kept, the way back through it is not.
  useEffect(() => {
    if (!draft || !canUndo(draft)) return;
    const warn = (e: BeforeUnloadEvent) => {
      e.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [draft]);
  const [viewingRun, setViewingRun] = useViewState<string | null>(place, "workflow:run", null, idValue);
  useEffect(() => {
    if (runParam) {
      setViewingRun(runParam);
      setRunParam(null);
    }
  }, [runParam, setRunParam, setViewingRun]);
  // A step just added to the draft — a palette click, a drop, a duplicate —
  // for the canvas to bring into view; one counter for all three.
  const [reveal, setReveal] = useState<{ id: string; nonce: number } | null>(null);
  const revealStep = useCallback((added: string) => setReveal((r) => ({ id: added, nonce: (r?.nonce ?? 0) + 1 })), []);

  // The workflow the goal points at, when there is no run to read it from.
  const stored = useAsync(async (s) => (goal.workflow && !run ? (await api.workflow(goal.workflow, s)).workflow : null), [goal.workflow, run?.id], { keep: run ? null : readKey("goal-workflow", goal.workflow) });
  // An earlier run picked in the overlay, fetched whole by its id.
  const earlier = viewingRun && viewingRun !== run?.id ? viewingRun : null;
  const shownRun = useAsync(
    async (s) => (viewingRun && viewingRun !== run?.id ? (await api.goalRunById(goal.id, viewingRun, s)).run : null),
    [viewingRun, run?.id],
    { keep: readKey("goal-run", goal.id, earlier) },
  );
  // Another live run takes the canvas back from the one being viewed —
  // when the run changes under the tab, never when the tab opens: the run
  // that was being viewed is what it opens on.
  const liveRun = useRef(run?.id);
  useEffect(() => {
    if (liveRun.current === run?.id) return;
    liveRun.current = run?.id;
    setViewingRun(null);
  }, [run?.id, setViewingRun]);
  // Where the canvas looked when it was left; the canvas reads it once, when it opens.
  const startViewport = useMemo(() => canvasViewportAt(place), [place]);
  const keepViewport = useCallback((v: { x: number; y: number; zoom: number }) => rememberViewportAt(place, v), [place]);
  // While the goal listens, its listeners: a hook start's calls and its secret's state.
  const since = (view.listening ?? goal.listening)?.since ?? null;
  const heard = useAsync<ListenerView[]>((s) => (since === null ? Promise.resolve([]) : api.goalListeners(goal.id, s)), [goal.id, since]);

  const activeRun: WorkflowRun | null = viewingRun && shownRun.data && shownRun.data.id === viewingRun ? shownRun.data : run;
  const workflow: Workflow | null = activeRun?.workflow ?? stored.data ?? null;
  // The same reading as the goal page's (`goalPageModel`): one formula for both.
  const { adoptGate, proposed, unfinished } = pageFacts({ goal, run, pendingGates: pending_gates, startable: null });
  const mode = modeOf(goal);
  // A workflow that moved elsewhere (`designerSession.goalTabRemote`): read again when
  // nothing is drawn; with a drawing, said once — its save is refused rather than
  // written over the other change.
  useEngineEvents((e) => {
    const action = goalTabRemote({ workflow: goal.workflow, drawing: !!draft, running: !!run }, e.payload);
    if (action === "ignore") return;
    if (action === "warn") {
      toast.info(t("workflow-goal-workflow-tab-workflow-changed-elsewhere-drawing-kept-saving"));
      return;
    }
    stored.reload();
    onChanged();
  });
  const failure = failedStep(activeRun);
  // A draft belongs to a design that has not run: the moment the goal has a
  // run the drawing is dropped, or the tab would keep asking to save into a
  // workflow the run has frozen and the window would warn on close for an
  // edit nobody can land.
  useEffect(() => {
    if (run && draft) setDraft(null);
  }, [run, draft, setDraft]);
  // One rule for every start the page offers (`runControl.mjs`): queued
  // behind a live run, never while the Workflow Agent is designing or
  // repairing; a design that begins on events starts by listening.
  const { listens, manualEntry } = listeningFacts(goal, workflow);
  const sessionRows = useSessions();
  const verbs = runVerbs({ goal, run, runs, guidance, proposed: !!proposed, startable: true, listens, manualEntry, liveSessions: liveSessionsOf(sessionRows, goal.id) });
  const offer = verbs.start;
  const listen = !!offer?.listen || (!!proposed && listens);
  const closed = !!goal.closed;

  // Arriving from the proposal's "Edit the steps" door, or a manual capture
  // (`?edit=1`): open the editor once the design has loaded — or on a blank
  // canvas when the goal has no workflow yet — then drop the flag.
  useEffect(() => {
    if (editParam !== "1" || draft || closed || run) return;
    if (!goal.workflow) {
      setDraft(create(blankWorkflow(goal.title ?? t("workflow-goal-workflow-tab-untitled-workflow"))));
      setEditParam(null);
      return;
    }
    if (!workflow || workflow.origin.origin !== "goal") return;
    setDraft(create(bodyOf(workflow)));
    setEditParam(null);
    // Opens once per flag: `goal` is rebuilt by every refetch, and the flag,
    // the draft, the run and the loaded design are what decide.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editParam, draft, closed, workflow, run]);

  // The body on the canvas: the draft being drawn, else the run's frozen copy
  // or the goal's workflow. Memoised on the workflow's identity and revision
  // — the objects themselves are rebuilt on every refetch — so the canvas
  // does not re-lay-out on every render.
  const shown: Workflow | null = activeRun ? activeRun.workflow : workflow;
  // Keyed on the identity and revision above, never the `shown` object a refetch rebuilds.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const stored_body = useMemo(() => (shown ? bodyOf(shown) : null), [shown?.id, shown?.revision, activeRun?.id]);
  const value: NewWorkflowBody | null = draft ? draft.present : stored_body;

  // A draft is judged as it is drawn.
  const validate = useCallback(
    (v: NewWorkflowBody, s: AbortSignal) => api.validateWorkflow(definitionBody(v), s).then((r) => r.problems),
    [],
  );
  const live = useLiveValidation(draft ? draft.present : null, validate);
  const draftProblems: readonly Problem[] = draft ? (live.problems ?? serverProblems) : NO_PROBLEMS;
  const draftEdit = (next: NewWorkflowBody) => {
    // Same reference when already empty: a state that does not change is not a commit.
    setServerProblems((p) => (p.length ? [] : p));
    setDraft((h) => (h ? push(h, next) : h));
  };

  const saveDesign = async () => {
    if (!draft) return;
    try {
      // Editing the proposed plan makes it the goal's own design: the stale
      // adopt gate is withdrawn on the node, and the goal shows the edited
      // steps with an explicit Start — nothing runs on its own.
      await api.setGoalWorkflow(goal.id, { definition: definitionBody(draft.present), revision: workflow?.revision });
      toast.ok(t("workflow-goal-workflow-tab-saved-review-steps-then-start-run"));
      setDraft(null);
      setServerProblems([]);
      onChanged();
    } catch (e) {
      const problems = e instanceof ApiError ? problemsFromErrorBody(e.body) : [];
      if (problems.length > 0) setServerProblems(problems);
      toast.error(failureText("workflow", "goal-workflow-tab-failed", e));
    }
  };

  // No workflow and nothing being drawn: the designing card — the agent's
  // standing on an auto or guided goal, the door to the designer on a manual
  // one. A design being drawn on a blank canvas falls through to the editor.
  if (!goal.workflow && !run && !draft) {
    return (
      <div className="flex h-full flex-col">
        <div data-scroll-keep="workflow:designing" className="min-h-0 flex-1 overflow-y-auto">
          <DesigningCard view={view} onChanged={onChanged} />
        </div>
      </div>
    );
  }

  if (!value || (!workflow && !draft)) {
    return (
      <div className="p-6">{stored.error ? <ErrorNote error={stored.error} retry={stored.reload} /> : <SkeletonRows rows={4} />}</div>
    );
  }

  // A remembered step the picture no longer has — an agent's save took it, another run is shown — is no pick.
  const selected = keptSelection(picked, value.steps);
  const record = selected && activeRun ? activeRun.steps[selected] : null;
  const step = selected ? value.steps.find((s) => s.id === selected) : undefined;
  const canSave = !!draft && draftProblems.length === 0 && !live.validating;
  const discardDraft = () => {
    setDraft(null);
    setServerProblems([]);
  };
  // The name and inputs the start dialog and the bar read: the stored
  // workflow's, or the drawing's on a blank canvas.
  const shownName = workflow?.name ?? value.name;
  // Starting by listening asks only what its events do not supply.
  const startInputs = askedAtStart(workflow ?? value, listen);

  const fit = fitColumns({ total: rowWidth, fixed: 0, rail: PALETTE_WIDTH, railMin: PALETTE_WIDTH, railOpen: draft !== null, right: INSPECTOR_WIDTH, rightMin: INSPECTOR_MIN, rightOpen: true });
  const paletteCompact = fit.railFolded;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {workflow && workflow.origin.origin === "goal" && (
        // The accent only while the strip asks something of the person — a
        // proposal to review and adopt; "designed for this goal" and a run's
        // own copy are facts, and wear the neutral ground.
        <div
          className={cn(
            "flex flex-wrap items-center gap-2 border-b px-3 py-1.5 text-2xs",
            proposed && !activeRun ? "border-accent/40 bg-accent-soft text-accent-ink" : "border-hairline bg-surface-2/70 text-text",
          )}
        >
          <ICON.coreAgent size={13} aria-hidden className={proposed && !activeRun ? undefined : "text-text-dim"} />
          <span className="font-medium">{proposed ? t("workflow-goal-workflow-tab-proposed-by-workflow-agent") : t("workflow-goal-workflow-tab-designed-for-goal")}</span>
          <span className="text-text-dim">
            {activeRun
              ? t("workflow-goal-workflow-tab-adopted-run-carries-own-copy")
              : proposed
                ? t("workflow-goal-workflow-tab-review-every-step-move-adopt-start")
                : t("workflow-goal-workflow-tab-not-adopted-yet-edit-adopt-choose")}
          </span>
        </div>
      )}
      {activeRun ? (
        <RunOverlay run={activeRun} runs={runs} onPickRun={setViewingRun} />
      ) : (
        <div className="flex flex-wrap items-center gap-2 border-b border-hairline px-3 py-1.5 text-2xs">
          <ICON.workflow size={13} aria-hidden className="text-text-dim" />
          <span className="font-medium">{shownName || t("workflow-goal-workflow-tab-untitled-workflow")}</span>
          {workflow ? <Chip tone="quiet">{t("workflow-goal-workflow-tab-rev", { revision: workflow.revision })}</Chip> : <Chip tone="quiet">{t("workflow-goal-workflow-tab-not-saved-yet")}</Chip>}
          <Chip tone="quiet">{t("workflow-goal-workflow-tab-steps", { steps: value.steps.length })}</Chip>
          <span className="text-text-dim">{workflow ? t("workflow-goal-workflow-tab-not-started") : t("workflow-goal-workflow-tab-being-designed")}</span>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-1.5">
        {!closed && unfinished && offer && workflow && (
          <Button size="sm" onClick={() => setStarting(true)}>
            {offer.listen ? <ICON.signal size={12} aria-hidden /> : <ICON.run size={12} aria-hidden />}
            {offer.label}
          </Button>
        )}
        {!closed && !unfinished && (
          <>
            {/* While a drawing is open its Save is the one ask; Start steps back. */}
            {offer && workflow && (
              <Button size="sm" variant={draft ? "default" : "primary"} onClick={() => setStarting(true)}>
                {offer.listen ? <ICON.signal size={12} aria-hidden /> : <ICON.run size={12} aria-hidden />}
                {offer.label}
              </Button>
            )}
            <Button size="sm" onClick={() => setChoosing((c) => !c)}>{t("workflow-goal-workflow-tab-choose-workflow")}</Button>
            {choosing && (
              <div className="w-72">
                <WorkflowPicker
                  goal={goal.id}
                  value={goal.workflow ?? null}
                  onChange={(id) =>
                    void attempt(() => api.setGoalWorkflow(goal.id, { workflow: id }), toast.error, () => {
                      setChoosing(false);
                      onChanged();
                    })
                  }
                />
              </div>
            )}
          </>
        )}
        {!activeRun && !draft && !closed && workflow && workflow.origin.origin === "goal" && (
          <Button size="sm" onClick={() => setDraft(create(bodyOf(workflow)))}>
            <ICON.edit size={12} aria-hidden />{t("workflow-goal-workflow-tab-edit-steps")}</Button>
        )}
        {!unfinished && !!activeRun && (
          <span className="text-2xs text-text-dim">
            {failure
              ? t("workflow-goal-workflow-tab-run-failed-history-read-only-workflow", { failure: failure.name, error: failure.error, flag: (failure.error) ? "yes" : "no", flag2: (designs(mode)) ? "yes" : "no", flag3: (mode === "auto") ? "yes" : "no" })
              : activeRun.started_at == null && !activeRun.cancelled
                ? t("workflow-goal-workflow-tab-queued-run-starts-when-live-run")
                : t("workflow-goal-workflow-tab-finished-run-history-read-only")}
          </span>
        )}
        {draft && (
          <>
            <span className="text-2xs text-text-dim">{workflow ? t("workflow-goal-workflow-tab-editing-plan-every-step-yours-change") : t("workflow-goal-workflow-tab-designing-workflow-save-then-start-when")}</span>
            {draftProblems.length > 0 && (
              <Chip tone="danger" icon={ICON.warn}>
                {t("workflow-goal-workflow-tab-problems-count", { n: draftProblems.length })}
              </Chip>
            )}
            <Button
              size="sm"
              variant="primary"
              disabled={!canSave}
              disabledReason={draftProblems.length > 0 ? t("workflow-goal-workflow-tab-fix-problems-first") : t("workflow-goal-workflow-tab-checking-steps")}
              onClick={() => void saveDesign()}
            >{t("workflow-goal-workflow-tab-save-changes")}</Button>
            {/* An untouched drawing closes at once; one that differs from the stored workflow asks, since nothing brings it back — a draft restored after a restart included (`draftChanged`). */}
            <Button size="sm" variant="ghost" onClick={() => (draftChanged(draft?.present, stored_body, !!draft && canUndo(draft)) ? setDiscarding(true) : discardDraft())}>{t("workflow-goal-workflow-tab-discard-changes")}</Button>
          </>
        )}
        <ConfirmDialog
          open={discarding}
          onClose={() => setDiscarding(false)}
          title={t("workflow-goal-workflow-tab-discard-your-changes")}
          body={t("workflow-goal-workflow-tab-discard-body")}
          confirmLabel={t("workflow-goal-workflow-tab-discard-changes")}
          danger
          onConfirm={() => {
            setDiscarding(false);
            discardDraft();
          }}
        />
      </div>

      {/* The goal's thread is its Conversation tab; this tab is the canvas. */}
      <div ref={setRowEl} className="flex min-h-0 flex-1">
        {draft && (
          <aside data-scroll-keep="workflow:palette" className={cn("shrink-0 overflow-y-auto border-r border-border", paletteCompact ? "w-12 px-1.5 py-2" : "w-40 p-2")}>
            <h3 className={paletteCompact ? "sr-only" : "mb-2 px-2 pt-1 text-sm font-semibold text-text"}>{t("workflow-goal-workflow-tab-steps-2")}</h3>
            <Palette
              compact={paletteCompact}
              onAdd={(kind) => {
                const { wf, id } = addStep(draft.present, kind);
                draftEdit(wf);
                setSelected(id);
                revealStep(id);
              }}
            />
            {!paletteCompact && <p className="mt-5 px-2 text-2xs leading-relaxed text-text-dim">{t("workflow-goal-workflow-tab-new-steps-part-plan-will-start")}</p>}
          </aside>
        )}
        <div className="min-w-0 flex-1">
          <Designer
            value={value}
            problems={draftProblems}
            run={activeRun}
            selected={selected}
            onSelect={setSelected}
            onChange={draftEdit}
            onUndo={() => setDraft((h) => (h ? undo(h) : h))}
            onRedo={() => setDraft((h) => (h ? redo(h) : h))}
            canUndo={draft ? canUndo(draft) : false}
            canRedo={draft ? canRedo(draft) : false}
            readOnly={!draft}
            settings={settings}
            onRefuse={(r) => toast.error(r)}
            reveal={reveal}
            onReveal={revealStep}
            startViewport={startViewport}
            onViewport={keepViewport}
          />
        </div>
        <aside data-scroll-keep="workflow:inspector" className="shrink-0 overflow-y-auto border-l border-border" style={{ width: fit.right ?? INSPECTOR_WIDTH }}>
          {draft && (
            <div className="border-b border-hairline px-4 py-3">
              <h3 className="mb-1.5 text-2xs font-semibold text-text-dim">{t("workflow-goal-workflow-tab-problems")}</h3>
              <ProblemsList problems={draftProblems} unreadable={live.error} onSelect={setSelected} />
            </div>
          )}
          {step && activeRun && !draft && (
            <div className="px-4 py-3">
              <StepActions run={activeRun.id} step={step} record={record} onChanged={onChanged} onOpenItem={onOpenItem} onDecide={onGoToGate} />
            </div>
          )}
          {step && activeRun && !draft && (
            <div className="border-t border-hairline px-4 py-2 text-2xs text-text-dim">
              {record?.error && <p className="text-danger">{record.error}</p>}
              {record?.output !== undefined && record?.output !== null && (
                <LinkedText as="pre" className="mt-1 max-h-64 overflow-auto rounded-control bg-surface-2 p-2 font-mono text-2xs" text={JSON.stringify(record.output, null, 2)} />
              )}
            </div>
          )}
          <Inspector value={value} selected={selected} problems={draftProblems} unreadable={draft ? live.error : null} onChange={draftEdit} onSelect={setSelected} readOnly={!draft} host={{ goal: goal.id }} listeners={heard.data ?? NO_LISTENERS} />
        </aside>
      </div>

      <StartRunDialog
        open={starting}
        goal={goal.id}
        name={shownName}
        inputs={startInputs}
        adopt={proposed ? { gate: adoptGate?.id ?? null } : null}
        queues={!!offer?.queues}
        listen={listen}
        at={offer?.at ?? null}
        onClose={() => setStarting(false)}
        onDone={onChanged}
      />
    </div>
  );
}
