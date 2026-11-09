/**
 * The desktop's half of the browser bridge (ide/18): an agent's browser
 * tool call, parked by the engine and heard here as a `browser_request`
 * frame, is performed in the tabs — opened, navigated, read, outlined,
 * clicked, typed into, pressed, scrolled, waited on, screenshotted, closed
 * — and answered over the node. The decisions are `browserBridgeModel.mjs`'s;
 * the tabs are `useBrowsers`'s; the webviews are `browser/session.ts`'s;
 * where the tab is shown is `browserPlacementModel.mjs`'s word.
 * `BrowserPanel.tsx` feeds this the webviews' words and hears the frames.
 *
 * An act that may move the page — a click, a fill, typing, a key, a
 * choice — is raced against the navigation it may start: when the page
 * moves, the bridge waits for the new page and answers it as `navigated`,
 * rather than reporting that the old page did not answer. A tab still
 * loading is waited for before anything is asked of its page.
 *
 * A tab an agent opens or snapshots is shown to the person through the one
 * door (`browserDoors.showBrowserTab`) — in the IDE's strip when it is on
 * the tab's home and its centre shows documents, else in the Details pane's
 * Browser occupant beside whatever they are on — unless the engine said it
 * is kept out of sight (`headless`, ide/18 §Headless tabs): such a tab
 * renders offstage, is snapped there, and is never revealed by anything an
 * agent does.
 */

import { errorFields, log } from "../log";
import { Lru } from "./lru.mjs";
import { api } from "../api";
import type { BrowserPending } from "../types";
import type { BridgeResult } from "./browserBridgeModel.mjs";

/** What a request is to perform: the parked row, or the frame that announced it. */
export type BrowserAsk = Pick<BrowserPending, "id" | "request" | "scope" | "headless">;
import { measureMessage, parseInspectorMessage } from "../ui/artifact/pageInspector.mjs";
import type { InspectorAnswer } from "../ui/artifact/pageInspector.mjs";
import { browserViewBack, browserViewForward, browserViewReload, driveBrowserView, navigateBrowserView, setBrowserViewAppearance } from "../browser/session";
import { ANSWER_MS, HANDLED_KEPT, LOAD_LATE, LOAD_MS, MEASURE_MS, MEASURE_POLL_MS, NAVIGATE_GRACE_MS, NOT_SHOWN, PAGE_SILENT, PLACEHOLDER_ID, SHOT_FAILED, TAB_GONE, WAIT_ANSWER_MS, answerResult, askAgain, driveOutcome, emulateResult, navigatedResult, planRequest, screenshotResult, tabResult, tabsResult, wireResult } from "./browserBridgeModel.mjs";
import { beginBrowserWork, endBrowserWork } from "./browserActivityStore";
import { showBrowserTab } from "./browserDoors";
import { inspectorSaid } from "./browserInspectorStore";
import { OFFSTAGE_VIEWPORT, fullPageHeight } from "./browserPlacementModel.mjs";
import { NotShownError, takeShot, uploadShot } from "./browserShots";
import { placementNow, settled } from "./browserShown";
import { BROWSER_COMMAND, fire, ideRoot } from "./shortcuts";
import { browserAsked, browserNavigated, browserSessions, browserState, closeBrowserTab, concealBrowserTab, emulateBrowserTab, openBrowserIn, revealBrowserTab, whyNotBrowser } from "./useBrowsers";
import { BLANK_URL, byAgent, byPage, sameViewport, sessionOf } from "./browsersModel.mjs";
import type { BrowserHistory, BrowserSession, TabViewport } from "./browsersModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * The requests taken up so far, so one heard as a frame and read again from
 * the parked list is performed once. Only the latest are kept: a request
 * answered leaves the engine's desk, and one this old is on no list any more.
 */
const handled = new Lru<true>(HANDLED_KEPT);
const answers = new Map<string, (m: InspectorAnswer) => void>();
const loads = new Map<string, Array<() => void>>();
const starts = new Map<string, Array<() => void>>();
let seq = 0;

/** The root the IDE is on — the place a new tab is at home in when the request names none. */
function currentRoot(): { scope: string; id: string } | null {
  return ideRoot();
}

/** A webview said its page moved, with its history: the store learns it, and whoever waits on the start or the load goes on. */
export function noteBrowserNavigated(key: string, url: string, event: "started" | "finished", history: BrowserHistory | null = null): void {
  browserNavigated(key, url, event, history);
  if (event === "started") {
    for (const began of starts.get(key) ?? []) began();
    starts.delete(key);
    return;
  }
  for (const done of loads.get(key) ?? []) done();
  loads.delete(key);
}

/**
 * A page asked for a window — a link with a target, `window.open` — and the
 * webview refused it: a tab of ours opens on the URL instead, at home where
 * the asking tab is and **as the asker stands** — in sight and to the front
 * beside a page a person looks at, kept out of sight beside one that is: a
 * hidden page puts nothing on screen. A window asked for a tab that is gone
 * — closed while its page still ran — is nobody's and opens nothing.
 */
export function noteBrowserOpened(key: string, url: string): void {
  const asker = sessionOf(browserState(), key);
  if (!asker) return;
  openBrowserIn({ home: asker.home, url, headless: asker.headless, by: byPage(key) });
}

/** A page said something: an answer to a request, its URL after a move of its own, or an inspector's word. The title is the webview's word, not the page's. */
export function noteBrowserMessage(key: string, message: unknown): void {
  const m = parseInspectorMessage(message);
  if (!m) return;
  if (m.type === "bisa:answer") {
    const waiting = answers.get(m.id);
    if (waiting) {
      answers.delete(m.id);
      waiting(m);
    }
    return;
  }
  if (m.type === "bisa:title") {
    // A page's own move — a hash, a pushState — that the webview's load
    // hooks do not see; the title itself comes with `browser:titled`.
    noteBrowserNavigated(key, m.url, "finished");
    return;
  }
  if (m.type === "bisa:key") {
    // A browser chord pressed inside the page: the bar of that tab answers
    // it as it would the chord typed in the main window.
    fire(BROWSER_COMMAND, { command: m.command, key });
    return;
  }
  inspectorSaid(key, m);
}

/** Resolves when the tab's page finishes loading, or after `ms`; answers whether it finished. */
function untilLoaded(key: string, ms: number = LOAD_MS): Promise<boolean> {
  return new Promise((resolve) => {
    const timer = window.setTimeout(() => finish(false), ms);
    function finish(loaded: boolean) {
      window.clearTimeout(timer);
      const left = (loads.get(key) ?? []).filter((f) => f !== done);
      // A tab nobody waits on holds no row: a closed tab's key is not kept for good.
      if (left.length > 0) loads.set(key, left);
      else loads.delete(key);
      resolve(loaded);
    }
    const done = () => finish(true);
    loads.set(key, [...(loads.get(key) ?? []), done]);
  });
}

/** Resolves when the tab's page starts a navigation within `ms` — and never otherwise, so a race is won only by a move. */
function untilStarted(key: string, ms: number): { started: Promise<true>; off: () => void } {
  let began: (() => void) | null = null;
  const started = new Promise<true>((resolve) => {
    began = () => resolve(true);
    starts.set(key, [...(starts.get(key) ?? []), began]);
  });
  const timer = window.setTimeout(off, ms);
  function off() {
    window.clearTimeout(timer);
    if (!began) return;
    const left = (starts.get(key) ?? []).filter((f) => f !== began);
    if (left.length > 0) starts.set(key, left);
    else starts.delete(key);
  }
  return { started, off };
}

/** Whether the tab's page is on its way right now. */
function loading(key: string): boolean {
  return sessionOf(browserState(), key)?.loading ?? false;
}

/**
 * What asking a page came to: its answer; silence — the script ran and
 * nothing came back in time; or the tab could not be driven at all — the
 * shell's command was refused, a webview gone. Silence and a gone tab are
 * different facts for an agent, and are told apart here, where they arise.
 */
type AskOutcome = { kind: "answered"; message: InspectorAnswer } | { kind: "silent"; waited_ms: number } | { kind: "gone"; error: unknown };

/** Ask a tab's page something and wait for its answer, its silence, or the tab's absence. */
function askPage(key: string, message: Record<string, unknown>, ms: number = ANSWER_MS): Promise<AskOutcome> {
  const id = `q${++seq}`;
  const began = Date.now();
  return new Promise((resolve) => {
    const timer = window.setTimeout(() => {
      answers.delete(id);
      resolve({ kind: "silent", waited_ms: Date.now() - began });
    }, ms);
    answers.set(id, (m) => {
      window.clearTimeout(timer);
      resolve({ kind: "answered", message: m });
    });
    void driveBrowserView(key, { ...message, id }).catch((error: unknown) => {
      window.clearTimeout(timer);
      answers.delete(id);
      resolve({ kind: "gone", error });
    });
  });
}

/** Show a tab to the person where it belongs — never one kept out of sight (`showBrowserTab`). */
function reveal(session: BrowserSession): void {
  showBrowserTab(session.key);
}

/** The page's measure — its viewport and its document's size — or null when it could not say: a blank tab has no page, a silent or gone one says nothing. */
async function measureOnce(key: string): Promise<{ viewport: TabViewport | null; scroll: { height: number } | null } | null> {
  const s = sessionOf(browserState(), key);
  if (!s || s.url === BLANK_URL) return null;
  const outcome = await askPage(key, measureMessage(PLACEHOLDER_ID));
  return outcome.kind === "answered" ? { viewport: outcome.message.viewport, scroll: outcome.message.scroll } : null;
}

/**
 * Wait for a tab out of sight to render at the viewport it was given
 * (ide/18 §Emulation): the layer re-places the webview on the store's word,
 * and the page's own `innerWidth` is the proof it landed. Asked every
 * `MEASURE_POLL_MS` for up to `MEASURE_MS`; answers the last size the page
 * said — the one asked, once it landed — or null when the page could not say.
 */
async function measureUntil(key: string, want: TabViewport): Promise<TabViewport | null> {
  const began = Date.now();
  let last: TabViewport | null = null;
  for (;;) {
    const measured = await measureOnce(key);
    if (!measured) return last;
    if (measured.viewport) {
      last = measured.viewport;
      if (Math.abs(last.width - want.width) <= 1 && Math.abs(last.height - want.height) <= 1) return last;
    }
    if (Date.now() - began >= MEASURE_MS) return last;
    await new Promise((resolve) => window.setTimeout(resolve, MEASURE_POLL_MS));
  }
}

/**
 * Grow a tab out of sight to its document for a full-page shot (ide/18
 * §Screenshots): its box becomes the document's height, capped, at the
 * width it had. Answers what to restore, or null when nothing grew — a seen
 * tab, a page no taller than its viewport, a blank tab.
 */
async function growForFullPage(key: string): Promise<{ before: TabViewport | null; grown: TabViewport } | null> {
  const s = sessionOf(browserState(), key);
  if (!s?.headless) return null;
  const before = s.viewport;
  const base = before ?? OFFSTAGE_VIEWPORT;
  const measured = await measureOnce(key);
  const grown = { width: base.width, height: fullPageHeight(measured?.scroll?.height, base) };
  if (sameViewport(grown, base)) return null;
  emulateBrowserTab(key, { viewport: grown });
  await measureUntil(key, grown);
  await settled();
  return { before, grown };
}

/** Put the tab's box back after a full-page shot — unless a `browser_emulate` landed meanwhile and the box is no longer the grown one: requests are not serialised per tab, and a later word is never clobbered. */
function restoreAfterFullPage(key: string, grew: { before: TabViewport | null; grown: TabViewport }): void {
  const now = sessionOf(browserState(), key);
  if (now && sameViewport(now.viewport, grew.grown)) emulateBrowserTab(key, { viewport: grew.before });
}

/** Perform one parked request and answer it; a request already handled is left alone. */
export async function performBrowserRequest(pending: BrowserAsk): Promise<void> {
  if (handled.has(pending.id)) return;
  handled.set(pending.id, true);
  const result = await perform(pending);
  // A refusal is data to the agent and a line here: the id and the act, never the page's words.
  if (!result.ok) log.warn("browser", "a browser request was refused", { id: pending.id, action: pending.request.action, tab: result.tab ?? null, error: result.error });
  try {
    // What leaves is built from the wire's keys by name (`wireResult`): the
    // node refuses an answer carrying a key `BrowserResult` does not declare.
    await api.answerBrowserRequest(pending.id, wireResult(result));
  } catch (e) {
    // Answered already, or timed out on the engine: nothing left to say to the agent — a line for the log.
    log.debug("browser", "an answer to a browser request was not taken", { id: pending.id, ...errorFields(e) });
  }
}

const refused = (error: string, extra: Partial<BridgeResult> = {}): BridgeResult => ({ ok: false, error, tabs: [], ...extra });

/** A request acting on a tab: the tab is busy from before the act until after it, whatever the act answers. */
async function working<T>(key: string, act: () => Promise<T>): Promise<T> {
  beginBrowserWork(key);
  try {
    return await act();
  } finally {
    endBrowserWork(key);
  }
}

async function perform(pending: BrowserAsk): Promise<BridgeResult> {
  const plan = planRequest(pending.request, pending.scope, browserState(), currentRoot(), pending.headless);
  const session = (key: string) => browserSessions().find((s) => s.key === key) ?? null;
  const asIs = (key: string): BridgeResult => {
    const s = session(key);
    return s ? { ...tabResult(s), tabs: [] } : refused(t("shell-browser-bridge-tab", { key }));
  };
  /** A history move — back, forward, reload — waited to its load, answered as the tab stands. */
  const moved = (key: string, move: () => Promise<void>): Promise<BridgeResult> =>
    working(key, async () => {
      const loaded = untilLoaded(key);
      await move().catch((e: unknown) => log.warn("browser", "a history move did not take", { key, ...errorFields(e) }));
      await loaded;
      return asIs(key);
    });
  switch (plan.kind) {
    case "refuse":
      return refused(plan.error);
    case "tabs":
      return { ...tabsResult(browserSessions()), tabs: tabsResult(browserSessions()).tabs ?? [] };
    case "open": {
      if (plan.tab) {
        const tab = plan.tab;
        return working(tab, async () => {
          // The agent's own word on sight for a tab it holds: shown on `headless: false`, put out of sight on `true`, left as it is otherwise (ide/18 §Headless tabs).
          const before = session(tab);
          if (plan.headless === false && before?.headless) revealBrowserTab(tab);
          else if (plan.headless === true && before && !before.headless) concealBrowserTab(tab);
          const standing = session(tab);
          if (plan.headless === false && standing) reveal(standing);
          browserAsked(tab, plan.url);
          const loaded = untilLoaded(tab);
          await navigateBrowserView(tab, plan.url).catch((e: unknown) => log.warn("browser", "a navigation did not take", { key: tab, ...errorFields(e) }));
          await loaded;
          return asIs(tab);
        });
      }
      const key = openBrowserIn({ home: plan.home, url: plan.url, headless: plan.headless === true, by: byAgent(pending.scope.agent) });
      if (!key) return refused(whyNotBrowser() ?? t("shell-browser-bridge-tab-could-open"));
      return working(key, async () => {
        const s = session(key);
        if (s) reveal(s);
        await untilLoaded(key);
        return asIs(key);
      });
    }
    case "back":
      return moved(plan.key, () => browserViewBack(plan.key));
    case "forward":
      return moved(plan.key, () => browserViewForward(plan.key));
    case "reload":
      return moved(plan.key, () => browserViewReload(plan.key));
    case "wait-load": {
      const key = plan.key;
      return working(key, async () => {
        const began = Date.now();
        const done = loading(key) ? await untilLoaded(key, plan.timeoutMs) : true;
        const s = session(key);
        if (!s) return refused(t("shell-browser-bridge-tab", { key }));
        const waited = { waited_ms: Date.now() - began };
        return done ? { ...tabResult(s, waited), tabs: [] } : refused(LOAD_LATE, { tab: key, url: s.url, ...waited });
      });
    }
    case "close":
      closeBrowserTab(plan.key);
      return { ok: true, tab: plan.key, tabs: [] };
    case "drive": {
      const key = plan.key;
      const s = session(key);
      if (!s) return refused(t("shell-browser-bridge-tab", { key }));
      return working(key, async () => {
        // A page still on its way is not asked yet: the script it would
        // answer with is the old page's.
        if (loading(key)) await untilLoaded(key);
        const isWait = plan.message.type === "bisa:wait";
        const action = pending.request.action;
        const gone = (error: unknown) => {
          log.warn("browser", "the page could not be asked", { key, action, ...errorFields(error) });
          return refused(TAB_GONE, { tab: key, url: s.url });
        };
        if (!plan.navigates) {
          // An act that cannot move the page is asked once more when the
          // page stayed silent — after the load it may have been in the
          // middle of. `navigates` is the boundary the plan already drew, so
          // no click, fill, type, press or select is ever repeated here.
          let outcome = await askPage(key, plan.message, isWait ? WAIT_ANSWER_MS : ANSWER_MS);
          if (askAgain(plan, outcome, 1)) {
            log.debug("browser", "the page did not answer; asking once more", { key, action, waited_ms: outcome.kind === "silent" ? outcome.waited_ms : null, attempt: 2 });
            if (loading(key)) await untilLoaded(key);
            outcome = await askPage(key, plan.message, isWait ? WAIT_ANSWER_MS : ANSWER_MS);
          }
          if (outcome.kind === "gone") return gone(outcome.error);
          if (outcome.kind === "silent") return refused(PAGE_SILENT, { tab: key, url: s.url });
          return { ...answerResult(s, outcome.message), tabs: [] };
        }
        // An act that may move the page: whichever comes first — the page's
        // answer, or the navigation it started — decides; an answer followed
        // by a move within the grace is a move too.
        const watch = untilStarted(key, ANSWER_MS);
        const raced = await Promise.race([askPage(key, plan.message).then((asked) => ({ asked })), watch.started.then(() => ({ started: true as const }))]);
        const asked = "asked" in raced ? raced.asked : null;
        let movedLate = false;
        if (asked !== null) {
          const late = untilStarted(key, NAVIGATE_GRACE_MS);
          const grace = new Promise<false>((resolve) => window.setTimeout(() => resolve(false), NAVIGATE_GRACE_MS));
          movedLate = await Promise.race([late.started, grace]);
          late.off();
        }
        watch.off();
        // The race's verdict is the model's (`driveOutcome`): a move wins
        // over any answer; past the grace the answer is read as it came.
        const verdict = driveOutcome({ asked, started: asked === null, movedLate });
        switch (verdict.kind) {
          case "navigated":
            await untilLoaded(key);
            return { ...navigatedResult(session(key) ?? s), tabs: [] };
          case "gone":
            return gone(verdict.error);
          case "silent":
            return refused(PAGE_SILENT, { tab: key, url: s.url });
          case "answered":
            return { ...answerResult(s, verdict.message), tabs: [] };
        }
      });
    }
    case "screenshot": {
      const key = plan.key;
      const s = session(key);
      if (!s) return refused(t("shell-browser-bridge-tab", { key }));
      return working(key, async () => {
        // A snapshot is of a tab that renders: a seen one is shown for it,
        // since a hidden webview renders nothing worth keeping; a headless
        // one renders offstage and is snapped there, out of sight — grown to
        // its document first for a full page, and put back after. The shot
        // itself waits for the tab to show (`browserShots.takeShot`).
        reveal(s);
        const grew = plan.fullPage ? await growForFullPage(key) : null;
        try {
          const shot = await takeShot(key);
          const uploaded = await uploadShot(shot);
          const now = session(key) ?? s;
          return { ...screenshotResult(now, uploaded, shot), tabs: [] };
        } catch (e) {
          if (e instanceof NotShownError) return refused(NOT_SHOWN, { tab: key, url: s.url });
          return refused(`${SHOT_FAILED}: ${e instanceof Error ? e.message : String(e)}`, { tab: key, url: s.url });
        } finally {
          if (grew) restoreAfterFullPage(key, grew);
        }
      });
    }
    case "emulate": {
      const key = plan.key;
      const s = session(key);
      if (!s) return refused(t("shell-browser-bridge-tab", { key }));
      return working(key, async () => {
        // The scheme first — the webview's appearance, which the page's
        // `prefers-color-scheme` follows — then the viewport, which the layer
        // draws on the store's word and the page confirms by its own size.
        if (plan.scheme) {
          try {
            await setBrowserViewAppearance(key, plan.scheme);
          } catch (e) {
            log.warn("browser", "a tab's scheme could not be set", { key, scheme: plan.scheme, ...errorFields(e) });
            return refused(t("shell-browser-bridge-scheme-could-set", { error: e instanceof Error ? e.message : String(e) }), { tab: key, url: s.url });
          }
          emulateBrowserTab(key, { scheme: plan.scheme });
        }
        let measured: TabViewport | null = null;
        if (plan.viewport) {
          emulateBrowserTab(key, { viewport: plan.viewport });
          measured = await measureUntil(key, plan.viewport);
        }
        const now = session(key) ?? s;
        const box = placementNow(key)?.rect ?? null;
        const viewport = measured ?? plan.viewport ?? (box ? { width: Math.round(box.width), height: Math.round(box.height) } : null);
        return { ...emulateResult(now, viewport, now.scheme), tabs: [] };
      });
    }
  }
}
