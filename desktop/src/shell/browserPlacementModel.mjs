/**
 * Where a browser tab's native webview is drawn (ide/18), as facts: the
 * hosts' slots — the workbench's centre while a browser tab is the active
 * one there, the Details pane's Browser occupant while it shows a tab — say
 * which tab each stands for; a tab standing in both draws in the centre,
 * the bigger box; a tab standing in neither, or one shown while a kit
 * surface is open over the window, is hidden — unless it is a **headless**
 * tab, which is drawn offstage: a box outside the window's view, in the
 * window still, so the page keeps rendering and answers a screenshot, and
 * nobody sees it. Plain `.mjs`, so `node --test` reads it.
 */

/** The hosts in the order they win. */
export const HOSTS = Object.freeze(["center", "aux"]);

/** The box a headless tab renders in, offstage, when it chose none of its own: a desktop page's viewport. */
export const OFFSTAGE_VIEWPORT = Object.freeze({ width: 1280, height: 800 });

/** The bounds of a viewport an agent may give a tab out of sight — the shot's own (`browser.screenshot.width`), so the shell never re-clamps an emulated width. */
export const MIN_VIEWPORT = 320;
export const MAX_VIEWPORT = 4096;
/** How tall a tab out of sight may grow for a full-page screenshot: a 1280×8192 PNG stays far under the attachment cap. */
export const MAX_FULL_PAGE_HEIGHT = 8192;

/**
 * Where offstage is: wholly to the left of the window, above its top-left
 * corner, never on screen — at the tab's own viewport, else a desktop page's.
 * @param {{width: number, height: number}} [viewport]
 */
export function offstageRect(viewport = OFFSTAGE_VIEWPORT) {
  return { left: -viewport.width - 64, top: -viewport.height - 64, width: viewport.width, height: viewport.height };
}

/**
 * A viewport an agent asked for, or `null`: two whole numbers of pixels,
 * each within the shot's bounds — both, or nothing.
 * @param {unknown} width @param {unknown} height
 * @returns {{width: number, height: number} | null}
 */
export function viewportOf(width, height) {
  const ok = (n) => typeof n === "number" && Number.isInteger(n) && n >= MIN_VIEWPORT && n <= MAX_VIEWPORT;
  return ok(width) && ok(height) ? { width, height } : null;
}

/**
 * How tall a tab out of sight is drawn for a full-page screenshot: the
 * document's height, never shorter than the viewport, never past the cap.
 * @param {unknown} pageHeight what the page said of its document
 * @param {{width: number, height: number}} viewport the tab's box as it stands
 */
export function fullPageHeight(pageHeight, viewport) {
  const said = typeof pageHeight === "number" && Number.isFinite(pageHeight) ? Math.round(pageHeight) : 0;
  return Math.min(MAX_FULL_PAGE_HEIGHT, Math.max(viewport.height, said));
}

/**
 * The slot a tab is drawn over, offstage for a headless one — at its own viewport when it has one — or `null` to hide it.
 * @param {string} key the tab's key
 * @param {{center: {rect: object | null, layer: string | null, key: string | null} | null, aux: {rect: object | null, layer: string | null, key: string | null} | null, mayShow: boolean, headless?: boolean, viewport?: {width: number, height: number} | null}} facts
 * @returns {{host: "center" | "aux" | "offstage", rect: {left: number, top: number, width: number, height: number}} | null}
 */
export function placementOf(key, facts) {
  if (facts?.mayShow) {
    for (const host of HOSTS) {
      const slot = facts[host];
      if (!slot || slot.layer !== "browser" || slot.key !== key) continue;
      const rect = slot.rect;
      if (!rect || !(rect.width > 0) || !(rect.height > 0)) continue;
      return { host, rect };
    }
  }
  // Out of sight, a dialog over the window changes nothing: offstage is offstage.
  if (facts?.headless) return { host: "offstage", rect: offstageRect(facts.viewport ?? OFFSTAGE_VIEWPORT) };
  return null;
}

/**
 * The tab a person is looking at, if any: the one the centre draws, else
 * the one the pane draws — what the footer's button is pressed for.
 * @param {{center: {layer: string | null, key: string | null} | null, aux: {layer: string | null, key: string | null} | null}} slots
 * @returns {string | null}
 */
export function shownTab(slots) {
  for (const host of HOSTS) {
    const slot = slots?.[host];
    if (slot && slot.layer === "browser" && slot.key) return slot.key;
  }
  return null;
}

/** Whether the tab renders right now — a host shows it, or it is offstage — what a screenshot waits for. */
export function isShown(key, facts) {
  return placementOf(key, facts) !== null;
}

/** How far, in CSS pixels, a drawn box may stray from the box asked before the layer says so. */
export const PLACEMENT_TOLERANCE = 1;

/**
 * Whether the box the shell read back after a placement is the box that
 * was asked — each edge within the tolerance, and shown. A `false` is the
 * one fact that tells where the page was drawn if it ever covers the bar.
 * @param {{left: number, top: number, width: number, height: number}} asked
 * @param {{left: number, top: number, width: number, height: number, shown: boolean}} drawn
 */
export function placedAsAsked(asked, drawn) {
  if (!drawn?.shown) return false;
  const near = (a, b) => Math.abs(Number(a) - Number(b)) <= PLACEMENT_TOLERANCE;
  return near(asked.left, drawn.left) && near(asked.top, drawn.top) && near(asked.width, drawn.width) && near(asked.height, drawn.height);
}
