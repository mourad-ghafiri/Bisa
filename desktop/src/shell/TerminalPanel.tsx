/**
 * The terminal layer: every shell and harness that is open, drawn over the
 * workbench's centre while a terminal tab is active there.
 *
 * # This file holds an invariant the model cannot
 *
 * **It renders `sessions` directly.** No local copy, no list held across
 * renders, no portal. A session removed from the store has its `<Terminal />`
 * unmounted, and unmounting closes the PTY — which is the only thing standing
 * between this app and a machine quietly carrying dozens of forgotten shells.
 * The model guarantees no two sessions share a mount key; *this* guarantees no
 * PTY outlives the list. Neither half is sufficient alone, and only one of them
 * is testable, which is why this paragraph is here.
 *
 * It also stays in `App.tsx`, outside the routed screen, outside `Suspense` and
 * outside the `ErrorBoundary` — for a reason about processes rather than
 * layout. A shell is the one thing on screen that keeps running when you stop
 * looking at it: a build, a test watcher, a `git rebase` half finished. Moving
 * this inside the workbench would end every one of them the moment you opened
 * the Inbox. It *looks* like part of the workbench because it is **positioned
 * over it** (`layerSlots.ts`): the workbench publishes the rect of its centre
 * body, and this layer sits on that rect with `position: fixed`. Not a portal
 * into the workbench's DOM — a portal target that unmounts with the route
 * would take every `<Terminal />` with it.
 *
 * This is the **only** file in the app that imports `<Terminal />`. That is the
 * property `terminal/index.ts` describes: every terminal on screen is a real
 * PTY, and one import site is the last place that fact is visible before
 * somebody renders one per row.
 *
 * # Why inactive panes are `invisible` and not `hidden`
 *
 * They must stay mounted, so the only question is how to hide them, and
 * `display: none` is wrong three times over. xterm measures its cell size
 * **once**, from a real layout box; in a display-none box that measurement is
 * zero, `proposeDimensions()` divides by it, and `safeFit` correctly refuses
 * forever — so a terminal first mounted while hidden never recovers. A
 * background pane would also keep whatever `cols` it had when it was hidden, so
 * a build's progress bar and any ncurses program would draw at the wrong width
 * into a buffer you are going to read later. And revealing one would be a 0→N
 * resize, which is an xterm reflow over the whole scrollback on the frame
 * somebody is watching.
 *
 * Stacked absolutely at full layer size, every pane is laid out at all times,
 * every PTY's geometry is always what you will see, and switching tabs is a
 * `visibility` flip with no reflow at all. The same rule hides the whole layer
 * when no terminal tab is active: it keeps its last box and goes
 * `visibility: hidden` (`slotStyle`), never to a zero size.
 */

import { RESUME_START_DEFAULT, readResumeStart } from "../terminal/resumeStartModel.mjs";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { isMac } from "../ui/KeyHint";
import { commandForEvent, interceptsInTerminal } from "./keymapModel.mjs";
import { contexts, currentKeymap } from "./shortcuts";
import { Terminal } from "../terminal";
import { DropZone, ICON, PaneDivider, TabStrip, cn, harnessMark, terminalTabDrag, PANE_RING } from "../ui";
import type { MenuItem, StripTab } from "../ui";
import { chordHint } from "../ui/keymapHints";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { terminalChip } from "../views/_workbench/contextChips.mjs";
import { showRightPanel } from "../views/_workbench/rightPanelStore";
import { terminalTabMenu } from "../views/_workbench/tabMenuModel.mjs";
import { terminalTail } from "../terminal/tails";
import { useHarnessLabels } from "./useHarnesses";
import { useLayerSlot } from "./layerSlots";
import { slotStyle } from "./centerSlotModel.mjs";
import { dividers, leafOfTab, leaves, rects } from "./paneTreeModel.mjs";
import {
  exitNote,
  isExited,
  isLive,
  livenessReason,
  livenessTone,
  mountKey,
  terminalTabLabel,
  terminalTitle,
  harnessOf,
} from "./terminalsModel.mjs";
import { requestCloseOthers, requestCloseTerminal } from "./terminalCloseGuard";
import {
  closeExitedTerminalTabs,
  focusTerminalPane,
  focusTerminalTab,
  moveTerminalToPane,
  openTerminalIn,
  reorderTerminalTab,
  restartTerminalTab,
  setTerminalPaneRatio,
  terminalExited,
  terminalLive,
  terminalUnverifiable,
  useTerminals,
  terminalRunning,
} from "./useTerminals";
import { t as tr } from "../i18n/l10n.mjs";

/** Each pane's own tab strip. */
const PANE_HEADER_PX = 30;

/**
 * The `terminal.*` settings the panel applies: machine and workspace scope,
 * no project — a terminal is rooted somewhere but the panel is the shell's,
 * not a project's. `ready` says the first answer landed (or failed, leaving
 * the defaults): a restored tab is not mounted before that, since whether it
 * replays its checkpoint is decided at mount and `terminal.restore_scrollback`
 * has to be known by then.
 */
function useTerminalSettings(): {
  ready: boolean;
  scrollback: number;
  restore: boolean;
  /** `terminal.resume_start`: a resumed harness is nudged into motion. */
  resumeStart: boolean;
  fontFamily: string | null;
  fontSize: number | null;
  cursorStyle: "block" | "bar" | "underline" | null;
} {
  const [s, setS] = useState<{
    ready: boolean;
    scrollback: number;
    restore: boolean;
    resumeStart: boolean;
    fontFamily: string | null;
    fontSize: number | null;
    cursorStyle: "block" | "bar" | "underline" | null;
  }>({ ready: false, scrollback: 5000, restore: true,
    resumeStart: RESUME_START_DEFAULT, fontFamily: null, fontSize: null, cursorStyle: null });
  // Read now, and again whenever Settings saves — a font size changed there
  // reaches every open shell without a reload.
  const [generation, setGeneration] = useState(0);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed") setGeneration((n) => n + 1);
  });
  useReloadOnReconnect(() => setGeneration((n) => n + 1));
  useEffect(() => {
    let alive = true;
    api
      .settingsResolved(null)
      .then((r) => {
        if (!alive) return;
        const get = (k: string) => r.settings.find((x) => x.key === k)?.value;
        const lines = get("terminal.scrollback_lines");
        const restore = get("terminal.restore_scrollback");
        const family = get("terminal.font_family");
        const size = get("terminal.font_size");
        const cursor = get("terminal.cursor_style");
        setS({
          ready: true,
          scrollback: typeof lines === "number" && Number.isFinite(lines) ? lines : 5000,
          restore: restore !== false,
          resumeStart: readResumeStart(r.settings),
          fontFamily: typeof family === "string" && family.trim() ? family : null,
          fontSize: typeof size === "number" && size >= 8 && size <= 32 ? size : null,
          cursorStyle: cursor === "bar" || cursor === "underline" || cursor === "block" ? cursor : null,
        });
      })
      .catch((e: unknown) => {
        // The defaults stand; a terminal that could not read a setting is
        // still a terminal — said in the log, since a shell drawn in the
        // wrong font every time is a fault somebody has to be able to find.
        log.debug("terminal", "the terminal settings could not be read; the defaults stand", errorFields(e));
        if (alive) setS((prev) => ({ ...prev, ready: true }));
      });
    return () => {
      alive = false;
    };
  }, [generation]);
  return s;
}

export function TerminalPanel() {
  const { sessions, panes, focusedPane } = useTerminals();
  const { rect, layer } = useLayerSlot("center");
  // The terminal layer is drawn while a terminal tab is the active one; a browser tab is the browser layer's.
  const visible = layer === "terminal";
  const labels = useHarnessLabels();
  const settings = useTerminalSettings();
  // A declared chord a PTY cannot mean — ⌘\\ to split, ⌘J, ⌘⇧E, Ctrl+Tab,
  // Ctrl+`, the palette — bubbles past the shell to the app's handler;
  // everything else is the shell's. The scopes are the ones the window
  // listener reads for the same key (`contexts`), so the two never disagree.
  const reserveKey = useCallback((e: KeyboardEvent) => {
    const km = currentKeymap();
    const cmd = commandForEvent(km, e, contexts(e.target), isMac);
    if (!cmd) return false;
    return interceptsInTerminal(km.bindings.find((b) => b.id === cmd), isMac);
  }, []);
  const body = useRef<HTMLDivElement>(null);
  const paneRects = useMemo(() => rects(panes), [panes]);
  const paneDividers = useMemo(() => dividers(panes), [panes]);
  const paneLeaves = useMemo(() => leaves(panes), [panes]);
  const manyPanes = paneLeaves.length > 1;
  const open = visible;

  /** A tab strip row per session, looked up by key when drawing each pane. */
  const tabsByKey = useMemo(() => {
    const m = new Map<string, StripTab>();
    for (const s of sessions) {
      const gone = exitNote(s);
      const reason = livenessReason(s);
      m.set(s.key, {
        id: s.key,
        label: terminalTabLabel(s, labels),
        // The three-value liveness on hover: an unverifiable tab says why.
        title: reason ? `${terminalTitle(s)} — ${reason}` : terminalTitle(s),
        // A harness's own mark on its tab; a plain shell keeps the terminal glyph.
        icon: harnessOf(s) ? harnessMark(harnessOf(s)) : ICON.harness,
        note: gone ?? undefined,
        noteTone: livenessTone(s),
      });
    }
    return m;
  }, [sessions, labels]);

  /** A terminal tab's context menu, on a split pane's strip; the centre strip draws the same. */
  const menuFor = (key: string): MenuItem[] => {
    const s = sessions.find((x) => x.key === key);
    if (!s) return [];
    const here = sessions.filter((x) => x.scope === s.scope && x.id === s.id);
    const spec = terminalTabMenu({ live: isLive(s), harness: harnessOf(s) !== null, others: here.length - 1, exited: here.filter(isExited).length });
    const act: Record<string, () => void> = {
      focus: () => focusTerminalTab(key),
      "new-shell": () => openTerminalIn({ scope: s.scope, id: s.id, label: s.label }),
      restart: () => restartTerminalTab(key),
      "send-to-agent": () => {
        const lines = terminalTail(key);
        if (lines) attachContext(terminalChip(`${harnessOf(s) ?? "shell"} · ${key}`, lines));
        showRightPanel("agents", `${s.scope}:${s.id}`);
      },
      "show-agents": () => showRightPanel("agents", `${s.scope}:${s.id}`),
      close: () => requestCloseTerminal(key),
      "close-others": () => requestCloseOthers(key),
      "close-exited": () => closeExitedTerminalTabs(s.scope, s.id),
    };
    return spec.map((item) => ({ label: item.label, danger: item.danger, disabled: item.disabled, separatorBefore: item.separatorBefore, shortcut: chordHint(item.command), onSelect: act[item.id] ?? (() => undefined) }));
  };

  if (sessions.length === 0) return null;

  return (
    <>
      {/*
        `data-terminal-panel` is read by the shell's Escape handler and the
        keymap's `terminal` context: inside a terminal, Escape is a keystroke
        somebody's editor is waiting for, not a request to close a pane.
      */}
      <aside
        data-terminal-panel
        data-pane
        aria-label={tr("shell-terminal-panel-terminals")}
        aria-hidden={!open || undefined}
        style={slotStyle(rect, open)}
        className="z-20 flex flex-col bg-surface"
      >
        {/*
          Every session, always mounted, stacked at full size. See this file's
          doc for why the inactive ones are `invisible` rather than `hidden`,
          and why that is not a style preference.
        */}
        {/* Moving between panes is the keymap's (`pane_left` …), not a key
            this element reads — so the chord is one a person can change. */}
        <div ref={body} className="relative min-h-0 flex-1" inert={!open}>
          {/*
            Per-pane tab strips, once split. Each sits at the top of its
            pane's rect; a tab dragged onto another pane's strip moves there.
            With one pane the centre strip is the tab strip.
            The terminals themselves live in one flat layer below, positioned
            over their pane, so a move never remounts one — and remounting is
            what ends a shell.
          */}
          {manyPanes &&
            paneRects.map((r) => {
              const leaf = paneLeaves.find((l) => l.id === r.leafId);
              if (!leaf) return null;
              const focused = leaf.id === focusedPane;
              const strip = leaf.tabs.map((k) => tabsByKey.get(k)).filter((t): t is StripTab => !!t);
              // A tab from another pane, dropped on this strip.
              const take = (d: { type: string; key?: string; pane?: string }) => {
                if (d.type === "terminal-tab" && typeof d.key === "string" && d.pane !== leaf.id) moveTerminalToPane(d.key, leaf.id);
              };
              return (
                <div
                  key={leaf.id}
                  style={{ left: `${r.x * 100}%`, top: `${r.y * 100}%`, width: `${r.w * 100}%`, height: PANE_HEADER_PX }}
                  className={cn(
                    "absolute z-10 flex items-center border-b border-r border-border bg-surface",
                    focused ? "border-b-accent/60" : "opacity-80",
                  )}
                  onMouseDown={() => focusTerminalPane(leaf.id)}
                >
                  <DropZone className="flex min-w-0 flex-1 items-center px-1" label={tr("shell-terminal-panel-terminal-pane", { leaf: leaf.id })} accepts={(d) => d.type === "terminal-tab" && d.pane !== leaf.id} onDrop={take}>
                    <TabStrip
                      className="min-w-0 flex-1"
                      size="sm"
                      label={tr("shell-terminal-panel-terminals-pane", { leaf: leaf.id })}
                      tabs={strip}
                      active={open ? leaf.active : null}
                      onSelect={(k) => {
                        focusTerminalPane(leaf.id);
                        focusTerminalTab(k);
                      }}
                      onClose={requestCloseTerminal}
                      onReorder={reorderTerminalTab}
                      dragData={(k) => terminalTabDrag(k, leaf.id, tabsByKey.get(k)?.label ?? k)}
                      onDropForeign={take}
                      menuFor={menuFor}
                    />
                  </DropZone>
                </div>
              );
            })}
          {manyPanes && paneDividers.map((d) => <PaneDivider key={d.splitId} d={d} body={body} onRatio={setTerminalPaneRatio} />)}
          {settings.ready && sessions.map((s) => {
            const leaf = leafOfTab(panes, s.key);
            const r = leaf ? paneRects.find((x) => x.leafId === leaf.id) : null;
            const showing = open && !!leaf && leaf.active === s.key;
            const header = manyPanes ? PANE_HEADER_PX : 0;
            const style: React.CSSProperties = r
              ? {
                  left: `${r.x * 100}%`,
                  top: `calc(${r.y * 100}% + ${header}px)`,
                  width: `${r.w * 100}%`,
                  height: `calc(${r.h * 100}% - ${header}px)`,
                }
              : { inset: 0 };
            return (
              <div
                key={mountKey(s)}
                style={style}
                // `inert` because xterm renders a focusable helper textarea per
                // instance: without it, Tab lands inside a terminal nobody can
                // see and a screen reader walks all of them.
                inert={!showing}
                aria-hidden={!showing || undefined}
                className={cn(
                  "absolute",
                  !showing && "invisible pointer-events-none",
                  manyPanes && leaf && leaf.id === focusedPane && PANE_RING,
                )}
                onMouseDown={() => leaf && focusTerminalPane(leaf.id)}
              >
                <Terminal
                  scope={s.scope}
                  id={s.id}
                  harness={s.harness}
                  login={s.login}
                  resume={s.resume}
                  run={s.run}
                  mobileDevelopment={s.mobileDevelopment}
                  startOnResume={settings.resumeStart}
                  visible={showing}
                  reserveKey={reserveKey}
                  active={showing && (!manyPanes || leaf?.id === focusedPane)}
                  sessionKey={s.key}
                  replayCheckpoint={s.restoring && settings.restore}
                  scrollback={settings.scrollback}
                  fontFamily={settings.fontFamily ?? undefined}
                  fontSize={settings.fontSize ?? undefined}
                  cursorStyle={settings.cursorStyle ?? undefined}
                  onExit={(code) => terminalExited(s.key, s.generation, code)}
                  onOpened={(tid, session) => terminalLive(s.key, s.generation, tid, session)}
                  onUnverifiable={(reason) => terminalUnverifiable(s.key, s.generation, reason)}
                  onProcess={(h) => terminalRunning(s.key, s.generation, h)}
                  className="rounded-none border-0"
                />
              </div>
            );
          })}
        </div>
      </aside>
    </>
  );
}
