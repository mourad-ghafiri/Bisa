/**
 * A document's scrollports keep their place across an unmount (ide/03 §Tabs).
 *
 * One hook at the document's root, not one per viewer. A scrollport that
 * wants its place kept says so where it is made — `data-scroll-keep="<name>"`
 * (`keptScrollModel.KEEP_ATTR`) — and this hears its `scroll` from the root in
 * the **capture** phase (scroll events do not bubble), keeping the offsets
 * one write per animation frame. On mount each marked scrollport is put back;
 * a rendering grows as it loads, so the restore is asked again whenever the
 * content resizes or the tree changes (`restoreStep`), until the place fits —
 * and it **gives up the moment the person scrolls, clicks or types**, or the
 * grace runs out: a restore must never fight the hand on the wheel.
 *
 * A restore **yields to a reveal** too (`yieldKeptScroll`): a screen that
 * must bring something of its own into view — the row an address names —
 * says so first, and the restore puts back what it can at once and ends.
 * The reveal then moves only if what it shows is still out of sight.
 *
 * Where the places live is the caller's (`read`, `write`): the kit keeps no
 * store of documents.
 */

import { useEffect, useRef, type RefObject } from "react";
import { KEEP_ATTR, RESTORE_GRACE_MS, placeOf, restoreStep, type Place } from "./keptScrollModel.mjs";

export interface KeptScroll {
  /** The place kept for a scrollport of this document, by its name. */
  read: (name: string) => Place | null;
  /** Keep — or, with null, forget — a scrollport's place. */
  write: (name: string, place: Place | null) => void;
}

/** What says the person has taken over: any of these ends a restore. */
const HANDS = ["wheel", "pointerdown", "keydown", "touchstart"] as const;

/** What a screen says before a reveal of its own: the restore under it takes its last step and ends. */
const YIELD = "keptscroll:yield";

/**
 * A screen is about to bring something into view in a scrollport that keeps
 * its place: the restore there puts back what it can, now, and ends — so
 * the reveal is the last word, and moves nothing that is already in sight.
 * @param within the screen's root — the element `useKeptScroll` was given — or anything inside it
 */
export function yieldKeptScroll(within: HTMLElement | null): void {
  within?.dispatchEvent(new Event(YIELD, { bubbles: true }));
}

/**
 * @param root the document's root element
 * @param kept where the places live; read through a ref, so a new object per render costs nothing
 * @param identity what the document is — a change of it is another document's places
 */
export function useKeptScroll(root: RefObject<HTMLElement | null>, kept: KeptScroll, identity: string): void {
  const keptRef = useRef(kept);
  keptRef.current = kept;

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const nameOf = (node: EventTarget | null) => (node instanceof HTMLElement ? node.getAttribute(KEEP_ATTR) : null);

    // --- keeping -----------------------------------------------------------
    // The writer is this document's, taken when the effect starts and again
    // at each scroll: by the time a cleanup runs the ref holds the *next*
    // document's, and the last scroll of this one would be kept under that.
    let own = keptRef.current;
    let restoring = true;
    // The offsets are read when the scroll happens and only *written* on the
    // frame: by the time an unmount's cleanup flushes, the element is
    // detached and reads 0 — which would erase the place it meant to keep.
    const pending = new Map<string, Place | null>();
    let frame = 0;
    const flush = () => {
      frame = 0;
      for (const [name, place] of pending) own.write(name, place);
      pending.clear();
    };
    const onScroll = (e: Event) => {
      const name = nameOf(e.target);
      // A scroll the restore itself made is not the person's place.
      if (!name || restoring) return;
      const port = e.target as HTMLElement;
      own = keptRef.current;
      pending.set(name, placeOf(port.scrollTop, port.scrollLeft));
      if (!frame) frame = requestAnimationFrame(flush);
    };
    el.addEventListener("scroll", onScroll, true);

    // --- restoring ---------------------------------------------------------
    const done = new Set<string>();
    const resize = new ResizeObserver(() => step());
    const mutation = new MutationObserver(() => step());
    let grace = 0;
    function finish() {
      if (!restoring) return;
      restoring = false;
      resize.disconnect();
      mutation.disconnect();
      clearTimeout(grace);
      for (const hand of HANDS) el!.removeEventListener(hand, finish, true);
    }
    function step() {
      if (!restoring) return;
      const ports = Array.from(el!.querySelectorAll<HTMLElement>(`[${KEEP_ATTR}]`));
      let waiting = false;
      for (const port of ports) {
        const name = port.getAttribute(KEEP_ATTR) ?? "";
        if (done.has(name)) continue;
        const next = restoreStep(keptRef.current.read(name), port);
        port.scrollTop = next.top;
        port.scrollLeft = next.left;
        if (next.done) done.add(name);
        else waiting = true;
        // The content inside is what grows: watch it, and the port itself.
        resize.observe(port);
        if (port.firstElementChild) resize.observe(port.firstElementChild);
      }
      // No port yet — a viewer still reading its bytes — is a reason to wait,
      // and the grace is the rendering's to grow in, not the read's: it
      // starts when the first scrollport is there.
      if (ports.length === 0) return;
      if (!waiting) finish();
      else if (!grace) grace = window.setTimeout(finish, RESTORE_GRACE_MS);
    }
    // A reveal is coming: what is kept is put back as far as it goes, once, and the restore ends.
    const giveWay = () => {
      step();
      finish();
    };
    mutation.observe(el, { childList: true, subtree: true });
    for (const hand of HANDS) el.addEventListener(hand, finish, true);
    el.addEventListener(YIELD, giveWay);
    step();

    return () => {
      finish();
      el.removeEventListener(YIELD, giveWay);
      el.removeEventListener("scroll", onScroll, true);
      if (frame) cancelAnimationFrame(frame);
      // The last scroll before an unmount may still be waiting for its frame.
      flush();
    };
  }, [root, identity]);
}
