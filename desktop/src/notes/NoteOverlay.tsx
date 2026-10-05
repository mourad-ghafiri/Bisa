/**
 * The notes overlay: a panel that floats over whatever screen you are on, and
 * a dock it collapses to.
 *
 * # Why it is an overlay and not a screen
 *
 * A note about this goal should be one keystroke away *while you are reading
 * the goal*. A screen you navigate to is a screen that costs you your place,
 * which is exactly the friction that stops anyone writing anything down. So
 * the panel floats, and never takes the screen away.
 *
 * # The panel keeps its place
 *
 * What it lists is its **tab** — *All* by default, then one kind of note —
 * and which note is open is a click on a row. Neither follows the route:
 * navigating from a goal to a project changes nothing here, because reading
 * a note about one thing while looking at another is the point. The route
 * has one say, and it is where a *new* note is filed: *New* aims at where
 * you stand, and on a tab with more than one place it is a menu with that
 * place first (`notesModel.noteTargets`).
 *
 * The search box narrows the loaded list (`filterNotes`): every note is
 * small and local, so the node is not asked. Find and replace *inside* a
 * note is the editor's (`NoteEditor`).
 *
 * # Where it is mounted, and at what depth
 *
 * `App.tsx`, **outside `<Screen>`, `Suspense` and the error boundary** — the
 * placement `TerminalPanel` uses, for the same reason: a note you are half-way
 * through writing must outlive the screen you opened it on, and a screen that
 * throws must not take your unsaved text with it.
 *
 * It sits at `z-40`, deliberately **below** the `z-50` tier. Every dialog,
 * menu, popover and tooltip in this app is portalled to the body at 50, so a
 * panel at the same level would sometimes cover a confirmation dialog — a
 * modal you cannot see but which is still swallowing your keystrokes is the
 * worst failure available here.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { ApiError, api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { errorFields, log } from "../log";
import { useRoute } from "../router";
import { MaximizeToggle } from "../shell/MaximizeToggle";
import { createLatest } from "../shell/latestModel.mjs";
import { UnsavedDialog } from "../shell/UnsavedDialog";
import { leaveWords } from "../shell/leaveGuardModel.mjs";
import { useMaximizedPanel } from "../shell/maximizedPanel";
import { useBrowserClear } from "../shell/browserClear";
import { useWorkspace } from "../shell/useWorkspaceData";
import type { NoteRow } from "../types";
import { Button, Chip, ConfirmDialog, EmptyState, ErrorNote, Menu, ResizeHandle, Spinner, Tabs, TextInput, Tooltip, sayFailure, useStoredSize, useToast } from "../ui";
import type { MenuItem } from "../ui";
import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import { NoteDock } from "./NoteDock";
import { NoteEditor } from "./NoteEditor";
import { RepoStrip } from "../shell/repo/RepoStrip";
import {
  NOTE_TABS,
  NOTE_TAB_LABEL,
  draftKey,
  filterNotes,
  noteRefusal,
  noteTargets,
  scopeOfRow,
  scopeWords,
  tabAdmits,
  tabKind,
  tabQuery,
  type NamedRecord,
  type OwnerScope,
  type OwnerScopeKind,
  type NoteTab,
  type ScopeNames,
} from "./notesModel.mjs";
import { clearActiveNote, notesGuard, openNote, setNotesMaximized, setNotesOpen, setNotesQuery, setNotesTab, settleRestoredNote, toggleNotes, toggleNotesMaximized, useNotesOverlay } from "./notesStore";
import { t as tr } from "../i18n/l10n.mjs";
import { usePublishFloatingPanel } from "../shell/floatingPanelStore";
import { forgetPref, webStorage } from "../shell/storedPrefModel.mjs";

/**
 * How big the panel is before anyone drags it, and how small it may get.
 *
 * The floor is where the editor stops being one: below it the toolbar's seven
 * marks wrap onto a second line and the preview column is narrower than the
 * words in it. The tab strip scrolls sideways below the default width rather
 * than wrapping.
 */
const DEFAULT_WIDTH = 560;
const DEFAULT_HEIGHT = 640;
const MIN_WIDTH = 360;
const MIN_HEIGHT = 320;

/** The glyph a scope's chip and a target's menu row wear. */
const KIND_ICON: Record<OwnerScopeKind, (typeof ICON)["note"]> = {
  workspace: ICON.organization,
  project: ICON.project,
  goal: ICON.goal,
  workflow: ICON.workflow,
  channel: ICON.channel,
  node: ICON.node,
};

const TABS = NOTE_TABS.map((id) => ({ id, label: NOTE_TAB_LABEL[id] }));

/** What an empty list says, per tab: the kind's own words, not a generic shrug. */
const EMPTY_WORDS: Record<NoteTab, string> = {
  all: tr("notes-note-overlay-nothing-here-yet-note-yours-stays"),
  workspace: tr("notes-note-overlay-no-workspace-notes-yet-ones-about"),
  projects: tr("notes-note-overlay-no-note-project-yet"),
  goals: tr("notes-note-overlay-no-note-goal-yet"),
  workflows: tr("notes-note-overlay-no-note-workflow-yet"),
  channels: tr("notes-note-overlay-no-note-channel-yet"),
  node: tr("notes-note-overlay-no-note-about-node-yet-harnesses"),
};

/**
 * The workflows by name, for the chips and the New menu. Not part of
 * `useWorkspace` — the library is its own screen's load — so the panel reads
 * it once per open and again when a definition changes.
 */
function useWorkflowNames(open: boolean): NamedRecord[] {
  const [names, setNames] = useState<NamedRecord[]>([]);
  const [tick, setTick] = useState(0);
  useEngineEvents((e) => {
    if (e.payload.type === "workflow_changed" && open) setTick((t) => t + 1);
  });
  useReloadOnReconnect(() => {
    if (open) setTick((t) => t + 1);
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
        // A chip without a name says the kind instead; the list still draws.
        if (ctrl.signal.aborted) return;
        log.debug("notes", "the workflow names could not be read; a chip says the kind instead", errorFields(e));
      });
    return () => ctrl.abort();
  }, [open, tick]);
  return names;
}

export function NoteOverlay() {
  const route = useRoute();
  const ws = useWorkspace();
  const { open, active, tab, query, dockVisible, maximized } = useNotesOverlay();
  const [notes, setNotes] = useState<NoteRow[]>([]);
  // Two keys because `useStoredSize` holds one number each — the same hook the
  // sidebar, the aux pane and the terminal panel use to remember a size.
  const [width, setWidth] = useStoredSize("bisa.notes.width", DEFAULT_WIDTH);
  const [height, setHeight] = useStoredSize("bisa.notes.height", DEFAULT_HEIGHT);
  // Maximized, the panel fills the content column — the frame `shell/maximizedPanel.ts` gives, shared with Draw.
  const fixed = useMaximizedPanel(open, maximized);
  // While it floats, the panel says its width, so the Draw panel stands beside it rather than on it (`draw/besideModel.mjs`).
  usePublishFloatingPanel("notes", open && !fixed ? width : null);
  // Over a browser tab the layer leaves a hole for the floating panel (ide/18); maximized it is a surface instead.
  const panel = useRef<HTMLElement>(null);
  useBrowserClear("notes-panel", panel, open && !fixed);
  // A way out parked behind the question about unsaved work (`shell/documentGuard.ts`).
  const leaving = notesGuard.usePending();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /**
   * Bumped whenever a note is written here or elsewhere, so the repository
   * strip under the list re-reads its count without polling.
   */
  const [repoTick, setRepoTick] = useState(0);
  const searchBox = useRef<HTMLInputElement>(null);
  const workflows = useWorkflowNames(open);

  /** The directory the chips and the New menu name things from. */
  const names = useMemo<ScopeNames>(
    () => ({
      projects: ws.projects.map((p) => ({ id: p.project.id, name: p.project.name })),
      goals: ws.goals.map((g) => ({ id: g.id, name: g.title || g.statement })),
      workflows,
      channels: ws.channels.map((c) => ({ id: c.channel.id, name: c.channel.name })),
    }),
    [ws.projects, ws.goals, ws.channels, workflows],
  );
  const projectOf = useCallback(
    (wid: string) => ws.workstreams.find((w) => w.workstream.id === wid)?.workstream.project,
    [ws.workstreams],
  );

  // The list's reads, in order (`latestModel`): a frame's read of the tab that
  // was and the read of the tab that is answer in whatever order — only the
  // newest asked for is drawn, so one tab never shows another's rows.
  const reads = useRef(createLatest());
  const load = useCallback(
    async (signal?: AbortSignal) => {
      const ticket = reads.current.begin();
      setLoading(true);
      try {
        const { notes } = await api.notes(tabQuery(tab), signal);
        if (signal?.aborted || !reads.current.lands(ticket)) return;
        setNotes(notes);
        // The note that came back from the last window may have gone since.
        settleRestoredNote(notes.map((n) => n.id));
        setError(null);
      } catch (e) {
        if (signal?.aborted || !reads.current.lands(ticket)) return;
        // What the node said is the log's; the panel says it in words a person reads.
        log.warn("notes", "the notes could not be read", { tab, ...errorFields(e) });
        setError(tr("notes-note-overlay-could-not-load-notes"));
      } finally {
        if (!signal?.aborted && reads.current.lands(ticket)) setLoading(false);
      }
    },
    [tab],
  );

  // Only while open, and per tab: a closed panel polling would be work nobody
  // asked for, and the route is not in this list on purpose. This list is the
  // panel's alone — the dock's count is its own (`noteCount.ts`), kept by the
  // frames whether or not the panel is open.
  useEffect(() => {
    if (!open) return;
    const ctrl = new AbortController();
    void load(ctrl.signal);
    return () => ctrl.abort();
  }, [load, open]);

  /**
   * The sole subscriber to `note_changed`.
   *
   * One listener, so a single frame produces one refetch and one re-render of
   * the open editor. And the node no longer emits for the editor's own
   * `PATCH`, so every frame that arrives here is somebody else's write — in
   * practice an agent's `note_append` from the conversation beside the note.
   */
  useEngineEvents((e) => {
    if (e.payload.type !== "note_changed" || !open) return;
    // The repository holds every scope's notes, so its count moves first.
    setRepoTick((t) => t + 1);
    // Only a kind this tab lists: an agent appending to a workspace note while
    // you read the Goals tab should not redraw what you are looking at.
    if (!tabAdmits(tab, e.payload.scope)) return;
    void load();
  });
  // What was written while the node was away was said by no frame.
  useReloadOnReconnect(() => {
    if (!open) return;
    setRepoTick((t) => t + 1);
    void load();
  });

  // Alt+N, everywhere. Not ⌘N — the palette owns the ⌘ combinations, and this
  // is a panel rather than a place.
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.altKey && !e.metaKey && !e.ctrlKey && e.key.toLowerCase() === "n") {
        e.preventDefault();
        toggleNotes();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /**
   * Stable, so the editor's save path cannot be re-armed by a parent render.
   *
   * This was an inline arrow, which meant a new identity on every render of
   * this component — and the editor held it in a dependency array, so every
   * refetch here re-fired a save down there.
   */
  const handleSaved = useCallback((next: NoteRow) => {
    setNotes((prev) => prev.map((n) => (n.id === next.id ? next : n)));
    setRepoTick((t) => t + 1);
  }, []);

  const handleDeleted = useCallback(() => {
    setNotes((prev) => prev.filter((n) => n.id !== active));
    setRepoTick((t) => t + 1);
    clearActiveNote();
  }, [active]);

  const current = notes.find((n) => n.id === active) ?? null;
  const shown = useMemo(() => filterNotes(notes, query), [notes, query]);
  const targets = useMemo(() => noteTargets(tab, route, names, projectOf), [tab, route, names, projectOf]);
  // A tab that names one place needs no chip on its rows; the rest say what each note is about.
  const chips = tabKind(tab) !== "workspace" && tabKind(tab) !== "node";

  /** A tab change is the one thing besides a click that changes what is shown: it lands on the list. */
  const changeTab = (next: string) => {
    setNotesTab(next as NoteTab);
    clearActiveNote();
  };

  const create = async (scope: OwnerScope) => {
    try {
      const { note } = await api.createNote({
        scope: scope.scope,
        id: scope.id,
        title: tr("notes-note-overlay-untitled-note"),
        body: "",
      });
      setNotes((prev) => [note, ...prev]);
      setRepoTick((t) => t + 1);
      openNote(note.id);
    } catch (e) {
      log.warn("notes", "a note could not be created", { scope: scope.scope, ...errorFields(e) });
      setError(tr("notes-note-overlay-could-not-create-note"));
    }
  };

  // Delete from the list, without opening the note: asked first in the
  // editor's own words (the Irreversible Asks Rule). `doomed` outlives the
  // question, so the dialog keeps its title while it closes.
  const toast = useToast();
  const [doomed, setDoomed] = useState<NoteRow | null>(null);
  const [asking, setAsking] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const askRemove = (n: NoteRow) => {
    setDoomed(n);
    setAsking(true);
  };
  const remove = async (n: NoteRow) => {
    setRemoving(n.id);
    try {
      await api.deleteNote(n.id);
    } catch (e) {
      // A note already gone is a delete that happened: the row leaves, as it would have.
      if (!(e instanceof ApiError && noteRefusal(e.status) === "gone")) {
        toast.error(sayFailure("notes", tr("notes-note-overlay-could-not-delete"), e));
        setRemoving(null);
        return;
      }
    }
    // A draft kept for a note that is gone has nowhere to go back to.
    forgetPref(webStorage(), draftKey(n.id));
    setNotes((prev) => prev.filter((x) => x.id !== n.id));
    setRepoTick((t) => t + 1);
    setRemoving(null);
    if (active === n.id) clearActiveNote();
  };

  // ⌘F over the list is the search box; inside a note the editor takes it.
  const onListKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      e.stopPropagation();
      searchBox.current?.select();
    }
  };

  // Escape restores a maximized panel — unless something in it took the key
  // first: the find bar closing, the search box clearing (both prevent the
  // default). A menu renders in its own portal, so its keys never reach here.
  const onPanelKey = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "Escape" || !maximized || e.defaultPrevented) return;
    e.preventDefault();
    setNotesMaximized(false);
  };

  const newButton =
    targets.length === 1 ? (
      <Button size="sm" variant="ghost" onClick={() => void create(targets[0].scope)}>{tr("notes-note-overlay-new")}</Button>
    ) : targets.length === 0 ? (
      <Button size="sm" variant="ghost" disabled disabledReason={tr("notes-note-overlay-nothing-file-note-under-yet")}>{tr("notes-note-overlay-new")}</Button>
    ) : (
      <Menu
        label={tr("notes-note-overlay-new-note")}
        items={targets.map(
          (t): MenuItem => ({
            label: t.here ? tr("notes-note-overlay-here", { t: t.label }) : t.label,
            icon: KIND_ICON[t.scope.scope],
            onSelect: () => void create(t.scope),
          }),
        )}
        trigger={
          <span className="anim inline-flex h-7 items-center gap-1 rounded-control px-2 text-xs font-medium text-text-dim hover:bg-surface-2 hover:text-text">{tr("notes-note-overlay-new")}<ICON.down size={11} aria-hidden />
          </span>
        }
      />
    );

  const body = (
    <>
      {open && (
        <section
          ref={panel}
          aria-label={tr("notes-note-dock-notes")}
          // A pane: a glass family frosts what is behind it (`theme/material.css`).
          data-pane
          data-maximized={fixed ? "true" : undefined}
          style={fixed ?? { width, height }}
          onKeyDown={onPanelKey}
          className={cn(
            "z-40 overflow-hidden border border-border bg-bg shadow-xl",
            fixed ? "fixed rounded-none" : "fixed right-4 bottom-20 max-h-[calc(100vh-8rem)] max-w-[calc(100vw-2rem)] rounded-card",
          )}
        >
          {/*
            The sheet. On a glass family `surface` is a translucent colour, and
            everywhere else it is read *over the page's ground* (`bg`) — which
            is what the theme's contrast promise is measured over. This panel
            floats over whatever screen is open, with no ground under it and no
            veil behind it, so it lays its own: `surface` over `bg`, the pair
            the main window reads as — nearly opaque on glass, unchanged on an
            opaque family — because a note is read and written for minutes at a
            time, and the screen behind it is not a surface to read over.
          */}
          <div className="flex h-full w-full bg-surface">
            {/*
              Handles as flex siblings, which is how `ResizeHandle` is built to
              be used. The panel is pinned to its bottom-right corner, so the
              edges that can grow are the left and the top — and the kit's
              `GROWS_WITH_POINTER` table already makes both of those widen as the
              pointer moves outward.

              The `max-*` classes above stay the real ceiling: they track the
              window, and a number read at render does not. Maximized, the
              panel has no edge to drag, and neither handle is drawn.
            */}
            {!fixed && <ResizeHandle side="left" size={width} min={MIN_WIDTH} max={1200} defaultSize={DEFAULT_WIDTH} onSize={setWidth} label={tr("notes-note-overlay-notes-panel-width")} />}
            <div className="flex min-w-0 flex-1 flex-col">
              {!fixed && <ResizeHandle side="top" size={height} min={MIN_HEIGHT} max={1400} defaultSize={DEFAULT_HEIGHT} onSize={setHeight} label={tr("notes-note-overlay-notes-panel-height")} />}
              {current ? (
                <NoteEditor
                  note={current}
                  scopeName={scopeWords(scopeOfRow(current), names)}
                  onSaved={handleSaved}
                  onBack={clearActiveNote}
                  onDeleted={handleDeleted}
                />
              ) : (
                <div className="flex min-h-0 flex-1 flex-col" onKeyDown={onListKey}>
                  <header className="flex shrink-0 items-center gap-1 border-b border-hairline py-1.5 pl-3 pr-2">
                    <ICON.note size={14} aria-hidden className="mr-1 shrink-0 text-text-dim" />
                    <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-text">{tr("notes-note-dock-notes")}</h2>
                    {newButton}
                    <MaximizeToggle maximized={maximized} onToggle={toggleNotesMaximized} size={13} />
                    <Tooltip label={tr("notes-note-overlay-close-notes")}>
                      <button
                        type="button"
                        aria-label={tr("notes-note-overlay-close-notes")}
                        onClick={() => setNotesOpen(false)}
                        className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
                      >
                        <ICON.close size={13} aria-hidden />
                      </button>
                    </Tooltip>
                  </header>
                  <Tabs tabs={TABS} active={tab} onChange={changeTab} className="shrink-0 overflow-x-auto px-1" />
                  <div className="relative shrink-0 px-2 py-1.5">
                    <ICON.search
                      size={12}
                      aria-hidden
                      className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 text-text-dim"
                    />
                    <TextInput
                      ref={searchBox}
                      value={query}
                      type="search"
                      aria-label={tr("notes-note-overlay-search-notes")}
                      placeholder={tr("notes-note-overlay-search-notes")}
                      onChange={(e) => setNotesQuery(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape" && query) {
                          e.preventDefault();
                          setNotesQuery("");
                        }
                      }}
                      className="h-7 w-full pl-6 text-xs"
                    />
                  </div>
                  <div className="min-h-0 flex-1 overflow-y-auto p-1">
                    {loading && notes.length === 0 && <Spinner />}
                    {error && (
                      <div className="p-1">
                        <ErrorNote error={error} retry={() => void load()} />
                      </div>
                    )}
                    {/* Doors, not prose: an empty tab offers the first note where you stand; a search that matched nothing, the way back. */}
                    {!loading && !error && notes.length === 0 && (
                      <EmptyState
                        icon={ICON.note}
                        title={tr("notes-note-overlay-no-notes-yet")}
                        hint={EMPTY_WORDS[tab]}
                        action={
                          targets.length > 0 ? (
                            <Button size="sm" variant="primary" onClick={() => void create(targets[0].scope)}>{tr("notes-note-overlay-new-note")}</Button>
                          ) : null
                        }
                      />
                    )}
                    {!loading && !error && notes.length > 0 && shown.length === 0 && (
                      <EmptyState
                        icon={ICON.search}
                        title={tr("notes-note-overlay-no-note-says")}
                        action={<Button size="sm" variant="ghost" onClick={() => setNotesQuery("")}>{tr("notes-note-overlay-clear-search")}</Button>}
                      />
                    )}
                    {shown.map((n) => {
                      const scope = scopeOfRow(n);
                      const Glyph = KIND_ICON[scope.scope];
                      // The row opens the note; its trash, a sibling and never nested, shows on hover or focus.
                      return (
                        <div key={n.id} className="group relative">
                          <button
                            type="button"
                            onClick={() => openNote(n.id)}
                            className="anim flex w-full flex-col items-start gap-0.5 rounded-control py-1.5 pr-9 pl-2 text-left group-hover:bg-surface-2"
                          >
                            <span className="flex w-full items-center gap-1.5">
                              {n.pinned && <ICON.pin size={10} aria-label={tr("notes-note-overlay-pinned")} className="shrink-0" />}
                              <span className="min-w-0 flex-1 truncate text-xs font-medium text-text">{n.title}</span>
                            </span>
                            <span className="flex w-full min-w-0 items-center gap-1.5 text-2xs text-text-dim">
                              {chips && (
                                <Chip icon={Glyph} className="max-w-40 shrink-0">
                                  <span className="min-w-0 truncate">{scopeWords(scope, names)}</span>
                                </Chip>
                              )}
                              <span className="line-clamp-1 min-w-0 flex-1">{n.body.trim().split("\n")[0] || tr("notes-note-overlay-empty")}</span>
                            </span>
                          </button>
                          <Tooltip label={tr("notes-note-overlay-delete-note", { title: n.title })}>
                            <button
                              type="button"
                              aria-label={tr("notes-note-overlay-delete-note", { title: n.title })}
                              disabled={removing === n.id}
                              onClick={() => askRemove(n)}
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
                  <RepoStrip repo={api.notesRepo} subject="Notes" pulls folder="notes" tick={repoTick} />
                </div>
              )}
            </div>
          </div>
        </section>
      )}
      {/* Off only hides the corner button — `Alt+N` above still opens the
          panel, which is what makes this a visibility switch rather than an
          off switch for the feature. */}
      {dockVisible && <NoteDock />}
      <UnsavedDialog open={leaving !== null} words={leaving ? leaveWords(leaving.kind, leaving.title) : null} saving={leaving?.saving ?? false} onCancel={notesGuard.cancel} onDiscard={notesGuard.discardAndGo} onSave={() => void notesGuard.saveAndGo()} />
      <ConfirmDialog
        open={asking}
        onClose={() => setAsking(false)}
        onConfirm={() => {
          setAsking(false);
          if (doomed) void remove(doomed);
        }}
        title={tr("notes-note-editor-delete-title", { title: doomed?.title.trim() || tr("shell-leave-guard-this-note") })}
        body={tr("notes-note-editor-delete-body")}
        confirmLabel={tr("notes-note-editor-delete-confirm")}
        danger
      />
    </>
  );

  return createPortal(body, document.body);
}
