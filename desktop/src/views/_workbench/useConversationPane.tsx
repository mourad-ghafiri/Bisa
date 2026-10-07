/**
 * Everything the IDE's Agent pane is wired to, in one hook (ide/09, 13 —
 * Conversations). The **surface** — the project's conversations, every
 * checkout's, the one this checkout is on, the list to pick one up or start
 * another, *New conversation* — is every owner's (`useConversationSurface`,
 * with the IDE's source and its `root` selection: the pick remembered per
 * root, the fold too, a link's `?conversation=` consumed on arrival). What
 * this hook adds is the **thread's wiring**, the IDE's own: the chips a
 * person attached and the ways to attach more, the `@` file index, who an
 * unaddressed message reaches, what the conversation's turns add up to and
 * the one to stop (beside *Send*, and nowhere else in the chat), what the
 * agents left on disk and the door to attach it, the frame the engine
 * states, the mode and the change ledger. Two painters draw from it — the
 * Agent occupant in the right panel (`AgentPane`) and the centre in Agent
 * Mode (`AgentModeCenter`) — so a rule about the thread lives once and the
 * two can never disagree.
 *
 * The thread's scope is the conversation's id; the chips and the draft are
 * kept per checkout under `workstream:<wid>` (`agentPaneStore.ts`) — what
 * you attached while looking at this checkout goes with the next message
 * here, whichever conversation that is. The list's search and its
 * live-or-archived switch are the root's view (`idePlace`): a tab switch,
 * leaving the IDE and a restart come back to the list as it was narrowed,
 * and the root's memory takes them along when it goes.
 */

import { terminalChipName } from "../../shell/terminalChipModel.mjs";
import { type KeyboardEvent as ReactKeyboardEvent, type ReactNode, useCallback, useEffect, useMemo } from "react";
import { api } from "../../api";
import type { ConversationMode } from "../../types";
import { useEngineEvents } from "../../bus";
import { setSearch, useSearchValue } from "../../router";
import { stopSession, useSessions } from "../../shell/sessionsStore";
import { stopWords } from "../../shell/sessionRosterModel.mjs";
import { usePathIndex } from "../../shell/pathIndexStore";
import { Button, ICON, Menu, failureText, focusComposer, isDragOf, useLinkHandler, useToast } from "../../ui";
import type { ComposerFiles, DragData, LinkRootRef, MenuItem } from "../../ui";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { terminalTail } from "../../terminal/tails";
import { useTerminals } from "../../shell/useTerminals";
import { useAsync } from "../_work/useAsync";
import { activeEditor } from "./editorRegistry";
import { attachContext, clearContext, removeContext, removeContextRef, useAgentPane } from "./agentPaneStore";
import { attachGitChanges } from "./gitContext";
import { fileChip, frameChips, hunkChip, selectionChip, terminalChip } from "./contextChips.mjs";
import { GENERAL_AGENT } from "./editorAgentModel.mjs";
import { addressee } from "./agentRailModel.mjs";
import { reachableIn } from "../_studio/addressModel.mjs";
import { useConversationSurface } from "../_studio/useConversationSurface";
import { BUILD_MESSAGE, isCheckoutOrigin, modeOf, nextMode, planBannerWords } from "../_studio/conversationModeModel.mjs";
import { commandForEvent } from "../../shell/keymapModel.mjs";
import { contexts, currentKeymap } from "../../shell/shortcuts";
import { isMac } from "../../ui/KeyHint";
import { ConversationModePicker } from "../_studio/ConversationModePicker";
import { AskCard } from "../_studio/AskCard";
import { TurnChangesCard } from "../_studio/TurnChangesCard";
import { TIMELINE_END, turnCardsByAnchor } from "../_studio/turnChangesModel.mjs";
import { reviewOpenDrafts } from "./reviewLensModel.mjs";
import { writeSessionDraft } from "../_work/gitPanelStore";
import { keepDocMode } from "./docViewStore";
import { sessionsOf, turnSummary } from "./conversationPaneModel.mjs";
import { showRightPanel } from "./rightPanelStore";
import { useWorkbench } from "./workbenchStore";
import { rootKey, tabsFor } from "./workbenchModel.mjs";
import { idePlace } from "./idePlacesModel.mjs";
import { useCoalesced } from "./useCoalesced";
import { useConversationAsks } from "./asksStore";
import { useConversationChanges, usePublishReviewChanges } from "./reviewStore";
import { t as tr } from "../../i18n/l10n.mjs";

/** How many open tabs the attach menu lists before it stops. */
const OPEN_TAB_ITEMS = 6;

function basename(path: string): string {
  return path.split("/").pop() || path;
}

/** *3 files changed* — the working tree's tally, for the attach menu's item. */
function changedLabel(n: number): string {
  return tr("workbench-use-conversation-pane-file-files-changed", { n });
}

export function useConversationPane(wid: string, pid: string, activeFile: string | null) {
  const toast = useToast();
  const ws = useWorkspace();
  /** The checkout's key: what its chips and its draft are kept under, and what remembers its conversation and its view. */
  const scopeKey = rootKey("workstream", wid);
  const { context } = useAgentPane(scopeKey);
  const project = useAsync((s) => api.project(pid, s), [pid]);
  const settings = useAsync((s) => api.settingsResolved(pid, s), [pid]);
  const sessionRows = useSessions();
  const { sessions: terminals, active: activeTerminal } = useTerminals();
  const workbench = useWorkbench();
  const index = usePathIndex("workstream", wid);

  // The surface: the project's conversations — every checkout's — the one
  // this checkout is on (remembered per root; a `?conversation=` link taken
  // in on arrival), the list and *New conversation*, which is about this
  // checkout. Its search and switch live under the root's place.
  const surface = useConversationSurface({ owner: { kind: "workstream", id: wid, project: pid }, project: pid, place: idePlace(scopeKey) }, "root");
  const current = surface.selected;
  const currentId = current?.id ?? null;

  // `?panel=agents` opens the panel, read once and taken off the address
  // with no history entry.
  const [panel] = useSearchValue("panel");
  useEffect(() => {
    if (panel === "agents") {
      showRightPanel("agents", scopeKey);
      setSearch({ panel: null }, { replace: true });
    }
  }, [panel, scopeKey]);

  const attachedGoals = useMemo(() => project.data?.goals ?? [], [project.data]);
  const goalLabel = (id: string) => {
    const g = ws.goals.find((x) => x.id === id);
    return g?.title ?? g?.statement?.split("\n")[0]?.slice(0, 60) ?? id.slice(-6);
  };
  /** The placement the engine states to the agent, word for word: the project, then its goals. */
  const frame = useMemo(
    () =>
      frameChips(
        project.data ? { name: project.data.project.name, slug: project.data.project.slug } : null,
        attachedGoals.map((id) => ({ id, label: goalLabel(id) })),
      ),
    // `goalLabel` is a plain function remade every render; the chips follow
    // the fact it reads, `ws.goals`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [project.data, attachedGoals, ws.goals],
  );
  const frameWords = frame.map((f) => (f.kind === "goal" ? `↳ ${f.label}` : f.label)).join(" · ");

  const setting = (k: string) => settings.data?.settings.find((x) => x.key === k)?.value;
  const terminalLines = typeof setting("agents.context.terminal_lines") === "number" ? (setting("agents.context.terminal_lines") as number) : undefined;
  const selectionLines = typeof setting("agents.context.selection_max_lines") === "number" ? (setting("agents.context.selection_max_lines") as number) : undefined;

  // The conversation's turns, and what they add up to. One source: the
  // summary's words and *Stop* both read these rows, and *Stop* names the
  // loudest stoppable one — none while every turn is idle.
  const rows = useMemo(() => sessionsOf(sessionRows, currentId), [sessionRows, currentId]);
  const summary = useMemo(() => turnSummary(rows), [rows]);
  const stoppable = summary.stoppable;

  // What the agents left on disk. Read from the checkout's own status, and
  // re-read when the watcher says the tree moved — a save writes a temp file
  // and renames it, so a burst is one read.
  const status = useAsync((s) => api.workstreamStatus(wid, s), [wid]);
  const reloadStatus = useCoalesced(status.reload);
  useEngineEvents((e) => {
    const p = e.payload;
    const moved =
      (p.type === "file_changed" && p.scope === "workstream" && p.id === wid) ||
      ((p.type === "workstream_changed" || p.type === "workstream_committed" || p.type === "workstream_edited") && p.workstream === wid);
    if (moved) reloadStatus();
  });
  const live = status.data?.status;
  const changedFiles = live ? live.staged + live.unstaged + live.untracked : 0;

  const attach = (ref: ReturnType<typeof fileChip>) => attachContext(ref, scopeKey);

  // The open documents of this workbench, minus the one on screen (it has its
  // own item), for "attach an open tab".
  const openTabs = useMemo(
    () =>
      tabsFor(workbench, scopeKey)
        .filter((t): t is { kind: "file"; path: string } => t.kind === "file" && t.path !== activeFile)
        .slice(0, OPEN_TAB_ITEMS),
    [workbench, scopeKey, activeFile],
  );

  const attachItems: MenuItem[] = [
    {
      label: activeFile ? tr("workbench-use-conversation-pane-open-file", { activeFile: basename(activeFile) }) : tr("workbench-use-conversation-pane-open-file-2"),
      icon: ICON.file,
      disabled: !activeFile,
      onSelect: () => activeFile && attach(fileChip(activeFile)),
    },
    ...openTabs.map((t) => ({ label: tr("workbench-use-conversation-pane-open-tab", { t: basename(t.path) }), icon: ICON.file, onSelect: () => attach(fileChip(t.path)) })),
    {
      label: tr("workbench-use-conversation-pane-editor-s-selection"),
      icon: ICON.file,
      separatorBefore: true,
      onSelect: () => {
        const ed = activeEditor();
        const sel = ed?.selection?.();
        if (!ed?.path || !sel || !sel.text.trim()) {
          toast.info(tr("workbench-use-conversation-pane-select-some-text-editor-first"));
          return;
        }
        attach(selectionChip(ed.path, sel.start, sel.end, sel.text, selectionLines));
      },
    },
    {
      // The checkout's changes, on request only: one chip per hunk, within
      // the budget (`gitContext.ts`).
      label: changedFiles > 0 ? tr("workbench-use-conversation-pane-changes-checkout", { changedFiles: changedLabel(changedFiles) }) : tr("workbench-use-conversation-pane-changes-checkout-2"),
      icon: ICON.stage,
      disabled: changedFiles === 0,
      onSelect: () => void attachGitChanges(wid, scopeKey),
    },
    {
      label: tr("workbench-use-conversation-pane-active-terminal-s-last-lines"),
      icon: ICON.harness,
      disabled: !activeTerminal,
      onSelect: () => {
        const key = activeTerminal;
        const lines = key ? terminalTail(key) : null;
        if (!key || !lines) {
          toast.info(tr("workbench-use-conversation-pane-no-terminal-showing"));
          return;
        }
        const s = terminals.find((t) => t.key === key);
        attach(terminalChip(s ? terminalChipName(s, terminals) : key, lines, terminalLines));
      },
    },
  ];

  /** `@` in the box: the root's paths; a pick is a chip, an edited-out token drops it. */
  const files = useMemo<ComposerFiles>(
    () => ({
      paths: index?.paths ?? null,
      onMention: (path) => attachContext(fileChip(path), scopeKey),
      onUnmention: (path) => removeContextRef(scopeKey, fileChip(path)),
    }),
    [index?.paths, scopeKey],
  );

  /** A file from the explorer or a hunk from a diff, dropped anywhere on the surface. */
  const onDrop = useCallback(
    (data: DragData) => {
      if (data.type === "path") for (const p of data.paths) attachContext(fileChip(p), scopeKey);
      else if (data.type === "hunk") attachContext(hunkChip(data.path, data.staged, data.text, data.id), scopeKey);
    },
    [scopeKey],
  );
  const accepts = useCallback((d: DragData) => isDragOf(d, "path", "hunk"), []);

  const abort = (id: string) =>
    void stopSession(id).then(
      ({ how, ended }) => toast.ok(stopWords(how, tr("workbench-project-rail-session-aborted"), ended)),
      (e: unknown) => toast.error(failureText("workbench", "use-conversation-pane-failed", e)),
    );

  // Who an unaddressed message reaches. The chip reads the project's setting;
  // picking writes it, so the next message goes where the chip says. A
  // conversation about a checkout never reaches the Workflow Agent.
  const originKind = current?.origin.kind ?? "workstream";
  const defaultId = typeof setting("agents.default") === "string" ? (setting("agents.default") as string) : GENERAL_AGENT;
  const reaches = addressee(ws.agents, defaultId, GENERAL_AGENT, originKind);
  const pickDefault = (id: string) => {
    void api.setSettings("project", { "agents.default": id }, pid).then(
      () => settings.reload(),
      (e: unknown) => toast.error(failureText("workbench", "use-conversation-pane-failed", e)),
    );
  };
  const addresseeChip: ReactNode = reaches && (
    <Menu
      label={tr("workbench-use-conversation-pane-who-unaddressed-message-reaches")}
      items={ws.agents
        .filter((a) => a.enabled !== false && reachableIn(a, originKind))
        .map((a) => ({ label: a.name, icon: a.id === reaches.id ? ICON.check : ICON.agent, onSelect: () => pickDefault(a.id) }))}
      trigger={
        <span
          className="anim inline-flex h-6 shrink-0 items-center gap-1 rounded-full border border-border px-2 text-2xs text-text-dim hover:bg-surface-2 hover:text-text"
          title={tr("workbench-use-conversation-pane-message-names-nobody-reaches-runs-pick", { reaches: reaches.name, harness: reaches.harness, flag: (reaches.harness) ? "yes" : "no" })}
        >
          <ICON.agent size={11} aria-hidden />
          <span className="max-w-32 truncate">{reaches.name}</span>
          <ICON.collapsed size={10} aria-hidden />
        </span>
      }
    />
  );

  // Where a path in this thread is looked up (ide/17): this checkout first,
  // then the project's other checkouts on disk — an agent here names files
  // of this project, not of every project the desktop knows.
  const linkRoots = useMemo<LinkRootRef[]>(
    () => [
      { scope: "workstream", id: wid },
      ...ws.workstreams
        .filter((w) => w.workstream.project === pid && w.workstream.id !== wid && w.exists && w.path)
        .map((w) => ({ scope: "workstream" as const, id: w.workstream.id })),
    ],
    [wid, pid, ws.workstreams],
  );

  // --- mode, the change ledger, and asks (ide/09 §Modes and the change ledger) ---

  /** A conversation about the checkout's project or the checkout itself — "a conversation about a checkout". */
  const checkout = isCheckoutOrigin(originKind);
  const mode: ConversationMode = current ? modeOf(current) : "manual";
  const harnesses = useAsync((s) => api.harnesses(s), []);
  const toolGuard = !!harnesses.data?.harnesses.find((h) => h.id === reaches?.harness)?.tool_guard;

  const setMode = useCallback(
    (next: ConversationMode) => {
      if (!current || next === mode) return;
      void api.patchConversation(current.id, { mode: next }).catch((e: unknown) => toast.error(failureText("workbench", "use-conversation-pane-failed", e)));
    },
    [current, mode, toast],
  );
  /** The keymap's `cycle_conversation_mode` — `Shift+Tab` by default, a rebinding followed — in the composer box, while a mode control shows for this conversation. */
  const onComposerKeyDownCapture = useCallback(
    (e: ReactKeyboardEvent) => {
      if (!checkout || !current) return;
      if (commandForEvent(currentKeymap(), e, contexts(e.target), isMac) === "cycle_conversation_mode") {
        e.preventDefault();
        setMode(nextMode(mode, toolGuard));
      }
    },
    [checkout, current, mode, setMode, toolGuard],
  );
  const composerLeading: ReactNode = checkout && current ? <ConversationModePicker mode={mode} onChange={setMode} toolGuard={toolGuard} /> : null;

  /** The changes turns of this conversation left, and the open asks — only for a conversation about a checkout. */
  const changesId = checkout ? currentId : null;
  const changes = useConversationChanges(changesId);
  usePublishReviewChanges(wid, checkout ? changes.data : null);
  const asks = useConversationAsks(changesId);
  const cardsView = useMemo(() => turnCardsByAnchor(changes.data), [changes.data]);
  const linkHandler = useLinkHandler();
  // Opened *for review*: the document's own mode and its lens's draft are
  // written first — Source mode, lens on — so the file lands on its diff
  // whatever mode it last opened in (`reviewOpenDrafts`), then the ordinary
  // path door opens it.
  const openReviewFile = useCallback(
    (path: string) => {
      const drafts = reviewOpenDrafts(scopeKey, path);
      keepDocMode(drafts.modeKey, drafts.mode);
      writeSessionDraft(drafts.lensKey, drafts.lensOn);
      linkHandler?.onLink({ kind: "path", path, line: null, col: null, raw: path }, { x: 0, y: 0 }, linkRoots, { direct: true });
    },
    [linkHandler, linkRoots, scopeKey],
  );

  const buildPlan = async () => {
    if (!current) return;
    try {
      // Leaving the plan is the node's act: the record remembers the mode the
      // plan was entered from, so nothing of it is kept on this side.
      await api.buildPlan(current.id);
      await api.postConversationMessage(current.id, { content: BUILD_MESSAGE });
    } catch (e) {
      // The node's own sentence when it refused; otherwise the catalog's, never a raw message.
      toast.error(failureText("workbench", tr("workbench-use-conversation-pane-could-not-build-plan"), e));
    }
  };

  /** A turn's card, and — at the end of the timeline — the plan banner. */
  const afterMessage = useCallback(
    (id: string) => {
      if (!checkout || !current) return null;
      const cards = id === TIMELINE_END ? cardsView.atEnd : (cardsView.byMessage.get(id) ?? []);
      const nodes: ReactNode[] = cards.map((card) => (
        <TurnChangesCard key={card.turn} conversationId={current.id} card={card} onOpenFile={openReviewFile} onPostNote={async (text) => void (await api.postConversationMessage(current.id, { content: text }))} />
      ));
      if (id === TIMELINE_END && mode === "plan" && !stoppable && current.message_count > 0) {
        const words = planBannerWords();
        nodes.push(
          // The plan waits on the person — build it or refine it — so the banner keeps the accent.
          <div key="plan-banner" className="mt-2 flex items-center gap-2 rounded-card border border-accent/40 bg-accent-soft px-3 py-2 text-2xs">
            <span className="min-w-0 flex-1 text-text">{tr("workbench-use-conversation-pane-reply-above-plan")}</span>
            <Button size="sm" variant="primary" onClick={() => void buildPlan()}>
              {words.build}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => focusComposer(current.id)}>
              {words.refine}
            </Button>
          </div>,
        );
      }
      return nodes.length > 0 ? <>{nodes}</> : null;
    },
    // `buildPlan` is a plain function remade every render; the cards follow
    // the facts it reads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [checkout, current, cardsView, mode, stoppable, openReviewFile],
  );

  /** *Restore to before this message*: offered on any message that woke a turn. */
  const restore = checkout && current
    ? {
        isPrompt: (id: string) => (changes.data?.turns ?? []).some((t) => t.prompt === id),
        onRestore: (id: string) => {
          const turn = (changes.data?.turns ?? []).find((t) => t.prompt === id);
          if (!turn) return;
          void api.restoreChanges(current.id, turn.turn).catch((e: unknown) => toast.error(failureText("workbench", "use-conversation-pane-failed", e)));
        },
      }
    : undefined;

  /** Open asks, at the foot of the timeline. */
  const askCards: ReactNode = checkout && current && (asks.data?.asks.length ?? 0) > 0
    ? (
        <>
          {asks.data!.asks.map((ask) => (
            <AskCard key={ask.id} ask={ask} conversationId={current.id} onAnswered={() => asks.reload()} />
          ))}
        </>
      )
    : null;

  /**
   * The bar above the composer: a checkout conversation's own ledger, as
   * `ChangedFilesBar` draws it (ide/20). A conversation about the project
   * itself has no ledger, so nothing is drawn — git's working tree is the
   * Git panel's, and *The changes in this checkout* stays in the attach menu.
   */
  const changedFilesProp =
    checkout && current && changes.data
      ? {
          view: changes.data,
          mode,
          conversationId: current.id,
          agentName: (id: string) => ws.agents.find((a) => a.id === id)?.name ?? id,
          onOpenFile: openReviewFile,
          onAttach: () => void attachGitChanges(wid, scopeKey),
        }
      : null;

  /** The `Conversation` props both surfaces share; each adds its own words. `null` while there is no conversation here. */
  const conversationProps = current
    ? {
        scope: current.id,
        kind: "conversation" as const,
        context,
        onRemoveContext: (i: number) => removeContext(scopeKey, i),
        onClearContext: () => clearContext(scopeKey),
        files,
        attachItems,
        stop: stoppable ? { onStop: () => abort(stoppable.id) } : null,
        activity: { words: summary.activity },
        changedFiles: changedFilesProp,
        addressee: addresseeChip,
        pinned: askCards,
        afterMessage,
        restore,
        composerLeading,
        onComposerKeyDownCapture: checkout ? onComposerKeyDownCapture : undefined,
      }
    : null;

  return {
    /** The one surface — the list, the pick, the views, the door, *New conversation*. */
    surface,
    scopeKey,
    project,
    frame,
    frameWords,
    summary,
    current,
    conversationProps,
    linkRoots,
    onDrop,
    accepts,
  };
}
