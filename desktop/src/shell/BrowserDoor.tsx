/**
 * A screen's Browser button (ide/18): the one door to the embedded browser
 * on every conversation's, goal's and workflow's header. The main click
 * shows or hides the Details pane's Browser occupant beside this screen —
 * pressed while it shows — and the button carries the count of open tabs
 * and a live dot while an agent is browsing in one, so a pane closed by
 * mistake is one click away and never a mystery. The caret lists every
 * open tab (the ones at home here first, the busy ones marked), *New tab*,
 * *Close the pane* and *Close every tab*. The words and the order are
 * `browserDoorModel.mjs`'s; which tabs are busy is
 * `browserActivityStore.ts`'s. Nothing outside the desktop shell, or with
 * the browser off.
 */

import { ICON, Menu, Tooltip, WorkingDot, cn } from "../ui";
import type { MenuItem } from "../ui";
import { useAux } from "./AuxPane";
import { useBusyBrowserTabs } from "./browserActivityStore";
import { openBrowserPane, toggleBrowserPane } from "./browserDoors";
import { doorMenu, doorWords } from "./browserDoorModel.mjs";
import { useBrowserPrefs } from "./browserPrefsStore";
import { useBrowserPlaces } from "./browserPlaces";
import { whereWords } from "./browserPlacesModel.mjs";
import { PERSON, browserLabel } from "./browsersModel.mjs";
import type { BrowserHome } from "./browsersModel.mjs";
import { canOpenBrowser, closeBrowserTab, openBrowserIn, useBrowsers } from "./useBrowsers";
import { t } from "../i18n/l10n.mjs";

export function BrowserDoor({ home = null, className }: { home?: BrowserHome | null; className?: string }) {
  const aux = useAux();
  const { sessions, active } = useBrowsers();
  const busy = useBusyBrowserTabs();
  const places = useBrowserPlaces();
  useBrowserPrefs();
  if (!canOpenBrowser()) return null;
  const showing = aux.kind === "browser";
  const words = doorWords({ count: sessions.length, busy: busy.length, showing });
  const items: MenuItem[] = doorMenu({
    tabs: sessions.map((s) => ({ key: s.key, label: browserLabel(s), home: s.home, where: whereWords(s, places), busy: busy.includes(s.key), headless: s.headless })),
    here: home,
    active,
    showing,
  }).map((item) => ({
    label: item.label,
    icon: (ICON as Record<string, typeof ICON.page>)[item.icon],
    danger: item.danger,
    separatorBefore: item.separatorBefore,
    onSelect: () => act(item.id),
  }));
  const act = (id: string) => {
    if (id.startsWith("tab:")) openBrowserPane(id.slice(4));
    else if (id === "new") openBrowserPane(openBrowserIn({ home, by: PERSON }));
    else if (id === "hide") aux.close();
    else if (id === "close-all") for (const s of sessions) closeBrowserTab(s.key);
  };
  const pressed = showing ? "border-accent/50 bg-accent-soft text-accent-ink" : "border-border text-text-dim hover:bg-surface-2 hover:text-text";
  return (
    <span className={cn("inline-flex items-center", className)}>
      <Tooltip label={words.hint}>
        <button type="button" aria-pressed={showing} onClick={() => toggleBrowserPane(aux, home)} className={cn("anim inline-flex h-7 items-center gap-1.5 rounded-l-control border border-r-0 px-2 text-xs font-medium", pressed)}>
          <ICON.page size={13} aria-hidden />
          {words.label}
          {words.count > 0 && <span className="tnum">{words.count}</span>}
          {busy.length > 0 && <WorkingDot title={t("shell-browser-door-agent-browsing")} />}
        </button>
      </Tooltip>
      <Menu
        label={t("shell-browser-door-browser-s-tabs")}
        items={items}
        trigger={
          <span className={cn("anim inline-flex h-7 items-center rounded-r-control border px-1", pressed)}>
            <ICON.expanded size={12} aria-hidden />
          </span>
        }
      />
    </span>
  );
}
