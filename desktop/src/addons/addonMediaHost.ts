/**
 * The media player beside an addon's window (18 — Addons): one native child
 * view on the node's player page — fetched as text and loaded under the
 * app's own base URL, so YouTube sees this app named in the Referer as its
 * rules ask — opened **above** the browser layer so the layer's holes never
 * cut it, placed where the addon's box falls on the page, hidden while the
 * window is — a drag or resize in the layer, a surface over it, the window
 * put away — and closed when the addon lets it go, when its frame reloads,
 * and when the window leaves. Every fact is `addonMediaModel.mjs`'s; this
 * hook performs them.
 *
 * The player's words come back through the shell's one door on every page
 * (`browser:message`, `bisa:media`) and reach the addon as its `media`
 * event; a window the page asks for — its "Watch on YouTube" link — goes
 * to the layer's open-link ask, like any link an addon wants opened.
 *
 * One embed per window: a second replaces the first. `BrowserPanel.tsx`
 * owns the browser tabs' webviews; this is the one other opener, under keys
 * of its own (`addon-media:<id>`), and the views' events it hears are its
 * own keys' alone. The opens and closes of one key run in order **across
 * instances** (`enqueue`, one map for the module): a window put away and
 * shown, its grants changed or its addon switched off and on is a new
 * instance of this hook whose open must wait for the old instance's close —
 * the shell refuses a key still open. An open closes the key first (a
 * no-op when nothing is open under it, and the way a view a gone instance
 * left is cleared) and, refused, is tried again after a short wait: a
 * closing view can still be seen for a beat after its close answered.
 */

import { useCallback, useEffect, useMemo, useRef, type RefObject } from "react";
import { api } from "../api";
import { browserAvailable, closeBrowserView, listenBrowserViews, openBrowserPage, setBrowserViewBounds } from "../browser/session";
import type { BrowserBounds } from "../browser/session";
import { errorFields, log } from "../log";
import { LAYOUT_CHANGED } from "../shell/layerSlots";
import type { Addon } from "../types";
import { layerMayShowNow, useLayerMayShow } from "../ui/openSurfaces";
import type { PlayerBox } from "./addonBridgeModel.mjs";
import { OPEN_RETRIES, OPEN_RETRY_MS, enqueue, mediaKey, pageRect, playerShown, stateOf } from "./addonMediaModel.mjs";
import type { MediaState, PageRect } from "./addonMediaModel.mjs";

/** Out of sight: one pixel, hidden — the browser panel's own idea of away. */
const HIDDEN: BrowserBounds = { left: 0, top: 0, width: 1, height: 1 };
/** The view's init script: the player page carries its own, so none is needed — but the door wants a program. */
const NO_SCRIPT = "/* the player page carries its own script */";
/** The opens and closes of every key, in order — one map for every instance of the hook. */
const chains = new Map<string, Promise<void>>();

export interface AddonMediaHost {
  /** Show YouTube's player for a video over a box of the window; a second video replaces the first. */
  embed: (video: string, box: PlayerBox) => void;
  /** The box moved — the addon laid itself out again. */
  move: (box: PlayerBox) => void;
  /** The addon let the player go. */
  remove: () => void;
}

const wait = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));

/** A close before an open that found nothing to close, or a view already going: expected, and said only in the debug log. */
const nothingToClose = (key: string) => (e: unknown) => log.debug("addons", "nothing to close under the player's key before its open", { key, ...errorFields(e) });

/** Open the key's view on `html`: the key closed first, then opened; an open the shell refused is tried again after a wait, at most `OPEN_RETRIES` times. */
async function openFresh(key: string, html: string): Promise<void> {
  await closeBrowserView(key).catch(nothingToClose(key));
  for (let tries = 0; ; tries++) {
    try {
      await openBrowserPage(key, html, NO_SCRIPT, false);
      return;
    } catch (e) {
      if (tries >= OPEN_RETRIES) throw e;
      await wait(OPEN_RETRY_MS);
      await closeBrowserView(key).catch(nothingToClose(key));
    }
  }
}

export function useAddonMediaHost(
  frameRef: RefObject<HTMLIFrameElement | null>,
  addon: Addon,
  {
    visible,
    shielded,
    epoch,
    onState,
    onOpenUrl,
  }: {
    /** Whether the window is on screen. */
    visible: boolean;
    /** A drag or resize runs somewhere in the layer. */
    shielded: boolean;
    /** The frame's key: a new one is a page loaded afresh, whose player is gone with the old page. */
    epoch: string;
    /** What the player said, for the addon's `media` event. */
    onState: (state: MediaState) => void;
    /** The page asked for a window: the layer asks the person. */
    onOpenUrl: (url: string) => Promise<boolean>;
  },
): AddonMediaHost {
  const key = mediaKey(addon.id);
  const mayShow = useLayerMayShow();
  /** The video shown, its box, and whether the view exists yet. */
  const current = useRef<{ video: string; box: PlayerBox; opened: boolean } | null>(null);
  /** The last placement sent, so an unchanged one sends nothing. */
  const last = useRef<{ rect: PageRect | null; shown: boolean } | null>(null);
  const latest = useRef({ visible, shielded, onState, onOpenUrl });
  latest.current = { visible, shielded, onState, onOpenUrl };

  /** A placement, open or close the shell refused: said in the log with the addon, never to the person — the addon hears `media` instead. */
  const warn = useCallback((what: string) => (e: unknown) => log.warn("addons", what, { addon: addon.id, ...errorFields(e) }), [addon.id]);

  /** A step on this key's chain — behind every open and close queued before it, by any instance. */
  const queue = useCallback(
    (step: () => Promise<void>) => {
      void enqueue(chains, key, step);
    },
    [key],
  );

  const place = useCallback(() => {
    const c = current.current;
    const frame = frameRef.current;
    if (!c || !c.opened || !frame) return;
    const r = frame.getBoundingClientRect();
    const rect = pageRect({ left: r.left, top: r.top, width: r.width, height: r.height }, c.box);
    const shown = playerShown({ embedded: true, visible: latest.current.visible, shielded: latest.current.shielded, mayShow: layerMayShowNow(), rect });
    const was = last.current;
    if (was && was.shown === shown && sameRect(was.rect, rect)) return;
    last.current = { rect, shown };
    void setBrowserViewBounds(key, shown && rect ? rect : HIDDEN, shown, { width: window.innerWidth, height: window.innerHeight }).catch(warn("the media player could not be placed")); // for the log
  }, [frameRef, key, warn]);

  const close = useCallback(() => {
    if (!current.current) return;
    current.current = null;
    last.current = null;
    queue(() => closeBrowserView(key).catch(warn("the media player could not be closed"))); // for the log
  }, [key, queue, warn]);

  const embed = useCallback(
    (video: string, box: PlayerBox) => {
      if (!browserAvailable()) {
        latest.current.onState({ state: "error", title: "", code: "unavailable", words: "" });
        return;
      }
      const c = current.current;
      if (c && c.video === video) {
        c.box = box;
        place();
        return;
      }
      close();
      const mine = { video, box, opened: false };
      current.current = mine;
      queue(async () => {
        if (current.current !== mine) return; // replaced or removed before its turn
        try {
          const html = await api.addonPlayerPage(addon.id, video);
          if (current.current !== mine) return; // replaced while the page was read
          await openFresh(key, html);
        } catch (e) {
          if (current.current === mine) current.current = null;
          warn("the media player could not be opened")(e); // for the log
          latest.current.onState({ state: "error", title: "", code: "unavailable", words: "" });
          return;
        }
        if (current.current !== mine) {
          // Replaced while opening: the view is nobody's; close it.
          await closeBrowserView(key).catch(warn("a replaced media player could not be closed")); // for the log
          return;
        }
        mine.opened = true;
        place();
      });
    },
    [addon.id, key, close, place, queue, warn],
  );

  const move = useCallback(
    (box: PlayerBox) => {
      const c = current.current;
      if (!c) return;
      c.box = box;
      place();
    },
    [place],
  );

  // Re-placed after every render (a drag re-renders the window), on a resize
  // of the frame or the window, and when the shell's layout moves — the
  // same moments the window's own clear is read again.
  useEffect(() => {
    place();
  });
  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const read = () => place();
    const ro = new ResizeObserver(read);
    ro.observe(frame);
    window.addEventListener("resize", read);
    window.addEventListener(LAYOUT_CHANGED, read);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", read);
      window.removeEventListener(LAYOUT_CHANGED, read);
    };
  }, [frameRef, place, epoch]);
  useEffect(() => place(), [visible, shielded, mayShow, place]);

  // The views' events: this key's alone.
  useEffect(() => {
    if (!browserAvailable()) return;
    let off: (() => void) | null = null;
    let gone = false;
    void listenBrowserViews({
      navigated: () => {},
      titled: () => {},
      said: (e) => {
        if (e.key !== key) return;
        const state = stateOf(e.message);
        if (state) latest.current.onState(state);
      },
      opened: (e) => {
        if (e.key !== key) return;
        void latest.current.onOpenUrl(e.url).catch(() => false);
      },
    })
      .then((unsubscribe) => {
        if (gone) unsubscribe();
        else off = unsubscribe;
      })
      .catch(warn("the media player's events could not be heard")); // for the log
    return () => {
      gone = true;
      off?.();
    };
  }, [key, warn]);

  // A page loaded afresh, a window that leaves: the player goes with it.
  useEffect(() => () => close(), [close, epoch]);

  return useMemo(() => ({ embed, move, remove: close }), [embed, move, close]);
}

function sameRect(a: PageRect | null, b: PageRect | null): boolean {
  if (a === null || b === null) return a === b;
  return a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height;
}
