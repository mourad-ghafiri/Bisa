/**
 * What a browser tab's bar shows (ide/18), as facts: a browser's bar,
 * whole, on every tab — back, forward, reload (*Stop* while the page
 * loads), the address field, the wand, the camera, the doors — each verb
 * enabled or held with its reason, never absent, so the bar reads the same
 * on a blank tab and on a page: what greys out says what the tab lacks.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/** Why a verb is held, in the words its tooltip wears. */
export const HELD = Object.freeze({
  blank: t("shell-browser-chrome-type-address-first"),
  noBack: t("shell-browser-chrome-nothing-go-back"),
  noForward: t("shell-browser-chrome-nothing-go-forward"),
  artifact: t("shell-browser-chrome-agent-s-own-page-nobody-s"),
});

/**
 * @param {{
 *   blank: boolean,           nothing loaded yet
 *   loading: boolean,         a page is on its way
 *   canBack: boolean,         the webview has a page to go back to
 *   canForward: boolean,      … and one to go forward to
 *   inIde: boolean,           the bar is the IDE's centre's, not the Details pane's
 *   atWorkbenchHome: boolean, the tab is at home in a workbench root
 *   annotatable: boolean,     the page can be annotated for an agent
 *   whyNot: string | null,    why not, when it cannot — the wand's held reason
 * }} facts
 * @returns {{
 *   back: {enabled: boolean, why: string | null},
 *   forward: {enabled: boolean, why: string | null},
 *   load: {verb: "reload" | "stop", enabled: boolean, why: string | null},
 *   address: true,
 *   wand: {enabled: boolean, why: string | null},
 *   camera: {enabled: boolean, why: string | null},
 *   openOutside: {enabled: boolean, why: string | null},
 *   openInIde: boolean,
 * }}
 */
export function chromeOf({ blank, loading, canBack, canForward, inIde, atWorkbenchHome, annotatable, whyNot = null }) {
  const held = (why) => ({ enabled: false, why });
  const free = { enabled: true, why: null };
  const page = blank ? held(HELD.blank) : free;
  return {
    back: canBack ? free : held(HELD.noBack),
    forward: canForward ? free : held(HELD.noForward),
    load: blank ? { verb: "reload", enabled: false, why: HELD.blank } : { verb: loading ? "stop" : "reload", enabled: true, why: null },
    address: true,
    wand: blank ? held(HELD.blank) : annotatable ? free : held(whyNot ?? HELD.artifact),
    camera: page,
    openOutside: page,
    openInIde: !inIde && atWorkbenchHome,
  };
}

/** The load button's word. @param {"reload" | "stop"} verb */
export function loadWords(verb) {
  return verb === "stop" ? t("shell-browser-chrome-stop-loading") : t("shell-browser-chrome-reload");
}

/**
 * The wand's tooltip: what pressing it does, or why it is held.
 * @param {{enabled: boolean, why: string | null}} wand
 * @param {boolean} inspecting the wand is on
 */
export function wandWords(wand, inspecting) {
  if (!wand.enabled) return t("shell-browser-chrome-annotate-page-agent", { why: wand.why });
  return inspecting ? t("shell-browser-chrome-stop-picking-elements-annotations-stay") : t("shell-browser-chrome-annotate-page-agent-hover-element-click");
}
