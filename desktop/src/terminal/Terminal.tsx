/**
 * A shell, in a panel.
 *
 * One component, one PTY, one lifetime. The rule it exists to keep is narrow
 * and unforgiving: **a mount opens a shell and an unmount closes it.** A panel
 * that unmounts without closing leaks a real process per visit — invisible in
 * the UI, invisible in the tests, and visible only as a machine that has
 * forty-one `zsh` processes an hour into a working day. Every piece of the
 * teardown below is there for that, including the parts that look paranoid.
 *
 * The awkward one is the open itself. `terminal_open` is asynchronous, React
 * unmounts synchronously, and StrictMode runs the effect twice in development
 * — so an unmount routinely lands *while the shell is still being spawned*,
 * with nothing to close yet. Setting a flag and closing on arrival is the only
 * ordering that cannot leak.
 *
 * The colour reasoning lives in `xtermTheme.mjs`, and the reason a PTY is a
 * Tauri command rather than a node route lives in `src-tauri/src/terminal.rs`.
 */

import { errorFields, log } from "../log";
import { useEffect, useRef, useState } from "react";
import type { FitAddon } from "@xterm/addon-fit";
import type { SearchAddon } from "@xterm/addon-search";
import type { SerializeAddon } from "@xterm/addon-serialize";
import type { Terminal as XTerminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { ICON, cn, useLinkHandler } from "../ui";
import type { WorkbenchScope } from "../routeModel.mjs";
import type { LinkHandler, LinkHit, LinkRootRef } from "../ui";
import { forgetScrollback, openTerminal, readScrollback, terminalAvailable, writeScrollback } from "./session";
import { linksAt } from "./terminalLinksModel.mjs";
import { typedFor } from "./typedKeysModel.mjs";
import { attachPathInput } from "./pathInput";
import { armed as armStart, due as startDue, nudgeText, onExit as startOnExit, onInput as startOnInput, onOutput as startOnOutput, pending as startPending, said as startSaid } from "./resumeStartModel.mjs";
import { failureTally } from "./checkpointModel.mjs";
import type { StartState } from "./resumeStartModel.mjs";
import { RELEASE_AFTER_MS, acquire, emptyPool, release as releaseSlot, wants } from "./webglPoolModel.mjs";
import { registerTerminalTail } from "./tails";
import type { TerminalScope, TerminalSession } from "./session";
import { currentScheme, fontSizeOf, monoFamily, readTerminalTokens } from "./tokens";
import { xtermTheme } from "./xtermTheme.mjs";
import { FindBar, emptyFind } from "../ui";
import { compileFind } from "../ui/find/findModel.mjs";
import { isMac } from "../ui/KeyHint";
import { chordFor, matchesEvent } from "../shell/keymapModel.mjs";
import { currentKeymap } from "../shell/shortcuts";
import type { Find } from "../ui";
import { t } from "../i18n/l10n.mjs";

export interface TerminalProps {
  /** Which rooted thing the shell opens in. Never a path — see `session.ts`. */
  scope: TerminalScope;
  id: string;
  /** A harness id from `GET /harnesses`, or `null`/absent for the login shell. */
  harness?: string | null;
  /**
   * A code host sign-in: the shell runs the CLI's browser login for this kind
   * at this host instead of a shell — the argv is the shell's own table.
   */
  login?: { kind: string; host: string } | null;
  /** Continue that harness's latest session here rather than starting a new one. */
  resume?: boolean;
  /** The project's run command runs here instead of a shell (ide/18). */
  run?: boolean;
  /** This checkout's Flutter app runs here on that device (ide/19). */
  mobileDevelopment?: { device: string } | null;
  /**
   * A resumed harness is nudged into motion once it has drawn its prompt
   * (`terminal.resume_start`, read by the panel). Meaningless without `resume`.
   */
  startOnResume?: boolean;
  /**
   * This is the visible pane.
   *
   * The panel keeps every terminal mounted — unmounting is what ends a PTY —
   * so this is how the one on screen learns to take the keyboard and re-fit.
   * It must **never** reach the boot effect's dependencies: a tab switch would
   * then respawn every shell it touched.
   */
  active?: boolean;
  /**
   * Whether this terminal is on screen at all — the pane it sits in is the
   * showing one. Drives the WebGL lease: visible terminals render on the GPU,
   * hidden ones give the slot back after a moment.
   */
  visible?: boolean;
  /**
   * A key the app reserves — a declared chord the shell must not see
   * (`keymapModel.interceptsInTerminal`). Returning true lets the event bubble
   * to the app's handler instead of reaching the PTY.
   */
  reserveKey?: (e: KeyboardEvent) => boolean;
  /** Fires once, when the shell exits on its own. Not on unmount. */
  onExit?: (code: number | null) => void;
  /**
   * The person typed: the bytes xterm encoded, after they went to the PTY.
   * What the owning host reads to say a harness's dialog was answered
   * (`useTerminals.terminalTyped`); never the platform's own resume nudge,
   * which is typed without passing here.
   */
  onInput?: (data: string) => void;
  /** The owning host has the shell: it spawned, or came back — with the live PTY's id. */
  onOpened?: (terminalId: string, session: string | null) => void;
  /** Contact was lost without a verdict — the spawn failed, the channel died. */
  onUnverifiable?: (reason: string) => void;
  /** The shell reads a harness under this shell (`Some`), or none any more (`null`) — the process table's word, every change once. */
  onProcess?: (harness: string | null) => void;
  /**
   * The tab's key, which names its scrollback checkpoint
   * (`run/terminals/<key>.scrollback`). Absent means no checkpointing.
   */
  sessionKey?: string | null;
  /** Replay the checkpoint before spawning — a tab restored after a restart. */
  replayCheckpoint?: boolean;
  /** Lines of scrollback the emulator keeps; `terminal.scrollback_lines`. */
  scrollback?: number;
  /** `terminal.font_family` / `terminal.font_size` / `terminal.cursor_style`; the theme's when absent. */
  fontFamily?: string;
  fontSize?: number;
  cursorStyle?: "block" | "bar" | "underline";
  className?: string;
}

/** How long after output stops before the buffer is checkpointed. */
const CHECKPOINT_DEBOUNCE_MS = 2_000;
/** How often a resume nudge in flight asks the model whether it is due. */
const START_TICK_MS = 250;

/**
 * Everything one mount owns, in one object.
 *
 * A record rather than a pile of refs so the cleanup below is a single list of
 * things to take down, in order, with no way to add a resource in the async
 * setup and forget it in the synchronous teardown.
 */
interface Live {
  disposed: boolean;
  term: XTerminal | null;
  fit: FitAddon | null;
  /**
   * The PTY behind this mount, kept after its process exits: the tab still
   * owns it, and closing the tab is what tells the host — and through it
   * the roster — that the tab is gone.
   */
  session: TerminalSession | null;
  /** The process ended; nothing more is written or resized. */
  exited: boolean;
  /** The latest `fontSize` prop, for the theme observer — a setting changed after boot. */
  fontSize: number | undefined;
  resize: ResizeObserver | null;
  theme: MutationObserver | null;
  /** Hands this pane's WebGL slot back to the pool. Idempotent. */
  releaseWebgl: (() => void) | null;
  /** The WebGL addon while this terminal holds a slot. */
  webgl: { dispose(): void } | null;
  /** The mount's key in the WebGL pool. */
  poolKey: string;
  /** Whether the pane is in sight — what an addon import that lands late must check before attaching. */
  wanted: boolean;
  /** A pending hand-back of the slot, armed when the terminal went out of sight. */
  webglRelease: number | null;
  serialize: SerializeAddon | null;
  search: SearchAddon | null;
  checkpointTimer: number | null;
  unregisterTail: (() => void) | null;
  /** Takes a pasted or dropped file as its path (`pathInput.ts`); its teardown. */
  pathInput: (() => void) | null;
  /** The resume nudge in flight, and its tick — `resumeStartModel`. */
  start: StartState | null;
  startTimer: number | null;
}

/** What the find bar shows: the kit's query, and where the addon stands. */
interface Finding {
  find: Find;
  /** `-1` for "no match" once a search ran; `null` before any. */
  index: number | null;
  count: number | null;
}

/** Attributes on `<html>` that mean the palette moved. */
const THEME_ATTRIBUTES = ["data-theme", "data-accent", "data-scheme", "data-density", "data-font-ui", "data-font-mono", "style"];

/**
 * The WebGL lease (`webglPoolModel.mjs`): visible terminals hold the GPU
 * renderer, hidden ones hand it back after a moment. One pool for the page.
 */
let webglPool = emptyPool();

/**
 * The search addon reports results from inside `boot`, which has no React
 * state; each mount registers a listener keyed by its `Live` and reads the
 * last result back. Tiny on purpose — one number pair per terminal.
 */
const findResults = { current: { index: -1, count: 0 } };
const findResultListeners = new Set<(live: Live) => void>();

export function Terminal({
  scope,
  id,
  harness,
  login = null,
  resume = false,
  run = false,
  mobileDevelopment = null,
  startOnResume = false,
  active,
  onExit,
  onInput,
  onOpened,
  onUnverifiable,
  onProcess,
  sessionKey = null,
  replayCheckpoint = false,
  scrollback = 5000,
  fontFamily,
  fontSize,
  cursorStyle,
  className,
  visible,
  reserveKey,
}: TerminalProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [booted, setBooted] = useState(0);
  const [finding, setFinding] = useState<Finding | null>(null);
  // Read through refs inside the effect so a caller passing an inline arrow —
  // or flipping `active` — does not tear down and respawn the shell.
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;
  const onInputRef = useRef(onInput);
  onInputRef.current = onInput;
  const onOpenedRef = useRef(onOpened);
  onOpenedRef.current = onOpened;
  const onUnverifiableRef = useRef(onUnverifiable);
  onUnverifiableRef.current = onUnverifiable;
  const onProcessRef = useRef(onProcess);
  onProcessRef.current = onProcess;
  const findRef = useRef<((open: boolean) => void) | null>(null);
  // Read every render so a click in a long-lived shell reaches the current
  // provider; the boot effect must not re-run for it.
  const linkRef = useRef<LinkHandler | null>(null);
  linkRef.current = useLinkHandler();
  const reserveKeyRef = useRef(reserveKey);
  reserveKeyRef.current = reserveKey;
  const activeRef = useRef(active);
  activeRef.current = active;
  /** What this mount owns, reachable from the visibility effect below. */
  const liveRef = useRef<Live | null>(null);

  // A setting that changed under a running shell reaches it: the options are
  // the emulator's to take live, and the boot effect must not re-run for
  // them — that would end and respawn the shell.
  useEffect(() => {
    const live = liveRef.current;
    const host = hostRef.current;
    const term = live?.term;
    if (!live || !term || !host) return;
    live.fontSize = fontSize;
    term.options.fontFamily = fontFamily ?? monoFamily();
    term.options.fontSize = fontSize ?? fontSizeOf(host);
    term.options.cursorStyle = cursorStyle ?? "block";
    term.options.scrollback = scrollbackLines(scrollback);
    if (live.fit) safeFit(live.fit);
  }, [booted, fontFamily, fontSize, cursorStyle, scrollback]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const live: Live = {
      disposed: false,
      term: null,
      fit: null,
      session: null,
      exited: false,
      fontSize,
      resize: null,
      theme: null,
      releaseWebgl: null,
      webgl: null,
      poolKey: `${scope}:${id}:${Math.random().toString(36).slice(2)}`,
      wanted: false,
      webglRelease: null,
      serialize: null,
      search: null,
      checkpointTimer: null,
      start: null,
      startTimer: null,
      unregisterTail: null,
      pathInput: null,
    };
    liveRef.current = live;
    setError(null);
    findRef.current = (open) => {
      if (!open) {
        setFinding(null);
        live.search?.clearDecorations();
        live.term?.focus();
        return;
      }
      setFinding((f) => f ?? { find: emptyFind(), index: null, count: null });
    };

    void boot(
      host,
      { scope, id, harness: harness ?? null, login: login ?? null, resume, run, mobileDevelopment: mobileDevelopment ?? null, startOnResume, sessionKey, replayCheckpoint, scrollback, fontFamily, fontSize, cursorStyle },
      activeRef,
      live,
      { onExit: onExitRef, onInput: onInputRef, onOpened: onOpenedRef, onUnverifiable: onUnverifiableRef, onProcess: onProcessRef, find: findRef, reserveKey: reserveKeyRef, link: linkRef },
      setError,
      () => setBooted((n) => n + 1),
    );

    return () => {
      live.disposed = true;
      if (live.checkpointTimer !== null) window.clearTimeout(live.checkpointTimer);
      if (live.startTimer !== null) window.clearInterval(live.startTimer);
      // The last word before the mount goes: whatever is on screen now —
      // unless the tab is closing and its checkpoint was dropped for good.
      checkpoint(live, sessionKey);
      if (sessionKey) forgotten.delete(sessionKey);
      live.unregisterTail?.();
      live.unregisterTail = null;
      live.pathInput?.();
      live.pathInput = null;
      live.resize?.disconnect();
      live.theme?.disconnect();
      if (live.webglRelease !== null) window.clearTimeout(live.webglRelease);
      live.webgl?.dispose();
      live.webgl = null;
      live.releaseWebgl?.();
      live.releaseWebgl = null;
      // Fire and forget: the shell has to die whether or not this promise is
      // ever settled, and there is no component left to tell if it fails. The
      // Rust side treats closing a terminal that has already gone as success,
      // so this cannot reject for the one race it is most likely to hit.
      void live.session?.close().catch((e: unknown) => log.warn("terminal", "a shell could not be closed on unmount", errorFields(e)));
      live.session = null;
      live.term?.dispose();
      live.term = null;
      if (liveRef.current === live) liveRef.current = null;
    };
    // `active` is deliberately absent. It changes on every tab switch, and a
    // tab switch that re-ran this effect would end and respawn the shell —
    // which is the whole thing the panel keeps every terminal mounted to avoid.
    // Visibility is handled by the effect below instead.
    // `resume` is deliberately absent: it is decided when the tab opens and
    // carried on the session, so it cannot change under a live shell. Listing
    // it would only add a way to end one.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, id, harness, login?.kind, login?.host, run, mobileDevelopment?.device]);

  // Search results arrive from the addon; mirror the ones for this mount into
  // the find bar when it is open.
  useEffect(() => {
    const listener = (live: Live) => {
      if (live !== liveRef.current) return;
      setFinding((f) => (f ? { ...f, index: findResults.current.index, count: findResults.current.count } : f));
    };
    findResultListeners.add(listener);
    return () => {
      findResultListeners.delete(listener);
    };
  }, []);

  // Becoming the visible pane: take the keyboard, and re-measure in case the
  // panel was resized while this one was hidden. Inactive panes keep their full
  // layout box (see `TerminalPanel.tsx`), so this is a cheap no-op far more
  // often than not — `fit()` does nothing when the geometry has not moved.
  useEffect(() => {
    if (!active) return;
    const live = liveRef.current;
    if (!live?.term || !live.fit) return;
    safeFit(live.fit);
    live.term.focus();
  }, [active]);

  // The WebGL lease: on screen → take a slot (when one is free); out of sight
  // for a moment → hand it back and draw with the DOM renderer. `booted`
  // re-runs this once the terminal exists, so a pane that was visible while
  // its shell spawned gets the GPU too.
  useEffect(() => {
    const live = liveRef.current;
    if (!live?.term) return;
    live.wanted = wants({ visible });
    if (live.wanted) {
      if (live.webglRelease !== null) {
        window.clearTimeout(live.webglRelease);
        live.webglRelease = null;
      }
      if (!live.webgl) void attachWebgl(live.term, live);
      return;
    }
    if (!live.webgl || live.webglRelease !== null) return;
    live.webglRelease = window.setTimeout(() => {
      live.webglRelease = null;
      live.webgl?.dispose();
      live.webgl = null;
      live.releaseWebgl?.();
      live.releaseWebgl = null;
    }, RELEASE_AFTER_MS);
  }, [visible, booted]);

  const runFind = (dir: "next" | "previous", f: Finding) => {
    const live = liveRef.current;
    // A pattern that does not compile — `(` in regex mode — is the bar's
    // state to show, never something handed to the search addon to throw on.
    if (!live?.search || !compileFind(f.find)) return;
    const opts = { regex: f.find.regex, caseSensitive: f.find.caseSensitive, decorations: FIND_DECORATIONS };
    if (dir === "next") live.search.findNext(f.find.query, opts);
    else live.search.findPrevious(f.find.query, opts);
  };

  return (
    <div
      className={cn(
        "relative h-full min-h-0 w-full overflow-hidden rounded-card border border-border bg-surface",
        className,
      )}
    >
      <div ref={hostRef} className="h-full w-full p-2" />
      {finding && (
        <FindBar
          className="absolute right-2 top-2 z-10"
          label={t("terminal-terminal-find-terminal")}
          find={finding.find}
          index={finding.index}
          count={finding.count}
          onChange={(find) => {
            const next = { ...finding, find };
            setFinding(next);
            runFind("next", next);
          }}
          onStep={(dir) => runFind(dir, finding)}
          onClose={() => findRef.current?.(false)}
        />
      )}
      {error && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-surface px-6 text-center">
          <ICON.warn size={20} aria-hidden className="text-danger" />
          <p className="max-w-md text-xs text-text-dim">{error}</p>
        </div>
      )}
    </div>
  );
}

/**
 * Mount xterm, spawn the shell, wire the three things that can change.
 *
 * Split out of the effect because every `await` in it needs the same guard,
 * and a chain of `if (live.disposed)` reads far better in one function than
 * threaded through a closure that also has to return a cleanup.
 */
interface BootWhat {
  scope: TerminalScope;
  id: string;
  harness: string | null;
  login: { kind: string; host: string } | null;
  resume: boolean;
  /** The project's run command runs here (ide/18). */
  run: boolean;
  /** The checkout's Flutter app runs here on that device (ide/19). */
  mobileDevelopment: { device: string } | null;
  startOnResume: boolean;
  sessionKey: string | null;
  replayCheckpoint: boolean;
  scrollback: number;
  fontFamily?: string;
  fontSize?: number;
  cursorStyle?: "block" | "bar" | "underline";
}

interface BootRefs {
  onExit: { current: ((code: number | null) => void) | undefined };
  /** The person's keystrokes, after the PTY has them. */
  onInput: { current: ((data: string) => void) | undefined };
  onOpened: { current: ((terminalId: string, session: string | null) => void) | undefined };
  onUnverifiable: { current: ((reason: string) => void) | undefined };
  onProcess: { current: ((harness: string | null) => void) | undefined };
  find: { current: ((open: boolean) => void) | null };
  reserveKey: { current: ((e: KeyboardEvent) => boolean) | undefined };
  /** The door a path or a URL in the output opens through (ide/17); null outside the provider. */
  link: { current: LinkHandler | null };
}

/** How matches are drawn; the colours are xterm's own theme-aware defaults. */
const FIND_DECORATIONS = {
  matchOverviewRuler: "#888888",
  activeMatchColorOverviewRuler: "#ffffff",
  matchBackground: "#6b6b6b55",
  activeMatchBackground: "#c9a22766",
};

/**
 * Save the buffer as a replayable escape stream. Best effort and silent: a
 * checkpoint that failed is a restart with a blank tab, which is what every
 * run before this existed did.
 */
/** `terminal.scrollback_lines`, clamped to sanity. */
function scrollbackLines(lines: number): number {
  return Math.max(200, Math.min(100_000, Math.round(lines) || 5000));
}

/**
 * Tabs whose checkpoint was dropped on purpose — a tab being closed — so the
 * unmount that follows the close does not write it back. The key leaves the
 * set when its mount is gone.
 */
const forgotten = new Set<string>();

/** Drop a tab's checkpoint, and keep it dropped through the tab's unmount. */
export function dropScrollback(key: string): Promise<void> {
  forgotten.add(key);
  checkpointRuns.forget(key);
  return forgetScrollback(key);
}

/** Checkpoints that threw in a row, by terminal — when a run of them is said is the model's (`checkpointModel`). */
const checkpointRuns = failureTally();

function checkpoint(live: Live, key: string | null) {
  if (!key || !live.term || !live.serialize || forgotten.has(key)) return;
  try {
    const data = live.serialize.serialize();
    checkpointRuns.succeeded(key);
    void writeScrollback(key, data).catch((e: unknown) => log.warn("terminal", "a scrollback checkpoint could not be written", { key, ...errorFields(e) }));
  } catch (e) {
    // The addon threw on a buffer mid-reflow; the next debounce tries again —
    // and a third failure in a row is said once, not swallowed forever.
    if (checkpointRuns.failed(key)) log.warn("terminal", "the scrollback could not be serialised three times in a row", { key, ...errorFields(e) });
  }
}

function scheduleCheckpoint(live: Live, key: string | null) {
  if (!key || live.disposed) return;
  if (live.checkpointTimer !== null) window.clearTimeout(live.checkpointTimer);
  live.checkpointTimer = window.setTimeout(() => {
    live.checkpointTimer = null;
    if (!live.disposed) checkpoint(live, key);
  }, CHECKPOINT_DEBOUNCE_MS);
}

async function boot(
  host: HTMLDivElement,
  what: BootWhat,
  activeRef: { current: boolean | undefined },
  live: Live,
  refs: BootRefs,
  setError: (message: string | null) => void,
  onBooted: () => void,
) {
  const { scope, id, harness, login, resume, run, mobileDevelopment, startOnResume, sessionKey, replayCheckpoint, scrollback, fontFamily, fontSize, cursorStyle } = what;
  if (!terminalAvailable()) {
    setError(t("terminal-terminal-shell-needs-desktop-app-running-browser"));
    return;
  }

  // Dynamic so xterm and its addons are their own chunk: this is a panel most
  // sessions never open, and it is the largest dependency in the app.
  const [{ Terminal: XTerm }, { FitAddon }, { SerializeAddon }, { SearchAddon }] = await Promise.all([
    import("@xterm/xterm"),
    import("@xterm/addon-fit"),
    import("@xterm/addon-serialize"),
    import("@xterm/addon-search"),
  ]);
  if (live.disposed) return;

  const term = new XTerm({
    theme: xtermTheme(readTerminalTokens(), currentScheme()),
    fontFamily: fontFamily ?? monoFamily(),
    fontSize: fontSize ?? fontSizeOf(host),
    cursorStyle: cursorStyle ?? "block",
    // 13px mono at 1.0 line height is a wall. This is the smallest step that
    // makes stacked output scannable without wasting a row of a short panel.
    lineHeight: 1.3,
    cursorBlink: true,
    // Deep enough that a build log is still there when you scroll back, small
    // enough that a runaway `yes` does not become the app's memory profile.
    // `terminal.scrollback_lines` from settings, clamped to sanity.
    scrollback: scrollbackLines(scrollback),
    // The webview's own right-click menu would otherwise eat the selection.
    rightClickSelectsWord: true,
    allowProposedApi: true,
  });
  live.term = term;
  term.open(host);
  // A file pasted or dropped here is its path, as a Mac terminal types it;
  // a screenshot is saved to a file first. Plain text stays xterm's (`pathInput.ts`).
  live.pathInput = attachPathInput(host, term, () => !live.disposed);

  const fit = new FitAddon();
  live.fit = fit;
  term.loadAddon(fit);
  safeFit(fit);

  const serialize = new SerializeAddon();
  live.serialize = serialize;
  term.loadAddon(serialize);
  if (sessionKey) {
    live.unregisterTail = registerTerminalTail(sessionKey, () => {
      const buf = term.buffer.active;
      const out: string[] = [];
      for (let i = 0; i < buf.length; i++) out.push(buf.getLine(i)?.translateToString(true) ?? "");
      return out;
    });
  }
  const search = new SearchAddon();
  live.search = search;
  term.loadAddon(search);

  // A URL or a path a build or a harness prints — a sign-in link, a docs
  // page, the `src/main.rs:42` in a compiler's error — opens with ⌘-click
  // (Ctrl-click elsewhere) through the link handler (ide/17): the card asks
  // before the browser, the menu before a file. A plain click keeps
  // selecting text. A relative path is read from **where the shell stands**
  // — its current directory, asked of the shell at the click (`cwd`), since
  // the tool that printed the path ran there: `src/lib.rs` after a `cd` into
  // a crate, `../README.md`, a `target/out.log` no index lists — then from
  // the terminal's own root. A shell that cannot say where it is reads from
  // its root alone; a machine-scoped shell (a code host sign-in) has no
  // root, and the handler falls back to every checkout on disk. One provider
  // over the app's one scanner (`findLinks`) for URLs and paths alike,
  // reading the *logical* line a row belongs to (`terminalLinksModel`): a
  // link the row edge cut in two — folded by xterm, or broken by a harness's
  // own frame — is one door, underlined across its rows.
  const roots: LinkRootRef[] = scope === "machine" ? [] : [{ scope: scope as WorkbenchScope, id }];
  const open = (hit: LinkHit, e: MouseEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const at = { x: e.clientX, y: e.clientY };
    const standing: Promise<string | null> = live.session?.cwd().catch(() => null) ?? Promise.resolve(null);
    void standing.then((from) => refs.link.current?.onLink(hit, at, roots, { from }));
  };
  const rowAt = (i: number) => {
    const line = term.buffer.active.getLine(i);
    return line ? { text: line.translateToString(false), wrapped: line.isWrapped } : null;
  };
  term.registerLinkProvider({
    provideLinks(y, callback) {
      callback(linksAt(rowAt, y - 1, term.cols).map((link) => ({ range: link.range, text: link.text, activate: (e) => open(link.hit, e) })));
    },
  });

  // A Mac terminal's text editing first: ⌘←/⌘→, ⌥←/⌥→, ⌘⌫, ⌥⌦ type the
  // keys a line editor reads (`typedKeysModel`; which chord is the keymap's)
  // through the same door as typing, and the keystroke ends here. Then the
  // app's reserved chords — a split, pane focus — bubble past the shell; then
  // the keymap's `find` chord (⌘F / Ctrl+F by default — a rebinding follows)
  // opens the find bar. Returning false tells xterm the keystroke is not its
  // business, and the DOM event goes on up.
  term.attachCustomKeyEventHandler((e) => {
    if (e.type !== "keydown") return true;
    const typed = typedFor(currentKeymap(), e, isMac);
    if (typed !== null) {
      e.preventDefault();
      e.stopPropagation();
      term.input(typed, true);
      return false;
    }
    if (refs.reserveKey.current?.(e)) return false;
    const find = chordFor(currentKeymap(), "find");
    if (find && matchesEvent(e, find, isMac)) {
      refs.find.current?.(true);
      return false;
    }
    return true;
  });

  // The WebGL renderer is the visibility effect's to attach (the lease), and
  // it needs the canvas the terminal only creates once it is in the document —
  // which is now.
  onBooted();

  // A restored tab: the last run's output first, then a line saying where it
  // ended, then the fresh shell below. The PTY does not survive; the output does.
  if (replayCheckpoint && sessionKey) {
    try {
      const saved = await readScrollback(sessionKey);
      if (live.disposed) return;
      if (saved) {
        term.write(saved);
        // The divider between the last run's output and the fresh shell, dimmed — the words the catalog's.
        term.write(`\r\n\x1b[2m── ${t("shell-terminal-restored-divider", { when: new Date().toLocaleString() })} ──\x1b[0m\r\n`);
      }
    } catch (e) {
      // No checkpoint to replay, or one that could not be read: the shell simply starts fresh.
      log.debug("terminal", "no scrollback checkpoint replayed", errorFields(e));
    }
  }

  let session: TerminalSession;
  // Throttle the activity ping to at most one every 400ms: a streaming turn is
  // one cheap signal, not one per chunk.
  try {
    session = await openTerminal(scope, id, harness, resume, login, term.rows, term.cols, (event) => {
      if (live.disposed) return;
      if (event.kind === "output") {
        term.write(event.data);
        if (live.start) live.start = startOnOutput(live.start, Date.now());
        scheduleCheckpoint(live, sessionKey);
        return;
      }
      if (event.kind === "process") {
        refs.onProcess.current?.(event.harness);
        return;
      }
      // The shell exited on its own. Say so in the terminal rather than
      // clearing it: whatever went wrong is on the rows above, and a panel
      // that blanks itself at exit destroys the only evidence.
      const what = mobileDevelopment ? "flutter" : (harness ?? "shell");
      term.write(`\r\n\x1b[2m[${what} exited${event.code === null ? "" : ` (${event.code})`}]\x1b[0m\r\n`);
      live.exited = true;
      if (live.start) live.start = startOnExit(live.start);
      if (live.checkpointTimer !== null) window.clearTimeout(live.checkpointTimer);
      live.checkpointTimer = null;
      checkpoint(live, sessionKey);
      refs.onExit.current?.(event.code);
    }, run, mobileDevelopment);
  } catch (e) {
    const reason = e instanceof Error ? e.message : String(e);
    setError(reason);
    // No shell, no verdict: the host never had it. Unverifiable, not exited.
    refs.onUnverifiable.current?.(reason);
    return;
  }

  // The unmount that happened while the shell was being spawned: there was
  // nothing to close then, so close it now. Without this, every StrictMode
  // remount and every fast navigation leaves a shell behind.
  if (live.disposed) {
    void session.close().catch((e: unknown) => log.warn("terminal", "a shell spawned into an unmounted tab could not be closed", errorFields(e)));
    return;
  }
  live.session = session;
  refs.onOpened.current?.(session.id, session.session);

  // A resumed harness picks its work up: once it has drawn its prompt, the
  // tab types the nudge and Enter through the same path a keystroke takes
  // (`resumeStartModel`). A sign-in shell is a form, not a session — never.
  if (resume && startOnResume && harness && !login) {
    live.start = armStart(Date.now());
    live.startTimer = window.setInterval(() => {
      const now = Date.now();
      const start = live.start;
      if (!start || live.disposed || !startPending(start, now)) {
        if (live.startTimer !== null) window.clearInterval(live.startTimer);
        live.startTimer = null;
        return;
      }
      if (!startDue(start, now)) return;
      live.start = startSaid(start);
      void session.write(nudgeText()).catch((e: unknown) => log.debug("terminal", "the resume nudge could not be typed", errorFields(e)));
    }, START_TICK_MS);
  }

  term.onData((data) => {
    if (live.exited) return;
    // The person's words go first: a nudge not yet said is dropped for good.
    if (live.start) live.start = startOnInput(live.start);
    void session.write(data).catch((e: unknown) => {
      // The shell is gone; its exit event is on its way up the channel and
      // says so properly.
      log.debug("terminal", "the typed bytes could not be written; the exit event says why", errorFields(e));
    });
    // The host hears what was typed — how a harness's dialog is known answered.
    refs.onInput.current?.(data);
  });

  search.onDidChangeResults(({ resultIndex, resultCount }) => {
    // Read by the find bar through React state; the bar only exists while
    // open, so an update to a closed bar is a no-op there.
    findResults.current = { index: resultIndex, count: resultCount };
    findResultListeners.forEach((l) => l(live));
  });

  // xterm decides the new geometry, so this is the authority on what the PTY
  // should be told — not the ResizeObserver, which only knows about pixels.
  term.onResize(({ rows, cols }) => {
    if (live.exited) return;
    void live.session?.resize(rows, cols).catch((e: unknown) => log.debug("terminal", "a resize did not reach the shell", { rows, cols, ...errorFields(e) }));
  });

  const observer = new ResizeObserver(() => safeFit(fit));
  observer.observe(host);
  live.resize = observer;

  // A MutationObserver rather than `watchAppearance`, because it catches all
  // four dials *and* the system flipping to dark under a "system" choice —
  // which moves `data-scheme` without any preference having changed. One
  // mechanism, no way for a new dial to be added and missed.
  const themeObserver = new MutationObserver(() => {
    if (live.disposed) return;
    term.options.theme = xtermTheme(readTerminalTokens(), currentScheme());
    const size = live.fontSize ?? fontSizeOf(host);
    if (size !== term.options.fontSize) {
      term.options.fontSize = size;
      safeFit(fit);
    }
  });
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: THEME_ATTRIBUTES,
  });
  live.theme = themeObserver;

  // Read at the end rather than from an argument: a terminal can finish
  // spawning while it is already the visible pane, and one that finished while
  // hidden must not steal the keyboard from the pane you are looking at.
  if (activeRef.current) term.focus();
}

/**
 * Take a WebGL slot for this terminal, when the pool has one and the webview
 * has WebGL at all; otherwise the DOM renderer keeps drawing.
 *
 * Never fatal. A terminal that renders slowly is a terminal; a terminal that
 * refused to load because the GPU was busy is a blank panel.
 */
async function attachWebgl(term: XTerminal, live: Live) {
  const asked = acquire(webglPool, live.poolKey);
  if (!asked.granted) return;
  webglPool = asked.pool;
  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    webglPool = releaseSlot(webglPool, live.poolKey);
  };
  try {
    const { WebglAddon } = await import("@xterm/addon-webgl");
    // The import landed late: a pane that went out of sight meanwhile
    // hands its slot back at once rather than holding it unseen.
    if (live.disposed || live.webgl || !live.wanted) {
      release();
      return;
    }
    const webgl = new WebglAddon();
    // A lost context is normal, not exceptional: sleep/wake, a driver reset,
    // another view exhausting the browser's context budget. Disposing the
    // addon hands rendering back to the DOM renderer. Keeping it would leave a
    // terminal that is running fine and drawing nothing.
    webgl.onContextLoss(() => {
      release();
      webgl.dispose();
      if (live.webgl === webgl) live.webgl = null;
    });
    live.releaseWebgl = release;
    live.webgl = webgl;
    term.loadAddon(webgl);
  } catch {
    // No WebGL here. The DOM renderer is already what is drawing.
    release();
  }
}

/**
 * Fit, unless there is nothing to fit into.
 *
 * A collapsed or hidden container makes `proposeDimensions` return zero or
 * `NaN`, and xterm throws rather than resizing to nothing. That happens on
 * every mount inside a tab that is not the active one.
 */
function safeFit(fit: FitAddon) {
  const proposed = fit.proposeDimensions();
  if (!proposed || !Number.isFinite(proposed.rows) || !Number.isFinite(proposed.cols)) return;
  if (proposed.rows < 1 || proposed.cols < 1) return;
  fit.fit();
}
