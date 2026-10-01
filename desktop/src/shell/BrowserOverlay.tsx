/**
 * The overlay under the footer's Browser read-out (ide/18 §The footer's
 * count), in the resource overlays' shape: the footer's sentence, a bar of
 * the tabs in sight against the ones kept out of sight, a control of three
 * dimensions — **Tabs**, every tab where it is at home; **Origins**, how
 * many at each place; **Unseen**, the tabs agents keep out of sight — the
 * rows of the chosen one, each a door to the pane on its tab (which shows
 * one kept out of sight — the person's act, never an agent's) with its ✕
 * beside the button and never inside it, *New tab* at home where the person
 * is and *Settings › Browser* in the header, and the out-of-sight policy as
 * the footnote. The dimension is remembered the way a resource's is
 * (`resourceDimensionStore`, under `browser`); the names of the tabs' homes
 * are read while the panel is open, never at rest. Every fact is
 * `browserStatModel.mjs`'s; this file paints.
 */

import { navigate } from "../router";
import { Button, ICON, Meter, SegmentedControl, StackedBar, WorkingDot, cn } from "../ui";
import type { Segment as ControlSegment } from "../ui";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { useBusyBrowserTabs } from "./browserActivityStore";
import { openBrowserPane, screenHome } from "./browserDoors";
import { useBrowserPlaces } from "./browserPlaces";
import { shownTab } from "./browserPlacementModel.mjs";
import { useBrowserPrefs } from "./browserPrefsStore";
import type { Dimension, Door } from "./browserStatModel.mjs";
import { DIMENSIONS, chosenDimension, dimensionIcon, dimensionLabel, emptyWords, footnote, overlayRows, statWords, visibilityBar } from "./browserStatModel.mjs";
import { useLayerSlot } from "./layerSlots";
import { chooseDimension, useChosenDimension } from "./resourceDimensionStore";
import { closeBrowserTab, openBrowserIn, useBrowsers } from "./useBrowsers";
import { PERSON } from "./browsersModel.mjs";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";
import { percent } from "../i18n/format.mjs";

function walk(door: Door, close: () => void) {
  if (!door) return;
  openBrowserPane(door.key);
  close();
}

export function BrowserOverlay({ close }: { close: () => void }) {
  const { sessions } = useBrowsers();
  const busy = useBusyBrowserTabs();
  const places = useBrowserPlaces();
  const prefs = useBrowserPrefs();
  const center = useLayerSlot("center");
  const aux = useLayerSlot("aux");
  const shown = shownTab({ center, aux });
  const dimension = chosenDimension(useChosenDimension("browser"));

  const words = statWords({ sessions, busy, shown });
  const segments = visibilityBar(sessions);
  // Who opened each tab is said by name: the agents are the workspace's the shell already holds.
  const { agents } = useWorkspace();
  const rows = overlayRows(dimension, { sessions, places, busy, shown, agentName: (id) => agents.find((a) => a.id === id)?.name ?? null });
  const options: ControlSegment<Dimension>[] = DIMENSIONS.map((d) => ({ id: d, label: dimensionLabel(d), icon: ICON[dimensionIcon(d)] }));

  const newTab = () => {
    const key = openBrowserIn({ home: screenHome(), by: PERSON });
    if (key) openBrowserPane(key);
    close();
  };
  const settings = () => {
    navigate({ name: "settings" }, settingsSearch("browser"));
    close();
  };

  return (
    <div className="flex min-w-0 flex-col gap-2" role="group" aria-label={words.title}>
      <div className="flex items-center gap-2 px-1">
        <p className="min-w-0 flex-1 truncate text-xs font-medium text-text" title={words.title}>
          {words.title}
        </p>
        <Button size="sm" variant="ghost" onClick={newTab} title={t("shell-browser-overlay-beside-screen-browser-pane")}>{t("shell-browser-overlay-new-tab")}</Button>
        <Button size="sm" variant="ghost" onClick={settings}>{settingsPath("browser")}</Button>
      </div>
      <StackedBar segments={segments} label={segments.map((s) => t("shell-browser-overlay-segment", { label: s.label, percent: percent(s.percent / 100) })).join(", ")} />
      <SegmentedControl<Dimension> size="sm" stretch label={t("shell-browser-overlay-words", { title: words.title })} options={options} value={dimension} onChange={(d) => chooseDimension("browser", d)} />
      {rows.length === 0 ? (
        <p className="px-2 py-3 text-center text-2xs text-text-dim">{emptyWords(dimension, sessions.length)}</p>
      ) : (
        <div className="flex max-h-80 flex-col overflow-y-auto">
          {rows.map((r) => {
            const body = (
              <>
                {r.busy && <WorkingDot title={t("shell-browser-door-agent-browsing")} />}
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className={cn("min-w-0 truncate text-2xs", r.current ? "text-accent-ink" : "text-text")}>{r.label}</span>
                  {r.sub && <span className="min-w-0 truncate text-3xs text-text-dim">{r.sub}</span>}
                </span>
                {r.percent !== null && <Meter percent={r.percent} tone="quiet" width="w-12" />}
                <span className="tnum w-16 shrink-0 text-right text-2xs text-text-dim">{r.value}</span>
              </>
            );
            const className = cn("flex min-w-0 flex-1 items-center gap-2 rounded-control px-1.5 py-1 text-left", r.dim && "opacity-60");
            return (
              <div key={r.key} className={cn("flex items-center gap-1", r.current && "rounded-control bg-accent-soft")}>
                {r.door ? (
                  <button type="button" onClick={() => walk(r.door, close)} aria-current={r.current ? "true" : undefined} title={r.hint ?? undefined} className={cn("anim hover:bg-surface-2", className)}>
                    {body}
                  </button>
                ) : (
                  <div title={r.hint ?? undefined} className={className}>
                    {body}
                  </div>
                )}
                {r.close && (
                  <button type="button" aria-label={t("shell-browser-overlay-close", { r: r.label })} onClick={() => closeBrowserTab(r.close as string)} className="anim shrink-0 rounded p-0.5 text-text-dim hover:text-text">
                    <ICON.close size={11} aria-hidden />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
      <p className="px-1 text-3xs text-text-dim">{footnote(prefs.headless)}</p>
    </div>
  );
}
