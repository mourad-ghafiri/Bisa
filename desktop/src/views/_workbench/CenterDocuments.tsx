/**
 * The centre of the workbench: one strip of documents *and* terminals, and the
 * body that shows the active one.
 *
 * Documents come from the workbench store; terminal tabs are the terminal
 * store's sessions rooted here, merged at render time (`mergedTabs`, in the
 * root's one strip order — newest last, terminals and documents alike) so there
 * is one owner of every shell. When the active tab is a terminal the body
 * renders an empty slot and publishes its rect: the terminal layer, mounted
 * in `App.tsx`, draws itself over that rect. It is never rendered *here* —
 * see `shell/TerminalPanel.tsx` for why that would end every shell on the
 * first route change.
 */

import { useEffect, useMemo, useRef } from "react";
import { errorFields, log } from "../../log";
import { LayerSlot } from "../../shell/LayerSlot";
import { TerminalStripControls } from "../../shell/TerminalStripControls";
import { useHarnessLabels } from "../../shell/useHarnesses";
import { dividers, leafOfTab, leaves, rects } from "../../shell/paneTreeModel.mjs";
import type { PaneNode } from "../../shell/paneTreeModel.mjs";
import { exitNote, isExited, isLive, livenessTone, sessionsRootedAt, terminalTabLabel, terminalTitle, harnessOf } from "../../shell/terminalsModel.mjs";
import { browserLabel, browserTitle, seenRootedAt } from "../../shell/browsersModel.mjs";
import { closeBrowserTab, focusBrowserTab, useBrowsers } from "../../shell/useBrowsers";
import { setBrowserInspecting } from "../../shell/browserInspectorStore";
import { useServing } from "../../shell/useServing";
import { annotatable } from "./serversModel.mjs";
import { BrowserDoc } from "./BrowserDoc";
import { useDevices } from "../../shell/devicesStore";
import { deviceById, deviceIcon, deviceNote, deviceTitle } from "./devicesModel.mjs";
import {
  closeExitedTerminalTabs,
  focusTerminalTab,
  openTerminalIn,
  restartTerminalTab,
  useTerminals,
} from "../../shell/useTerminals";
import { requestCloseOthers, requestCloseTerminal } from "../../shell/terminalCloseGuard";
import { terminalTail } from "../../terminal/tails";
import { inDesktopShell, openExternal, revealPath } from "../../api";
import { DropZone, ICON, PaneDivider, TabStrip, cn, docTabDrag, revealLabel, useToast, copyText as copyToClipboard, PANE_RING, StripControlButton, harnessMark } from "../../ui";
import type { DragData } from "../../ui";
import type { MenuItem, StripTab } from "../../ui";
import { chordHint } from "../../ui/keymapHints";
import { joinPath } from "../../ui/fileTreeModel.mjs";
import { attachContext } from "./agentPaneStore";
import { terminalChip } from "./contextChips.mjs";
import { showRightPanel } from "./rightPanelStore";
import { browserTabMenu, docTabMenu, terminalTabMenu } from "./tabMenuModel.mjs";
import type { WorkbenchScope } from "../../router";
import { editorKey, useDirtyEditors } from "./editorRegistry";
import { usePendingReviewPaths } from "./reviewStore";
import { leafStrip, mergedTabs, parseTabId, tabId, tabIn, tabLabel, tabTitle } from "./workbenchModel.mjs";
import type { WorkbenchTab } from "./workbenchModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";


export function CenterDocuments({
  scope,
  id,
  docs,
  activeId,
  panes,
  pinned,
  previews,
  onSelect,
  onKeep,
  onCloseDoc,
  onCloseAll,
  onCloseOthers,
  onCloseRight,
  onCloseSaved,
  onReorderDoc,
  strip,
  rootPath,
  onRevealInFiles,
  onSplit,
  onClosePane,
  onMoveDoc,
  onFocusPane,
  onRatio,
  onTogglePin,
  project,
  renderDoc,
  landing,
}: {
  scope: WorkbenchScope;
  id: string;
  /** The stored documents for this root. */
  docs: WorkbenchTab[];
  activeId: string | null;
  /** The pane tree over the document tab ids (`workbenchModel.mjs`). */
  panes: PaneNode;
  /** Tab ids pinned in this root. */
  pinned: readonly string[];
  /** Tab ids that are previews — italic, one per pane (`workbenchModel.mjs`). */
  previews: readonly string[];
  onSelect: (id: string | null) => void;
  /** *Keep open*: a preview becomes a kept tab. */
  onKeep: (id: string) => void;
  onCloseDoc: (id: string) => void;
  onCloseAll: () => void;
  /** *Close others*: every document but this one and the pinned ones. */
  onCloseOthers: (id: string) => void;
  /** *Close to the right*: the unpinned documents after this one in its strip. */
  onCloseRight: (id: string) => void;
  /** *Close saved*: every document with nothing unsaved, the pinned ones excepted. */
  onCloseSaved: () => void;
  /** A tab dragged along its strip: put it at `index` (strip order). */
  /** A drag along a strip: the tab, where it landed, and the strip's ids as they were shown. */
  onReorderDoc: (id: string, index: number, shown: readonly string[]) => void;
  /** The root's one strip order — documents and terminals as opened or dragged. */
  strip: readonly string[];
  /** The root's absolute path, for *Copy absolute path* and the OS reveal. */
  rootPath: string | null;
  /** Show a document's path in the Files tree. */
  onRevealInFiles: (path: string) => void;
  onSplit: (dir: "row" | "col", leafId: string) => void;
  onClosePane: (leafId: string) => void;
  onMoveDoc: (id: string, leafId: string) => void;
  onFocusPane: (leafId: string) => void;
  onRatio: (splitId: string, ratio: number) => void;
  onTogglePin: (id: string) => void;
  /** The project the root belongs to — a browser tab's annotations go to its default agent (ide/18). */
  project: string | null;
  /** The body for a document tab. */
  renderDoc: (tab: WorkbenchTab) => React.ReactNode;
  /** The body when nothing is active. */
  landing: React.ReactNode;
}) {
  const { sessions, active: activeTerminal } = useTerminals();
  const labels = useHarnessLabels();
  const toast = useToast();
  const dirty = useDirtyEditors();
  const toReview = usePendingReviewPaths(scope === "workstream" ? id : "");
  const here = useMemo(() => sessionsRootedAt(sessions, scope, id), [sessions, scope, id]);
  // The browser tabs rooted here ride the strip with the terminals (ide/18)
  // — the seen ones; a tab kept out of sight rides no strip until shown.
  const { sessions: browserSessions, active: activeBrowser } = useBrowsers();
  const browsersHere = useMemo(() => seenRootedAt(browserSessions, scope, id), [browserSessions, scope, id]);
  // Every server up, for whether a browser tab's page can be annotated (ide/18).
  const serving = useServing(scope === "workstream" ? id : null);
  const tabs = useMemo(() => mergedTabs(docs, here, strip, browsersHere), [docs, here, strip, browsersHere]);
  const terminalIds = useMemo(() => [...here.map((s) => `terminal:${s.key}`), ...browsersHere.map((s) => `browser:${s.key}`)], [here, browsersHere]);
  const active = tabIn(docs, activeId);
  const activeIsTerminal = active?.kind === "terminal" && here.some((s) => s.key === active.key);
  const activeIsBrowser = active?.kind === "browser" && browsersHere.some((s) => s.key === active.key);
  const activeIsLayer = activeIsTerminal || activeIsBrowser;
  const paneLeaves = useMemo(() => leaves(panes), [panes]);
  const paneRects = useMemo(() => rects(panes), [panes]);
  const paneDividers = useMemo(() => dividers(panes), [panes]);
  const manyPanes = paneLeaves.length > 1;
  const body = useRef<HTMLDivElement>(null);
  const activeLeaf = activeId ? leafOfTab(panes, activeId) : null;

  // URL → terminal store: the active terminal tab is the focused session.
  // Each of the four mirrors below runs on its own side's change alone:
  // following the other side too would have the pair answer each other.
  useEffect(() => {
    if (active?.kind === "terminal" && activeTerminal !== active.key && here.some((s) => s.key === active.key)) {
      focusTerminalTab(active.key);
    }
    // On the address alone; the store's side is the mirror below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeId]);
  // Terminal store → URL: a shell opened or focused here becomes the active tab.
  useEffect(() => {
    if (!activeTerminal) return;
    if (!here.some((s) => s.key === activeTerminal)) return;
    const want = `terminal:${activeTerminal}`;
    if (activeId !== want) onSelect(want);
    // On the focused session alone; the address's side is the mirror above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTerminal]);
  // The same two ways for the browser tabs.
  useEffect(() => {
    if (active?.kind === "browser" && activeBrowser !== active.key && browsersHere.some((s) => s.key === active.key)) focusBrowserTab(active.key);
    // On the address alone; the store's side is the mirror below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeId]);
  useEffect(() => {
    if (!activeBrowser) return;
    if (!browsersHere.some((s) => s.key === activeBrowser)) return;
    const want = `browser:${activeBrowser}`;
    if (activeId !== want) onSelect(want);
    // On the focused tab alone; the address's side is the mirror above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeBrowser]);

  // The devices, read only while a device document is open here.
  const { devices: devicesHere } = useDevices(tabs.some((t) => t.kind === "device"));
  const stripTabFor = (t: WorkbenchTab): StripTab => {
    if (t.kind === "terminal") {
      const s = here.find((x) => x.key === t.key);
      const gone = s ? exitNote(s) : null;
      return {
        id: tabId(t),
        label: s ? terminalTabLabel(s, labels) : tabLabel(t),
        title: s ? terminalTitle(s) : tabTitle(t),
        // The harness's own mark when one is about — launched, or typed into the shell.
        icon: s && harnessOf(s) ? harnessMark(harnessOf(s)) : ICON.harness,
        note: gone ?? undefined,
        noteTone: s ? livenessTone(s) : undefined,
        closeable: true,
      };
    }
    if (t.kind === "browser") {
      const s = browsersHere.find((x) => x.key === t.key);
      return {
        id: tabId(t),
        label: s ? browserLabel(s) : tabLabel(t),
        title: s ? browserTitle(s) : tabTitle(t),
        icon: ICON.page,
        note: s?.loading ? tr("workbench-artifact-document-loading") : undefined,
        closeable: true,
      };
    }
    const tid = tabId(t);
    const isPinned = pinned.includes(tid);
    const isPreview = previews.includes(tid);
    if (t.kind === "device") {
      // A device's name and state from the devices store (ide/19); the id
      // until the list is read.
      const d = deviceById(devicesHere, t.id);
      return {
        id: tid,
        label: d?.name ?? tabLabel(t),
        title: d ? deviceTitle(d) : tabTitle(t),
        icon: d ? ICON[deviceIcon(d)] : ICON.device,
        note: d ? (deviceNote(d) ?? undefined) : undefined,
        closeable: !isPinned,
      };
    }
    return {
      id: tid,
      label: tabLabel(t),
      title: isPinned ? tr("workbench-center-documents-pinned", { t: tabTitle(t) }) : isPreview ? tr("workbench-center-documents-preview", { t: tabTitle(t) }) : tabTitle(t),
      icon: isPinned ? ICON.pin : undefined,
      closeable: !isPinned,
      dirty: dirty.has(editorKey(`${scope}:${id}`, tid)),
      preview: isPreview,
      // An agent's change to this file still waits for a word (ide/20).
      note: t.kind === "file" && toReview.has(t.path) ? tr("workbench-center-documents-review") : undefined,
    };
  };

  /** A double-click on a document tab pins or unpins it; a terminal's or a browser tab's is nothing. */
  const doubleClick = (tid: string) => {
    const kind = parseTabId(tid)?.kind;
    if (kind !== "terminal" && kind !== "browser") onTogglePin(tid);
  };

  /** One pane's rendered ids: its documents and, in the first pane, the terminals rooted here — the root's one order, pinned first. */
  const shownIn = (leaf: { id: string; tabs: string[] }, first: boolean): string[] => leafStrip(strip, leaf.tabs, first ? terminalIds : [], pinned);
  /** One pane's strip, drawn from the root's order. */
  const stripFor = (leaf: { id: string; tabs: string[] }, first: boolean): StripTab[] => {
    const byId = new Map(tabs.map((t) => [tabId(t), t]));
    return shownIn(leaf, first)
      .map((tid) => byId.get(tid))
      .filter((t): t is WorkbenchTab => !!t)
      .map(stripTabFor);
  };

  const close = (tid: string) => {
    const t = parseTabId(tid);
    if (t?.kind === "terminal") requestCloseTerminal(t.key);
    else if (t?.kind === "browser") closeBrowserTab(t.key);
    else onCloseDoc(tid);
  };

  const copyText = (text: string) => {
    void copyToClipboard(text).then((ok) => (ok ? toast.ok(tr("workbench-center-documents-copied")) : toast.error(tr("workbench-center-documents-clipboard-refused"))));
  };

  /** A tab's context menu: a document's or a terminal's, from `tabMenuModel`. */
  const menuFor = (leaf: { id: string; tabs: string[] }) => (tid: string): MenuItem[] => {
    const t = parseTabId(tid);
    if (!t) return [];
    if (t.kind === "browser") {
      const s = browsersHere.find((x) => x.key === t.key);
      if (!s) return [];
      const blank = s.url === "about:blank";
      const spec = browserTabMenu({ others: browsersHere.length - 1, blank, annotatable: serving.read && annotatable(s, serving.all) });
      const act: Record<string, () => void> = {
        reload: () =>
          void import("../../browser/session")
            .then((m) => m.browserViewReload(s.key))
            .catch((e: unknown) => log.warn("browser", "a reload did not take", { key: s.key, ...errorFields(e) })),
        annotate: () => {
          setBrowserInspecting(s.key, true);
          focusBrowserTab(s.key);
        },
        "copy-url": () => copyText(s.url),
        "open-outside": () => void openExternal(s.url).catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e))),
        close: () => closeBrowserTab(s.key),
        "close-others": () => {
          for (const other of browsersHere) if (other.key !== s.key) closeBrowserTab(other.key);
        },
      };
      return spec.map((item) => ({ label: item.label, danger: item.danger, disabled: item.disabled, separatorBefore: item.separatorBefore, shortcut: chordHint(item.command), onSelect: act[item.id] ?? (() => undefined) }));
    }
    if (t.kind === "terminal") {
      const s = here.find((x) => x.key === t.key);
      if (!s) return [];
      const spec = terminalTabMenu({ live: isLive(s), harness: harnessOf(s) !== null, others: here.length - 1, exited: here.filter(isExited).length });
      const act: Record<string, () => void> = {
        focus: () => onSelect(tid),
        "new-shell": () => openTerminalIn({ scope: s.scope, id: s.id, label: s.label }),
        restart: () => restartTerminalTab(s.key),
        "send-to-agent": () => {
          const lines = terminalTail(s.key);
          if (lines) attachContext(terminalChip(`${harnessOf(s) ?? "shell"} · ${s.key}`, lines));
          showRightPanel("agents", `${scope}:${id}`);
        },
        "show-agents": () => showRightPanel("agents", `${scope}:${id}`),
        close: () => requestCloseTerminal(s.key),
        "close-others": () => requestCloseOthers(s.key),
        "close-exited": () => closeExitedTerminalTabs(s.scope, s.id),
      };
      return spec.map((item) => ({ label: item.label, danger: item.danger, disabled: item.disabled, separatorBefore: item.separatorBefore, shortcut: chordHint(item.command), onSelect: act[item.id] ?? (() => undefined) }));
    }
    const isPinnedTab = pinned.includes(tid);
    // The documents after this one in the strip's order — a shell to the right is not closed by *Close to the right*.
    const ordered = leafStrip(strip, leaf.tabs, [], pinned);
    const right = ordered.slice(ordered.indexOf(tid) + 1).filter((x) => !pinned.includes(x)).length;
    const others = docs.filter((d) => tabId(d) !== tid && !pinned.includes(tabId(d))).length;
    const saved = docs.filter((d) => !pinned.includes(tabId(d)) && !dirty.has(editorKey(`${scope}:${id}`, tabId(d)))).length;
    const spec = docTabMenu({
      pinned: isPinnedTab,
      preview: previews.includes(tid),
      others,
      right,
      saved,
      canSplit: leaf.tabs.length >= 2,
      inRoot: t.kind === "file",
      onDisk: t.kind === "file" || t.kind === "loose",
      desktop: inDesktopShell() && (t.kind === "loose" || !!rootPath),
      reveal: REVEAL_LABEL,
    });
    const path = t.kind === "file" ? t.path : "";
    // Where the file is on this machine: a loose file's own path, a root file's under the root.
    const absolute = t.kind === "loose" ? t.path : rootPath ? joinPath(rootPath, path) : path;
    const act: Record<string, () => void> = {
      close: () => onCloseDoc(tid),
      "close-others": () => onCloseOthers(tid),
      "close-right": () => onCloseRight(tid),
      "close-saved": () => onCloseSaved(),
      "close-all": () => onCloseAll(),
      keep: () => onKeep(tid),
      pin: () => onTogglePin(tid),
      "split-right": () => onSplit("row", leaf.id),
      "split-down": () => onSplit("col", leaf.id),
      "copy-path": () => copyText(path),
      "copy-absolute": () => copyText(absolute),
      "reveal-files": () => onRevealInFiles(path),
      "reveal-os": () => {
        void revealPath(absolute).catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e)));
      },
    };
    return spec.map((item) => ({ label: item.label, danger: item.danger, disabled: item.disabled, separatorBefore: item.separatorBefore, shortcut: chordHint(item.command), onSelect: act[item.id] ?? (() => undefined) }));
  };

  /**
   * A tab dragged along the strip: documents and terminals are one run, so
   * the drop is the model's (`moveTab` over the strip as it was shown); the
   * pins are the one boundary, and the model says so when a drag crosses it.
   */
  const reorder = (leaf: { id: string; tabs: string[] }, first: boolean) => (tid: string, index: number) => {
    onReorderDoc(tid, index, shownIn(leaf, first));
  };

  // The terminal's controls belong to the strip the terminals ride — the
  // first pane's; every other pane keeps its own split and close controls
  // while a terminal is on screen, so a document pane can still be closed.
  const docControls = (leafId: string, canSplit: boolean, first: boolean) =>
    activeIsTerminal && first ? (
      <TerminalStripControls />
    ) : (
      <div className="flex shrink-0 items-center gap-0.5 px-1">
        {activeId && !activeIsLayer && (
          <StripControlButton
            label={pinned.includes(activeId) ? tr("workbench-center-documents-unpin-tab") : tr("workbench-center-documents-pin-tab")}
            title={pinned.includes(activeId) ? tr("workbench-center-documents-unpin-can-closed-again") : tr("workbench-center-documents-pin-first-strip-kept-close-all")}
            active={pinned.includes(activeId)}
            onClick={() => onTogglePin(activeId)}
          >
            <ICON.pin size={12} aria-hidden />
          </StripControlButton>
        )}
        <StripControlButton label={tr("workbench-center-documents-split-right")} title={tr("workbench-center-documents-split-right-active-document-moves-into")} disabled={!canSplit} onClick={() => onSplit("row", leafId)}>
          <ICON.splitRight size={12} aria-hidden />
        </StripControlButton>
        <StripControlButton label={tr("workbench-center-documents-split-down")} title={tr("workbench-center-documents-split-down-active-document-moves-into")} disabled={!canSplit} onClick={() => onSplit("col", leafId)}>
          <ICON.splitDown size={12} aria-hidden />
        </StripControlButton>
        {manyPanes && (
          <StripControlButton label={tr("workbench-center-documents-close-pane")} title={tr("workbench-center-documents-close-pane-documents-move-next-door")} onClick={() => onClosePane(leafId)}>
            <ICON.close size={12} aria-hidden />
          </StripControlButton>
        )}
        {!manyPanes && activeId && !activeIsTerminal && tabs.length > 1 && (
          <StripControlButton label={tr("workbench-center-documents-close-others")} title={tr("workbench-center-documents-close-every-other-document-pinned-ones")} onClick={() => onCloseOthers(activeId)}>
            <span className="px-1 text-2xs">{tr("workbench-center-documents-close-others")}</span>
          </StripControlButton>
        )}
        {!manyPanes && tabs.length > 1 && (
          <StripControlButton label={tr("workbench-center-documents-close-all")} title={tr("workbench-center-documents-close-every-document-pinned-ones-stay")} onClick={onCloseAll}>
            <span className="px-1 text-2xs">{tr("workbench-center-documents-close-all")}</span>
          </StripControlButton>
        )}
      </div>
    );

  const bodyFor = (leaf: { id: string; tabs: string[]; active: string | null }, first: boolean) => {
    const shownId = first && activeIsLayer ? activeId : leaf.active;
    // The stored tab, by identity: the editor keys its backend and its
    // registration on it, and a fresh object per render was a render loop.
    const tab = tabIn(docs, shownId);
    if (!tab) return first ? landing : <div className="h-full" />;
    if (tab.kind === "terminal") return <LayerSlot layer="terminal" label={tr("workbench-center-documents-terminal")} />;
    if (tab.kind === "browser") {
      const s = browsersHere.find((x) => x.key === tab.key);
      return s ? <BrowserDoc key={s.key} session={s} project={project} /> : <div className="h-full" />;
    }
    return renderDoc(tab);
  };

  /** What a tab carries when dragged: its id, the pane it leaves, its label for the ghost. */
  const dragOf = (leaf: { id: string }, tabs: { id: string; label: string }[]) => (tid: string) => docTabDrag(tid, leaf.id, tabs.find((t) => t.id === tid)?.label ?? tid);
  /** A document tab from another pane, dropped on this pane's strip or body. */
  const takeTab = (leafId: string) => (data: DragData) => {
    if (data.type !== "doc-tab" || data.id.startsWith("terminal:") || data.id.startsWith("browser:")) return;
    onMoveDoc(data.id, leafId);
  };

  if (!manyPanes) {
    const leaf = paneLeaves[0];
    const strip = stripFor(leaf, true);
    return (
      <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        <TabStrip
          className="shrink-0 border-b border-border"
          label={tr("workbench-center-documents-open-documents-terminals")}
          tabs={strip}
          active={activeId}
          onSelect={(tid) => onSelect(tid)}
          onDoubleClick={doubleClick}
          onClose={close}
          onReorder={reorder(leaf, true)}
          dragData={dragOf(leaf, strip)}
          menuFor={menuFor(leaf)}
          trailing={docControls(leaf.id, leaf.tabs.length >= 2, true)}
        />
        <div className="min-h-0 flex-1">{bodyFor(leaf, true)}</div>
      </main>
    );
  }

  return (
    <main className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div ref={body} className="relative min-h-0 flex-1">
        {paneDividers.map((d) => (
          <PaneDivider key={d.splitId} d={d} body={body} onRatio={onRatio} />
        ))}
        {paneRects.map((r, i) => {
          const leaf = paneLeaves.find((l) => l.id === r.leafId);
          if (!leaf) return null;
          const focused = activeLeaf?.id === leaf.id;
          const first = i === 0;
          const strip = stripFor(leaf, first);
          return (
            <section
              key={leaf.id}
              aria-label={tr("workbench-center-documents-pane", { i: i + 1 })}
              style={{ left: `${r.x * 100}%`, top: `${r.y * 100}%`, width: `${r.w * 100}%`, height: `${r.h * 100}%` }}
              className={cn("absolute flex flex-col border-r border-b border-border", focused && PANE_RING)}
              onMouseDown={() => onFocusPane(leaf.id)}
            >
              {/* The whole pane takes a tab from its neighbour — the strip
                  through `onDropForeign`, the body through the zone. */}
              <DropZone className="flex min-h-0 flex-1 flex-col" label={tr("workbench-center-documents-pane", { i: i + 1 })} accepts={(d) => d.type === "doc-tab" && d.pane !== leaf.id && !d.id.startsWith("terminal:") && !d.id.startsWith("browser:")} onDrop={takeTab(leaf.id)}>
                <TabStrip
                  className="shrink-0 border-b border-border"
                  size="sm"
                  label={tr("workbench-center-documents-documents-pane", { i: i + 1 })}
                  tabs={strip}
                  active={focused || (first && activeIsLayer) ? activeId : leaf.active}
                  onSelect={(tid) => {
                    onFocusPane(leaf.id);
                    onSelect(tid);
                  }}
                  onDoubleClick={doubleClick}
                  onClose={close}
                  onReorder={reorder(leaf, first)}
                  dragData={dragOf(leaf, strip)}
                  onDropForeign={takeTab(leaf.id)}
                  menuFor={menuFor(leaf)}
                  trailing={docControls(leaf.id, leaf.tabs.length >= 2, first)}
                />
                <div className="min-h-0 flex-1">{bodyFor(leaf, first)}</div>
              </DropZone>
            </section>
          );
        })}
      </div>
    </main>
  );
}

/** Computed once: the platform does not change while the app runs. */
const REVEAL_LABEL = revealLabel(typeof navigator === "undefined" ? "" : navigator.userAgent);

