/**
 * The browser layer (ide/18): every browser tab's native webview, opened
 * for each session the store holds, laid over the host that shows it — the
 * workbench's centre while its tab is the active one there, the Details
 * pane's Browser occupant while it shows the tab — hidden otherwise, and
 * hidden while any surface is open, since a native view paints above every
 * dialog. Which host is `browserPlacementModel.mjs`'s word. The main page's
 * floating overlays — the Notes and Draw panels, their docks, the pet, the
 * addon windows — are not surfaces: the layer leaves a hole for each one
 * over a showing tab (`browserClear.ts`, `browser.rs` `layer`), so they show
 * and take their clicks while the page stays live. Mounted in
 * `App.tsx` beside the terminal layer, outside the routed screen, so a
 * screen change closes nothing.
 *
 * This is the **only** file that opens, moves and closes the webviews
 * (`browser/session.ts`). It also hears the agents' requests
 * (`browser_request` frames, and the parked list at start) and hands them to
 * `browserBridge.ts`, reads the parked list every `BROWSER_PRESENCE_MS` so
 * the engine knows a desktop is home, and feeds the browser's settings to
 * `browserPrefsStore.ts`, being the one browser component always mounted.
 */

import { useCallback, useEffect, useRef } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import { readInspectorTheme } from "../ui/artifact/inspectorTokens";
import { browserScript } from "../ui/artifact/pageInspector.mjs";
import { layerMayShowNow, useLayerMayShow } from "../ui/openSurfaces";
import { browserAvailable, closeBrowserView, focusMainView, listenBrowserViews, openBrowserView, setBrowserClears, setBrowserViewBounds } from "../browser/session";
import type { BrowserBounds, BrowserClear } from "../browser/session";
import { browserClearsNow, noteBrowserCuts, useBrowserClears } from "./browserClear";
import { clearsOver, sameClears } from "./browserClearModel.mjs";
import { noteBrowserMessage, noteBrowserNavigated, noteBrowserOpened, performBrowserRequest } from "./browserBridge";
import { forgetBrowserWork } from "./browserActivityStore";
import { forgetBrowserInspector } from "./browserInspectorStore";
import { presenceLapsed } from "./browserBridgeModel.mjs";
import { placedAsAsked, placementOf } from "./browserPlacementModel.mjs";
import { relayChords } from "./keymapModel.mjs";
import { useNewBrowserHereDoor } from "./browserDoors";
import { currentKeymap } from "./shortcuts";
import { prefsOf, setBrowserPrefs } from "./browserPrefsStore";
import { layerSlot, useLayerSlot } from "./layerSlots";
import { browserPrefsChanged, browserTitled, useBrowsers } from "./useBrowsers";
import { useResolvedSettings } from "./useResolvedSettings";

/** Out of the way and one pixel: a webview that is not showing. */
const HIDDEN: BrowserBounds = { left: 0, top: 0, width: 1, height: 1 };
/** How often an open desktop reads the parked list — under the engine's presence window, so it is never thought gone. */
export const BROWSER_PRESENCE_MS = 20_000;

export function BrowserPanel() {
  const { sessions } = useBrowsers();
  // The keymap's *New browser tab here*: answered by the layer, which is
  // mounted whenever the browser is on, so the door is never left unanswered.
  useNewBrowserHereDoor();
  const center = useLayerSlot("center");
  const aux = useLayerSlot("aux");
  const mayShow = useLayerMayShow();
  const opened = useRef(new Set<string>());
  /** Every key the store has held, so a tab that left is forgotten once. */
  const known = useRef(new Set<string>());
  /** The tabs whose page was once drawn somewhere else than asked — said once each. */
  const strayed = useRef(new Set<string>());
  const available = browserAvailable();
  const { resolved } = useResolvedSettings(null);
  const overlays = useBrowserClears();
  /** What the layer was last told to cut, and against which viewport — so an unchanged frame sends nothing. */
  const cut = useRef<{ clears: BrowserClear[]; width: number; height: number } | null>(null);
  /** The floating panels open at the last look — one that opens takes the keyboard from a page. */
  const panelsOpen = useRef(new Set<string>());

  // Put a webview where its tab shows, or out of sight — with the page's
  // viewport, so the shell anchors the box on the page rather than on the
  // window (a content inset or a zoom would otherwise put the page over the
  // bar) — and compare what the shell drew with what was asked: a page
  // drawn elsewhere is a logged fact, not a mystery.
  const place = useCallback((key: string, placed: { rect: BrowserBounds } | null) => {
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    void setBrowserViewBounds(key, placed ? placed.rect : HIDDEN, placed !== null, viewport)
      .then((drawn) => {
        if (!placed || placedAsAsked(placed.rect, drawn) || strayed.current.has(key)) return;
        strayed.current.add(key);
        log.warn("browser", "browser placed out of place", { key, asked: placed.rect, drawn, viewport });
      })
      .catch((e: unknown) => log.warn("browser", "browser placement refused", { key, shown: placed !== null, error: e instanceof Error ? e.message : String(e) }));
  }, []);
  /** Place a tab by the slots as they stand right now — for a webview that has just opened. */
  const placeNow = useCallback(
    (key: string, headless: boolean) => place(key, placementOf(key, { center: layerSlot("center"), aux: layerSlot("aux"), mayShow: layerMayShowNow(), headless })),
    [place],
  );

  // The browser's settings, read once here for every reader outside React.
  useEffect(() => {
    setBrowserPrefs(prefsOf(resolved));
    browserPrefsChanged();
  }, [resolved]);

  // The webviews' words, and the agents' requests parked before this window opened.
  useEffect(() => {
    if (!available) return;
    let off: (() => void) | null = null;
    let gone = false;
    void listenBrowserViews({
      navigated: (e) => noteBrowserNavigated(e.key, e.url, e.event, { canBack: e.canBack, canForward: e.canForward }),
      said: (e) => noteBrowserMessage(e.key, e.message),
      titled: (e) => browserTitled(e.key, e.title),
      opened: (e) => noteBrowserOpened(e.key, e.url),
    }).then((unsubscribe) => {
      if (gone) unsubscribe();
      else off = unsubscribe;
    });
    void api
      .browserRequests()
      .then((r) => {
        for (const pending of r.requests) void performBrowserRequest(pending);
      })
      .catch((e: unknown) => log.debug("browser", "the pending browser requests could not be read", errorFields(e)));
    return () => {
      gone = true;
      off?.();
    };
  }, [available]);
  useEngineEvents((e) => {
    if (e.payload.type === "browser_request" && available) void performBrowserRequest(e.payload);
  });

  // One webview per session: opened as a session appears — on the blank
  // page when the tab has none — placed the moment it exists, since a
  // placement asked before that is refused; closed as the session goes.
  useEffect(() => {
    const wanted = new Set(sessions.map((s) => s.key));
    // A tab that left is forgotten everywhere it was known — its busy
    // count, its picks — wherever this runs; only the webviews are the shell's.
    for (const key of [...known.current]) {
      if (wanted.has(key)) continue;
      known.current.delete(key);
      forgetBrowserInspector(key);
      forgetBrowserWork(key);
    }
    for (const s of sessions) known.current.add(s.key);
    if (!available) return;
    for (const s of sessions) {
      if (opened.current.has(s.key)) continue;
      opened.current.add(s.key);
      // The page's program carries the browser chords as the keymap spells them now (ide/15) and the overlay's theme as the app wears it now: a tab opened after a rebinding relays the new ones, and its every document starts dressed.
      void openBrowserView(s.key, s.url, browserScript(relayChords(currentKeymap()), readInspectorTheme())).then(
        () => {
          if (opened.current.has(s.key)) placeNow(s.key, s.headless);
        },
        () => {
          opened.current.delete(s.key);
        },
      );
    }
    for (const key of [...opened.current]) {
      if (wanted.has(key)) continue;
      opened.current.delete(key);
      strayed.current.delete(key);
      void closeBrowserView(key).catch((e: unknown) => log.warn("browser", "a browser view could not be closed", { key, ...errorFields(e) }));
    }
  }, [sessions, available, placeNow]);

  // Where each webview is: over the host that shows its tab while no
  // surface is open; offstage for a headless tab; hidden otherwise.
  useEffect(() => {
    if (!available) return;
    for (const s of sessions) {
      if (!opened.current.has(s.key)) continue;
      place(s.key, placementOf(s.key, { center, aux, mayShow, headless: s.headless }));
    }
  }, [sessions, center, aux, mayShow, available, place]);

  // The overlays over a browser tab's slot, cut out of the layer: once a
  // frame, only when they changed — kept while a surface hides the tabs, so a
  // tab shows again already cut around them.
  useEffect(() => {
    if (!available) return;
    const frame = window.requestAnimationFrame(() => {
      const viewport = { width: window.innerWidth, height: window.innerHeight };
      const now = browserClearsNow();
      // A panel that opens while a page shows takes the keyboard: the page may
      // hold it natively, and the panel's field would look focused but type nowhere.
      const panels = new Set(now.map((e) => e.id).filter((id) => id === "notes-panel" || id === "draw-panel" || id.startsWith("addon:")));
      const opened = [...panels].some((id) => !panelsOpen.current.has(id));
      panelsOpen.current = panels;
      if (opened && [center, aux].some((s) => s.visible && s.layer === "browser")) {
        void focusMainView().catch((e: unknown) => log.warn("browser", "the keyboard could not be handed back to the app", errorFields(e)));
      }
      const clears = clearsOver(now, [center, aux]);
      const last = cut.current;
      if (last && last.width === viewport.width && last.height === viewport.height && sameClears(last.clears, clears)) return;
      cut.current = { clears, ...viewport };
      void setBrowserClears(clears, viewport)
        .then(noteBrowserCuts)
        .catch((e: unknown) => {
          cut.current = null;
          // Nothing was cut: an addon window must hide from the page again rather than sit unseen under it.
          noteBrowserCuts(false);
          log.warn("browser", "the browser layer could not cut around the overlays", { overlays: clears.length, ...errorFields(e) });
        });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [overlays, center, aux, available]);

  // The layer is going: no hole is left in it.
  useEffect(
    () => () => {
      if (!browserAvailable()) return;
      void setBrowserClears([], { width: window.innerWidth, height: window.innerHeight }).catch((e: unknown) =>
        log.warn("browser", "the browser layer's holes could not be closed as it went", errorFields(e)),
      );
    },
    [],
  );

  // A desktop that is open says so: the engine refuses a request at once
  // as not available when none has read the list within its presence
  // window, instead of parking it for a wait nobody ends.
  useEffect(() => {
    if (!available) return;
    // Misses in a row: the read is the desktop's heartbeat to the engine, and
    // on the second miss the engine is about to answer agents "nobody home".
    let failures = 0;
    const read = () =>
      void api
        .browserRequests()
        .then((r) => {
          failures = 0;
          for (const pending of r.requests) void performBrowserRequest(pending);
        })
        .catch((e: unknown) => {
          failures += 1;
          const fields = { failures, ...errorFields(e) };
          if (presenceLapsed(failures)) log.warn("browser", "the presence read keeps failing: agents are about to be told nobody is home", fields);
          else log.debug("browser", "the pending browser requests could not be read", fields);
        });
    const timer = window.setInterval(read, BROWSER_PRESENCE_MS);
    return () => window.clearInterval(timer);
  }, [available]);

  // The layer is going: every webview with it.
  useEffect(() => {
    const current = opened.current;
    return () => {
      for (const key of current) void closeBrowserView(key).catch((e: unknown) => log.warn("browser", "a browser view could not be closed as the layer went", { key, ...errorFields(e) }));
      current.clear();
    };
  }, []);

  return null;
}
