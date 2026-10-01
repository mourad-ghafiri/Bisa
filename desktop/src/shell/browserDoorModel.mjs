/**
 * A screen's Browser door (ide/18), as facts: the one button every
 * conversation, goal and workflow header carries — the word, how many tabs
 * are open, whether an agent is browsing right now, whether the Browser
 * pane shows — and what its menu lists: every open tab (the ones at home
 * here first, the busy ones marked), a new tab, the pane's close, and the
 * closing of every tab. Plain `.mjs`, so `node --test` reads it.
 */

import { t as tr } from "../i18n/l10n.mjs";

/**
 * The button's words.
 * @param {{count: number, busy: number, showing: boolean}} facts open tabs, tabs an agent is working in, the pane showing
 * @returns {{label: string, count: number, hint: string}}
 */
export function doorWords({ count, busy, showing }) {
  let hint;
  if (busy > 0) hint = tr("shell-browser-door-agent-browsing-tabs-open", { count });
  else if (showing) hint = tr("shell-browser-door-hide-browser-pane-l");
  else if (count > 0) hint = tr("shell-browser-door-show-pane-tabs-open", { count });
  else hint = tr("shell-browser-door-embedded-browser-beside-screen-agents-tabs");
  return { label: tr("shell-browser-pane-browser"), count, hint };
}

/**
 * The menu's items, in order and a rule at each group's start: every open
 * tab — the ones at home here first, then the rest with where they are at
 * home, the active one leading its group, a busy one marked, one kept out
 * of sight marked *unseen* with the hidden glyph (a pick shows it) — then
 * *New tab*, then *Close the pane* while it shows, then *Close every tab*.
 * @param {{
 *   tabs: readonly {key: string, label: string, home: {scope: string, id: string} | null, where: string, busy: boolean, headless?: boolean}[],   `where`: the tab's home by name (`browserPlacesModel.whereWords`)
 *   here: {scope: string, id: string} | null,   the screen's place, when it has one
 *   active: string | null,
 *   showing: boolean,
 * }} facts
 * @returns {{id: string, label: string, icon: string, danger?: boolean, separatorBefore?: boolean}[]}
 */
export function doorMenu({ tabs, here, active, showing }) {
  const atHome = (t) => !!here && !!t.home && t.home.scope === here.scope && t.home.id === here.id;
  const lead = (list) => {
    const i = list.findIndex((t) => t.key === active);
    return i <= 0 ? list : [list[i], ...list.slice(0, i), ...list.slice(i + 1)];
  };
  const ours = lead((tabs ?? []).filter(atHome));
  const others = lead((tabs ?? []).filter((t) => !atHome(t)));
  const row = (t, elsewhere) => ({
    id: `tab:${t.key}`,
    label: [t.label, ...(elsewhere ? [t.where] : []), ...(t.headless ? [tr("shell-browser-door-unseen")] : [])].join(" · ") + (t.busy ? tr("shell-browser-door-agent-browsing-aside") : ""),
    icon: t.headless ? "hidden" : "page",
  });
  const items = [];
  ours.forEach((t, i) => items.push({ ...row(t, false), separatorBefore: i === 0 }));
  others.forEach((t, i) => items.push({ ...row(t, true), separatorBefore: i === 0 }));
  items.push({ id: "new", label: tr("shell-browser-overlay-new-tab"), icon: "add", separatorBefore: items.length > 0 });
  if (showing) items.push({ id: "hide", label: tr("shell-browser-door-close-pane"), icon: "close", separatorBefore: true });
  if ((tabs ?? []).length > 0) items.push({ id: "close-all", label: (tabs ?? []).length === 1 ? tr("shell-browser-door-close-tab") : tr("shell-browser-door-close-every-tab", { tabs: tabs.length }), icon: "close", danger: true, separatorBefore: !showing });
  return items;
}

/** The footer's word while an agent works. @param {number} n the tabs an agent is working in */
export function busyWords(n) {
  return n === 1 ? tr("shell-browser-door-agent-browsing-1-tab") : tr("shell-browser-door-agent-browsing-tabs", { n });
}
