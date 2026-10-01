/**
 * The footer's Browser read-out (ide/18 §The footer's count), as facts: one
 * glyph and one number — every open tab, in sight or not — and, on a click,
 * an overlay in the resource read-outs' shape: a bar of the tabs in sight
 * against the ones kept out of sight, a control of three dimensions —
 * **Tabs**, every tab where it is at home; **Origins**, how many at each
 * place; **Unseen**, the tabs agents keep out of sight — the rows of the
 * chosen one, and the out-of-sight policy as the footnote. The words come
 * from the models every list of tabs already wears (`browserPlacesModel`,
 * `browsersModel`, `browserDoorModel`); this file composes. Plain `.mjs`,
 * so `node --test` reads it.
 */

import { busyWords } from "./browserDoorModel.mjs";
import { footerBrowserWords, groupTabs, whereWords } from "./browserPlacesModel.mjs";
import { browserLabel, openerWords, seenSessions, visibilityWords } from "./browsersModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The overlay's dimensions, in the control's order. */
export const DIMENSIONS = Object.freeze(["tabs", "origins", "unseen"]);

/** The dimension the control opens on before a choice, and after a choice it no longer offers. */
const FIRST = "tabs";

const WORDS = Object.freeze({ tabs: t("shell-browser-stat-tabs-2"), origins: t("shell-browser-stat-origins"), unseen: t("shell-browser-stat-unseen-2") });
const GLYPHS = Object.freeze({ tabs: "page", origins: "layout", unseen: "hidden" });

/** A dimension's word on the control — one short word, so the row never cuts one. @param {string} dimension */
export function dimensionLabel(dimension) {
  return WORDS[dimension] ?? String(dimension);
}

/** A dimension's glyph, as a key of the icon registry (`ui/icons.ts`). @param {string} dimension */
export function dimensionIcon(dimension) {
  return GLYPHS[dimension] ?? "page";
}

/** The remembered dimension while the control still offers it, else the first. @param {string | null | undefined} remembered */
export function chosenDimension(remembered) {
  return DIMENSIONS.includes(remembered) ? remembered : FIRST;
}

const headlessOf = (sessions) => (sessions ?? []).filter((s) => s.headless);
/** The tabs an agent's request opened — what the sentence names, so a number that surprises says whose it is. */
const agentsOf = (sessions) => (sessions ?? []).filter((s) => s.by?.kind === "agent");

/**
 * The read-out's facts: the number is every open tab, the title the
 * footer's sentence — how many of them agents opened among its asides — the
 * dot's words while an agent browses, pressed while a host draws a tab.
 * @param {{sessions: readonly object[], busy: readonly string[], shown: string | null}} facts `shown`: the key of the tab on screen (`browserPlacementModel.shownTab`)
 * @returns {{value: string, title: string, live: string | null, pressed: boolean}}
 */
export function statWords({ sessions, busy, shown }) {
  const tabs = sessions ?? [];
  const working = (busy ?? []).length;
  const shownTab = shown ? tabs.find((s) => s.key === shown) ?? null : null;
  return {
    value: String(tabs.length),
    title: footerBrowserWords({ count: tabs.length, headless: headlessOf(tabs).length, busy: working, agents: agentsOf(tabs).length, shown: shownTab ? browserLabel(shownTab) : null }),
    live: working > 0 ? busyWords(working) : null,
    pressed: shownTab !== null,
  };
}

/**
 * The bar over every tab: the ones in sight, then the ones kept out of
 * sight, each as a percent of the whole; an empty part left out, nothing
 * for no tab at all.
 * @param {readonly {headless: boolean}[]} sessions
 * @returns {{key: string, label: string, percent: number, tone: "accent" | "quiet"}[]}
 */
export function visibilityBar(sessions) {
  const tabs = sessions ?? [];
  if (tabs.length === 0) return [];
  const unseen = headlessOf(tabs).length;
  const seen = tabs.length - unseen;
  const share = (n) => (n / tabs.length) * 100;
  return [
    { key: "seen", label: t("shell-browser-stat-sight"), percent: share(seen), tone: "accent" },
    { key: "unseen", label: t("shell-browser-stat-out-sight"), percent: share(unseen), tone: "quiet" },
  ].filter((s) => s.percent > 0);
}

/**
 * A tab's row: its label, where it is at home by name and **who opened it**
 * — an agent by name, a page; nothing for the person's own — whether it is
 * on screen, kept out of sight or an agent's for the moment — a door to the
 * pane on it (which shows one kept out of sight), and closable beside it.
 */
function tabRow(s, { places, busy, shown, agentName }) {
  const working = (busy ?? []).includes(s.key);
  const current = s.key === shown;
  const where = working ? t("shell-browser-stat-where-agent-browsing", { where: whereWords(s, places) }) : whereWords(s, places);
  const opener = openerWords(s.by, agentName);
  return {
    key: s.key,
    label: browserLabel(s),
    sub: opener ? t("shell-browser-stat-where-opener", { where, opener }) : where,
    value: s.headless ? t("shell-browser-stat-unseen") : current ? t("shell-browser-stat-screen") : "",
    percent: null,
    door: { kind: "tab", key: s.key },
    close: s.key,
    hint: visibilityWords(s),
    current,
    dim: !!s.headless,
    busy: working,
  };
}

/** An origin's sub: how many tabs, how many out of sight, how many an agent works in — the asides only when there are any. */
function originSub(tabs, busy) {
  const parts = [tabs.length === 1 ? "1 tab" : t("shell-browser-stat-tabs", { tabs: tabs.length })];
  const unseen = headlessOf(tabs).length;
  if (unseen > 0) parts.push(t("shell-browser-stat-out-sight-2", { unseen }));
  const working = tabs.filter((s) => (busy ?? []).includes(s.key)).length;
  if (working > 0) parts.push(t("shell-browser-stat-agent-browsing", { working }));
  return parts.join(" · ");
}

/**
 * The rows of one dimension — **tabs**: one row per tab in the order
 * opened; **origins**: one row per origin in the list's order, empty ones
 * left out, its bar the share of the largest, its door the origin's first
 * tab in sight; **unseen**: the tabs kept out of sight alone.
 * @param {"tabs" | "origins" | "unseen"} dimension
 * @param {{sessions: readonly object[], places: object, busy: readonly string[], shown: string | null, agentName?: (id: string) => string | null | undefined}} ctx `agentName`: an agent's name by id, for the opener's words
 * @returns {object[]}
 */
export function overlayRows(dimension, ctx) {
  const tabs = ctx?.sessions ?? [];
  switch (dimension) {
    case "origins": {
      const groups = groupTabs(tabs);
      const largest = Math.max(0, ...groups.map((g) => g.tabs.length));
      return groups.map((g) => {
        const first = seenSessions(g.tabs)[0] ?? null;
        return {
          key: `origin:${g.kind}`,
          label: g.label,
          sub: originSub(g.tabs, ctx.busy),
          value: String(g.tabs.length),
          percent: largest > 0 ? (g.tabs.length / largest) * 100 : 0,
          door: first ? { kind: "tab", key: first.key } : null,
          close: null,
          hint: null,
          current: g.tabs.some((s) => s.key === ctx.shown),
          dim: first === null,
          busy: g.tabs.some((s) => (ctx.busy ?? []).includes(s.key)),
        };
      });
    }
    case "unseen":
      return headlessOf(tabs).map((s) => tabRow(s, ctx));
    default:
      return tabs.map((s) => tabRow(s, ctx));
  }
}

/**
 * The line under a dimension with no row: no tab at all, or — under
 * *Unseen* while tabs are open — every tab in sight.
 * @param {string} dimension @param {number} count every open tab
 */
export function emptyWords(dimension, count) {
  if ((Number(count) || 0) === 0) return t("shell-browser-stat-browser-tab-open-new-tab-here");
  return dimension === "unseen" ? t("shell-browser-stat-every-tab-sight") : t("shell-resource-overlay-nothing-show-yet");
}

/**
 * The footnote: the out-of-sight policy (`browser.agents.headless`) in one
 * sentence — the switch itself is in Settings › Browser.
 * @param {string} policy `unattended` · `always` · `never`
 */
export function footnote(policy) {
  switch (policy) {
    case "always":
      return t("shell-browser-stat-every-tab-agent-opens-kept-out");
    case "never":
      return t("shell-browser-stat-every-tab-agent-opens-shown-beside");
    default:
      return t("shell-browser-stat-agent-s-tab-kept-out-sight");
  }
}
