/**
 * The embedded browser's tabs (ide/18), as facts — no React, no Tauri:
 * which tabs are open, where each is at home, **who opened it**, what it
 * shows, which is active; what a URL typed into the bar becomes; what is
 * remembered across a restart. A tab is born only from an act — a person's,
 * an agent's request, a page's window — and wears it (`by`): a target that
 * names no opener opens nothing, so no door can forget to say, and a count
 * that surprises explains itself. One store for the whole window: a tab at home in a workbench
 * root rides that root's strip in the Project IDE, and every tab shows in
 * the Details pane's Browser occupant beside any screen; a tab with no home
 * is the workspace's. The webviews themselves are the shell's
 * (`BrowserPanel.tsx`), one per session here, keyed by the session's key —
 * as a terminal's PTY is keyed by its tab. Plain `.mjs`, so `node --test`
 * reads it.
 */

import { WORKBENCH_SCOPES } from "../routeModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** Where the tabs are remembered. */
export const BROWSER_SESSIONS_KEY = "bisa.browser.sessions.v3";
const BROWSER_SESSIONS_VERSION = 3;

/** Who may open a tab: a person's act, an agent's request, a window a page asked for. */
export const OPENERS = Object.freeze(["person", "agent", "page"]);
/** A person opened it — a door, a link, a chord. */
export const PERSON = Object.freeze({ kind: "person" });

/** An agent opened it, by its request; `null` when the request named nobody. @param {string | null | undefined} agent the agent's id */
export function byAgent(agent) {
  return { kind: "agent", agent: typeof agent === "string" && agent ? agent : null };
}

/** A page asked for a window — a link with a target, `window.open`. @param {string} from the asking tab's key */
export function byPage(from) {
  return { kind: "page", from: String(from) };
}

/**
 * An opener as given, or `null` for a shape nobody knows — what a target
 * must carry to open a tab, and what a remembered tab must carry to be read.
 * @param {unknown} raw
 * @returns {{kind: "person"} | {kind: "agent", agent: string | null} | {kind: "page", from: string} | null}
 */
export function openerOf(raw) {
  if (!raw || typeof raw !== "object") return null;
  const r = /** @type {Record<string, unknown>} */ (raw);
  switch (r.kind) {
    case "person":
      return PERSON;
    case "agent":
      return byAgent(/** @type {string | null | undefined} */ (r.agent));
    case "page":
      return typeof r.from === "string" && r.from ? byPage(r.from) : null;
    default:
      return null;
  }
}

/**
 * Who opened a tab, in words a row wears — nothing for a person's own, the
 * common case, which stays quiet: *opened by Reviewer*, *opened by an
 * agent*, *opened by a page*.
 * @param {{kind: string, agent?: string | null} | null | undefined} by
 * @param {(id: string) => string | null | undefined} [agentName] an agent's name by id
 * @returns {string | null}
 */
export function openerWords(by, agentName = () => null) {
  switch (by?.kind) {
    case "agent": {
      const name = by.agent ? agentName(by.agent) || by.agent : null;
      return name ? t("shell-browsers-opened-by", { who: name }) : t("shell-browsers-opened-by-agent");
    }
    case "page":
      return t("shell-browsers-opened-by-page");
    default:
      return null;
  }
}
/**
 * Where a tab may be at home: the workbench's roots (`workstream`, `goal`,
 * `work_item`), a workflow on the designer, a channel, a direct message, or
 * a conversation — each the screen it was opened beside, and what every list
 * names the tab by (`browserPlacesModel.whereWords`).
 */
export const BROWSER_SCOPES = Object.freeze(["workstream", "goal", "work_item", "workflow", "channel", "dm", "conversation"]);
/** A new tab with nothing typed yet. */
export const BLANK_URL = "about:blank";

export const EMPTY_BROWSERS = Object.freeze({ sessions: Object.freeze([]), active: null, seq: 0 });

/**
 * What the bar's field becomes when Enter is pressed: an http(s) URL, or the
 * reason it is not one. A bare host — `localhost:5173`, `127.0.0.1:3000`,
 * `example.com/docs` — is given `http://`; another scheme is refused.
 * @param {string} input
 * @returns {{url: string} | {error: string}}
 */
export function normalizeUrl(input) {
  const raw = String(input ?? "").trim();
  if (!raw) return { error: t("shell-browsers-type-url") };
  // A scheme is a word before a colon that is not a port: `localhost:5173`
  // names a host, `javascript:alert(1)` and `file:///x` name schemes.
  const withScheme = /^[a-zA-Z][a-zA-Z0-9+.-]*:(?!\d)/.test(raw) ? raw : `http://${raw}`;
  let parsed;
  try {
    parsed = new URL(withScheme);
  } catch {
    return { error: t("shell-browsers-url", { raw }) };
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return { error: t("shell-browsers-only-http-https", { protocol: parsed.protocol.replace(/:$/, "") }) };
  }
  if (!parsed.hostname) return { error: t("shell-browsers-names-host", { raw }) };
  return { url: parsed.href };
}

/** The bar's word for a URL: its host and path, the scheme and a trailing slash dropped. @param {string} url */
export function urlWords(url) {
  try {
    const u = new URL(String(url));
    const path = u.pathname === "/" ? "" : u.pathname;
    return `${u.host}${path}${u.search}`;
  } catch {
    return String(url ?? "");
  }
}

/**
 * A tab's home as given, or `null` for the workspace: a scope the tabs know
 * with an id, else nothing. A home the tabs do not know is not one.
 * @param {{scope?: string, id?: string} | null | undefined} home
 * @returns {{scope: string, id: string} | null}
 */
/**
 * A tab's home as a root of the Project IDE — where *Open in the Project
 * IDE* leads — or null: a tab at home in a channel, a direct channel, a
 * workflow or a conversation has no workbench to open in.
 * @param {{home?: {scope?: string, id?: string} | null} | null | undefined} session
 * @returns {{scope: "workstream" | "work_item" | "goal", id: string} | null}
 */
export function workbenchHomeOf(session) {
  const home = homeOf(session?.home);
  return home && WORKBENCH_SCOPES.includes(home.scope) ? home : null;
}

export function homeOf(home) {
  if (!home || typeof home !== "object") return null;
  const { scope, id } = home;
  if (!BROWSER_SCOPES.includes(scope) || typeof id !== "string" || !id) return null;
  return { scope, id };
}

/**
 * Open a tab — always a new one — at home in a place (or the workspace's,
 * with none), on a URL (or blank), **by somebody**: a target that names no
 * opener opens nothing and answers the same state.
 * @param {{sessions: readonly object[], active: string | null, seq: number}} state
 * @param {{by: {kind: string}, home?: {scope: string, id: string} | null, url?: string | null, pane?: string | null, headless?: boolean}} target `by`: who opens it (`PERSON`, `byAgent`, `byPage`); `headless`: kept out of sight — no host draws it, and it is not made active
 */
export function openBrowser(state, target) {
  const { home = null, url = null, pane = null, headless = false } = target ?? {};
  const by = openerOf(target?.by);
  if (!by) return state;
  const session = {
    key: `b${state.seq + 1}`,
    home: homeOf(home),
    by,
    url: url ?? BLANK_URL,
    title: "",
    loading: url !== null,
    canBack: false,
    canForward: false,
    headless: !!headless,
    pane,
  };
  // A tab kept out of sight takes no host from the one a person is looking at.
  return { sessions: [...state.sessions, session], active: headless ? state.active : session.key, seq: state.seq + 1 };
}

/** Bring a tab kept out of sight into view: it takes a host from now on, and comes to the front. */
export function reveal(state, key) {
  const s = sessionOf(state, key);
  if (!s) return state;
  const next = s.headless ? patch(state, key, { headless: false }) : state;
  return focusBrowser(next, key);
}

/** The tabs a person can look at: the ones not kept out of sight. @param {readonly object[]} sessions */
export function seenSessions(sessions) {
  return (sessions ?? []).filter((s) => !s.headless);
}

/** The seen tabs at home in a place. */
export function seenRootedAt(sessions, scope, id) {
  return seenSessions(browsersRootedAt(sessions, scope, id));
}

/** The word a list wears beside a tab kept out of sight, or null for a seen one. @param {{headless: boolean}} session */
export function visibilityWords(session) {
  return session?.headless ? t("shell-browsers-unseen-agent-browses-here-out-sight") : null;
}

/** The tab a URL bar or the bridge names, or null. @param {{sessions: readonly object[]}} state @param {string} key */
export function sessionOf(state, key) {
  return state.sessions.find((s) => s.key === key) ?? null;
}

/** The tabs at home in a place, in the order they were opened. @param {readonly object[]} sessions @param {string} scope @param {string} id */
export function browsersRootedAt(sessions, scope, id) {
  return (sessions ?? []).filter((s) => s.home !== null && s.home.scope === scope && s.home.id === id);
}

/** Whether a tab is at home in this place. @param {{home: {scope: string, id: string} | null}} session @param {{scope: string, id: string} | null} place */
export function atHome(session, place) {
  return !!place && !!session?.home && session.home.scope === place.scope && session.home.id === place.id;
}

/**
 * The scope a tab's drafts — its annotations — are kept under in the
 * session: the workbench root's key for a tab at home in one (the same
 * key the IDE's documents use), the conversation's for a tab at home
 * there, and `workspace` for the workspace's own tab.
 * @param {{home: {scope: string, id: string} | null}} session
 */
export function draftScopeOf(session) {
  const home = session?.home ?? null;
  return home ? `${home.scope}:${home.id}` : "workspace";
}

function patch(state, key, fields) {
  const idx = state.sessions.findIndex((s) => s.key === key);
  if (idx === -1) return state;
  const s = state.sessions[idx];
  const next = { ...s, ...fields };
  if (Object.keys(fields).every((k) => s[k] === next[k])) return state;
  const sessions = state.sessions.slice();
  sessions[idx] = next;
  return { ...state, sessions };
}

/**
 * The tab's page moved — a navigation started (loading) or finished. The
 * URL is what the webview says, not what was typed; the history is the
 * webview's own word on whether there is a page behind and one ahead, kept
 * as it was when the move says nothing of it.
 * @param {object} state @param {string} key @param {string} url @param {"started" | "finished"} event
 * @param {{canBack: boolean, canForward: boolean} | null} [history]
 */
export function navigated(state, key, url, event, history = null) {
  const moved = { url: String(url), loading: event === "started" };
  return patch(state, key, history ? { ...moved, canBack: !!history.canBack, canForward: !!history.canForward } : moved);
}

/** The page said its title. @param {object} state @param {string} key @param {string} title */
export function titled(state, key, title) {
  return patch(state, key, { title: String(title ?? "").trim().slice(0, 200) });
}

/** The bar asked for a URL: the tab shows it loading until the webview says otherwise. */
export function asked(state, key, url) {
  return patch(state, key, { url: String(url), loading: true });
}

/**
 * Close every tab — the browser switched off. The sequence is kept: a key
 * is never minted twice in a run, so nothing that remembers a tab by its
 * key — the address, a busy count, an inspector's picks — meets a stranger
 * under the same name.
 */
export function closeAll(state) {
  if (state.sessions.length === 0 && state.active === null) return state;
  return { sessions: Object.freeze([]), active: null, seq: state.seq };
}

/** Close a tab; the active one falls to its right neighbour, then its left. */
export function closeBrowser(state, key) {
  const idx = state.sessions.findIndex((s) => s.key === key);
  if (idx === -1) return state;
  const sessions = state.sessions.filter((s) => s.key !== key);
  let active = state.active;
  if (active === key) {
    const next = sessions[idx] ?? sessions[idx - 1] ?? null;
    active = next ? next.key : null;
  }
  return { ...state, sessions, active };
}

/** Bring a tab to the front. */
export function focusBrowser(state, key) {
  if (!state.sessions.some((s) => s.key === key) || state.active === key) return state;
  return { ...state, active: key };
}

/** The strip's word for a tab: its title, else its URL's host and path, else *Browser*. @param {object} session */
export function browserLabel(session) {
  if (!session) return t("shell-browser-pane-browser");
  const title = String(session.title ?? "").trim();
  if (title) return title;
  if (!session.url || session.url === BLANK_URL) return t("shell-browser-pane-browser");
  return urlWords(session.url);
}

/** The strip's tooltip: the URL in full. @param {object} session */
export function browserTitle(session) {
  if (!session || !session.url || session.url === BLANK_URL) return t("shell-browsers-browser-type-url");
  const title = String(session.title ?? "").trim();
  return title ? `${title} — ${session.url}` : session.url;
}

/**
 * What survives a restart: the tabs **a person can look at** — where each
 * was at home and who opened it; nothing about loading or history. A tab
 * kept out of sight is an agent's working page for that run: it ends with
 * the window, and is never loaded again at a launch nobody asked it of.
 */
export function serializeBrowsers(state) {
  return {
    version: BROWSER_SESSIONS_VERSION,
    sessions: seenSessions(state.sessions).map((s) => ({ key: s.key, home: s.home, by: s.by, url: s.url, title: s.title, pane: s.pane })),
    active: state.active,
    seq: state.seq,
  };
}

/** The tabs as they were, or nothing for a shape this version does not know; a tab that names no opener is not read. @param {unknown} raw */
export function parseBrowsers(raw) {
  if (!raw || typeof raw !== "object") return EMPTY_BROWSERS;
  const r = /** @type {Record<string, unknown>} */ (raw);
  if (r.version !== BROWSER_SESSIONS_VERSION || !Array.isArray(r.sessions)) return EMPTY_BROWSERS;
  const sessions = r.sessions
    .filter((s) => s && typeof s === "object" && /^b\d+$/.test(String(s.key)) && openerOf(s.by) !== null)
    .map((s) => ({
      key: String(s.key),
      home: homeOf(s.home),
      by: openerOf(s.by),
      url: typeof s.url === "string" && s.url ? s.url : BLANK_URL,
      title: typeof s.title === "string" ? s.title : "",
      loading: false,
      canBack: false,
      canForward: false,
      // Only a tab in sight is ever remembered.
      headless: false,
      pane: typeof s.pane === "string" ? s.pane : null,
    }));
  const seq = Math.max(Number(r.seq) || 0, ...sessions.map((s) => Number(s.key.slice(1)) || 0));
  const active = sessions.some((s) => s.key === r.active) ? String(r.active) : (sessions.at(-1)?.key ?? null);
  return { sessions, active, seq };
}
