/**
 * The desktop's half of the browser bridge (ide/18), as facts: what an
 * agent's request means for the tabs — which tab, at home where, shown
 * where, which message to hand the page — and what the answer is when the
 * tabs say no. The store and the webviews do the work; this decides. The
 * engine decides where a tab is at home (`scope.home`); this only reads it.
 */

import { BLANK_URL, BROWSER_SCOPES, atHome, browsersRootedAt, normalizeUrl, sessionOf } from "./browsersModel.mjs";
import { clickMessage, consoleMessage, evalMessage, fillMessage, findTextMessage, pointMessage, pressMessage, readMessage, scrollMessage, selectMessage, snapshotMessage, typeMessage, waitMessage } from "../ui/artifact/pageInspector.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** How long a `wait until load` waits when the agent names no time, and at most. */
/** How many taken-up requests the bridge remembers, so one heard twice is performed once — the latest, never every one since the window opened. */
export const HANDLED_KEPT = 256;

export const DEFAULT_WAIT_MS = 10_000;
export const MAX_WAIT_MS = 30_000;

/**
 * Where a new tab is at home for a request: where the engine said the
 * asking session speaks — a checkout, a goal, a work item, a workflow, a
 * channel, a direct message, a conversation (`scope.home`, one of the
 * desktop's own scopes) — else the root the IDE is on, else the workspace
 * (`null`): a tab always has somewhere to be, since the Details pane's
 * Browser occupant shows every tab beside any screen.
 * @param {{home?: {scope: string, id: string} | null} | null | undefined} scope
 * @param {{scope: string, id: string} | null} current
 */
export function rootFor(scope, current) {
  const home = scope?.home ?? null;
  if (home && BROWSER_SCOPES.includes(home.scope) && typeof home.id === "string" && home.id) return { scope: home.scope, id: home.id };
  return current;
}

/**
 * Where a tab is shown to the person: the IDE, when it is on the tab's home
 * already *and its centre shows documents* — the tab rides that root's strip
 * — else the Details pane's Browser occupant, beside whatever the person is
 * on: another screen, or the IDE with the conversation or the Board in its
 * centre, where the strip is not drawn. `null` for a tab kept out of sight
 * (ide/18 §Headless tabs): an agent's act never reveals one — only the
 * person, or its own `headless: false`.
 * @param {{home: {scope: string, id: string} | null, headless: boolean}} session
 * @param {{scope: string, id: string} | null} current the root the IDE is on
 * @param {"documents" | "conversation" | "board"} centre what that root's centre shows
 * @returns {"ide" | "pane" | null}
 */
export function revealPlan(session, current, centre) {
  if (session?.headless) return null;
  if (!atHome(session, current)) return "pane";
  return centre === "documents" ? "ide" : "pane";
}

/** The acts that may move the page: the bridge waits for a navigation they start before answering. */
export const NAVIGATING = Object.freeze(["click", "fill", "type", "press", "select"]);

/** A `wait`'s bound, in milliseconds: what was asked within the most, else the default. @param {unknown} ms */
export function waitBound(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return DEFAULT_WAIT_MS;
  return Math.min(MAX_WAIT_MS, Math.floor(n));
}

/**
 * The plan for one request.
 * @param {{action: string, tab?: string | null, url?: string | null, target?: string | null, query?: string | null, text?: string | null, format?: string | null, all?: boolean | null, until?: string | null, timeout_ms?: number | null, key?: string | null, modifiers?: readonly string[] | null, clear?: boolean | null, submit?: boolean | null, value?: string | null, label?: string | null, to?: string | null, by_x?: number | null, by_y?: number | null, clear_console?: boolean | null, expression?: string | null}} request
 * @param {{home?: {scope: string, id: string} | null} | null | undefined} scope
 * @param {{sessions: readonly object[]}} state
 * @param {{scope: string, id: string} | null} current
 * @param {boolean} [headless] the engine's decision: a tab this request opens is kept out of sight
 * @returns {import("./browserBridgeModel.mjs").BridgePlan}
 */
export function planRequest(request, scope, state, current, headless = false) {
  const tabOf = () => {
    const key = String(request.tab ?? "");
    const session = key ? sessionOf(state, key) : null;
    return session ? { key, session } : null;
  };
  const withPage = () => {
    const t = tabOf();
    if (!t) return { refuse: refuseTab(request) };
    if (t.session.url === BLANK_URL) return { refuse: { kind: "refuse", error: tr("shell-browser-bridge-tab-shows-page-yet-browser-open", { key: t.key }) } };
    return { key: t.key };
  };
  const drive = (message, navigates = false) => {
    const page = withPage();
    if (page.refuse) return page.refuse;
    return { kind: "drive", key: page.key, message, navigates };
  };
  const target = String(request.target ?? "").trim();
  const needsTarget = () => ({ kind: "refuse", error: tr("shell-browser-bridge-browser-needs-target-ref-from-browser", { action: request.action }) });
  switch (request.action) {
    case "open": {
      const norm = normalizeUrl(request.url ?? "");
      if ("error" in norm) return { kind: "refuse", error: norm.error };
      if (request.tab && !sessionOf(state, request.tab)) return { kind: "refuse", error: tr("shell-browser-bridge-tab-browser-tabs-lists-open-ones", { tab: request.tab }) };
      return { kind: "open", url: norm.url, tab: request.tab ?? null, home: request.tab ? null : rootFor(scope, current), headless: !!headless };
    }
    case "tabs":
      return { kind: "tabs" };
    case "screenshot": {
      const page = withPage();
      return page.refuse ?? { kind: "screenshot", key: page.key };
    }
    case "back":
    case "forward":
    case "reload": {
      const t = tabOf();
      if (!t) return refuseTab(request);
      if (request.action === "reload" && t.session.url === BLANK_URL) return { kind: "refuse", error: tr("shell-browser-bridge-tab-shows-page-yet-browser-open", { key: t.key }) };
      return { kind: request.action, key: t.key };
    }
    case "close": {
      const t = tabOf();
      return t ? { kind: "close", key: t.key } : refuseTab(request);
    }
    case "wait": {
      const until = String(request.until ?? "load");
      if (until === "load") {
        const t = tabOf();
        return t ? { kind: "wait-load", key: t.key, timeoutMs: waitBound(request.timeout_ms) } : refuseTab(request);
      }
      if (!["selector", "text", "gone", "idle"].includes(until)) return { kind: "refuse", error: tr("shell-browser-bridge-browser-wait-waits-until-load-selector", { until }) };
      if ((until === "selector" || until === "gone") && !target) return { kind: "refuse", error: tr("shell-browser-bridge-browser-wait-until-needs-target", { until }) };
      if (until === "text" && !String(request.query ?? "").trim()) return { kind: "refuse", error: tr("shell-browser-bridge-browser-wait-until-text-needs-words") };
      return drive(waitMessage(PLACEHOLDER_ID, until, target || null, until === "text" ? String(request.query) : null, waitBound(request.timeout_ms)));
    }
    case "read":
      return drive(readMessage(PLACEHOLDER_ID, target || null, request.format === "html" ? "html" : "text"));
    case "find":
      if (!String(request.query ?? "").trim()) return { kind: "refuse", error: tr("shell-browser-bridge-browser-find-needs-words-find") };
      return drive(findTextMessage(PLACEHOLDER_ID, String(request.query)));
    case "snapshot":
      return drive(snapshotMessage(PLACEHOLDER_ID, target || null, !!request.all));
    case "click":
      if (!target) return needsTarget();
      return drive(clickMessage(PLACEHOLDER_ID, target), true);
    case "fill":
      if (!target) return needsTarget();
      return drive(fillMessage(PLACEHOLDER_ID, target, String(request.text ?? "")), true);
    case "type":
      if (!target) return needsTarget();
      return drive(typeMessage(PLACEHOLDER_ID, target, String(request.text ?? ""), !!request.clear, !!request.submit), true);
    case "press": {
      if (!String(request.key ?? "").trim()) return { kind: "refuse", error: tr("shell-browser-bridge-browser-press-needs-key-enter-escape") };
      return drive(pressMessage(PLACEHOLDER_ID, String(request.key).trim(), target || null, [...(request.modifiers ?? [])]), true);
    }
    case "select":
      if (!target) return needsTarget();
      if (request.value == null && request.label == null) return { kind: "refuse", error: tr("shell-browser-bridge-browser-select-needs-option-s-value") };
      return drive(selectMessage(PLACEHOLDER_ID, target, request.value ?? null, request.label ?? null), true);
    case "hover":
      if (!target) return needsTarget();
      return drive(pointMessage(PLACEHOLDER_ID, target));
    case "scroll":
      return drive(scrollMessage(PLACEHOLDER_ID, request.to ?? null, Number(request.by_x) || 0, Number(request.by_y) || 0));
    case "console":
      return drive(consoleMessage(PLACEHOLDER_ID, !!request.clear_console));
    case "eval":
      if (!String(request.expression ?? "").trim()) return { kind: "refuse", error: tr("shell-browser-bridge-browser-eval-needs-expression") };
      return drive(evalMessage(PLACEHOLDER_ID, String(request.expression)));
    default:
      return { kind: "refuse", error: `unknown browser action ${String(request.action)}` }; // for the agent, never a person
  }
}

/** The id the bridge replaces with the request's own when it asks the page. */
export const PLACEHOLDER_ID = "?";

function refuseTab(request) {
  return { kind: "refuse", error: request.tab ? tr("shell-browser-bridge-tab-browser-tabs-lists-open-ones", { tab: request.tab }) : tr("shell-browser-bridge-name-tab-browser-open-answers-key") };
}

/** The tabs as the agent reads them. @param {readonly {key: string, url: string, title: string}[]} sessions */
export function tabsResult(sessions) {
  return { ok: true, tabs: (sessions ?? []).filter((s) => s.url !== BLANK_URL).map((s) => ({ key: s.key, url: s.url, title: s.title ?? "" })) };
}

/** The answer for a tab as it stands — after an open, a back, a wait for its load. @param {{key: string, url: string, title: string}} session */
export function tabResult(session, extra = {}) {
  return { ok: true, tab: session.key, url: session.url, title: session.title ?? "", ...extra };
}

/**
 * The page's answer as the agent reads it: the tab, where it is, and every
 * fact the page gave that the wire has a key for — the text, a count, a
 * wait's length, a scroll, a value, the console, the dialogs. The element's
 * path the page says beside them (`selector`) is the inspector's own: the
 * wire's `BrowserResult` has no key for it, and the node refuses an answer
 * carrying a key it does not know.
 * @param {{key: string, url: string, title: string}} session
 * @param {{ok?: boolean, error?: string | null, url?: string, title?: string, text?: string | null, selector?: string | null, count?: number | null, waitedMs?: number | null, scroll?: object | null, value?: unknown, dialogs?: readonly object[], console?: readonly object[]}} said
 */
export function answerResult(session, said) {
  const base = { tab: session.key, url: said.url ?? session.url, title: said.title ?? session.title ?? "" };
  const extras = {};
  if (typeof said.count === "number") extras.count = said.count;
  if (typeof said.waitedMs === "number") extras.waited_ms = said.waitedMs;
  if (said.scroll) extras.scroll = said.scroll;
  if (said.value !== undefined && said.value !== null) extras.value = said.value;
  if (said.dialogs?.length) extras.dialogs = [...said.dialogs];
  if (said.console?.length) extras.console = [...said.console];
  if (said.ok === false) return { ok: false, error: said.error ?? tr("shell-browser-bridge-page-refused"), ...base, ...extras };
  return { ok: true, ...base, text: said.text ?? null, ...extras };
}

/** The answer when an act moved the page: the page it landed on, said so. @param {{key: string, url: string, title: string}} session */
export function navigatedResult(session) {
  return tabResult(session, { navigated: true });
}

/** The refusal when the page never answered — a navigation swallowed the script, a page still on its way. An act that cannot move the page was asked twice before this is said. */
export const PAGE_SILENT = tr("shell-browser-bridge-page-did-answer-may-have-navigated");
/** The refusal when the tab could not be driven at all — its webview is gone, closed or never opened: a different fact from a silent page. */
export const TAB_GONE = tr("shell-browser-bridge-tab-could-reached-webview-may-have");
/**
 * Whether the page is asked once more: only when it stayed silent, and only
 * for an act the plan already says cannot move the page — `navigates` is
 * the boundary, so no click, fill, type, press or select is ever repeated;
 * a second click is a different act, a second read is the same one.
 * @param {{ navigates: boolean }} plan
 * @param {{ kind: string }} outcome what asking the page came to: `answered`, `silent` or `gone`
 * @param {number} attempt the ask that just came back, from 1
 */
export function askAgain(plan, outcome, attempt) {
  return attempt < 2 && !plan.navigates && outcome.kind === "silent";
}

/** How many presence reads in a row may fail before the miss is a warning: on the second the engine is about to think nobody is home. */
export const PRESENCE_LAPSE_AFTER = 2;

/** Whether `failures` consecutive misses of the presence read are a warning rather than a line for the log. @param {number} failures */
export function presenceLapsed(failures) {
  return failures >= PRESENCE_LAPSE_AFTER;
}

/** The refusal when a wait for the load ran out. */
export const LOAD_LATE = tr("shell-browser-bridge-page-still-loading-wait-again-read");
/** The refusal when the tab could not be shown for its snapshot in time. */
export const NOT_SHOWN = tr("shell-browser-bridge-tab-could-shown-screenshot-desktop-s");
/** The refusal when the shell took too long, or could not, snapshot the tab. */
export const SHOT_FAILED = tr("shell-browser-bridge-screenshot-could-taken");

/** The answer for a screenshot: the tab as it stands, and the PNG the desktop uploaded. @param {{key: string, url: string, title: string}} session @param {{sha256: string, name: string, mime: string, size: number}} screenshot @param {{width: number, height: number}} size */
export function screenshotResult(session, screenshot, size) {
  return { ...tabResult(session), screenshot, width: size.width, height: size.height };
}

/** A string, else nothing. */
const word = (v) => (typeof v === "string" ? v : undefined);
/** A whole number from zero — what the wire's counts, lengths and sizes are — else nothing. */
const whole = (v) => (typeof v === "number" && Number.isInteger(v) && v >= 0 ? v : undefined);
/** A whole number of either sign, a scroll's. */
const integer = (v) => (typeof v === "number" && Number.isInteger(v) ? v : undefined);
/** A record with fields, not a list. */
const record = (v) => (!!v && typeof v === "object" && !Array.isArray(v) ? v : null);

/** The wire's keys of each fact an answer nests, by name. */
const NESTED = Object.freeze({
  tab: (t) => {
    const key = word(t.key), url = word(t.url);
    return key === undefined || url === undefined ? null : { key, url, title: word(t.title) ?? "" };
  },
  scroll: (s) => {
    const x = integer(s.x), y = integer(s.y), width = integer(s.width), height = integer(s.height);
    return x === undefined || y === undefined || width === undefined || height === undefined ? null : { x, y, width, height };
  },
  dialog: (d) => {
    const kind = word(d.kind), message = word(d.message), answer = word(d.answer);
    if (kind === undefined || message === undefined) return null;
    return answer === undefined ? { kind, message } : { kind, message, answer };
  },
  line: (l) => {
    const level = word(l.level), text = word(l.text);
    return level === undefined || text === undefined ? null : { level, text, at: whole(l.at) ?? 0 };
  },
  attachment: (a) => {
    const sha256 = word(a.sha256), name = word(a.name), mime = word(a.mime), size = whole(a.size);
    return sha256 === undefined || name === undefined || mime === undefined || size === undefined ? null : { sha256, name, mime, size };
  },
});

/** A list of nested facts, each by its own keys; what is no such fact is left out. */
function listOf(v, one) {
  if (!Array.isArray(v)) return [];
  return v.map((item) => (record(item) ? one(item) : null)).filter((item) => item !== null);
}

/**
 * The answer as it leaves for the node: the wire's `BrowserResult`, **built
 * from its keys by name**. An answer is assembled from what a page said and
 * from the tabs' own records, and the node refuses a body carrying a key the
 * type does not declare — the agent's call would then wait out its deadline
 * on an answer nobody took. So nothing passes through: a key that is not the
 * wire's stays here, a fact that is absent is left out, a nested fact keeps
 * its own keys alone, and `path` is never sent — it is the engine's to set.
 * @param {Record<string, unknown> | null | undefined} result what the bridge assembled
 */
export function wireResult(result) {
  const r = record(result) ?? {};
  /** @type {Record<string, unknown>} */
  const out = { ok: r.ok === true };
  for (const key of ["error", "tab", "url", "title", "text"]) {
    const v = word(r[key]);
    if (v !== undefined) out[key] = v;
  }
  const tabs = listOf(r.tabs, NESTED.tab);
  if (tabs.length > 0) out.tabs = tabs;
  if (typeof r.navigated === "boolean") out.navigated = r.navigated;
  for (const key of ["count", "waited_ms", "width", "height"]) {
    const v = whole(r[key]);
    if (v !== undefined) out[key] = v;
  }
  const scroll = record(r.scroll) ? NESTED.scroll(r.scroll) : null;
  if (scroll) out.scroll = scroll;
  if (r.value !== undefined && r.value !== null) out.value = r.value;
  const dialogs = listOf(r.dialogs, NESTED.dialog);
  if (dialogs.length > 0) out.dialogs = dialogs;
  const lines = listOf(r.console, NESTED.line);
  if (lines.length > 0) out.console = lines;
  const screenshot = record(r.screenshot) ? NESTED.attachment(r.screenshot) : null;
  if (screenshot) out.screenshot = screenshot;
  return out;
}

/** The tabs the bridge may act on for a root: what `browser_tabs` lists. */
export { browsersRootedAt };
