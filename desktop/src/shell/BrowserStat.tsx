/**
 * The footer's Browser read-out (ide/18 §The footer's count) — one glyph
 * and one number, every open tab in sight or not, tinted while a host draws
 * a tab, the working dot while an agent browses, the footer's sentence as
 * the tooltip and the accessible name — and, on a click, its overlay
 * (`BrowserOverlay`). Controlled, so a row that opened something can close
 * the panel behind it. It follows the browser's switch and both hosts'
 * slots, so it never says a browser that is not there: nothing outside the
 * shell, while the browser is off, or before this machine's settings have
 * been read (`canOpenBrowser`). Every word is `browserStatModel.mjs`'s.
 */

import { useState } from "react";
import { t } from "../i18n/l10n.mjs";
import { ICON, Popover, Tooltip, WorkingDot, cn } from "../ui";
import { useBusyBrowserTabs } from "./browserActivityStore";
import { BrowserOverlay } from "./BrowserOverlay";
import { shownTab } from "./browserPlacementModel.mjs";
import { useBrowserPrefs } from "./browserPrefsStore";
import { statWords } from "./browserStatModel.mjs";
import { useLayerSlot } from "./layerSlots";
import { canOpenBrowser, useBrowsers } from "./useBrowsers";

export function BrowserStat() {
  const [open, setOpen] = useState(false);
  // The subscription: the gate below re-evaluates as the settings are read.
  useBrowserPrefs();
  const { sessions } = useBrowsers();
  const busy = useBusyBrowserTabs();
  const center = useLayerSlot("center");
  const aux = useLayerSlot("aux");
  if (!canOpenBrowser()) return null;
  const words = statWords({ sessions, busy, shown: shownTab({ center, aux }) });
  return (
    <Popover
      label={words.title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-[calc(var(--type-rem)*40)] max-w-[90vw]"
      trigger={
        <Tooltip label={words.title}>
          <span aria-label={t("shell-browser-stat-browser-value", { value: words.value })} aria-pressed={words.pressed} className={cn("anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1 hover:bg-surface-2", words.pressed ? "bg-accent-soft text-accent-ink" : "text-text-dim")}>
            <ICON.page size={13} aria-hidden />
            <span className="tnum text-2xs text-text">{words.value}</span>
            {words.live && <WorkingDot title={words.live} />}
          </span>
        </Tooltip>
      }
    >
      {open && <BrowserOverlay close={() => setOpen(false)} />}
    </Popover>
  );
}
