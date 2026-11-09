/**
 * Whether a browser tab's page renders right now, and a wait until it does
 * (ide/18 §Screenshots) — one answer for the bar's camera and the agents'
 * bridge alike, so neither asks the shell for a snapshot of a hidden
 * webview. A tab renders when a host shows it or it is offstage
 * (`browserPlacementModel.isShown`), and only while no kit surface is open
 * (`ui/openSurfaces.ts`); both facts change, so the wait listens to both.
 */

import { layerMayShowNow, subscribeSurfaces } from "../ui/openSurfaces";
import { isShown, placementOf } from "./browserPlacementModel.mjs";
import { sessionOf } from "./browsersModel.mjs";
import { layerSlot, subscribeLayerSlots } from "./layerSlots";
import { browserState } from "./useBrowsers";

/** How long a tab gets to be shown before the wait gives up. */
const SHOW_MS = 5_000;
/** Frames the layer gets to settle over its new box, then a beat for the page's own paint — after a change, never when the tab already shows. */
const SETTLE_FRAMES = 2;
const SETTLE_MS = 150;

/** The facts a placement reads of a tab right now: the hosts' slots, whether a surface is over the window, and the tab's own — kept out of sight, and the viewport it chose there (ide/18 §Emulation). */
function factsNow(key: string) {
  const s = sessionOf(browserState(), key);
  return { center: layerSlot("center"), aux: layerSlot("aux"), mayShow: layerMayShowNow(), headless: s?.headless ?? false, viewport: s?.viewport ?? null };
}

/** Whether the tab renders right now: a host shows it, or it is offstage, and no surface is over the window. */
export function shownNow(key: string): boolean {
  return isShown(key, factsNow(key));
}

/** Where the tab is drawn right now — a host's box, or its offstage one — or `null` while it is hidden. */
export function placementNow(key: string) {
  return placementOf(key, factsNow(key));
}

/** Two frames for the layer to settle over a new box, then a beat for the page's own paint — what a shot or a measure waits after a tab's box changed. */
export function settled(): Promise<void> {
  return new Promise((resolve) => {
    let frames = SETTLE_FRAMES;
    const tick = () => {
      if (frames-- > 0) window.requestAnimationFrame(tick);
      else window.setTimeout(resolve, SETTLE_MS);
    };
    window.requestAnimationFrame(tick);
  });
}

/**
 * Wait until the tab renders: at once when it already does; else on the
 * slots' or the surfaces' next word that it does, then two frames and a
 * beat for the paint; `false` when `ms` pass first.
 */
export function untilShown(key: string, ms: number = SHOW_MS): Promise<boolean> {
  if (shownNow(key)) return Promise.resolve(true);
  return new Promise((resolve) => {
    const offs: Array<() => void> = [];
    const stop = () => {
      for (const off of offs) off();
      offs.length = 0;
    };
    const timer = window.setTimeout(() => {
      stop();
      resolve(false);
    }, ms);
    const check = () => {
      if (!shownNow(key)) return;
      window.clearTimeout(timer);
      stop();
      void settled().then(() => resolve(true));
    };
    offs.push(subscribeLayerSlots(check), subscribeSurfaces(check));
    check();
  });
}
