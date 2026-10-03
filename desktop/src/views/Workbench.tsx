/**
 * The workbench: one screen, rooted at the thing you are working on.
 *
 * A workstream, a work item or a goal — addressed `#/projects/{scope}/{id}`,
 * which is the same `{scope}/{id}` the file routes take. A project's root is
 * its primary workstream, so `#/projects/workstream/<project id>`
 * is the project.
 *
 * Three regions (layout), and one that is deliberately absent:
 *
 * - **The rail**, left: the projects, their workstreams and the sessions in
 *   them (`_workbench/ProjectRail.tsx`).
 * - **The centre**, in one of three modes: Project — one strip of documents
 *   *and* terminals, over the active one, a terminal tab's body a slot the
 *   terminal layer is drawn over; Agent — the workstream's conversation;
 *   Board — every workstream's card (`_board/BoardCenter.tsx`).
 * - **The right panel**: Files, Git, Workstreams, Agent and About on one
 *   rail, remembered per root.
 * - **Terminals**: *not rendered here.* The layer lives in `App.tsx`, outside
 *   the routed screen, because a shell outlives the surface that opened it.
 *   Mounting it inside this component would end every running build the
 *   moment somebody opened the Inbox. See `shell/TerminalPanel.tsx`.
 *
 * What is in the URL, and what is not, is argued in `_workbench/workbenchModel.mjs`:
 * the active document is a place (`?doc=`), the set of open tabs is furniture.
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { api, droppedPaths, pickFiles, revealPath } from "../api";
import { isFileDrop, pathsForDrop } from "./_workbench/dropModel.mjs";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import { navigate, useSearchValue, type WorkbenchScope } from "../router";
import { TerminalLauncher } from "../shell/TerminalLauncher";
import { BrowserLauncher } from "../shell/BrowserLauncher";
import { DeviceLauncher } from "../shell/DeviceLauncher";
import { DeviceDoc } from "./_workbench/DeviceDoc";
import { closeBrowserTab, focusBrowserTab, useBrowsers } from "../shell/useBrowsers";
import { seenRootedAt } from "../shell/browsersModel.mjs";
import { useAux } from "../shell/AuxPane";
import { openBrowserPane } from "../shell/browserDoors";
import { followCentre } from "../shell/browserDoorsModel.mjs";
import { publishWorkbenchCentre } from "./_workbench/workbenchCentreStore";
import {
  CLOSE_ACTIVE_TAB,
  CLOSE_OTHER_TABS,
  NEW_DOCUMENT,
  NEW_WORKSTREAM,
  OPEN_FILE,
  OPEN_LOOSE,
  NEXT_TAB,
  PREV_TAB,
  REOPEN_TAB,
  REVEAL_ACTIVE_IN_FILES,
  REVEAL_ACTIVE_IN_FINDER,
  SHOW_TERMINAL,
  fire,
  onDoor,
  openPlaces,
  KEEP_TAB,
  CLOSE_SAVED_TABS,
  SELECT_TAB_AT,
} from "../shell/shortcuts";
import { CommandHint } from "../shell/CommandHint";
import { openTerminalIn } from "../shell/useTerminals";
import { useWorkspace } from "../shell/useWorkspaceData";
import {
  Avatar,
  Button,
  Chip,
  ErrorNote,
  ICON,
  ResizeHandle,
  SegmentedControl,
  SkeletonRows,
  failureText,
  focusComposer,
  Tooltip,
  requestReveal,
  useStoredSize,
  useToast,
  EmptyState,
  usePhotoThumb,
} from "../ui";
import { CommitDocument } from "./_work/CommitDocument";
import { DiffView } from "./_work/DiffView";
import { PatchDocument } from "./_work/PatchDocument";
import { NewWorkstreamDoor } from "./_workbench/NewWorkstreamDoor";
import { firstCaution, hasCautions } from "./_work/connectionModel.mjs";
import { identityState } from "./_work/gitIdentityModel.mjs";
import { OccupantRail } from "./_workbench/OccupantRail";
import { availableOccupants, resolveOccupant } from "./_workbench/rightPanelModel.mjs";
import { ProjectRail } from "./_workbench/ProjectRail";
import { EmptyIdeLanding } from "./_workbench/CenterLanding";
import { ArtifactDocument } from "./_workbench/ArtifactDocument";
import { TranscriptDocument } from "./_workbench/TranscriptDocument";
import { RenderedFileDoc } from "./_workbench/RenderedFileDoc";
import { docKindOf, isRenderedDoc } from "./_workbench/fileDocModel.mjs";
import { homeAfterLeaving, homeRoot, homeRoute, leavesRoot } from "./_workbench/ideHomeModel.mjs";
import { useGonePlace } from "../shell/useGonePlace";
import { NewProjectDialog } from "./_work/NewProjectDialog";
import { replace } from "../router";
import { CenterDocuments } from "./_workbench/CenterDocuments";
import { CenterLanding } from "./_workbench/CenterLanding";
import { RightPanel } from "./_workbench/RightPanel";
import { setLspOpener } from "./_workbench/lspClient";
import { openPanelView, pressOccupant, recallRightTab, settleOccupant, showRightPanel, useRightPanel } from "./_workbench/rightPanelStore";
import { AgentModeCenter } from "./_workbench/AgentModeCenter";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, DOCUMENT_MODE, MODES, centreOf, type IdeMode } from "./_workbench/ideModeModel.mjs";
import { setIdeMode, useIdeMode } from "./_workbench/ideModeStore";
import { lastRoot, rememberLastRoot } from "./_workbench/lastRootStore";
import { useRememberedPick } from "./_studio/conversationPickStore";
import { BoardCenter } from "./_board/BoardCenter";
import { BOARD_PLACE } from "./_workbench/idePlacesModel.mjs";
import { gitBadge, workstreamsBadge } from "./_workbench/railBadgesModel.mjs";
import { identityMoved } from "./_work/gitIdentityModel.mjs";
import { useWorkstreamStatus } from "../shell/workstreamStatusStore";
import { useSessions } from "../shell/sessionsStore";
import { useGitSession, useSessionDraft } from "./_work/gitPanelStore";
import { draftKeys } from "./_work/agentReviewModel.mjs";
import { reviewRun } from "./_work/reviewStepModel.mjs";
import type { ReviewRun } from "./_work/reviewStepModel.mjs";
import { modeSegments } from "./_workbench/workbenchChromeModel.mjs";
import { boolOf, choiceOf } from "../shell/settingsModel.mjs";
import { BOARD_DEFAULTS, BOARD_ENABLED_KEY } from "./_board/boardSettings.mjs";
import { useResolvedSettings } from "../shell/useResolvedSettings";
import { ProjectRailToggle, RightPanelToggle } from "./_workbench/PanelDoors";
import { setProjectRailFolded, useProjectRail } from "./_workbench/projectRailStore";
import { fitColumns } from "./_workbench/ideColumnsModel.mjs";
import { boardLabel } from "./_work/types";
import { useAsync } from "./_work/useAsync";
import {
  isPinned,
  mergedTabs,
  nextActiveAfterClose,
  panesFor,
  parseTabId,
  tabIn,
  rootKey,
  tabId,
  tabLabel,
  tabsFor,
  tabsRightOf,
  tabsUnder,
  previewIds,
  savedTabs,
} from "./_workbench/workbenchModel.mjs";
import { cycle } from "../ui";
import { joinPath } from "../ui/fileTreeModel.mjs";
import type { WorkbenchTab } from "./_workbench/workbenchModel.mjs";
import {
  closeAllDocs,
  closeDocsRight,
  closeOtherDocs,
  closeDoc,
  forgetWorkbenchRoot,
  moveDocTab,
  activateDocTab,
  closeDocsPane,
  focusDocs,
  moveDoc,
  openDoc,
  openDocBeside,
  reopenLastClosedDoc,
  restoreDocs,
  retargetDocs,
  setDocsPaneRatio,
  splitDocs,
  togglePinnedDoc,
  useWorkbench,
  keepDoc,
  closeSavedDocs,
  openUntitledDoc,
  replaceDoc,
  reconcileStripOrder,
} from "./_workbench/workbenchStore";
import { EditorDoc } from "./_workbench/EditorDoc";
import { guardWords } from "./_workbench/closeGuardModel.mjs";
import { UnsavedDialog } from "../shell/UnsavedDialog";
import { anyDirty, editorKey, requestLine, saveDocument, setRevealer, useDirtyEditors } from "./_workbench/editorRegistry";
import { restoreLayout, sameLayout, serializeLayout } from "./_workbench/ideLayoutModel.mjs";
import { notifyLayoutChanged } from "../shell/layerSlots";
import { sessionsRootedAt } from "../shell/terminalsModel.mjs";
import { terminalSessionOf } from "../shell/followedSessionModel.mjs";
import { followSession, registerFollowedOpener } from "../shell/followedSessionStore";
import { useTerminals } from "../shell/useTerminals";
import { requestCloseTerminal } from "../shell/terminalCloseGuard";
import { t as tr } from "../i18n/l10n.mjs";

const RIGHT_KEY = "bisa.ide.right.width";
const RIGHT_DEFAULT = 380;
const RIGHT_MIN = 300;
const RIGHT_MAX = 720;
const RAIL_KEY = "bisa.ide.rail.width";
const RAIL_DEFAULT = 380; // room for the rail's tabs with their badges; a narrower rail folds them to their glyphs one at a time, Workflows first
const RAIL_MIN = 220;
const RAIL_MAX = 480;
/** A header word that folds to its glyph below the header's `@5xl`: off the line, still the button's name. */
const FOLDED_WORD = "sr-only @5xl:not-sr-only";

const SCOPE_NOUN: Record<WorkbenchScope, string> = {
  workstream: tr("screens-workbench-workstream-2"),
  work_item: tr("screens-workbench-work-item"),
  goal: tr("screens-workbench-goal-2"),
};

export default function Workbench({ scope, id }: { scope: WorkbenchScope; id: string }) {
  const ws = useWorkspace();
  const toast = useToast();
  const key = rootKey(scope, id);
  const state = useWorkbench();
  const [docParam, setDoc] = useSearchValue("doc");
  const [rightWidth, setRightWidth] = useStoredSize(RIGHT_KEY, RIGHT_DEFAULT, { min: RIGHT_MIN, max: RIGHT_MAX });
  const [railWidth, setRailWidth] = useStoredSize(RAIL_KEY, RAIL_DEFAULT);
  const right = useRightPanel();
  const terminals = useTerminals();
  const browsers = useBrowsers();
  /** A tab drawn by a layer — a terminal's or a browser's — never a document. */
  const isLayerId = (tid: string) => tid.startsWith("terminal:") || tid.startsWith("browser:");
  // Which centre this root shows: what was remembered for it, else the
  // default the settings name (ide/09 §Agent Mode, ide/16). The keys have no
  // project scope, so the workspace's resolution is the whole answer. The
  // Board is a mode only while its setting has it on.
  const { resolved } = useResolvedSettings(null);
  const boardEnabled = boolOf(resolved, BOARD_ENABLED_KEY, BOARD_DEFAULTS.enabled);
  const mode = useIdeMode(key, choiceOf(resolved, DEFAULT_MODE_KEY, MODES, DEFAULT_MODE), boardEnabled);
  // The root changed: recall the occupant it last showed.
  useEffect(() => recallRightTab(key), [key]);
  // Who commits in this checkout's repository — a warning in the header when
  // nobody does, before anyone reaches the commit box. A read that fails — a
  // plain folder, a checkout that is gone — is `null` and draws nothing:
  // never the previous root's answer left on screen.
  const [identityTick, setIdentityTick] = useState(0);
  const identity = useAsync(async (s) => (scope === "workstream" ? api.gitIdentity(id, s).catch(() => null) : null), [scope, id, identityTick]);
  // The pair may be set outside the platform — `git config` in a terminal —
  // and no frame says so; the window coming back to the front is when the
  // person looks, so it is read again then.
  useEffect(() => {
    const onFocus = () => setIdentityTick((n) => n + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);
  // What the checkout will use to reach its remote — a caution (the wrong
  // author for the organization, a key ssh-agent lacks, no account) wears a
  // chip beside the committer's, before anyone pushes. A plain folder answers
  // with an error and shows nothing.
  const connection = useAsync(async (s) => (scope === "workstream" ? api.gitConnection(id, s).catch(() => null) : null), [scope, id]);
  // Who commits was set or asked for — from About › Checkout, the ask
  // dialog, the CLI — or the person's git setup moved — a profile, a key, an
  // account, their global identity: the chip must not outlive the answer, and
  // the caution must not either. One rule, `identityMoved`, shared with
  // About › Checkout so the two never disagree.
  useEngineEvents((e) => {
    if (scope === "workstream" && identityMoved(e.payload.type)) {
      identity.reload();
      connection.reload();
    }
    // The project under this root archived or removed elsewhere — the command
    // line, another window, an agent: the IDE leaves for the next home the
    // same moment, computed from the facts in hand with that project taken
    // out (`ideHomeModel`). A removal made from here has moved the person
    // already, and the fact then names another root's project: nothing twice.
    const leaving = leavesRoot(e.payload, { scope, id }, ws.workstreams);
    if (leaving) replace(homeRoute(homeAfterLeaving(leaving, lastRoot(), ws.projects, ws.workstreams)));
  });

  // Go-to-definition into another file lands here: open it and show the
  // line. Registered once; the handler reads the latest `openAt` through a
  // ref rather than re-registering on every render.
  const openAtRef = useRef<(path: string, line: number | null) => void>(() => undefined);
  useEffect(() => {
    setLspOpener((path, line) => openAtRef.current(path, line));
    return () => setLspOpener(null);
  }, []);
  // ⌘J from the keymap: show this terminal tab — or, when it is already the
  // active tab, go back to the document that was showing before it.
  const activeRef = useRef<string | null>(null);
  const lastDoc = useRef<string | null>(null);
  useEffect(
    () =>
      onDoor(SHOW_TERMINAL, (k) => {
        if (typeof k !== "string") return;
        const want = `terminal:${k}`;
        setDoc(activeRef.current === want ? lastDoc.current : want);
      }),
    [key, setDoc],
  );
  // Where you were, for `#/projects` to come back to.
  useEffect(() => {
    if (scope === "workstream") rememberLastRoot(id);
  }, [scope, id]);

  const docs = useMemo(() => tabsFor(state, key), [state, key]);
  const dirty = useDirtyEditors();
  /** A tab whose close is waiting on Save / Discard / Cancel. */
  const [guarding, setGuarding] = useState<{ ids: string[]; then: () => void } | null>(null);
  const activeId = docParam ?? null;
  activeRef.current = activeId;
  // A harness tab brought to the centre chooses its session as the one this
  // workstream is about — what the pet follows (ide/09).
  useEffect(() => {
    if (scope !== "workstream") return;
    const session = terminalSessionOf(terminals.sessions, activeId);
    if (session) followSession(id, session);
  }, [scope, id, activeId, terminals.sessions]);
  // The pet's door back in: a click on a following pet shows this
  // workstream's Agent panel.
  useEffect(() => registerFollowedOpener((wid) => showRightPanel("agents", rootKey("workstream", wid))), []);
  if (activeId && !isLayerId(activeId)) lastDoc.current = activeId;
  // The stored tab by identity, so the effects keyed on it run when it
  // changes and not on every render.
  const active = useMemo(() => tabIn(docs, activeId), [docs, activeId]);

  const placement = useAsync((s) => api.placement(scope, id, s), [scope, id]);
  const goals = useMemo(() => ws.goals.map((i) => ({ id: i.id, label: boardLabel(i) })), [ws.goals]);

  /**
   * A work item is addressed by id alone — here and on every route about
   * one: its home (a goal, or a run of the workspace) comes back beside it.
   */
  const located = useAsync(
    (s) => (scope === "work_item" ? api.workItem(id, s) : Promise.resolve(null)),
    [scope, id],
  );

  /**
   * The workstream this workbench is rooted at, with its project.
   * The primary's id is the project's, which is what `isPrimary` reads.
   */
  const detail = useAsync(async (s) => (scope === "workstream" ? api.workstream(id, s) : null), [scope, id]);
  // A checkout deleted while the app was closed, or by a hand no fact on the
  // bus reached this window from, answers *not found*: the root is left as
  // every detail screen leaves a place that is gone — for `#/projects`, whose
  // home is a live project's or the landing.
  const missing = scope === "workstream" && detail.missing;
  useGonePlace(missing, { name: "workbench", scope, id });
  const project = detail.data?.workstream.project ?? null;
  const isPrimary = scope === "workstream" && project === id;
  /**
   * What the centre shows under the mode: the conversation or the Board
   * only on a checkout with a project, documents otherwise (`centreOf`).
   * Published for the doors that show a browser tab (ide/18 §The browser
   * tab), and withdrawn with the root.
   */
  const centre = centreOf(mode, { scope, hasProject: scope === "workstream" && project !== null });
  const conversationCentre = centre === "conversation";
  const boardCentre = centre === "board";
  useEffect(() => publishWorkbenchCentre({ scope, id }, centre), [scope, id, centre]);
  // The browser follows the centre: a tab at home here rides the strip while
  // the centre shows documents and the Details pane otherwise, so a switch
  // carries it — the strip's tab to the pane leaving documents, the pane's
  // tab at home here back to the strip returning, the pane's occupant
  // closing. Only a switch of this same root moves anything: a mount or a
  // root change leaves the pane as the person had it.
  const aux = useAux();
  const browsersHere = useMemo(() => new Set(seenRootedAt(browsers.sessions, scope, id).map((s) => s.key)), [browsers.sessions, scope, id]);
  const activeBrowserHere = ((t) => (t?.kind === "browser" && browsersHere.has(t.key) ? t.key : null))(activeId ? parseTabId(activeId) : null);
  const paneBrowserHere = aux.kind === "browser" && aux.id && browsersHere.has(aux.id) ? aux.id : null;
  const centreWas = useRef({ key, centre });
  useEffect(() => {
    const was = centreWas.current;
    centreWas.current = { key, centre };
    if (was.key !== key || was.centre === centre) return;
    const carried = followCentre(centre, activeBrowserHere, paneBrowserHere);
    if (carried?.show === "pane") openBrowserPane(carried.key);
    else if (carried?.show === "centre") {
      focusBrowserTab(carried.key);
      aux.close();
    }
    // On a switch of the centre only: the two browser facts and `aux` are
    // read as they stand at that moment, and following them would run this
    // on every render for the guard above to swallow.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, centre]);

  const owner = useAsync(
    async (s) => {
      if (scope === "goal") return id;
      if (scope === "workstream") {
        const d = await api.workstream(id, s);
        return d.workstream.goal ?? (await api.project(d.workstream.project, s)).goals[0] ?? null;
      }
      // A run of the workspace's item belongs to no goal.
      const home = (await api.workItem(id, s)).home;
      return home.home === "goal" ? home.goal : null;
    },
    [scope, id],
  );

  // A `?doc=` this store has never seen — a reload, a pasted link, Back to a
  // preview that has since been replaced — is adopted rather than ignored, as
  // a kept tab. A terminal is the terminal store's, so a `?doc=terminal:` that
  // names no session here simply shows the landing.
  // An untitled document is the one tab its id does not describe — a reload
  // of `?doc=untitled:1` has nothing to bring back — so the URL steps to what
  // the strip holds instead of opening an empty buffer nobody typed into.
  useEffect(() => {
    if (!active || active.kind === "terminal" || active.kind === "browser") return;
    if (docs.some((t) => tabId(t) === activeId)) return;
    if (active.kind === "untitled") {
      setDoc(docs.length > 0 ? tabId(docs[0]) : null);
      return;
    }
    openDoc(key, active);
  }, [key, activeId, active, docs, setDoc]);

  /**
   * Open a file — kept, or as the focused pane's preview (a glance from the
   * explorer or a search hit). The root goes to the mode that shows documents
   * first (`DOCUMENT_MODE`, the step a link takes too): a file clicked in
   * Files while the centre was the conversation or the Board must land in
   * front of the person, not behind. The store and the URL move in the same
   * synchronous step: the adoption effect above re-opens any active id the
   * store lacks as kept, so a wait between the two would bring a preview this
   * one just replaced straight back.
   */
  const openFile = (path: string, { preview = false }: { preview?: boolean } = {}) => {
    const tab: WorkbenchTab = { kind: "file", path };
    setIdeMode(key, DOCUMENT_MODE);
    openDoc(key, tab, { preview });
    setDoc(tabId(tab));
  };
  /** A device beside the code (ide/19): its document in a split of its own, or shown where it is. */
  const openDeviceDocument = (deviceId: string) => {
    const tab: WorkbenchTab = { kind: "device", id: deviceId };
    setIdeMode(key, DOCUMENT_MODE);
    openDocBeside(key, tab);
    setDoc(tabId(tab));
  };
  /** ⌘N: a new untitled document, kept, in front of the person like any document door. */
  const openUntitled = () => {
    setIdeMode(key, DOCUMENT_MODE);
    const tab = openUntitledDoc(key);
    setDoc(tabId(tab));
  };
  /**
   * The ids untitled documents became when a save named them, so a close
   * that was waiting on that save (the close guard's *Save*) follows the tab to its
   * new id instead of closing a tab that no longer exists.
   */
  const namedAs = useRef(new Map<string, string>());
  /** A document was saved somewhere new — into the root, or elsewhere on this machine: its tab becomes that one, in place, and stays the active one. */
  const nameDoc = (fromId: string, tab: WorkbenchTab) => {
    replaceDoc(key, fromId, tab);
    namedAs.current.set(fromId, tabId(tab));
    if (activeId === fromId) setDoc(tabId(tab));
  };
  /**
   * Files from this machine, dropped or picked (ide/03 §Loose files): each a
   * kept loose document here, the last one in front, in Project mode like
   * every document door.
   */
  const openLoose = (paths: string[]) => {
    if (paths.length === 0) return;
    setIdeMode(key, DOCUMENT_MODE);
    let last: WorkbenchTab | null = null;
    for (const path of paths) {
      last = { kind: "loose", path };
      openDoc(key, last);
    }
    if (last) setDoc(tabId(last));
  };
  // A file dragged from the file manager: the DOM's drop carries names and
  // bytes, the shell knows the paths (`dropModel`). The composer's own drop
  // — an attachment — has already claimed its event and is left alone.
  const onFileDragOver = (e: DragEvent<HTMLDivElement>) => {
    if (isFileDrop(e.dataTransfer.types)) e.preventDefault();
  };
  const onFileDrop = (e: DragEvent<HTMLDivElement>) => {
    if (e.isDefaultPrevented() || !isFileDrop(e.dataTransfer.types)) return;
    e.preventDefault();
    const names = Array.from(e.dataTransfer.files).map((f) => f.name);
    void droppedPaths().then((paths) => {
      const matched = pathsForDrop(names, paths);
      if (matched.length === 0) {
        toast.info(tr("screens-workbench-desktop-could-not-tell-where-file"));
        return;
      }
      openLoose(matched);
    });
  };
  /** The workstream's patch as a document, shown where documents are. */
  const openDiffDocument = () => {
    setIdeMode(key, DOCUMENT_MODE);
    openDoc(key, { kind: "diff" });
    setDoc("diff");
  };
  /**
   * One changed file's patch on one side, as a document — what selecting a
   * row in Git › Changes does: a glance, so a preview that the next row
   * replaces; an act on it keeps the tab, as for a file.
   */
  const openPatchDocument = (path: string, staged: boolean) => {
    const tab: WorkbenchTab = { kind: "patch", path, staged };
    setIdeMode(key, DOCUMENT_MODE);
    openDoc(key, tab, { preview: true });
    setDoc(tabId(tab));
  };
  /** One commit as a document — what a row's click in Git › History does; a preview, like a glance at a patch. */
  const openCommitDocument = (sha: string) => {
    const tab: WorkbenchTab = { kind: "commit", sha };
    setIdeMode(key, DOCUMENT_MODE);
    openDoc(key, tab, { preview: true });
    setDoc(tabId(tab));
  };
  /** Open a file and show a line of it — a search hit, a symbol, a definition. The line waits for the editor (`requestLine`). */
  const openAt = (path: string, line: number | null, opts: { preview?: boolean } = {}) => {
    openFile(path, opts);
    if (line !== null) requestLine(editorKey(key, `file:${path}`), line);
  };
  openAtRef.current = openAt;
  // A preview that was edited is a tab the person means to keep: the dirty
  // transition promotes it (a save passes through it too). Idempotent, so
  // running on every dirty-set change is free.
  useEffect(() => {
    for (const tid of previewIds(state, key)) if (dirty.has(editorKey(key, tid))) keepDoc(key, tid);
  }, [dirty, state, key]);

  const strip = useMemo(() => state.roots.find((r) => r.key === key)?.strip ?? [], [state, key]);
  const stripTabs = useMemo(
    () => mergedTabs(docs, sessionsRootedAt(terminals.sessions, scope, id), strip, seenRootedAt(browsers.sessions, scope, id)),
    [docs, terminals.sessions, browsers.sessions, scope, id, strip],
  );
  // The strip learns what it drew: a terminal that just opened is recorded
  // last, so a document opened after it lands after it (`reconcileStrip`).
  useEffect(() => {
    reconcileStripOrder(key, stripTabs.map(tabId));
  }, [key, stripTabs]);
  const closeNow = (was: string) => {
    // An untitled tab that a guarded save just named closes as the file it
    // became — in the same place in the strip, which this render's list
    // still shows under the old id.
    const closing = namedAs.current.get(was) ?? was;
    namedAs.current.delete(was);
    const shown = closing === was ? stripTabs : stripTabs.map((t) => (tabId(t) === was ? (parseTabId(closing) ?? t) : t));
    const next = nextActiveAfterClose(shown, closing, activeId === was ? closing : activeId);
    closeDoc(key, closing);
    setDoc(next);
  };
  // The active document is shown in its pane: the URL says which, the tree follows.
  useEffect(() => {
    if (activeId && !isLayerId(activeId)) activateDocTab(key, activeId);
  }, [key, activeId]);
  // Closing a dirty tab asks — Save, Discard, Cancel — through the strip's
  // own close path, so middle-click and Delete ask too. A pinned tab is not
  // closed at all: unpinning it is the one way, and the strip says so.
  const close = (closing: string) => {
    if (isPinned(state, key, closing)) {
      toast.info(tr("screens-workbench-pinned-unpin-close", { closing: tabLabel(parseTabId(closing)) }));
      return;
    }
    if (dirty.has(editorKey(key, closing))) {
      setGuarding({ ids: [closing], then: () => closeNow(closing) });
      return;
    }
    closeNow(closing);
  };
  const closeAll = () => {
    const dirtyIds = docs.map(tabId).filter((tid) => !isPinned(state, key, tid) && dirty.has(editorKey(key, tid)));
    const then = () => {
      closeAllDocs(key);
      setDoc(null);
    };
    if (dirtyIds.length > 0) setGuarding({ ids: dirtyIds, then });
    else then();
  };
  // *Close to the right*: the unpinned documents after this one in its pane's
  // strip; a dirty one among them asks first.
  const closeRight = (from: string) => {
    const right = tabsRightOf(state, key, from);
    const dirtyIds = right.filter((tid) => dirty.has(editorKey(key, tid)));
    const then = () => {
      closeDocsRight(key, from);
      if (activeId && right.includes(activeId)) setDoc(from);
    };
    if (dirtyIds.length > 0) setGuarding({ ids: dirtyIds, then });
    else then();
  };
  // *Close others* keeps the named tab and the pinned ones (ide/03); a dirty
  // tab among the rest asks first, the way *Close all* does.
  const closeOthers = (keep: string) => {
    const dirtyIds = docs
      .map(tabId)
      .filter((tid) => tid !== keep && !isPinned(state, key, tid) && dirty.has(editorKey(key, tid)));
    const then = () => {
      closeOtherDocs(key, keep);
      setDoc(keep);
    };
    if (dirtyIds.length > 0) setGuarding({ ids: dirtyIds, then });
    else then();
  };
  // A rename on disk — from the explorer or from outside — is followed by the
  // documents open at or under it: a dirty buffer is saved first, and one
  // that will not save keeps its tab where it was (and says why).
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type !== "file_changed" || p.kind !== "renamed" || p.scope !== scope || p.id !== id || !p.from) return;
    const from = p.from;
    const to = p.path;
    const affected = tabsUnder(docs, from).map(tabId);
    const dirtyOnes = affected.filter((tid) => dirty.has(editorKey(key, tid)));
    const follow = () => {
      const moved = retargetDocs(key, from, to);
      const active = moved.find(([was]) => tabId({ kind: "file", path: was }) === activeId);
      if (active) setDoc(tabId({ kind: "file", path: active[1] }));
    };
    if (dirtyOnes.length === 0) {
      follow();
      return;
    }
    // One at a time: a document off screen is shown to be saved.
    const saveAll = async () => {
      let clean = true;
      for (const tid of dirtyOnes) clean = (await saveDocument(editorKey(key, tid))) && clean;
      return clean;
    };
    void saveAll().then((clean) => {
      if (clean) follow();
      else toast.error(tr("screens-workbench-renamed-disk-but-has-unsaved-changes", { from: tabLabel(parseTabId(from)) }));
    });
  });

  // ⌘W and ⌘⇧T from the keymap: the active tab closes through the same
  // guarded path the strip uses; a terminal tab closes its shell.
  // The keymap's tab commands, as window events. The handlers close over
  // this render's state, so they live in a ref the listeners read; the
  // listeners themselves are registered once, not seven pairs per render.
  const tabCommands = useRef({
    closeActive: () => undefined as void,
    reopen: () => undefined as void,
    next: () => undefined as void,
    prev: () => undefined as void,
    closeOthers: () => undefined as void,
    keep: () => undefined as void,
    closeSaved: () => undefined as void,
    selectAt: (_n: number) => undefined as void,
    revealInFiles: () => undefined as void,
    revealInFinder: () => undefined as void,
    newDocument: () => undefined as void,
    openFile: () => undefined as void,
    openLoose: (_paths: string[]) => undefined as void,
  });
  tabCommands.current = {
    closeActive: () => {
      if (!activeId) return;
      const t = parseTabId(activeId);
      if (t?.kind === "terminal") requestCloseTerminal(t.key);
      else if (t?.kind === "browser") closeBrowserTab(t.key);
      else close(activeId);
    },
    reopen: () => {
      const tab = reopenLastClosedDoc(key);
      if (tab) setDoc(tabId(tab));
    },
    // Ctrl+Tab / Ctrl+Shift+Tab: the strip's order, terminals included, wrapping.
    next: () => {
      const next = cycle(stripTabs.map(tabId), activeId, 1);
      if (next) setDoc(next);
    },
    prev: () => {
      const next = cycle(stripTabs.map(tabId), activeId, -1);
      if (next) setDoc(next);
    },
    closeOthers: () => {
      if (activeId && !isLayerId(activeId)) closeOthers(activeId);
    },
    keep: () => {
      if (activeId && !isLayerId(activeId)) keepDoc(key, activeId);
    },
    // ⌘1 … ⌘9: the strip's nth tab, terminals included; nine is the last.
    selectAt: (n: number) => {
      const ids = stripTabs.map(tabId);
      const target = n === 9 ? ids[ids.length - 1] : ids[n - 1];
      if (target) setDoc(target);
    },
    // *Close saved*: every unpinned document with nothing unsaved. Nothing to
    // guard — the dirty ones are exactly what stays.
    closeSaved: () => {
      const dirtyIds = docs.map(tabId).filter((tid) => dirty.has(editorKey(key, tid)));
      const gone = new Set(savedTabs(state, key, dirtyIds));
      if (gone.size === 0) return;
      closeSavedDocs(key, dirtyIds);
      if (activeId && gone.has(activeId)) setDoc(stripTabs.map(tabId).find((tid) => !gone.has(tid)) ?? null);
    },
    // The active document, in the Files tree or in the OS file manager.
    revealInFiles: () => {
      if (active?.kind !== "file") return;
      showRightPanel("files", key);
      requestReveal(key, active.path);
    },
    revealInFinder: () => {
      if (active?.kind !== "file" || !placement.data?.path) return;
      void revealPath(joinPath(placement.data.path, active.path)).catch((e: unknown) => toast.error(failureText("view", "workbench-failed", e)));
    },
    newDocument: openUntitled,
    // ⌘O: the OS's open dialog, then the same door a drop takes.
    openFile: () => {
      void pickFiles(tr("screens-workbench-open-file-from-machine")).then(openLoose);
    },
    openLoose,
  };
  useEffect(() => {
    const on: [string, keyof typeof tabCommands.current][] = [
      [CLOSE_ACTIVE_TAB, "closeActive"],
      [REOPEN_TAB, "reopen"],
      [NEXT_TAB, "next"],
      [PREV_TAB, "prev"],
      [CLOSE_OTHER_TABS, "closeOthers"],
      [KEEP_TAB, "keep"],
      [CLOSE_SAVED_TABS, "closeSaved"],
      [REVEAL_ACTIVE_IN_FILES, "revealInFiles"],
      [REVEAL_ACTIVE_IN_FINDER, "revealInFinder"],
      [NEW_DOCUMENT, "newDocument"],
      [OPEN_FILE, "openFile"],
    ];
    // Every one a door (`shortcuts.onDoor`): a listener the window is given
    // by hand is one `fire` never dispatches to.
    const offs = [
      ...on.map(([event, name]) => onDoor(event, () => (tabCommands.current[name] as () => void)())),
      onDoor(SELECT_TAB_AT, (n) => tabCommands.current.selectAt(Number(n))),
      // A link card's *Open in the IDE* on a path outside every root: the paths ride in `detail`.
      onDoor(OPEN_LOOSE, (detail) => {
        const paths = (detail as { paths?: unknown } | undefined)?.paths;
        if (Array.isArray(paths)) tabCommands.current.openLoose(paths.filter((p): p is string => typeof p === "string"));
      }),
    ];
    return () => {
      for (const off of offs) off();
    };
  }, []);
  // A document off screen is brought on screen to be saved (`saveDocument`):
  // its own editor is the one place that names an untitled document, merges
  // a conflict and says why a save failed. This root answers for its tabs.
  const revealRef = useRef<(registryKey: string) => boolean>(() => false);
  revealRef.current = (registryKey) => {
    const tid = docs.map(tabId).find((t) => editorKey(key, t) === registryKey);
    if (!tid) return false;
    setIdeMode(key, DOCUMENT_MODE);
    setDoc(tid);
    return true;
  };
  useEffect(() => setRevealer((registryKey) => revealRef.current(registryKey)), []);
  const guardCopy = guarding ? guardWords(guarding.ids.map((tid) => ({ label: tabLabel(parseTabId(tid)), untitled: parseTabId(tid)?.kind === "untitled" }))) : null;
  const saveGuarded = async () => {
    if (!guarding) return;
    const { ids, then } = guarding;
    // The question is answered: it must not stand over the document it shows.
    setGuarding(null);
    for (const tid of ids) {
      // A conflict or a failure: the tab stays, on screen, and says why.
      if (!(await saveDocument(editorKey(key, tid)))) return;
    }
    then();
  };

  // Closing the window with unsaved text asks once.
  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (anyDirty()) e.preventDefault();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, []);

  // Session restore: the saved layout opens this root's documents once, when
  // the root has none open yet; every change is saved back on a debounce.
  /**
   * The root whose saved layout has been read (or found absent) — saving is
   * allowed from then on, and the landing is drawn from then on: state, not
   * a ref, so the centre knows the layout is still on its way and draws its
   * empty ground rather than the landing for the beat before the documents
   * come back.
   */
  const [restoredKey, setRestoredKey] = useState<string | null>(null);
  const lastSaved = useRef<unknown>(null);
  useEffect(() => {
    // Saving waits for the answer: unblocking it now would let a 500 ms
    // debounce write an empty layout over the one still on its way.
    setRestoredKey(null);
    lastSaved.current = null;
    if (state.roots.some((r) => r.key === key && r.tabs.length > 0)) {
      setRestoredKey(key);
      return;
    }
    let alive = true;
    void api
      .ideLayout(scope, id)
      .then(({ layout }) => {
        if (!alive) return;
        const saved = restoreLayout(layout);
        if (saved) {
          restoreDocs(key, saved.tabs, saved.panes, saved.pinned, saved.strip);
          if (!docParam && saved.active && saved.tabs.some((t) => tabId(t) === saved.active)) {
            setDoc(saved.active);
          }
          lastSaved.current = serializeLayout(saved.tabs, saved.active, saved.panes, saved.pinned, [], saved.strip);
        }
      })
      .catch((e: unknown) => {
        // No saved layout for this root yet: an empty workbench is the answer.
        log.debug("workbench", "the saved layout could not be read; an empty workbench is the answer", { scope, id, key, ...errorFields(e) });
      })
      .finally(() => {
        if (alive) setRestoredKey(key);
      });
    return () => {
      alive = false;
    };
    // Once per root: `state.roots` and the address are read as they stand
    // on arrival, and following them would restore the saved layout over
    // every tab the person then opened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  const entry = state.roots.find((r) => r.key === key);
  const panes = panesFor(state, key);
  const pinned = useMemo(() => entry?.pinned ?? [], [entry?.pinned]);
  const previews = useMemo(() => Object.values(entry?.preview ?? {}), [entry?.preview]);
  useEffect(() => {
    if (restoredKey !== key) return;
    // Previews are left out, so a glance through files writes nothing.
    const layout = serializeLayout(docs, activeId, panes, pinned, previews, strip);
    if (sameLayout(layout, lastSaved.current)) return;
    const t = window.setTimeout(() => {
      lastSaved.current = layout;
      api.ideSaveLayout(scope, id, layout).catch((e: unknown) => {
        // Furniture: a save that does not land costs a restore, never the work.
        log.debug("workbench", "the layout could not be saved; it costs a restore, never the work", { scope, id, key, ...errorFields(e) });
      });
    }, 500);
    return () => window.clearTimeout(t);
  }, [docs, activeId, scope, id, key, restoredKey, panes, pinned, previews, strip]);

  /**
   * Documents whose files were deleted close together, through one guard:
   * the dirty ones among them ask once, and the active one hands over to the
   * first survivor. A pinned tab stays, as it does for every close.
   */
  const closeDeleted = (paths: string[]) => {
    const ids = [...new Set(paths.flatMap((p) => tabsUnder(docs, p).map(tabId)))].filter((tid) => !isPinned(state, key, tid));
    if (ids.length === 0) return;
    const gone = new Set(ids);
    const then = () => {
      for (const tid of ids) closeDoc(key, tid);
      if (activeId && gone.has(activeId)) setDoc(stripTabs.map(tabId).find((tid) => !gone.has(tid)) ?? null);
    };
    const dirtyIds = ids.filter((tid) => dirty.has(editorKey(key, tid)));
    if (dirtyIds.length > 0) setGuarding({ ids: dirtyIds, then });
    else then();
  };

  const rootLabel =
    scope === "work_item"
      ? (located.data?.item.instructions.split("\n")[0]?.slice(0, 60) ?? tr("screens-workbench-work-item-2", { slice: id.slice(-6) }))
      : scope === "workstream" && detail.data
        ? isPrimary
          ? detail.data.project.name
          : (detail.data.workstream.name ?? detail.data.branch ?? tr("screens-workbench-workstream", { slice: id.slice(-6) }))
        : `${SCOPE_NOUN[scope]} ${id.slice(-6)}`;
  const exists = placement.data?.exists ?? false;
  const photo = detail.data?.project.photo;
  const photoThumb = usePhotoThumb(photo?.sha256 ?? null);
  // The project rail's open flag is a store (`projectRailStore`): the chord
  // and the top bar's toggle both move it.
  const railOpen = useProjectRail().open;
  // The columns share the row's measured width (`ideColumnsModel`): the side
  // columns give way to the centre — the right panel first, then the rail,
  // which folds when even its least leaves the centre too little.
  const row = useRef<HTMLDivElement>(null);
  const [rowWidth, setRowWidth] = useState(0);
  useLayoutEffect(() => {
    const el = row.current;
    if (!el) return;
    setRowWidth(el.getBoundingClientRect().width);
    const ro = new ResizeObserver((entries) => setRowWidth(entries[0]?.contentRect.width ?? 0));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  // The occupant rail's column (`IconRail`, w-10) never moves.
  const fit = fitColumns({ total: rowWidth, fixed: 40, rail: railWidth, railMin: RAIL_MIN, railOpen, right: rightWidth, rightMin: RIGHT_MIN, rightOpen: right.open });
  useEffect(() => setProjectRailFolded(fit.railFolded), [fit.railFolded]);
  useEffect(() => () => setProjectRailFolded(false), []);
  // The rail and the right panel move the centre without resizing the window.
  useEffect(() => notifyLayoutChanged(), [fit.rail, fit.right]);

  const renderDoc = (tab: WorkbenchTab) => {
    // A PDF, a picture, a recording, a sheet, a document, a deck: drawn as
    // they are, never asked of the editor's text route (ide/03).
    if (tab.kind === "file" && isRenderedDoc(docKindOf(tab.path))) {
      return (
        <RenderedFileDoc
          key={`${key}:${tab.path}`}
          scope={scope}
          id={id}
          path={tab.path}
          onRevealInFiles={(p) => {
            showRightPanel("files", key);
            requestReveal(key, p);
          }}
          onReveal={(p) => {
            if (!placement.data?.path) return;
            void revealPath(joinPath(placement.data.path, p)).catch((e: unknown) => toast.error(failureText("view", "workbench-failed", e)));
          }}
          className="h-full"
        />
      );
    }
    // A device mirrored beside the code (ide/19): a checkout's alone, since
    // the app it runs is the checkout's.
    if (tab.kind === "device" && scope === "workstream") {
      return <DeviceDoc key={`${key}:${tabId(tab)}`} wid={id} pid={project} deviceId={tab.id} />;
    }
    if (tab.kind === "untitled") {
      return (
        <EditorDoc
          key={`${key}:${tabId(tab)}`}
          scope={scope}
          id={id}
          source={tab}
          project={scope === "workstream" ? project : null}
          registryKey={editorKey(key, tabId(tab))}
          onOpenFile={openFile}
          onNamed={(next) => nameDoc(tabId(tab), next)}
          className="h-full"
        />
      );
    }
    if (tab.kind === "loose") {
      return (
        <EditorDoc
          key={`${key}:${tabId(tab)}`}
          scope={scope}
          id={id}
          source={tab}
          project={scope === "workstream" ? project : null}
          registryKey={editorKey(key, tabId(tab))}
          onOpenFile={openFile}
          onNamed={(next) => nameDoc(tabId(tab), next)}
          className="h-full"
        />
      );
    }
    if (tab.kind === "file") {
      return (
        <EditorDoc
          key={`${key}:${tab.path}`}
          scope={scope}
          id={id}
          source={tab}
          project={scope === "workstream" ? project : null}
          registryKey={editorKey(key, tabId(tab))}
          onOpenFile={openFile}
          onRevealInFiles={(p) => {
            showRightPanel("files", key);
            requestReveal(key, p);
          }}
          onReveal={(p) => {
            if (!placement.data?.path) return;
            void revealPath(joinPath(placement.data.path, p)).catch((e: unknown) => toast.error(failureText("view", "workbench-failed", e)));
          }}
          className="h-full"
        />
      );
    }
    if (tab.kind === "diff" && scope === "workstream" && !isPrimary) return <WorkstreamDiffDocument wid={id} />;
    if (tab.kind === "patch" && scope === "workstream") {
      return <PatchDocument key={tabId(tab)} wid={id} path={tab.path} staged={tab.staged} onOpenFile={(p) => openFile(p)} onOpenPatch={openPatchDocument} onOpenCommit={openCommitDocument} />;
    }
    if (tab.kind === "commit" && scope === "workstream") return <CommitDocument key={tabId(tab)} wid={id} sha={tab.sha} />;
    if (tab.kind === "artifact") {
      return <ArtifactDocument key={tabId(tab)} message={tab.message} ordinal={tab.ordinal} />;
    }
    if (tab.kind === "transcript") return <TranscriptDocument key={tabId(tab)} session={tab.session} />;
    return (
      <EmptyState
        title={tr("screens-workbench-nothing-show-tab-here")}
        hint={tr("screens-workbench-document-belongs-another-kind-root")}
        action={
          <Button size="sm" variant="ghost" onClick={() => setDoc(null)}>{tr("screens-workbench-back-landing")}</Button>
        }
      />
    );
  };

  /** How many goals the project is attached to, for the chip's "+N". */
  const attachedCount = ws.projects.find((r) => r.project.id === project)?.goals.length ?? 0;
  const available = availableOccupants({ scope, hasProject: scope === "workstream" && !!project, centre: conversationCentre ? "conversation" : "documents" });
  const shown = resolveOccupant(right.tab, available);
  // The rail's marks: what the Git and Workstreams tabs say about this
  // checkout while their columns are closed — from the shared status, the
  // git act in flight, and an agent review or fix run kept for this scope.
  const status = useWorkstreamStatus(scope === "workstream" ? id : null);
  const gitSession = useGitSession(key);
  const sessions = useSessions();
  const [reviewDraft] = useSessionDraft<ReviewRun | null>(draftKeys(key).run, null);
  const badges = useMemo(
    () => ({
      git: gitBadge(status),
      workstreams: workstreamsBadge({
        kind: detail.data?.workstream.kind.kind ?? null,
        pr: status?.pr ?? null,
        busy: gitSession.busy ?? null,
        run: reviewRun(sessions, id, reviewDraft, null),
      }),
    }),
    [status, detail.data, gitSession.busy, sessions, id, reviewDraft],
  );
  // A door that wanted the Agent occupant while the centre already is the
  // conversation: the centre answers — the caret goes to the composer of the
  // conversation this root is on, which is what the composer registers under
  // (the root's key names the owner, `ownerKey`) — and the panel settles on
  // what it can show, remembered for this root. Nothing picked: no caret to
  // move; the centre shows the list or the empty state.
  const [pickedConversation] = useRememberedPick(key);
  useEffect(() => {
    if (!conversationCentre || right.tab !== "agents") return;
    if (pickedConversation) focusComposer(pickedConversation);
    settleOccupant(shown, key);
  }, [conversationCentre, right.tab, shown, key, pickedConversation]);

  return (
    <div className="flex h-full min-h-0 flex-col" onDragOver={onFileDragOver} onDrop={onFileDrop}>
      {/* The root, what it is, and the things you do to it from anywhere. */}
      {/* An `@container`: below `@5xl` the mode, Quick open and the launchers
          fold to their glyphs (their words stay their names), so the root's
          name keeps its room on a narrow window. */}
      <header className="@container flex h-11 shrink-0 items-center gap-2 border-b border-hairline px-3">
        {/* The project rail's toggle first, on the side it moves; then the root. */}
        <ProjectRailToggle open={railOpen && !fit.railFolded} />
        {scope === "workstream" && project ? (
          <Avatar
            id={project}
            name={detail.data?.project.name ?? ""}
            url={photoThumb}
            size={18}
          />
        ) : (
          <ICON.workItem size={14} aria-hidden className="shrink-0 text-text-dim" />
        )}
        {scope === "workstream" && detail.data && !isPrimary && (
          <>
            <span className="min-w-0 truncate text-xs text-text-dim">{detail.data.project.name}</span>
            <ICON.collapsed size={12} aria-hidden className="shrink-0 text-text-dim" />
          </>
        )}
        {/* The root's name holds a third of the line before it truncates: the
            project's name, the branch and the chips give way first. */}
        <h2 className="max-w-1/3 shrink-0 truncate text-xs font-semibold" title={rootLabel}>{rootLabel}</h2>
        {scope === "workstream" && detail.data?.branch && (
          <Chip title={detail.data.branch} className="min-w-0 max-w-48">
            <span className="min-w-0 truncate">{detail.data.branch}</span>
          </Chip>
        )}
        {scope === "workstream" && owner.data && (
          <Tooltip label={tr("screens-workbench-attached-more-open-goal", { owner: goals.find((g) => g.id === owner.data)?.label ?? tr("screens-workbench-goal"), attachedCount: attachedCount - 1, flag: (attachedCount > 1) ? "yes" : "no" })}>
            <a href={`#/goals/${owner.data}`} className="inline-flex min-w-0">
              <Chip icon={ICON.goal} className="anim hover:bg-selected hover:text-text">
                <span className="max-w-40 truncate">{goals.find((g) => g.id === owner.data)?.label ?? tr("screens-workbench-goal-chip")}</span>
              </Chip>
            </a>
          </Tooltip>
        )}
        {isPrimary && (
          <Tooltip label={tr("screens-workbench-primary-workstream-project-s-own-root")}>
            <span>
              <Chip>{tr("screens-workbench-primary")}</Chip>
            </span>
          </Tooltip>
        )}
        {scope === "workstream" && identity.data && identityState(identity.data) === "missing" && (
          <Tooltip label={identity.data?.suggested ? tr("screens-workbench-nobody-set-commit-repository-about-checkout", { login: identity.data.suggested.login }) : tr("screens-workbench-nobody-set-commit-repository-set-who")}>
            <button
              type="button"
              className="anim inline-flex hover:opacity-80"
              onClick={() => {
                openPanelView("about", "checkout", key);
              }}
            >
              <Chip tone="warn" icon={ICON.warn}>{tr("screens-workbench-no-committer")}</Chip>
            </button>
          </Tooltip>
        )}
        {scope === "workstream" && hasCautions(connection.data) && (
          <Tooltip label={firstCaution(connection.data) ?? tr("screens-workbench-connection-has-caution-see-about-checkout")}>
            <button
              type="button"
              className="anim inline-flex hover:opacity-80"
              onClick={() => {
                openPanelView("about", "checkout", key);
              }}
            >
              <Chip tone="warn" icon={ICON.warn}>
                {tr("screens-workbench-cautions", { cautions: connection.data!.cautions.length })}
              </Chip>
            </button>
          </Tooltip>
        )}
        <span className="flex-1" />
        {/* The centre's mode — a value, not a place: Project (documents and
            terminals), Agent (the conversation) or Board (every workstream's
            card). Remembered per root; the Board offered while its setting is on.
            Two drawings of one value: the words from `@5xl`, the glyphs below
            it (each word its glyph's name and tooltip). The one not shown is
            `display: none`, so a screen reader meets one group. */}
        {scope === "workstream" && project && (
          <>
            <SegmentedControl
              label={tr("screens-workbench-centre-s-mode")}
              size="sm"
              className="hidden @5xl:inline-flex"
              value={mode}
              onChange={(v) => setIdeMode(key, v as IdeMode)}
              options={modeSegments(boardEnabled).map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon] }))}
            />
            <SegmentedControl
              label={tr("screens-workbench-centre-s-mode")}
              size="sm"
              iconOnly
              className="@5xl:hidden"
              value={mode}
              onChange={(v) => setIdeMode(key, v as IdeMode)}
              options={modeSegments(boardEnabled).map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon] }))}
            />
          </>
        )}
        <Button size="sm" onClick={openPlaces}>
          <ICON.search size={12} aria-hidden />
          <span className={FOLDED_WORD}>{tr("screens-workbench-quick-open")}</span>
          <CommandHint id="quick_open" />
        </Button>
        <TerminalLauncher
          scope={scope}
          id={id}
          project={scope === "workstream" ? project : null}
          label={rootLabel}
          disabledReason={placement.loading || exists ? null : tr("screens-workbench-folder-not-disk-yet")}
          wordClassName={FOLDED_WORD}
        />
        {/* Browser (ide/18): a tab here; its menu runs or serves the checkout. */}
        <BrowserLauncher scope={scope} id={id} rootLabel={rootLabel} wordClassName={FOLDED_WORD} disabledReason={scope === "workstream" && !(placement.loading || exists) ? tr("screens-workbench-folder-not-disk-yet") : null} />
        {/* Devices (ide/19): a Flutter app on a simulator, an emulator or a phone, beside the code — a checkout's alone. */}
        {scope === "workstream" && (
          <DeviceLauncher wid={id} label={rootLabel} wordClassName={FOLDED_WORD} openDevices={docs.filter((t) => t.kind === "device").map((t) => t.id)} onOpenDevice={openDeviceDocument} disabledReason={placement.loading || exists ? null : tr("screens-workbench-folder-not-disk-yet")} />
        )}
        {/* The right panel's toggle last, over its rail's column; every
            occupant is a tab on that rail. */}
        <RightPanelToggle open={right.open} />
      </header>

      <div ref={row} className="flex min-h-0 flex-1">
        {fit.rail !== null && (
          <>
            <aside className="flex min-h-0 shrink-0 flex-col border-r border-border" style={{ width: fit.rail }} aria-label={tr("screens-workbench-projects")}>
              <ProjectRail current={{ scope, id }} />
            </aside>
            <ResizeHandle side="right" size={fit.rail} min={RAIL_MIN} max={RAIL_MAX} defaultSize={RAIL_DEFAULT} onSize={setRailWidth} label={tr("screens-workbench-resize-rail")} />
          </>
        )}

        {boardCentre ? (
          <BoardCenter place={BOARD_PLACE} />
        ) : conversationCentre && project ? (
          <AgentModeCenter wid={id} pid={project} title={rootLabel} branch={detail.data?.branch ?? null} activeFile={active?.kind === "file" ? active.path : null} />
        ) : (
        <CenterDocuments
          scope={scope}
          id={id}
          docs={docs}
          activeId={activeId}
          panes={panes}
          pinned={pinned}
          previews={previews}
          onSelect={(tid) => setDoc(tid)}
          onKeep={(tid) => keepDoc(key, tid)}
          onCloseDoc={close}
          onCloseAll={closeAll}
          onCloseOthers={closeOthers}
          onCloseRight={closeRight}
          onCloseSaved={() => tabCommands.current.closeSaved()}
          onReorderDoc={(tid, index, shown) => {
            const refused = moveDocTab(key, tid, index, shown);
            if (refused) toast.info(refused);
          }}
          strip={strip}
          rootPath={placement.data?.path ?? null}
          onRevealInFiles={(path) => {
            showRightPanel("files", key);
            requestReveal(key, path);
          }}
          onSplit={(dir, leafId) => splitDocs(key, dir, leafId)}
          onClosePane={(leafId) => closeDocsPane(key, leafId)}
          onMoveDoc={(tid, leafId) => moveDoc(key, tid, leafId)}
          onFocusPane={(leafId) => focusDocs(key, leafId)}
          onRatio={(splitId, ratio) => setDocsPaneRatio(key, splitId, ratio)}
          onTogglePin={(tid) => togglePinnedDoc(key, tid)}
          project={scope === "workstream" ? project : null}
          renderDoc={renderDoc}
          // The landing waits for the saved layout: until it has been read the
          // centre draws its empty ground, never a card that the documents
          // then replace.
          landing={
            restoredKey !== key ? null : <CenterLanding
              title={rootLabel}
              subtitle={
                isPrimary
                  ? tr("screens-workbench-project-s-own-root-work-here")
                  : scope === "workstream"
                    ? tr("screens-workbench-workstream-own-checkout-own-branch-own")
                    : null
              }
              path={placement.data?.path ?? null}
              onOpenFile={openPlaces}
              onNewTerminal={exists ? () => openTerminalIn({ scope, id, label: rootLabel }) : null}
              onAgents={scope === "workstream" && project ? () => setIdeMode(key, "agent") : null}
              onNewWorkstream={isPrimary && project ? () => fire(NEW_WORKSTREAM) : null}
            />
          }
        />
        )}

        {fit.right !== null && (
          <>
            <ResizeHandle side="left" size={fit.right} min={RIGHT_MIN} max={RIGHT_MAX} defaultSize={RIGHT_DEFAULT} onSize={setRightWidth} label={tr("screens-workbench-resize-right-panel")} />
            <div className="flex min-h-0 shrink-0 flex-col border-l border-border" style={{ width: fit.right }}>
              <RightPanel
                occupant={shown}
                scope={scope}
                id={id}
                project={project}
                isPrimary={isPrimary}
                rootLabel={rootLabel}
                goals={goals}
                owner={owner.data ?? null}
                exists={exists}
                path={placement.data?.path ?? null}
                activeFile={active?.kind === "file" ? active.path : null}
                openPath={active?.kind === "file" ? active.path : null}
                onOpenFile={openFile}
                onOpenAt={openAt}
                onFileDeleted={closeDeleted}
                onOpenWorkstream={(wid) => {
                  // A checkout you just opened or picked lands on its own
                  // panel: remember Workstreams for that root before going.
                  showRightPanel("workstreams", rootKey("workstream", wid));
                  navigate({ name: "workbench", scope: "workstream", id: wid });
                }}
                onOpenDiffDocument={scope === "workstream" && !isPrimary ? openDiffDocument : undefined}
                onOpenPatch={openPatchDocument}
                onOpenCommit={openCommitDocument}
                // Archived or removed from About: the door has forgotten what
                // went with the roots; this root is left for the home it named.
                onLeft={(home) => replace(homeRoute(home))}
                onLeftWorkstream={() => {
                  forgetWorkbenchRoot(key);
                  navigate(project ? { name: "workbench", scope: "workstream", id: project } : { name: "projects" });
                }}
              />
            </div>
          </>
        )}
        {/* The rail stays whether or not the column shows: every
            occupant this root can show, one click away. */}
        <OccupantRail available={available} shown={shown} open={right.open} onPress={pressOccupant} badges={badges} />
      </div>
      {/* The one door to a new workstream: every `NEW_WORKSTREAM` lands here, a
          door with no project meaning the current root's. */}
      <NewWorkstreamDoor defaultPid={scope === "workstream" ? project : null} />
      <UnsavedDialog
        open={guarding !== null}
        words={guardCopy}
        onCancel={() => setGuarding(null)}
        onDiscard={() => {
          guarding?.then();
          setGuarding(null);
        }}
        onSave={() => void saveGuarded()}
      />
    </div>
  );
}

/** The workstream's patch, as a document — the widest thing in the app. */
function WorkstreamDiffDocument({ wid }: { wid: string }) {
  const diff = useAsync((s) => api.workstreamDiff(wid, s), [wid]);
  if (diff.error) return <ErrorNote error={diff.error} retry={diff.reload} />;
  if (!diff.data) return <SkeletonRows rows={8} className="p-4" />;
  if (!diff.data.diff.trim()) {
    return (
      <EmptyState
        icon={ICON.workstream}
        title={tr("screens-workbench-nothing-uncommitted")}
        hint={tr("screens-workbench-checkout-matches-base")}
        action={
          <Button size="sm" variant="ghost" onClick={diff.reload}>{tr("screens-workbench-check-again")}</Button>
        }
      />
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col p-4">
      <DiffView diff={diff.data.diff} className="min-h-0 flex-1 overflow-auto rounded-control border border-border bg-surface-2" />
    </div>
  );
}


/**
 * `#/projects`: straight into the IDE (D9). The last workstream you were in,
 * else the first project's primary — and, with no project at all, the rail
 * and the way in.
 */
export function ProjectsHome() {
  const ws = useWorkspace();
  const [creating, setCreating] = useState(false);
  const [railWidth, setRailWidth] = useStoredSize(RAIL_KEY, RAIL_DEFAULT);
  useEffect(() => {
    if (!ws.ready) return;
    const home = homeRoot(lastRoot(), ws.projects, ws.workstreams);
    if (home) replace({ name: "workbench", scope: home.scope, id: home.id });
  }, [ws.ready, ws.projects, ws.workstreams]);
  if (!ws.ready) return <SkeletonRows rows={6} className="p-4" />;
  return (
    <div className="flex h-full min-h-0">
      <aside className="flex min-h-0 shrink-0 flex-col border-r border-border" style={{ width: railWidth }} aria-label={tr("screens-workbench-projects")}>
        <ProjectRail current={null} />
      </aside>
      <ResizeHandle side="right" size={railWidth} min={RAIL_MIN} max={RAIL_MAX} defaultSize={RAIL_DEFAULT} onSize={setRailWidth} label={tr("screens-workbench-resize-rail")} />
      <div className="min-w-0 flex-1">
        <EmptyIdeLanding onNewProject={() => setCreating(true)} />
      </div>
      {/* The rail's `+` and menus fire `NEW_WORKSTREAM` here as on the workbench. */}
      <NewWorkstreamDoor defaultPid={null} />
      <NewProjectDialog
        open={creating}
        onClose={() => setCreating(false)}
        onCreated={(created) => {
          ws.refresh();
          navigate({ name: "workbench", scope: "workstream", id: created.project.id });
        }}
      />
    </div>
  );
}
