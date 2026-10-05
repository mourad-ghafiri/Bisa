/**
 * The Draw overlay (19 — Drawings): a panel that floats over whatever screen
 * you are on, and a dock it collapses to — the notes overlay's twin, for the
 * reasons `NoteOverlay.tsx` gives: a picture of the system should be one
 * keystroke away while you read the goal; the panel keeps its place and
 * never follows the route; it is mounted in `App.tsx` outside `<Screen>`,
 * `Suspense` and the error boundary; it sits at `z-40`, under every dialog.
 *
 * # Maximized
 *
 * The one thing a canvas needs that a note does not is room. **Maximize**
 * makes the panel fill exactly the content column — everything but the
 * header, the footer and the sidebar — the box `App.tsx` publishes
 * (`shell/contentBox.ts`), still at `z-40`, and says so to the native
 * browser layer (`useSurface`), which yields under any DOM surface. Escape
 * restores it, once the canvas has nothing of its own to cancel.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { ApiError, api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { errorFields, log } from "../log";
import { useRoute } from "../router";
import { MaximizeToggle } from "../shell/MaximizeToggle";
import { UnsavedDialog } from "../shell/UnsavedDialog";
import { leaveWords } from "../shell/leaveGuardModel.mjs";
import { useMaximizedPanel } from "../shell/maximizedPanel";
import { useBrowserClear } from "../shell/browserClear";
import { RepoStrip } from "../shell/repo/RepoStrip";
import { useWorkspace } from "../shell/useWorkspaceData";
import type { DrawingDetail, DrawingRow } from "../types";
import { Button, Chip, ConfirmDialog, EmptyState, ErrorNote, ResizeHandle, Spinner, Tabs, TextInput, Tooltip, sayFailure, useStoredSize, useToast } from "../ui";
import { NewDrawingDialog } from "./NewDrawingDialog";
import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import type { ExcalidrawImperativeAPI } from "../ui/excalidraw";
import { loadExcalidraw } from "../ui/excalidraw";
import { restoreOnEscape } from "../shell/contentBoxModel.mjs";
import { createLatest } from "../shell/latestModel.mjs";
import { DrawDock } from "./DrawDock";
import { DrawEditor } from "./DrawEditor";
import { DRAW_TABS, DRAW_TAB_LABEL, drawTabAdmits, drawTabKind, drawTabQuery, drawTargets, filterDrawings, goneQuietly, scopeOfRow, scopeWords, type DrawTab, type NamedRecord, type OwnerScope, type OwnerScopeKind, type ScopeNames } from "./drawModel.mjs";
import { clearActiveDrawing, drawGuard, openDrawing, restoredDrawing, setDrawMaximized, setDrawOpen, setDrawQuery, setDrawTab, toggleDrawMaximized, useDrawOverlay } from "./drawStore";
import { useDrawPrefs } from "./drawPrefsStore";
import { templateOf } from "./templates/index.mjs";
import { t as tr } from "../i18n/l10n.mjs";
import { useFloatingPanelWidth, useViewportWidth } from "../shell/floatingPanelStore";
import { besideOffset } from "./besideModel.mjs";

const DEFAULT_WIDTH = 720;
const DEFAULT_HEIGHT = 560;
const MIN_WIDTH = 480;
const MIN_HEIGHT = 360;

const KIND_ICON: Record<OwnerScopeKind, (typeof ICON)["note"]> = {
  workspace: ICON.organization,
  project: ICON.project,
  goal: ICON.goal,
  workflow: ICON.workflow,
  channel: ICON.channel,
  node: ICON.node,
};

const TABS = DRAW_TABS.map((id) => ({ id, label: DRAW_TAB_LABEL[id] }));

const EMPTY_WORDS: Record<DrawTab, string> = {
  all: tr("draw-overlay-nothing-here-yet"),
  workspace: tr("draw-overlay-no-workspace-drawings-yet"),
  projects: tr("draw-overlay-no-project-drawing-yet"),
  goals: tr("draw-overlay-no-goal-drawing-yet"),
  workflows: tr("draw-overlay-no-workflow-drawing-yet"),
  channels: tr("draw-overlay-no-channel-drawing-yet"),
  node: tr("draw-overlay-no-node-drawing-yet"),
};

function useWorkflowNames(open: boolean): NamedRecord[] {
  const [names, setNames] = useState<NamedRecord[]>([]);
  const [tick, setTick] = useState(0);
  useEngineEvents((e) => {
    if (e.payload.type === "workflow_changed" && open) setTick((n) => n + 1);
  });
  useReloadOnReconnect(() => {
    if (open) setTick((n) => n + 1);
  });
  useEffect(() => {
    if (!open) return;
    const ctrl = new AbortController();
    api
      .workflows({ scope: "all" }, ctrl.signal)
      .then((r) => {
        if (!ctrl.signal.aborted) setNames(r.workflows.map((w) => ({ id: w.workflow.id, name: w.workflow.name })));
      })
      .catch((e: unknown) => {
        if (ctrl.signal.aborted) return;
        log.debug("draw", "the workflow names could not be read; a chip says the kind instead", errorFields(e));
      });
    return () => ctrl.abort();
  }, [open, tick]);
  return names;
}

export function DrawOverlay() {
  const route = useRoute();
  const ws = useWorkspace();
  const { open, active, tab, query, dockVisible, maximized } = useDrawOverlay();
  const { enabled } = useDrawPrefs();
  const [rows, setRows] = useState<DrawingRow[]>([]);
  const [detail, setDetail] = useState<DrawingDetail | null>(null);
  const [width, setWidth] = useStoredSize("bisa.draw.width", DEFAULT_WIDTH);
  const [height, setHeight] = useStoredSize("bisa.draw.height", DEFAULT_HEIGHT);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [repoTick, setRepoTick] = useState(0);
  const searchBox = useRef<HTMLInputElement>(null);
  const canvasApi = useRef<ExcalidrawImperativeAPI | null>(null);
  const workflows = useWorkflowNames(open);
  // Maximized, the panel fills the content column — the frame `shell/maximizedPanel.ts` gives, shared with Notes.
  const fixed = useMaximizedPanel(open, maximized);
  // Beside a floating Notes panel when the window holds both, never on it (`besideModel.mjs`).
  const notesWidth = useFloatingPanelWidth("notes");
  const viewport = useViewportWidth();
  const beside = fixed ? 0 : besideOffset({ notesWidth, drawWidth: width, viewport });
  // Over a browser tab the layer leaves a hole for the floating panel (ide/18); maximized it is a surface instead.
  const panel = useRef<HTMLElement>(null);
  useBrowserClear("draw-panel", panel, open && !fixed);
  // A way out parked behind the question about unsaved work (`shell/documentGuard.ts`).
  const leaving = drawGuard.usePending();

  const names = useMemo<ScopeNames>(
    () => ({
      projects: ws.projects.map((p) => ({ id: p.project.id, name: p.project.name })),
      goals: ws.goals.map((g) => ({ id: g.id, name: g.title || g.statement })),
      workflows,
      channels: ws.channels.map((c) => ({ id: c.channel.id, name: c.channel.name })),
    }),
    [ws.projects, ws.goals, ws.channels, workflows],
  );
  const projectOf = useCallback((wid: string) => ws.workstreams.find((w) => w.workstream.id === wid)?.workstream.project, [ws.workstreams]);

  // The list's reads, in order (`latestModel`): a frame's read of the tab that
  // was and the read of the tab that is answer in whatever order — only the
  // newest asked for is drawn, so one tab never shows another's rows.
  const reads = useRef(createLatest());
  const load = useCallback(
    async (signal?: AbortSignal) => {
      const ticket = reads.current.begin();
      setLoading(true);
      try {
        const { drawings } = await api.drawings(drawTabQuery(tab), signal);
        if (signal?.aborted || !reads.current.lands(ticket)) return;
        setRows(drawings);
        setError(null);
      } catch (e) {
        if (signal?.aborted || !reads.current.lands(ticket)) return;
        // What the node said is the log's; the panel says it in words a person reads.
        log.warn("draw", "the drawings could not be read", { tab, ...errorFields(e) });
        setError(tr("draw-overlay-could-not-load"));
      } finally {
        if (!signal?.aborted && reads.current.lands(ticket)) setLoading(false);
      }
    },
    [tab],
  );

  useEffect(() => {
    if (!open) return;
    const ctrl = new AbortController();
    void load(ctrl.signal);
    return () => ctrl.abort();
  }, [load, open]);

  // The open drawing's detail — the scene — read when it is picked.
  useEffect(() => {
    if (!open || !active) {
      setDetail(null);
      return;
    }
    const ctrl = new AbortController();
    api
      .drawing(active, ctrl.signal)
      .then((r) => {
        if (ctrl.signal.aborted) return;
        restoredDrawing();
        setDetail(r.drawing);
      })
      .catch((e: unknown) => {
        if (ctrl.signal.aborted) return;
        // The drawing that came back from the last window may have gone since: it opens nothing, and says nothing.
        if (!goneQuietly(active, restoredDrawing(), e instanceof ApiError ? e.status : null)) {
          log.warn("draw", "a drawing could not be read", { drawing: active, ...errorFields(e) });
          setError(tr("draw-overlay-could-not-load"));
        }
        clearActiveDrawing();
      });
    return () => ctrl.abort();
  }, [open, active]);

  // The canvas's chunk, fetched the moment the panel opens, so a drawing opens without a wait.
  useEffect(() => {
    if (open && enabled) void loadExcalidraw().catch((e: unknown) => log.warn("draw", "the canvas could not be loaded", errorFields(e)));
  }, [open, enabled]);

  /** The sole subscriber to `drawing_changed` for the list; the editor hears it for its own drawing. */
  useEngineEvents((e) => {
    if (e.payload.type !== "drawing_changed" || !open) return;
    setRepoTick((n) => n + 1);
    if (!drawTabAdmits(tab, e.payload.scope)) return;
    void load();
  });
  // What was drawn while the node was away was said by no frame.
  useReloadOnReconnect(() => {
    if (!open) return;
    setRepoTick((n) => n + 1);
    void load();
  });

  const handleSaved = useCallback((next: DrawingDetail) => {
    setDetail((prev) => (prev && prev.id === next.id ? { ...prev, title: next.title, hash: next.hash, element_count: next.element_count, updated_at: next.updated_at, pinned: next.pinned } : prev));
    setRows((prev) => prev.map((r) => (r.id === next.id ? { ...r, title: next.title, hash: next.hash, element_count: next.element_count, updated_at: next.updated_at, pinned: next.pinned } : r)));
    setRepoTick((n) => n + 1);
  }, []);

  const handleDeleted = useCallback(() => {
    setRows((prev) => prev.filter((r) => r.id !== active));
    setRepoTick((n) => n + 1);
    clearActiveDrawing();
  }, [active]);

  const shown = useMemo(() => filterDrawings(rows, query), [rows, query]);
  const targets = useMemo(() => drawTargets(tab, route, names, projectOf), [tab, route, names, projectOf]);
  const chips = drawTabKind(tab) !== "workspace" && drawTabKind(tab) !== "node";

  const changeTab = (next: string) => {
    setDrawTab(next as DrawTab);
    clearActiveDrawing();
  };

  const [newOpen, setNewOpen] = useState(false);
  const create = async (scope: OwnerScope, template: string, title: string) => {
    try {
      const mod = await loadExcalidraw();
      const skeleton = templateOf(template).skeleton();
      const elements = skeleton.length > 0 ? mod.convertToExcalidrawElements(skeleton as Parameters<typeof mod.convertToExcalidrawElements>[0], { regenerateIds: true }) : [];
      const { drawing } = await api.createDrawing({
        scope: scope.scope,
        id: scope.id,
        title,
        scene: { elements, app_state: { view_background_color: "#ffffff", grid: false } },
      });
      setRows((prev) => [drawing, ...prev]);
      setRepoTick((n) => n + 1);
      openDrawing(drawing.id);
    } catch (e) {
      log.warn("draw", "a drawing could not be created", { scope: scope.scope, template, ...errorFields(e) });
      setError(tr("draw-overlay-could-not-create"));
    }
  };

  // Delete from the list, without opening the drawing: asked first in the
  // editor's own words (the Irreversible Asks Rule). `doomed` outlives the
  // question, so the dialog keeps its title while it closes.
  const toast = useToast();
  const [doomed, setDoomed] = useState<DrawingRow | null>(null);
  const [asking, setAsking] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const askRemove = (r: DrawingRow) => {
    setDoomed(r);
    setAsking(true);
  };
  const remove = async (r: DrawingRow) => {
    setRemoving(r.id);
    try {
      await api.deleteDrawing(r.id);
    } catch (e) {
      // A drawing already gone is a delete that happened: the row leaves, as it would have.
      if (!(e instanceof ApiError && e.status === 404)) {
        toast.error(sayFailure("draw", tr("draw-overlay-could-not-delete"), e));
        setRemoving(null);
        return;
      }
    }
    setRows((prev) => prev.filter((x) => x.id !== r.id));
    setRepoTick((n) => n + 1);
    setRemoving(null);
    if (active === r.id) clearActiveDrawing();
  };

  const onListKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      e.stopPropagation();
      searchBox.current?.select();
    }
  };

  // Escape restores a maximized panel — once the canvas has nothing of its own to cancel.
  const onPanelKey = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "Escape" || !maximized) return;
    if (!restoreOnEscape(canvasApi.current?.getAppState() ?? null)) return;
    e.preventDefault();
    setDrawMaximized(false);
  };

  // *New* opens the dialog — the gallery, a title, and *Where* when the tab
  // offers several places — never a menu of `places × templates` rows.
  const newButton = (
    <Button size="sm" variant="ghost" disabled={targets.length === 0} disabledReason={tr("draw-overlay-nothing-to-file-under-yet")} onClick={() => setNewOpen(true)}>
      {tr("draw-overlay-new")}
    </Button>
  );

  const body = (
    <>
      {open && enabled && (
        // `contents`: no box of its own — it only hands the panel how far beside the Notes panel it stands.
        <div className="[display:contents]" style={{ "--draw-beside": `${beside}px` } as CSSProperties}>
          <section
            ref={panel}
            aria-label={tr("draw-dock-drawings")}
            data-pane
            data-draw-panel
            data-maximized={fixed ? "true" : undefined}
            style={fixed ?? { width, height }}
            onKeyDown={onPanelKey}
            className={cn(
              "z-40 overflow-hidden border border-border bg-bg shadow-xl",
              fixed ? "fixed rounded-none" : "fixed right-4 bottom-20 max-h-[calc(100vh-8rem)] max-w-[calc(100vw-2rem)] rounded-card mr-[var(--draw-beside,0px)]",
            )}
          >
            <div className="flex h-full w-full bg-surface">
              {!fixed && <ResizeHandle side="left" size={width} min={MIN_WIDTH} max={1600} defaultSize={DEFAULT_WIDTH} onSize={setWidth} label={tr("draw-overlay-panel-width")} />}
              <div className="flex min-w-0 flex-1 flex-col">
                {!fixed && <ResizeHandle side="top" size={height} min={MIN_HEIGHT} max={1400} defaultSize={DEFAULT_HEIGHT} onSize={setHeight} label={tr("draw-overlay-panel-height")} />}
                {detail && active === detail.id ? (
                  <DrawEditor
                    key={detail.id}
                    detail={detail}
                    scopeName={scopeWords(scopeOfRow(detail), names)}
                    onSaved={handleSaved}
                    onBack={clearActiveDrawing}
                    onDeleted={handleDeleted}
                    onApi={(api_) => {
                      canvasApi.current = api_;
                    }}
                  />
                ) : (
                  <div className="flex min-h-0 flex-1 flex-col" onKeyDown={onListKey}>
                    <header className="flex shrink-0 items-center gap-1 border-b border-hairline py-1.5 pl-3 pr-2">
                      <ICON.draw size={14} aria-hidden className="mr-1 shrink-0 text-text-dim" />
                      <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-text">{tr("draw-dock-drawings")}</h2>
                      {newButton}
                      <MaximizeToggle maximized={maximized} onToggle={toggleDrawMaximized} size={13} />
                      <Tooltip label={tr("draw-overlay-close")}>
                        <button type="button" aria-label={tr("draw-overlay-close")} onClick={() => setDrawOpen(false)} className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
                          <ICON.close size={13} aria-hidden />
                        </button>
                      </Tooltip>
                    </header>
                    <Tabs tabs={TABS} active={tab} onChange={changeTab} className="shrink-0 overflow-x-auto px-1" />
                    <div className="relative shrink-0 px-2 py-1.5">
                      <ICON.search size={12} aria-hidden className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 text-text-dim" />
                      <TextInput
                        ref={searchBox}
                        value={query}
                        type="search"
                        aria-label={tr("draw-overlay-search")}
                        placeholder={tr("draw-overlay-search")}
                        onChange={(e) => setDrawQuery(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Escape" && query) {
                            e.preventDefault();
                            setDrawQuery("");
                          }
                        }}
                        className="h-7 w-full pl-6 text-xs"
                      />
                    </div>
                    <div className="min-h-0 flex-1 overflow-y-auto p-1">
                      {loading && rows.length === 0 && <Spinner />}
                      {error && (
                        <div className="p-1">
                          <ErrorNote error={error} retry={() => void load()} />
                        </div>
                      )}
                      {/* Doors, not prose: an empty tab offers the first drawing; a search that matched nothing, the way back. */}
                      {!loading && !error && rows.length === 0 && (
                        <EmptyState
                          icon={ICON.draw}
                          title={tr("draw-overlay-no-drawings-yet")}
                          hint={EMPTY_WORDS[tab]}
                          action={targets.length > 0 ? <Button size="sm" variant="primary" onClick={() => setNewOpen(true)}>{tr("draw-overlay-new-drawing")}</Button> : null}
                        />
                      )}
                      {!loading && !error && rows.length > 0 && shown.length === 0 && (
                        <EmptyState
                          icon={ICON.search}
                          title={tr("draw-overlay-no-drawing-named-so")}
                          action={<Button size="sm" variant="ghost" onClick={() => setDrawQuery("")}>{tr("draw-overlay-clear-search")}</Button>}
                        />
                      )}
                      {shown.map((r) => {
                        const scope = scopeOfRow(r);
                        const Glyph = KIND_ICON[scope.scope];
                        // The row opens the drawing; its trash, a sibling and never nested, shows on hover or focus.
                        return (
                          <div key={r.id} className="group relative">
                            <button type="button" onClick={() => openDrawing(r.id)} className="anim flex w-full flex-col items-start gap-0.5 rounded-control py-1.5 pr-9 pl-2 text-left group-hover:bg-surface-2">
                              <span className="flex w-full items-center gap-1.5">
                                {r.pinned && <ICON.pin size={10} aria-label={tr("draw-overlay-pinned")} className="shrink-0" />}
                                <span className="min-w-0 flex-1 truncate text-xs font-medium text-text">{r.title}</span>
                              </span>
                              <span className="flex w-full min-w-0 items-center gap-1.5 text-2xs text-text-dim">
                                {chips && (
                                  <Chip icon={Glyph} className="max-w-40 shrink-0">
                                    <span className="min-w-0 truncate">{scopeWords(scope, names)}</span>
                                  </Chip>
                                )}
                                <span className="tnum min-w-0 flex-1">{tr("draw-overlay-elements", { count: r.element_count })}</span>
                              </span>
                            </button>
                            <Tooltip label={tr("draw-overlay-delete-drawing", { title: r.title })}>
                              <button
                                type="button"
                                aria-label={tr("draw-overlay-delete-drawing", { title: r.title })}
                                disabled={removing === r.id}
                                onClick={() => askRemove(r)}
                                className="row-actions anim absolute top-1/2 right-1.5 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-control text-text-dim hover:bg-danger-soft hover:text-danger disabled:opacity-45"
                              >
                                <ICON.delete size={13} aria-hidden />
                              </button>
                            </Tooltip>
                          </div>
                        );
                      })}
                    </div>
                    {/* content, never translated: the folder's word for a commit subject nobody wrote */}
                    <RepoStrip repo={api.drawingsRepo} subject="Drawings" pulls={false} folder="drawings" tick={repoTick} />
                  </div>
                )}
              </div>
            </div>
          </section>
        </div>
      )}
      <NewDrawingDialog
        open={newOpen}
        onClose={() => setNewOpen(false)}
        targets={targets}
        onCreate={(scope, template, title) => {
          setNewOpen(false);
          void create(scope, template, title);
        }}
      />
      {/* The dock's count is its own (`drawingCount.ts`); this list is the panel's. */}
      {dockVisible && enabled && <DrawDock />}
      <UnsavedDialog open={leaving !== null} words={leaving ? leaveWords(leaving.kind, leaving.title) : null} saving={leaving?.saving ?? false} onCancel={drawGuard.cancel} onDiscard={drawGuard.discardAndGo} onSave={() => void drawGuard.saveAndGo()} />
      <ConfirmDialog
        open={asking}
        onClose={() => setAsking(false)}
        onConfirm={() => {
          setAsking(false);
          if (doomed) void remove(doomed);
        }}
        title={tr("draw-editor-delete-title", { title: doomed?.title.trim() || tr("shell-leave-guard-this-drawing") })}
        body={tr("draw-editor-delete-body")}
        confirmLabel={tr("draw-editor-delete-confirm")}
        danger
      />
    </>
  );

  return createPortal(body, document.body);
}
