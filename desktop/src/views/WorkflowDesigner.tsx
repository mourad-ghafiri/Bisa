/**
 * One workflow, on the canvas.
 *
 * Palette on the left, canvas in the middle, and on the right one column with
 * a rail of two icon tabs at the screen's edge — **Properties** (the
 * `Inspector`: the selected step's forms, or the workflow's own) and
 * **Agent** (`WorkflowAgentPane`: the conversations about this workflow,
 * where `@Workflow Agent` reads, validates and saves it) — resizable,
 * remembered (`designerPanelStore`), a step picked on the canvas showing
 * Properties. The header's arrow is the door back to Workflows. The
 * document is a `designerSession` (bodies in a bounded undo history, the
 * revision on the stored base): every edit is a new body, validation follows
 * the cursor, and a save is a revision after `workflow.autosave.delay_ms` of
 * quiet. A save never loses a keystroke — what was typed while it was in
 * flight is still there, still unsaved — and undo never resends a stale
 * revision.
 *
 * A conflict is a choice, not a reload: when somebody saved first, a banner
 * offers *Keep mine on top of theirs* and *Take theirs*, and nothing is sent
 * until the person picks. A workflow a goal is running right now is
 * **read-only** until that run finishes (`used_by[].live`): the canvas and
 * the inspector refuse edits, the palette is off, and nothing autosaves.
 *
 * Every workflow here is stored: *New workflow* in the library creates a
 * draft on the node — one start, by hand — and opens it, so the Agent pane
 * and the conversations are there from the first second. A workflow that
 * begins on events wears its On/Off in the header (`ListeningSwitch`): it
 * hears its start events only once a person turns it On, and while it is
 * On its listeners say when each next comes due and give a hook start its
 * calls and its secret.
 * The screen is keyed by its route (`App.tsx`), so opening another workflow
 * is another designer: this one unmounts and its autosave flushes the last
 * edits on the way out. What it was left on comes back with it, in this
 * window and after a restart: the step it had picked and where its canvas
 * looked (`designerMemoryStore`, kept through the view memory under the
 * workflow's place), where the palette and the properties were scrolled,
 * and the Agent pane's conversation, kept per workflow on the machine
 * (`WorkflowAgentPane`) — every door back here opens a bare
 * `#/workflows/<id>`. The workflow is drawn from what the window last read
 * of it while the node is read behind: a newer revision is stood on when it
 * lands (`designerSession.caughtUp`), and a workflow that is gone is left
 * for the library (`useGonePlace`). While it is dirty it is a *dirty
 * source* of the editor registry, so the quit question counts it and the
 * close flow saves it like a document.
 *
 * The screen does not name itself: the shell's chrome renders the `<h1>`,
 * and this header names the workflow.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { href, navigate, setSearch, useSearchValue } from "../router";
import { useGonePlace } from "../shell/useGonePlace";
import { useKeymap } from "../shell/useKeymap";
import { useViewScroll } from "../shell/useViewScroll";
import { placeOf } from "../shell/viewMemoryStore";
import { chordFor } from "../shell/keymapModel.mjs";
import type { ListenerView, NewWorkflowBody, Problem, Workflow } from "../types";
import { Button, Chip, ConfirmDialog, ErrorNote, ICON, IconRail, MoreMenu, PageHeader, ResizeHandle, SkeletonRows, cn, useStoredSize, useToast, type MenuItem } from "../ui";
import { railAnchor } from "../ui/iconRailModel.mjs";
import { attempt, useAsync } from "./_work/useAsync";
import { readKey } from "./_work/keptReadsModel.mjs";
import { forgetRead, keepRead } from "./_work/keptReadsStore";
import { retiredWords } from "./_work/retireModel.mjs";
import { RetireDialog } from "./_work/RetireDialog";
import { registerDirtySource } from "./_workbench/editorRegistry";
import { WorkflowAgentPane } from "./_workflow/WorkflowAgentPane";
import { WorkflowRunsPane } from "./_workflow/WorkflowRunsPane";
import { PANE_COMMAND, PANEL_PARAM, PANES, isPane, panelTabs, type DesignerPane } from "./_workflow/designerPanelModel.mjs";
import { pressDesignerPane, revealSelection, showDesignerPane, useDesignerPanel } from "./_workflow/designerPanelStore";
import { designerMemoryOf, rememberCanvasViewport, rememberSelectedStep } from "./_workflow/designerMemoryStore";
import { keptSelection } from "./_workflow/designerMemoryModel.mjs";
import { useWorkflowVerbs } from "./_workflow/WorkflowCard";
import { HeldSignals } from "./_workflow/HeldSignals";
import { ListeningSwitch } from "./_workflow/ListeningSwitch";
import { movesListeningOf } from "./_workflow/listeningModel.mjs";
import { liveGoals } from "./_workflow/workflowVerbs.mjs";
import { movesRunCount } from "./_workflow/workflowRunsModel.mjs";
import { Designer } from "./_workflow/Designer";
import { Inspector } from "./_workflow/Inspector";
import { Palette } from "./_workflow/Palette";
import { canRedoEdit, canUndoEdit, caughtUp, definitionBody, dirty, edit, keepMine, open, present, putBody, redoEdit, remoteLoaded, rowBehind, statusLine, takeTheirs, undoEdit, type Session, remoteAction } from "./_workflow/designerSession.mjs";
import { addStep } from "./_workflow/workflowGraph.mjs";
import { fitColumns } from "./_workbench/ideColumnsModel.mjs";
import { useAutosave } from "./_workflow/useAutosave";
import { useDesignerSettings } from "./_workflow/useDesignerSettings";
import { useLiveValidation } from "./_workflow/useLiveValidation";
import { catalogSlugOf } from "./_workflow/cardHelpers";
import { errorFields, log } from "../log";
import { t } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";

const PANEL_KEY = "bisa.workflow.panel.width";
const PANEL_DEFAULT = 380;
const PANEL_MIN = 300;
const PANEL_MAX = 720;
/** The palette's column with its words (`w-40`). */
const PALETTE_WIDTH = 160;

export default function WorkflowDesigner({ id }: { id: string }) {
  const toast = useToast();
  const settings = useDesignerSettings();
  const place = placeOf({ name: "workflow", id });
  const workflowRead = readKey("workflow", id);
  const { data, error, loading, reload, missing, at } = useAsync((s) => api.workflow(id, s), [id], { keep: workflowRead });
  // The node says there is no such workflow — it went while the app was
  // closed, or the link was wrong: what the window kept of it is no longer
  // its answer, and a place the person had been on is left for the library.
  useEffect(() => {
    if (missing) forgetRead(workflowRead);
  }, [missing, workflowRead]);
  useGonePlace(missing, { name: "workflow", id });
  // While it is On, its listeners: when each next comes due, a hook's calls and its secret.
  const since = data?.listening?.since ?? null;
  const heard = useAsync<ListenerView[]>((s) => (since === null ? Promise.resolve([]) : api.workflowListeners(id, s)), [id, since]);
  const listeners = heard.data ?? [];
  // The session opens once the stored workflow has loaded.
  const [session, setSession] = useState<Session | null>(null);
  // Where this designer was left, read once: the screen is keyed by its
  // route, so `id` never changes under it.
  const [memory] = useState(() => designerMemoryOf(id));
  const [selected, setSelected] = useState<string | null>(memory.selected);
  useEffect(() => rememberSelectedStep(id, selected), [id, selected]);
  // A step just added — by the palette's click, a drop, a duplicate: the
  // canvas brings it into view. One counter, so the asks never race.
  const [reveal, setReveal] = useState<{ id: string; nonce: number } | null>(null);
  const revealStep = useCallback((added: string) => setReveal((r) => ({ id: added, nonce: (r?.nonce ?? 0) + 1 })), []);
  const [retiring, setRetiring] = useState<"archive" | "delete" | null>(null);
  const [unarchiving, setUnarchiving] = useState(false);
  const [panelWidth, setPanelWidth] = useStoredSize(PANEL_KEY, PANEL_DEFAULT, { min: PANEL_MIN, max: PANEL_MAX });
  // The right panel — Properties or Agent, open or closed — is the machine's
  // furniture (`designerPanelStore`). A link may name a pane (`?panel=agent`,
  // a conversation's door): read once, shown, and taken off the address.
  const panel = useDesignerPanel();
  // The palette, the canvas and the column share the row as the Project
  // IDE's columns do (`ideColumnsModel.fitColumns`): past the canvas's least,
  // the column gives way to its own least, then the palette folds to its
  // glyphs — while the window is that narrow, never in the stored width.
  const [rowEl, setRowEl] = useState<HTMLDivElement | null>(null);
  const [rowWidth, setRowWidth] = useState(0);
  useEffect(() => {
    if (!rowEl) return;
    setRowWidth(rowEl.getBoundingClientRect().width);
    const ro = new ResizeObserver((entries) => setRowWidth(entries[0]?.contentRect.width ?? 0));
    ro.observe(rowEl);
    return () => ro.disconnect();
  }, [rowEl]);
  // The pane rail's column (`IconRail`, w-10) never moves.
  const fit = fitColumns({ total: rowWidth, fixed: 40, rail: PALETTE_WIDTH, railMin: PALETTE_WIDTH, railOpen: true, right: panelWidth, rightMin: PANEL_MIN, rightOpen: panel.open });
  const paletteCompact = fit.railFolded;
  const [panelParam] = useSearchValue(PANEL_PARAM);
  useEffect(() => {
    if (!panelParam) return;
    if (isPane(panelParam)) showDesignerPane(panelParam);
    // Replaced, never pushed: Back from here leaves the designer rather than
    // landing on the `?panel=` address that would send it back again.
    setSearch({ [PANEL_PARAM]: null }, { replace: true });
  }, [panelParam]);
  const pane: DesignerPane = panel.tab;
  // A step picked on the canvas — a click, a palette add, a drop — shows its
  // properties: the column opens on Properties if it was elsewhere. The step
  // remembered from the last visit is no new pick: the pane the person left
  // open — the Agent's, mid-conversation — stays.
  const revealed = useRef(selected);
  useEffect(() => {
    if (revealed.current === selected) return;
    revealed.current = selected;
    revealSelection(selected);
  }, [selected]);
  const keymap = useKeymap();

  // The screen is keyed by its route, so `id` never changes under it.
  // `useAsync` keeps the previous answer while the next loads, so the load
  // is opened only once it is the route's — and once: a re-read after a
  // save or a remote change goes through the session (`remoteLoaded`),
  // never over the person's edits. The one answer taken after the first is
  // the node's own, when the session was opened on what the window had kept
  // of the last visit (`at` is null until the node answers): what moved
  // while the person was away is stood on when it lands (`caughtUp`).
  const openedOn = useRef<"kept" | "read" | null>(null);
  useEffect(() => {
    if (!data || data.workflow.id !== id) return;
    const was = openedOn.current;
    if (was === "read" || (was === "kept" && at === null)) return;
    openedOn.current = at === null ? "kept" : "read";
    if (was === null) setSession((s) => (s && s.base.id === id ? s : open(data.workflow, data.problems)));
    else setSession((s) => (s ? caughtUp(s, data.workflow, data.problems) : s));
  }, [data, id, at]);
  // The window keeps the workflow as the session last confirmed it, so the
  // next visit is drawn on what was saved here, not on what was read first.
  const confirmed = session?.base ?? null;
  const confirmedProblems = session?.problems;
  useEffect(() => {
    if (!confirmed || !data || data.workflow.id !== confirmed.id || confirmed.revision <= data.workflow.revision) return;
    keepRead(workflowRead, { ...data, workflow: confirmed, problems: confirmedProblems ?? data.problems });
  }, [confirmed, confirmedProblems, data, workflowRead]);
  // A save of the designer's own moved the stored workflow: the row — what
  // it starts on, what turning On and *Run…* ask, whether it runs in the
  // workspace — is read again, so the header's switch and the menu's verbs
  // say what was just drawn. Once per revision: the session moves at every
  // keystroke, and a read asked again at each would give up the one in
  // flight before it could land.
  const rowAsked = useRef<number | null>(null);
  useEffect(() => {
    if (!rowBehind(data, session)) return;
    const revision = session?.base.revision ?? null;
    if (rowAsked.current === revision) return;
    rowAsked.current = revision;
    reload();
  }, [data, session, reload]);
  // The palette and the properties come back where they were scrolled. Two
  // roots, so the Agent pane between them is left to itself — a thread's
  // place is the thread's own to keep — and each is drawn once the workflow
  // is: what is on screen changes then, and the scroll is put back then.
  const paletteRoot = useRef<HTMLDivElement>(null);
  const propertiesRoot = useRef<HTMLDivElement>(null);
  const drawn = session ? "drawn" : "reading";
  useViewScroll(paletteRoot, `${place}#${drawn}`, place);
  useViewScroll(propertiesRoot, `${place}#${drawn}:${panel.open ? pane : "closed"}:${selected ?? ""}`, place);

  // Retired under this screen — here, from the CLI, from another desktop:
  // a deletion leaves once; an archive re-reads, and the chip says so.
  const left = useRef(false);
  const leave = useCallback(
    (words: string) => {
      if (left.current) return;
      left.current = true;
      auto.abandon();
      toast.ok(words);
      navigate({ name: "workflows" });
    },
    // `auto` is stable for the life of the screen.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [toast],
  );
  useEngineEvents((e) => {
    // Turned On or Off — here, from the CLI — or a start event that could not start its run: the switch reads again.
    if (movesListeningOf(e, `workspace:${id}`)) {
      reload();
      heard.reload();
    }
    // A run of it started or ended — in the workspace, or a goal's: the row's
    // counts moved, and with them the menu's verbs and the freeze.
    if (movesRunCount(e, id)) reload();
    // What a fact about workflows means here is the session's to say (`remoteAction`).
    const action = remoteAction(session, e.payload);
    if (action === "ignore" || !session?.base) return;
    const wfid = session.base.id;
    if (action === "leave") {
      leave(t("screens-workflow-designer-workflow-deleted"));
      return;
    }
    void api
      .workflow(wfid)
      .then((row) => setSession((s) => (s ? remoteLoaded(s, row.workflow, row.problems) : s)))
      .catch((e: unknown) => {
        // The next event, or the next save, asks again — but the log knows
        // the base stayed stale, so a 409 on that save has its cause.
        log.warn("designer", action === "remark" ? "could not reload a workflow whose mark changed" : "could not reload a workflow another writer changed", { ...errorFields(e), workflow: wfid });
      });
    if (action === "remark") reload();
  });

  // What a restarted node turned off, resumed or ended reaches the row and
  // its listeners by no frame: both reads are made again when the bus comes
  // back (`useAsync`).

  const validate = useCallback(
    (v: NewWorkflowBody, s: AbortSignal) => api.validateWorkflow(definitionBody(v), s).then((r) => r.problems),
    [],
  );
  const body = session ? present(session) : null;
  // A picked step the workflow no longer has — an agent's save took it, or
  // it was remembered from before — is no pick.
  useEffect(() => {
    if (!body) return;
    const kept = keptSelection(selected, body.steps);
    if (kept !== selected) setSelected(kept);
  }, [body, selected]);
  const live = useLiveValidation(body, validate);
  const problems: Problem[] = live.problems ?? session?.problems ?? [];
  // A save the node could not read says why beside the list too: the
  // validator has not judged this body either.
  const unreadable = live.error ?? (session?.status === "rejected" ? session.failure : null);

  const auto = useAutosave(session, setSession, {
    update: (wfid, b, revision, o) => api.putWorkflow(wfid, putBody(b, revision), undefined, o),
    fetchStored: async (wfid) => (await api.workflow(wfid)).workflow,
    delay: settings.autosaveDelayMs,
  });

  // A goal running this workflow right now freezes it: no edit lands, and
  // nothing is registered as unsaved.
  const liveOn = liveGoals(data?.used_by);
  const running = liveOn.length;
  const frozen = running > 0;
  // The run verbs — *Run…*, *Stop every run*, *Restart every run* — the
  // same items and dialogs the library card carries.
  const runVerbs = useWorkflowVerbs(data ?? null, reload);
  const change = (next: NewWorkflowBody) => {
    if (frozen) return;
    setSession((s) => (s ? edit(s, next) : s));
  };
  const isDirty = session ? dirty(session) : false;
  const saving = session?.status === "saving";

  // While dirty, the designer counts as an unsaved document: the quit
  // question names it and the close flow saves it before the window goes.
  const dirtyRef = useRef(isDirty);
  dirtyRef.current = isDirty;
  useEffect(
    () =>
      frozen ? undefined : registerDirtySource(`workflow:${id}`, {
        dirty: () => dirtyRef.current,
        save: () => auto.flush(),
      }),
    [id, auto, frozen],
  );

  const menu: MenuItem[] = useMemo(
    () => [
      // A conflict blocks every save until the person picks an exit, and a
      // save in flight is the one being waited for, so the item says so by
      // being disabled rather than doing nothing.
      { label: t("screens-workflow-designer-save-now"), icon: ICON.ok, onSelect: () => void auto.flush(), disabled: !isDirty || saving || session?.status === "conflict" },
      // On and Off are the header's switch here (`ListeningSwitch`), never a second door in the menu.
      ...runVerbs.items.filter((item) => item.verb !== "turn_on" && item.verb !== "turn_off").map((item, i) => (i === 0 ? { ...item, separatorBefore: true } : item)),
      // The conversations about this workflow, with the Workflow Agent or
      // anyone: the Agent pane, opened on its list.
      {
        label: t("screens-workflow-designer-conversations-about-workflow"),
        icon: ICON.dm,
        onSelect: () => {
          showDesignerPane("agent");
          setSearch({ conversations: "1" });
        },
      },
      ...(session?.base.archived
        ? [{ label: t("screens-workflow-designer-unarchive-2"), icon: ICON.archive, separatorBefore: true, onSelect: () => setUnarchiving(true) }]
        : [{ label: t("screens-workflow-designer-archive-workflow"), icon: ICON.archive, separatorBefore: true, onSelect: () => setRetiring("archive") }]),
      {
        label: t("screens-workflow-designer-delete-workflow"),
        icon: ICON.delete,
        danger: true,
        onSelect: () => setRetiring("delete"),
      },
    ],
    [auto, isDirty, saving, session?.base.archived, session?.status, runVerbs.items],
  );

  // Still loading, or loaded but not yet this route's: the skeleton, never
  // *not found* for the length of a fetch.
  if (!session && (loading || (data && data.workflow.id !== id))) {
    return (
      <div className="p-6">
        <SkeletonRows rows={6} className="max-w-2xl" />
      </div>
    );
  }
  // Gone, however much of it the window had kept: the note, never a designer on nothing.
  if (!session || !body || missing) {
    return (
      <div className="p-6">
        <ErrorNote error={error ?? t("screens-goal-detail-not-found")} retry={reload} />
      </div>
    );
  }
  const base: Workflow = session.base;
  const slug = catalogSlugOf(base.origin);
  const used = data?.used_by ?? [];
  const theirs = session.conflict?.theirs ?? null;
  // The rail's tabs: Properties, Agent, Runs, each with its chord beside its name.
  const tabs = panelTabs({ open: panel.open, tab: pane }).map((tab) => {
    const chord = chordFor(keymap, PANE_COMMAND[tab.id]);
    return {
      id: tab.id,
      icon: ICON[tab.icon as keyof typeof ICON] as typeof ICON.workflow,
      label: tab.label,
      tooltip: chord ? `${tab.label} · ${chord}` : tab.label,
      showing: tab.showing,
    };
  });

  return (
    <div className="flex h-full min-h-0 flex-col" data-designer-screen>
      <PageHeader
        className="shrink-0 items-center border-b border-border px-3 py-2"
        icon={ICON.workflow}
        back={{ label: t("screens-workflow-designer-back-to-workflows"), onClick: () => navigate({ name: "workflows" }) }}
        title={body.name || t("screens-workflow-designer-untitled-workflow")}
        subtitle={body.description || undefined}
        meta={
          <>
            {/* What a person acts on first: whether it can run, and whether the last edit is saved. */}
            {problems.length > 0 ? (
              <Chip tone="danger" icon={ICON.warn}>
                {t("workflow-workflow-card-problem-s", { problems: problems.length })}
              </Chip>
            ) : (
              <Chip tone="ok" icon={ICON.ok}>{t("screens-workflow-designer-runs")}</Chip>
            )}
            <span className={`text-2xs ${session.status === "failed" || session.status === "rejected" || session.status === "conflict" ? "text-danger" : "text-text-dim"}`}>
              {statusLine(session)}
            </span>
            {session.status === "failed" && (
              <Button size="sm" variant="ghost" onClick={() => void auto.flush()}>{t("screens-workflow-designer-retry")}</Button>
            )}
            {/* The facts behind it, quiet words in one group: its revision, where it came from, who points at it. */}
            <span className="flex min-w-0 items-center gap-2 text-2xs text-text-dim">
              <span className="tnum">{t("workflow-goal-workflow-tab-rev", { revision: base.revision })}</span>
              {slug ? (
                <span className="inline-flex min-w-0 items-center gap-1">
                  <ICON.template size={11} aria-hidden className="shrink-0" />
                  <span className="truncate">{t("screens-workflow-designer-installed-from", { slug })}</span>
                </span>
              ) : (
                <span>{t("screens-workflows-yours")}</span>
              )}
              {used.length > 0 && <span className="tnum">{t("screens-workflow-designer-used", { used: used.length })}</span>}
            </span>
          </>
        }
        actions={
          <>
            {/* What its events hold for a person to read, while it listens. */}
            {data && <HeldSignals host={{ workflow: id }} listening={Boolean(data.listening)} onChanged={heard.reload} />}
            {data && <ListeningSwitch row={data} listeners={listeners} onChanged={reload} />}
            <MoreMenu label={t("screens-workflow-designer-workflow-actions")} items={menu} className="h-7 w-7" />
          </>
        }
      />

      {theirs && (
        // A choice that waits on the person now: the summons, not an alarm.
        <div className="flex flex-wrap items-center gap-2 border-b border-accent/40 bg-accent-soft px-3 py-1.5 text-2xs text-accent-ink">
          <ICON.warn size={13} aria-hidden />
          <span className="font-medium">{t("screens-workflow-designer-somebody-saved-revision", { theirs: theirs.revision, mine: base.revision })}</span>
          <span className="text-text-dim">{t("screens-workflow-designer-nothing-saved-until-choose")}</span>
          <Button size="sm" variant="primary" className="ml-auto" onClick={() => setSession((s) => (s ? keepMine(s) : s))}>{t("screens-workflow-designer-keep-mine-top-theirs")}</Button>
          <Button size="sm" onClick={() => setSession((s) => (s ? takeTheirs(s) : s))}>{t("screens-workflow-designer-take-theirs")}</Button>
        </div>
      )}
      {/* A goal's run of it freezes the library copy until it finishes; the
          goal is where that run stops or restarts, so each is a link to it.
          A run of the workspace runs its own copy and freezes nothing. */}
      {frozen && !theirs && (
        // It informs and asks nothing, so it is neutral: the accent is for what waits on you.
        <div className="flex flex-wrap items-center gap-2 border-b border-border bg-surface-2/70 px-3 py-1.5 text-2xs text-text">
          <ICON.run size={13} aria-hidden className="text-text-dim" />
          <span className="text-text-dim">
            {rich(
              "screens-workflow-designer-running-in-goals-read-only",
              {
                lead: (inner) => <span className="font-medium text-text">{inner}</span>,
                goals: liveOn.map((g, i) => (
                  <span key={g.id}>
                    {i > 0 ? t("screens-workflow-designer-list-separator") : ""}
                    <a href={href({ name: "goal", id: g.id })} className="underline underline-offset-2 hover:text-text">
                      {g.label || g.id}
                    </a>
                  </span>
                )),
              },
              { n: running },
            )}
          </span>
        </div>
      )}
      {runVerbs.dialogs}

      <div ref={setRowEl} className="flex min-h-0 flex-1">
        {/* The root the palette's scroll is kept from; it draws no box of its own. */}
        <div ref={paletteRoot} className="[display:contents]">
        <aside data-scroll-keep="palette" className={cn("shrink-0 overflow-y-auto border-r border-border", paletteCompact ? "w-12 px-1.5 py-2" : "w-40 p-2")}>
            <h3 className={paletteCompact ? "sr-only" : "mb-2 px-2 pt-1 text-sm font-semibold text-text"}>{t("screens-workflow-designer-steps")}</h3>
            <Palette
              compact={paletteCompact}
              disabled={frozen}
              onAdd={(kind) => {
                const { wf, id: added } = addStep(body, kind);
                change(wf);
                setSelected(added);
                revealStep(added);
              }}
            />
            {!paletteCompact && <p className="mt-3 px-2 text-2xs leading-relaxed text-text-dim">{t("screens-workflow-designer-palette-hint")}</p>}
          </aside>
        </div>

        <div className="min-w-0 flex-1">
          <Designer
            value={body}
            problems={problems}
            selected={selected}
            onSelect={setSelected}
            onChange={change}
            onUndo={() => setSession((s) => (s ? undoEdit(s) : s))}
            onRedo={() => setSession((s) => (s ? redoEdit(s) : s))}
            canUndo={canUndoEdit(session)}
            canRedo={canRedoEdit(session)}
            settings={settings}
            readOnly={frozen}
            onRefuse={(reason) => toast.error(reason)}
            reveal={reveal}
            onReveal={revealStep}
            startViewport={memory.viewport}
            onViewport={(v) => rememberCanvasViewport(id, v)}
          />
        </div>

        {/* The column, when open: Properties, Agent or Runs. The rail stays either way. */}
        {panel.open && (
          <>
            <ResizeHandle side="left" size={panelWidth} min={PANEL_MIN} max={PANEL_MAX} defaultSize={PANEL_DEFAULT} onSize={setPanelWidth} label={t("workflow-designer-panel-resize")} />
            <aside className="flex min-h-0 shrink-0 flex-col border-l border-border" style={{ width: fit.right ?? panelWidth }} role="tabpanel" aria-label={tabs.find((tab) => tab.id === pane)?.label}>
              {pane === "agent" ? (
                <WorkflowAgentPane workflow={base} />
              ) : pane === "runs" && data ? (
                <WorkflowRunsPane row={data} onChanged={reload} />
              ) : (
                // Each step's properties are kept under the step's own name: another step's form is another page.
                <div ref={propertiesRoot} className="flex min-h-0 flex-1 flex-col">
                  <div data-scroll-keep={`properties:${selected ?? ""}`} className="min-h-0 flex-1 overflow-y-auto">
                    <Inspector value={body} selected={selected} problems={problems} unreadable={unreadable} onChange={change} onSelect={setSelected} readOnly={frozen} host={{ workflow: id }} listeners={listeners} />
                  </div>
                </div>
              )}
            </aside>
          </>
        )}
        <IconRail label={t("workflow-designer-panel-rail")} items={tabs} anchor={railAnchor(PANES, { open: panel.open, tab: pane })} onPress={pressDesignerPane} />
      </div>

      {retiring && (
        <RetireDialog
          kind="workflow"
          id={base.id}
          name={body.name}
          wanted={retiring}
          open
          onClose={() => setRetiring(null)}
          onRetired={(done, choices) => {
            const said = retiredWords("workflow", choices.thing, done.terminated);
            if (choices.thing === "delete") leave(said);
            else {
              toast.ok(said);
              reload();
            }
          }}
        />
      )}
      <ConfirmDialog
        open={unarchiving}
        onClose={() => setUnarchiving(false)}
        title={t("screens-workflow-designer-unarchive", { body: body.name })}
        confirmLabel={t("screens-workflow-designer-unarchive-2")}
        body={t("screens-workflow-designer-returns-library-pickers-goal-may-point")}
        onConfirm={() => {
          setUnarchiving(false);
          if (!base) return;
          void attempt(() => api.archiveWorkflow(base.id, false), toast.error, () => toast.ok(t("screens-workflow-designer-workflow-unarchived")));
        }}
      />
    </div>
  );
}
