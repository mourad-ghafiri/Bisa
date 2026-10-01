/**
 * The footer's network read-out — the network glyph, a dot in the tone and
 * one short word (*DOWN* in red while this Mac cannot reach the internet,
 * *VPN* while a tunnel is up, *UP* otherwise, a dash before the first
 * read), the sentence as the tooltip and the accessible name — and, on a
 * click, its overlay (`NetworkOverlay`). Controlled, so the overlay's door
 * can close the panel behind it. The facts are `networkStore.ts`'s, read
 * again on a `network.*` write, on focus and when the window's network
 * flips; every word is `networkStatModel.mjs`'s.
 */

import { useState } from "react";
import { useEngineEvents } from "../bus";
import { t } from "../i18n/l10n.mjs";
import { Dot, ICON, Popover, Tooltip, cn } from "../ui";
import { NetworkOverlay } from "./NetworkOverlay";
import { statWords } from "./networkStatModel.mjs";
import { refreshNetwork, useNetwork } from "./networkStore";

export function NetworkStat() {
  const [open, setOpen] = useState(false);
  const { facts, status, online } = useNetwork();
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("network."))) void refreshNetwork();
  });
  const words = statWords(facts, status, online);
  return (
    <Popover
      label={words.title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-[30rem] max-w-[90vw]"
      trigger={
        <Tooltip label={words.title}>
          <span aria-label={t("shell-network-stat-network-value", { value: words.value })} className={cn("anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1 text-text-dim hover:bg-surface-2")}>
            <ICON.network size={13} aria-hidden />
            <Dot tone={words.tone} title={words.value} />
            <span className="text-2xs text-text">{words.value}</span>
          </span>
        </Tooltip>
      }
    >
      {open && <NetworkOverlay close={() => setOpen(false)} />}
    </Popover>
  );
}
