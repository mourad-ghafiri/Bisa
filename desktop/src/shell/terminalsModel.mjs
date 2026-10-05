/**
 * Which shells are open and which one you are looking at — the whole rule,
 * with no React in it. Where they are *drawn* is the shell's business: the
 * panel is positioned over the workbench's centre (`shell/layerSlots.ts`),
 * so there is no "open" and no height here any more.
 *
 * A `<Terminal />` is a real process, so the state that decides how many are
 * mounted is the state most worth being able to test. There is no jsdom in this
 * repo, so it lives here as plain `.mjs` (with a `.d.mts` beside it, following
 * `ui/fileTreeModel.mjs`) and the component is paint.
 *
 * The `Model` suffix is load-bearing, not decoration. Named `terminals.mjs`
 * beside a `Terminals.tsx` it would differ only in case, and on a
 * case-insensitive filesystem (every default macOS one) Rollup resolves the
 * import to *this* file and fails the production build with "… is not
 * exported" — while `tsc`, which resolves case-sensitively, passes. A green
 * typecheck and a red `npm run build` is a bad half-hour.
 *
 * # What replaces "there is nowhere to put the second one"
 *
 * The drawer this succeeds held a single target or `null`, and its doc was
 * blunt about why: *there is no arrangement of these functions that leaves two
 * shells running, because there is nowhere to put the second one.* That
 * invariant was doing two jobs at once, and only one of them was about
 * duplicates.
 *
 * 1. **No duplicate PTY behind one tab.** Now a property of this module:
 *    `key` is minted from a strictly increasing `seq` and never reused, and
 *    `generation` only ever increases, so `mountKey` is unique across
 *    `sessions` at every point in every sequence of transitions. That is
 *    testable, and it is tested.
 * 2. **No orphaned PTY.** Now a property of `TerminalPanel.tsx`, which renders
 *    `sessions` **directly** — no local copy, no memo held across renders, no
 *    portal. A session removed from this list has its `<Terminal />` unmounted,
 *    and unmounting closes the PTY. That one cannot be enforced here, which is
 *    exactly why it is written at the top of that file too.
 *
 * There is **no cap**. Opening as many shells and harnesses as the work needs
 * is the point of this rewrite. What the old limit of one was really protecting
 * against — a machine quietly carrying dozens of forgotten `zsh` processes — is
 * answered by (2) plus visibility: every session has a tab, always, even when
 * it belongs to another place, and a dead one keeps its tab until it is
 * closed rather than being swept away.
 *
 * # Identity is the change signal
 *
 * Every transition that changes nothing returns the **same object**. The store
 * notifies on identity, and a notification re-renders the panel; a new-but-equal
 * object would remount a terminal, and remounting a terminal ends a shell.
 * `setHeight` in particular runs on every pointer move of a drag.
 */

import { addTab, closeLeaf, findLeaf, leafOfTab, leaves, moveWithin, neighbor, parseTree, removeTab, setActiveTab, setRatio, singleLeaf, splitLeaf } from "./paneTreeModel.mjs";
import { isEnded, stateOf } from "../ui/sessionState.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/**
 * Unix seconds now — the default clock for a shell's open and exit instants.
 * The reducers take an explicit `at` so they stay pure under test;
 * this is only the default when a caller does not pass one.
 */
export function nowSecs() {
  return Math.floor(Date.now() / 1000);
}

/**
 * Where a shell may be rooted: the places the workbench roots at — the node's
 * `FileScope` but `run` (a project's own tree is its primary workstream, so
 * there is no project scope; a run of the workspace is reached through its
 * work items, and a tab rooted at one would have no workbench to open in) —
 * and `machine`, the desktop's own: the person's home directory, where a code
 * host sign-in runs. `machine` never reaches a placement route: the shell
 * resolves it itself (`sessionDoors.ts`).
 */
export const TERMINAL_SCOPES = ["goal", "workstream", "work_item", "machine"];

/**
 * Three values, not two (ide/06). The rule: **loss of contact
 * is never evidence of process death.** A tab restored from a checkpoint
 * whose shell has not been respawned is `unverifiable`, not `exited`; an IPC
 * channel that stopped answering is `unverifiable`; only a positive exit from
 * the host that owns the PTY is `exited`.
 */
export const LIVE = Object.freeze({ status: "live" });

/** @param {string} reason */
export function unverifiable(reason) {
  return { status: "unverifiable", reason };
}

/** @param {number | null} code */
export function exited(code) {
  return { status: "exited", code: code ?? null };
}

/** What a restored tab says until its shell is back. */
export const RESTORED_REASON = tr("shell-terminals-restored-from-checkpoint-shell-has-been");

export function emptyTerminals() {
  return {
    sessions: [],
    active: null,
    seq: 0,
    launched: [],
    // The pane tree over the tabs. One leaf until somebody splits.
    panes: singleLeaf("p1"),
    paneSeq: 1,
    focusedPane: "p1",
  };
}

/**
 * How many places we remember having started a harness in.
 *
 * A bound rather than a design decision: the list only answers "has this one
 * run here before", and a workspace with two hundred distinct harness/place
 * pairs behind it has long since stopped caring about the oldest.
 */
export const LAUNCH_MEMORY = 200;

/**
 * The identity of "this harness, in this place".
 *
 * `null` for a plain shell — it has no session to continue, so there is nothing
 * to remember about one.
 *
 * **Joined on NUL, which no component can contain.** A readable separator was
 * the first attempt and it collided: harness ids carry colons (`preset:goose`,
 * `acp:omp`, `custom:mine`), so a `:`-joined key made `preset:goose` in place
 * `a` and `goose` in a place called `a:preset` the same string. That held only
 * as long as no id ever contained a colon, which is an assumption about every
 * future id rather than a property of this function. Nothing parses the key
 * back, so legibility buys nothing and ambiguity costs a wrong resume.
 */
export function launchKey(harness, scope, id) {
  return harness ? `${scope}\u0000${id}\u0000${harness}` : null;
}

/** Whether this harness has been started in this place before. */
export function hasLaunched(launched, key) {
  return key !== null && launched.includes(key);
}

/**
 * Note that a harness has run here, most recent first.
 *
 * **Why this is remembered at all.** `claude --continue` in a directory with no
 * prior conversation is an error, not an empty session — it prints its
 * complaint and exits. So the resume flag cannot be unconditional, and the
 * cheapest true answer to "is there something to continue" is whether we have
 * ever started one here. Nothing on the node knows: a terminal harness in a
 * terminal is not an engine session, and deliberately never becomes one.
 *
 * Deduped and capped, and the existing entry *moves* to the front rather than
 * being left where it was — otherwise the place you use every day is the one
 * that ages out.
 */
export function rememberLaunch(launched, key, cap = LAUNCH_MEMORY) {
  if (key === null) return launched;
  if (launched[0] === key) return launched;
  return [key, ...launched.filter((k) => k !== key)].slice(0, cap);
}

/**
 * The React key one mount is keyed by — **not** the same as the tab's key.
 *
 * The two are separate on purpose. A restart has to keep the tab *where it is*,
 * in the same position, still selected (that is `key`), while forcing React to
 * unmount the dead terminal before mounting a live one (that is `generation`).
 * Fold the generation into the tab key and a restart makes the tab jump to a
 * fresh identity, losing its place in the strip and its selection.
 */
export function mountKey(session) {
  return session ? `${session.key}#${session.generation}` : "";
}

/** Whether a session is rooted in this exact thing. */
export function isRootedAt(session, scope, id) {
  return !!session && session.scope === scope && session.id === id;
}

/** Live means the owning host has the shell and has not reported an exit. */
export function isLive(session) {
  return !!session && session.liveness.status === "live";
}

/** Positively exited — the only state a sweep may act on. */
export function isExited(session) {
  return !!session && session.liveness.status === "exited";
}

/**
 * A roster session as the tab that claims it says: a tab that exited ends
 * the session it hosts as of its exit — *done* on a clean status, *failed*
 * otherwise — with no sub-agents, since the process they ran in is gone. A
 * live tab, or no tab, leaves the roster's word. The node says the same once
 * the tab's exit reaches it; this is the desktop not waiting to be told.
 * @template {{state: object, since: number, children?: object[]}} S
 * @param {S} session
 * @param {TerminalSessionState | null | undefined} tab
 * @returns {S}
 */
export function settledByTab(session, tab) {
  // A claimed tab's word is its session's: a session the roster already
  // ended — done, failed, aborted — keeps its own end and its reason.
  if (!tab || !isExited(tab) || isEnded(session.state)) return session;
  const code = tab.liveness.code;
  const state = code === 0 ? { state: "done" } : { state: "failed", reason: code == null ? tr("shell-terminals-process-ended") : tr("shell-terminals-exited-status", { code }) };
  return { ...session, state, since: tab.exitedAt ?? session.since, children: [] };
}

/**
 * The roster as the tabs say it: every session through {@link settledByTab}
 * with the tab that claims it. The one roster every surface that marks or
 * counts sessions reads — the rail's rows and marks, the Workstreams panel,
 * the footer, the pet's counts, the tray, the Agents screen — so a tab that
 * exited settles its mark, its pill and its count alike, and no two of
 * them disagree about one session. The same array when no tab changes a
 * row, so a memo holds.
 * @template {{id: string, state: object, since: number, children?: object[]}} S
 * @param {readonly S[]} sessions the roster (`SessionRow[]`)
 * @param {readonly {sessionId?: string | null}[]} terminals the tabs
 * @returns {readonly S[]}
 */
export function settledRoster(sessions, terminals) {
  const tabs = new Map();
  for (const t of terminals ?? []) if (t.sessionId) tabs.set(t.sessionId, t);
  let changed = false;
  const settled = (sessions ?? []).map((s) => {
    const next = settledByTab(s, tabs.get(s.id));
    if (next !== s) changed = true;
    return next;
  });
  return changed ? settled : sessions ?? [];
}

/**
 * Unix seconds the wait a row shows began — the session's own, else the
 * first waiting sub-agent's (a child speaks only to ask); `null` when nothing
 * on the row waits.
 * @param {{state: object, since: number, children?: readonly {state: object, since: number}[]} | null | undefined} row
 * @returns {number | null}
 */
function waitSince(row) {
  if (!row) return null;
  if (stateOf(row.state) === "waiting") return row.since;
  return (row.children ?? []).find((c) => stateOf(c.state) === "waiting")?.since ?? null;
}

/**
 * The keystrokes that answer a harness's dialog, as xterm encodes them: Enter,
 * Escape alone, a digit choosing an option, `y`/`n`, Ctrl-C. One key at a
 * time — an arrow moving the dialog's cursor is `\x1b[A`, three bytes, and a
 * paste is many; neither answers anything.
 */
const ANSWER_KEYS = /^(?:\r|\n|\x1b|\x03|[0-9]|[yYnN])$/;

/**
 * Whether what was just typed into a tab answers the dialog the harness in
 * it has up (ide/06 §Reporting). No hook of the harness's says how a dialog
 * was answered — only that it showed, and later that the tool ran — so the
 * hand on the roster would stay up for as long as the tool takes, or until
 * the next prompt. The tab that showed the dialog knows the moment: the
 * roster says the session (or one of its sub-agents) waits, the tab is live,
 * and the bytes are an answering key. Said **once per wait**: `said` is the
 * `since` of the wait last answered from this tab, and the same wait is
 * never answered twice by a second key.
 * @param {TerminalSessionState | null | undefined} tab the tab typed into
 * @param {{state: object, since: number, children?: readonly {state: object, since: number}[]} | null | undefined} row the roster row the tab claims
 * @param {string} data the bytes xterm encoded for the keystroke
 * @param {number | null | undefined} said the `since` of the wait last answered from this tab
 * @returns {number | null} the `since` of the wait this answers — to remember as `said` — or `null`
 */
export function answerDue(tab, row, data, said) {
  if (!tab || !isLive(tab) || !tab.terminalId) return null;
  const since = waitSince(row);
  if (since === null || since === said) return null;
  return ANSWER_KEYS.test(data) ? since : null;
}

function indexOfKey(state, key) {
  return state.sessions.findIndex((s) => s.key === key);
}

/**
 * Open a shell — or a harness — rooted in something. **Always a new session.**
 *
 * Deliberately not deduplicating: two shells in one workstream is the ordinary
 * reason somebody presses this twice, and a button that silently focused an
 * existing terminal instead of opening one would make a second shell
 * unreachable. {@link revealTerminal} is the deduplicating door, and the
 * surface buttons use that one.
 */
/**
 * A sign-in target's `{kind, host}`, or `null` for anything else — a tab
 * carries one only when a code host sign-in opened it.
 * @param {unknown} login
 * @returns {{kind: string, host: string} | null}
 */
export function loginOf(login) {
  if (!login || typeof login !== "object") return null;
  const { kind, host } = /** @type {{kind?: unknown, host?: unknown}} */ (login);
  if (typeof kind !== "string" || !kind || typeof host !== "string" || !host) return null;
  return { kind, host };
}

/** A device a tab runs the checkout's app on (ide/19), or `null` for anything else. */
export function mobileDevelopmentOf(mobileDevelopment) {
  if (!mobileDevelopment || typeof mobileDevelopment !== "object") return null;
  const { device } = /** @type {{device?: unknown}} */ (mobileDevelopment);
  return typeof device === "string" && device ? { device } : null;
}

export function openTerminal(state, target, at = nowSecs()) {
  const { scope, id, harness = null, label = null, login = null, resume, run = false, mobileDevelopment = null } = target ?? {};
  if (!TERMINAL_SCOPES.includes(scope) || !id) return state;
  const key = launchKey(harness ?? null, scope, id);
  const session = {
    key: `t${state.seq + 1}`,
    scope,
    id,
    harness: harness ?? null,
    label: label ?? null,
    // A code host sign-in: the shell runs the CLI's browser login for this
    // kind at this host instead of a shell — its argv is the shell's own.
    login: loginOf(login),
    // The project's run command (ide/18): the node's answer for this
    // checkout, run through the login shell — never a program named here.
    run: run === true,
    // The checkout's Flutter app on a device (ide/19): the node's run line
    // for that device, the same way — the Device document finds this tab by it.
    mobileDevelopment: mobileDevelopmentOf(mobileDevelopment),
    // Decided once, here, and carried on the session — so a restart repeats
    // what this tab did rather than what the place has since become. An
    // explicit `resume` on the target wins: that is the menu's fresh entry
    // saying so in as many words.
    resume: resume ?? hasLaunched(state.launched, key),
    generation: 0,
    liveness: LIVE,
    restoring: false,
    // The PTY's id, learned when the mount reports it live; the port scanner
    // maps a terminal-rooted port back through it. Not persisted — a restart
    // spawns a new PTY with a new id.
    terminalId: null,
    // The roster session the harness in it reports as, learned with the PTY
    // id when the mount reports live. Not persisted: a restart registers anew.
    sessionId: null,
    // The harness the process table shows running under this shell right now
    // — a `claude` typed into a plain shell — told by the host as it changes
    // (`noteRunning`). Not persisted: a restart is a fresh shell.
    running: null,
    // When this shell opened, and when it exited — the rail's "open 12m" /
    // "ran 5m 20s". Client-side and unpersisted; a restart is a
    // fresh clock, a restored tab learns `openedAt` when its PTY reports live.
    openedAt: at,
    exitedAt: null,
  };
  // Into the focused pane — or the one the caller named, when it still exists.
  const pane =
    (target.pane && findLeaf(state.panes, target.pane) ? target.pane : null) ??
    (findLeaf(state.panes, state.focusedPane) ? state.focusedPane : leaves(state.panes)[0].id);
  return {
    ...state,
    sessions: [...state.sessions, session],
    launched: rememberLaunch(state.launched, key),
    active: session.key,
    panes: addTab(state.panes, pane, session.key),
    focusedPane: pane,
    // A terminal opened into a collapsed panel would be a process nobody can
    // see. It also has to be *laid out* to be measured: xterm reads its cell
    // size once at open, and a zero-height box makes that zero forever.
    seq: state.seq + 1,
  };
}

/**
 * Show a shell for this thing: the first live one that matches, or a new one.
 *
 * This is what a surface's terminal button does, and the dedupe is what
 * re-earns a property the old single-shell drawer had structurally — *pressing
 * the button when you cannot see the drawer must not restart your build.* With
 * many terminals the button can no longer be a toggle (a toggle that closes is
 * one misclick from ending a running build), so it reveals instead.
 */
export function revealTerminal(state, target) {
  const { scope, id, harness = null, run = false, mobileDevelopment = null } = target ?? {};
  const device = mobileDevelopmentOf(mobileDevelopment)?.device ?? null;
  const existing = state.sessions.find(
    (s) =>
      isLive(s) &&
      isRootedAt(s, scope, id) &&
      (s.harness ?? null) === (harness ?? null) &&
      (s.run === true) === (run === true) &&
      (s.mobileDevelopment?.device ?? null) === device,
  );
  if (!existing) return openTerminal(state, target);
  return focusTerminal(state, existing.key);
}

/** Bring a tab to the front of its pane and focus that pane. */
export function focusTerminal(state, key) {
  if (indexOfKey(state, key) === -1) return state;
  const leaf = leafOfTab(state.panes, key);
  const panes = leaf ? setActiveTab(state.panes, leaf.id, key) : state.panes;
  const focusedPane = leaf ? leaf.id : state.focusedPane;
  if (state.active === key && panes === state.panes && focusedPane === state.focusedPane) {
    return state;
  }
  return { ...state, active: key, panes, focusedPane };
}


/**
 * Close one tab. Unmounting its terminal is what ends the shell.
 *
 * The neighbour to the right becomes active, then the one to the left — the
 * same rule every editor uses, and the one that keeps your place when you close
 * a run of tabs left to right.
 */
export function closeTerminal(state, key) {
  return closeTerminals(state, [key]);
}

/**
 * Close several tabs as one move — a retirement's burst, the aborts of one
 * bus frame — so the store moves once and the panel renders once for N
 * unmounts. Keys not open are ignored; nothing is thrown for a pane tree
 * that is missing or empty (a restored state can hold one): the focused
 * pane then stays what it was.
 * @param {TerminalsState} state @param {readonly string[]} keys
 */
export function closeTerminals(state, keys) {
  const gone = new Set(keys.filter((k) => indexOfKey(state, k) !== -1));
  if (gone.size === 0) return state;
  const firstAt = Math.min(...[...gone].map((k) => indexOfKey(state, k)));
  const sessions = state.sessions.filter((s) => !gone.has(s.key));
  let panes = state.panes;
  for (const k of gone) panes = removeTab(panes, k);
  const root = leaves(panes)[0] ?? null;
  if (sessions.length === 0) {
    return { ...state, sessions, active: null, panes, focusedPane: root?.id ?? state.focusedPane };
  }
  // The pane the tab was in picks its own next tab (right, then left); the
  // globally active tab follows it when the closed one was active.
  const focusedPane = findLeaf(panes, state.focusedPane) ? state.focusedPane : (root?.id ?? state.focusedPane);
  let active = state.active;
  if (active !== null && gone.has(active)) {
    const wasIn = leafOfTab(state.panes, active);
    const pane = wasIn && findLeaf(panes, wasIn.id) ? findLeaf(panes, wasIn.id) : findLeaf(panes, focusedPane);
    active = pane?.active ?? (sessions[firstAt] ?? sessions[firstAt - 1] ?? sessions[0]).key;
  }
  return { ...state, sessions, active, panes, focusedPane };
}

/**
 * Spawn a fresh shell in the same tab, whether or not the old one died.
 *
 * Same `key`, same position, same selection — only `mountKey` moves, which is
 * what makes React tear the dead terminal down before building the new one.
 * Nothing restarts on its own: a shell that respawned itself would silently
 * re-run whatever the login files do, forever.
 */
export function restartTerminal(state, key, at = nowSecs()) {
  const idx = indexOfKey(state, key);
  if (idx === -1) return state;
  const sessions = state.sessions.slice();
  sessions[idx] = {
    ...sessions[idx],
    generation: sessions[idx].generation + 1,
    liveness: LIVE,
    // A restart is a fresh shell by request; it does not replay a checkpoint.
    restoring: false,
    // A new PTY is coming; forget the old one's id until it reports live,
    // and whatever ran under the old one with it.
    terminalId: null,
    sessionId: null,
    running: null,
    // A fresh clock: the old run's spans do not carry into the new shell.
    openedAt: at,
    exitedAt: null,
  };
  return { ...state, sessions };
}

/**
 * Record that a shell exited on its own.
 *
 * Keyed **and** generation-checked. An exit event can arrive after a restart
 * has already replaced the shell it came from, and letting a dead shell's last
 * word mark its live successor as exited is the kind of bug that only shows up
 * when somebody is restarting something that keeps failing.
 *
 * Nothing else moves: not the order, not the active tab, not the panel. A
 * `cargo build` that died three tabs away leaves its evidence exactly where it
 * was, which is the whole reason the tab is not closed here.
 */
export function noteExit(state, key, generation, code, at = nowSecs()) {
  const idx = indexOfKey(state, key);
  if (idx === -1) return state;
  if (state.sessions[idx].generation !== generation) return state;
  if (isExited(state.sessions[idx])) return state;
  const sessions = state.sessions.slice();
  // Stamp when it exited so the row can read how long it ran. The session id
  // stays: the roster row ends on its own retention and the tab still names it.
  sessions[idx] = { ...sessions[idx], liveness: exited(code), restoring: false, exitedAt: at, running: null };
  return { ...state, sessions };
}

/**
 * The host read which harness runs under this shell — the process table's
 * word, every change once (ide/06 §What runs in a shell). Generation-checked
 * like an exit, and nothing for an exited tab: the process is gone with it.
 */
export function noteRunning(state, key, generation, harness) {
  const idx = indexOfKey(state, key);
  if (idx === -1) return state;
  const s = state.sessions[idx];
  if (s.generation !== generation || isExited(s)) return state;
  const next = typeof harness === "string" && harness ? harness : null;
  if (s.running === next) return state;
  const sessions = state.sessions.slice();
  sessions[idx] = { ...s, running: next };
  return { ...state, sessions };
}

/**
 * The harness a tab is about: the one it was opened with, else the one the
 * process table shows running in it — so a `claude` typed into a shell is a
 * harness everywhere, and a shell again when it exits. Every surface reads
 * this, never `harness` alone; `harness` stays the launch's own fact, which
 * the mount keys the PTY on.
 */
export function harnessOf(session) {
  if (!session) return null;
  return session.harness ?? session.running ?? null;
}

/**
 * The owning host confirmed the shell is running — a restored tab's PTY came
 * back, or a spawn completed. Generation-checked like an exit.
 */
export function noteLive(state, key, generation, terminalId = null, sessionId = null, at = nowSecs()) {
  const idx = indexOfKey(state, key);
  if (idx === -1) return state;
  const s = state.sessions[idx];
  if (s.generation !== generation) return state;
  if (s.liveness === LIVE && !s.restoring && s.terminalId === terminalId && s.sessionId === sessionId) return state;
  if (isExited(s)) return state;
  const sessions = state.sessions.slice();
  // A restored tab has no open instant until its PTY reports live; a freshly
  // opened one already stamped `openedAt` and keeps it. The roster session
  // the harness reports as comes with the PTY: the node registered it at open.
  sessions[idx] = { ...s, liveness: LIVE, restoring: false, terminalId, sessionId, openedAt: s.openedAt ?? at };
  return { ...state, sessions };
}

/**
 * Contact was lost without a verdict: the spawn failed, or the channel
 * stopped answering. Recorded as *unverifiable*, never as exited.
 */
export function noteUnverifiable(state, key, generation, reason) {
  const at = indexOfKey(state, key);
  if (at === -1) return state;
  const s = state.sessions[at];
  if (s.generation !== generation) return state;
  if (isExited(s)) return state;
  if (s.liveness.status === "unverifiable" && s.liveness.reason === reason) return state;
  const sessions = state.sessions.slice();
  sessions[at] = { ...s, liveness: unverifiable(reason) };
  return { ...state, sessions };
}

// ---------------------------------------------------------------------------
// Panes
// ---------------------------------------------------------------------------

/**
 * Split the focused pane and open a shell in the new half — in the same place
 * as the pane's active tab unless `target` says otherwise. A pane with
 * nothing to split beside does nothing.
 */
export function splitPane(state, dir, target) {
  const leaf = findLeaf(state.panes, state.focusedPane) ?? leaves(state.panes)[0];
  const active = leaf.active ? state.sessions.find((s) => s.key === leaf.active) : null;
  const where = target ?? (active ? { scope: active.scope, id: active.id, label: active.label } : null);
  if (!where) return state;
  const splitId = `s${state.paneSeq + 1}`;
  const newLeaf = `p${state.paneSeq + 2}`;
  const panes = splitLeaf(state.panes, leaf.id, dir, splitId, newLeaf);
  if (panes === state.panes) return state;
  return openTerminal(
    { ...state, panes, paneSeq: state.paneSeq + 2, focusedPane: newLeaf },
    { ...where, pane: newLeaf },
  );
}

/** Close a pane; its tabs move next door. The only pane stays. */
export function closePane(state, leafId) {
  const panes = closeLeaf(state.panes, leafId);
  if (panes === state.panes) return state;
  const focusedPane = findLeaf(panes, state.focusedPane) ? state.focusedPane : leaves(panes)[0].id;
  const active = findLeaf(panes, focusedPane)?.active ?? state.active;
  return { ...state, panes, focusedPane, active };
}

/** Move a tab into another pane and show it there. */
export function moveTabToPane(state, key, leafId) {
  if (indexOfKey(state, key) === -1 || !findLeaf(state.panes, leafId)) return state;
  const panes = addTab(state.panes, leafId, key);
  if (panes === state.panes) return state;
  const focusedPane = findLeaf(panes, leafId) ? leafId : leaves(panes)[0].id;
  return { ...state, panes, focusedPane, active: key };
}

/**
 * A drag along a **split pane's** strip: put `key` at `index` among that pane's
 * tabs. The leaf's order is what a split strip draws, and it is what moves.
 */
export function reorderTerminal(state, key, index) {
  if (indexOfKey(state, key) === -1) return state;
  const panes = moveWithin(state.panes, key, index);
  return panes === state.panes ? state : { ...state, panes };
}

/**
 * Put `key` at `index` among the sessions sharing its `scope`+`id`, leaving
 * every other session where it is. `index` is the position among those
 * siblings. Identity-stable when nothing moves.
 * @param {readonly object[]} sessions @param {string} key @param {number} index
 */
function moveSessionWithin(sessions, key, index) {
  const me = (sessions ?? []).find((s) => s.key === key);
  if (!me) return sessions;
  const siblings = sessions.filter((s) => s.scope === me.scope && s.id === me.id);
  const from = siblings.findIndex((s) => s.key === key);
  const to = Math.max(0, Math.min(index, siblings.length - 1));
  if (from === to) return sessions;
  const reordered = [...siblings];
  reordered.splice(from, 1);
  reordered.splice(to, 0, me);
  // Splice the reordered siblings back over the slots they occupy in the full
  // array, so non-sibling sessions keep their positions.
  let i = 0;
  return sessions.map((s) => (s.scope === me.scope && s.id === me.id ? reordered[i++] : s));
}

/**
 * A drag along the **centre strip** (single pane) or a rail's harness rows,
 * where terminals are drawn in **session order**: reorder among scope siblings.
 * This is the order the centre strip (`sessionsRootedAt`) and the rail read,
 * which the pane-tree reorder never touched — the "moves but does not stick" bug.
 */
export function reorderTerminalSession(state, key, index) {
  const sessions = moveSessionWithin(state.sessions, key, index);
  return sessions === state.sessions ? state : { ...state, sessions };
}

/** Close every other tab rooted where `key` is — the live ones too; the caller asks first. */
export function closeOtherTerminals(state, key) {
  const me = state.sessions.find((s) => s.key === key);
  if (!me) return state;
  let out = state;
  for (const s of state.sessions) {
    if (s.key !== key && isRootedAt(s, me.scope, me.id)) out = closeTerminal(out, s.key);
  }
  return out;
}

/** Close every exited tab rooted in one place — the shells that are already over. */
export function closeExitedTerminals(state, scope, id) {
  let out = state;
  for (const s of sessionsRootedAt(state.sessions, scope, id)) {
    if (isExited(s)) out = closeTerminal(out, s.key);
  }
  return out;
}

/** Focus a pane; the globally active tab becomes its active tab. */
export function focusPane(state, leafId) {
  const leaf = findLeaf(state.panes, leafId);
  if (!leaf) return state;
  const active = leaf.active ?? state.active;
  if (state.focusedPane === leafId && state.active === active) return state;
  return { ...state, focusedPane: leafId, active };
}

/** Focus the pane next door, by geometry. Nothing there is nothing to do. */
export function focusNeighborPane(state, direction) {
  const next = neighbor(state.panes, state.focusedPane, direction);
  return next ? focusPane(state, next) : state;
}

/** Drag a divider. Identity-stable: runs on every pointer move. */
export function setPaneRatio(state, splitId, ratio) {
  const panes = setRatio(state.panes, splitId, ratio);
  return panes === state.panes ? state : { ...state, panes };
}

// ---------------------------------------------------------------------------
// Persistence — what survives a restart, and how it comes back
// ---------------------------------------------------------------------------

export const SESSIONS_VERSION = 3;

/**
 * What is worth keeping across a restart: the tabs (where, what, whether
 * they resume), the panes, and the counters — never a PTY id and never the
 * liveness, which is unknowable across a restart by definition.
 */
export function serializeTerminals(state) {
  return {
    version: SESSIONS_VERSION,
    sessions: state.sessions.map((s) => ({
      key: s.key,
      scope: s.scope,
      id: s.id,
      harness: s.harness,
      label: s.label,
      login: s.login,
      resume: s.resume,
      run: s.run === true,
      mobileDevelopment: s.mobileDevelopment ?? null,
      generation: s.generation,
    })),
    panes: state.panes,
    focusedPane: state.focusedPane,
    active: state.active,
    seq: state.seq,
    paneSeq: state.paneSeq,
  };
}

/**
 * Bring stored tabs back as `unverifiable` — their shells have not been
 * respawned — with the generation bumped so the mount is new, and marked
 * `restoring` so the mount replays its checkpoint before it spawns. Anything
 * malformed is dropped; nothing usable is an empty panel.
 */
export function restoreTerminals(json, launched = []) {
  const empty = { ...emptyTerminals(), launched };
  if (!json || typeof json !== "object" || json.version !== SESSIONS_VERSION) return empty;
  const raw = Array.isArray(json.sessions) ? json.sessions : [];
  const seen = new Set();
  const sessions = [];
  for (const s of raw) {
    if (!s || typeof s !== "object") continue;
    if (typeof s.key !== "string" || !/^t\d+$/.test(s.key) || seen.has(s.key)) continue;
    if (!TERMINAL_SCOPES.includes(s.scope) || typeof s.id !== "string" || !s.id) continue;
    seen.add(s.key);
    sessions.push({
      key: s.key,
      scope: s.scope,
      id: s.id,
      harness: typeof s.harness === "string" ? s.harness : null,
      label: typeof s.label === "string" ? s.label : null,
      login: loginOf(s.login),
      resume: s.resume === true,
      run: s.run === true,
      mobileDevelopment: mobileDevelopmentOf(s.mobileDevelopment),
      generation: (Number.isInteger(s.generation) && s.generation >= 0 ? s.generation : 0) + 1,
      liveness: unverifiable(RESTORED_REASON),
      restoring: true,
      terminalId: null,
      sessionId: null,
      running: null,
      openedAt: null,
      exitedAt: null,
    });
  }
  if (sessions.length === 0) return empty;
  const keys = sessions.map((s) => s.key);
  let panes = parseTree(json.panes, keys);
  // Tabs the stored tree forgot go into the first pane.
  const first = panes ? leaves(panes)[0] : null;
  if (!panes) panes = singleLeaf("p1", keys);
  else {
    for (const k of keys) {
      if (!leafOfTab(panes, k)) panes = addTab(panes, first.id, k, false);
    }
  }
  const maxSeq = Math.max(...keys.map((k) => Number(k.slice(1))));
  const seq = Math.max(Number.isInteger(json.seq) ? json.seq : 0, maxSeq);
  const paneIds = leaves(panes).map((l) => Number(l.id.slice(1))).filter(Number.isFinite);
  const paneSeq = Math.max(Number.isInteger(json.paneSeq) ? json.paneSeq : 1, ...paneIds, 1);
  const focusedPane = findLeaf(panes, json.focusedPane) ? json.focusedPane : leaves(panes)[0].id;
  const active = keys.includes(json.active) ? json.active : (findLeaf(panes, focusedPane)?.active ?? keys[0]);
  return {
    ...empty,
    sessions,
    panes,
    focusedPane,
    active,
    seq,
    paneSeq,
  };
}

/** The sessions standing in one place — a workstream's own terminals. */
/**
 * The terminal tab a roster session lives in — the tab whose reporter
 * registered that session — or null when no tab claims it (ide/06
 * §Reporting: the tab is the row). What the Inbox's session row and the
 * rail's agent row open.
 * @param {readonly {key: string, sessionId?: string | null}[]} terminals
 * @param {string} sessionId
 */
export function tabOfSession(terminals, sessionId) {
  return (terminals ?? []).find((t) => t.sessionId === sessionId) ?? null;
}

export function sessionsRootedAt(sessions, scope, id) {
  return (sessions ?? []).filter((s) => isRootedAt(s, scope, id));
}

const SCOPE_NOUN = {
  goal: tr("shell-browser-places-goal"),
  workstream: tr("shell-terminals-workstream"),
  work_item: tr("shell-terminals-work-item"),
  // A shell in the person's home directory — a code host sign-in opens one.
  machine: tr("shell-terminals-machine"),
};

/**
 * What a tab calls itself.
 *
 * The place is always named, even when the surface handed over a label: two
 * shells in one working session may both be called `fix-total`, and *which
 * folder is this rooted in* is the one thing a tab exists to answer once it has
 * outlived the screen that opened it.
 */
export function terminalTitle(session) {
  if (!session) return "";
  const noun = SCOPE_NOUN[session.scope] ?? session.scope;
  const label = typeof session.label === "string" ? session.label.trim() : "";
  const where = label ? `${noun} · ${label}` : tr("shell-terminals-noun-tail", { noun, tail: String(session.id).slice(-6) });
  if (session.mobileDevelopment) return tr("shell-terminals-title-flutter-where", { where });
  if (session.run) return tr("shell-terminals-title-run-where", { where });
  const harness = harnessOf(session);
  return harness ? `${harness} · ${where}` : where;
}

/**
 * What runs in a tab, as its word: the harness by its label, the run
 * command, the device run, else a plain shell. The one word the strip's tab,
 * the rail's row and the footer read.
 * @param {object | null | undefined} session
 * @param {Readonly<Record<string, string>>} [harnessLabels]
 */
export function runningWord(session, harnessLabels = {}) {
  if (!session) return "";
  if (session.mobileDevelopment) return tr("shell-terminals-flutter");
  if (session.run) return tr("shell-terminals-run");
  const harness = harnessOf(session);
  return harness ? (harnessLabels[harness] ?? harness) : shellWord();
}

/** The word for a plain shell — nothing launched in it, nothing running under it. */
export function shellWord() {
  return tr("shell-terminals-shell");
}

/** The short form for a tab strip: what is running, and where. */
export function terminalTabLabel(session, harnessLabels = {}) {
  if (!session) return "";
  const what = runningWord(session, harnessLabels);
  const label = typeof session.label === "string" ? session.label.trim() : "";
  return label ? `${what} · ${label}` : what;
}

/**
 * A tab's liveness as its word — *open* · *exited* · *exited (n)* ·
 * *unverifiable* — the one rule the strip, the rail's shell row and the
 * footer read (ide/06 §Liveness). Loss of contact is never *exited*.
 * @param {{status: string, code?: number | null} | null | undefined} liveness
 */
export function livenessWord(liveness) {
  if (!liveness || liveness.status === "live") return tr("shell-terminals-open");
  if (liveness.status === "unverifiable") return tr("shell-terminals-unverifiable");
  const code = liveness.code;
  if (code === null || code === undefined || code === 0) return tr("shell-terminals-exited");
  return tr("shell-terminals-exited-code", { code });
}

/**
 * How a tab reports a shell that is not known to be running, or `null` while
 * it lives: `exited`, `exited (n)`, or `unverifiable` — `livenessWord`'s.
 */
export function exitNote(session) {
  if (!session || session.liveness.status === "live") return null;
  return livenessWord(session.liveness);
}

/** *ran 12 s* — how long an exited tab's shell lived. @param {string} duration the words for the length */
export function ranWords(duration) {
  return tr("shell-terminals-ran", { duration });
}

/** The tone a tab's note takes: danger for a non-zero exit, dim otherwise. */
export function livenessTone(session) {
  const l = session?.liveness;
  return l && l.status === "exited" && l.code ? "danger" : "dim";
}

/** The sentence behind an `unverifiable` note, or null. */
export function livenessReason(session) {
  const l = session?.liveness;
  return l && l.status === "unverifiable" ? l.reason : null;
}

/**
 * The live tab running the checkout's app on that device (ide/19), or
 * `null` — what the Device document's Hot reload, Hot restart and Stop
 * write to.
 */
export function mobileDevelopmentRunSession(sessions, wid, device) {
  return sessions.find((s) => isLive(s) && isRootedAt(s, "workstream", wid) && s.mobileDevelopment?.device === device) ?? null;
}

/** Every live tab running the checkout's app on some device. */
export function mobileDevelopmentRunSessions(sessions, wid) {
  return sessions.filter((s) => isLive(s) && isRootedAt(s, "workstream", wid) && s.mobileDevelopment !== null && s.mobileDevelopment !== undefined);
}
