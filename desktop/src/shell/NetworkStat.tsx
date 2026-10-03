/**
 * The footer's network read-out — the network glyph and a dot in the tone
 * (danger while this Mac cannot reach the internet, ok while it can, quiet
 * before the first read), with the word *VPN* beside it only while a tunnel
 * is up (`barWord`); the value and its sentence are the tooltip and the
 * accessible name — and, on a
 * click, its overlay (`NetworkOverlay`). Controlled, so the overlay's door
 * can close the panel behind it. The facts are `networkStore.ts`'s, read
 * again on a `network.*` write, on focus and when the window's network
 * flips; every word is `networkStatModel.mjs`'s.
 */

import { useState } from "react";
import { useEngineEvents } from "../bus";
import { t } from "../i18n/l10n.mjs";
import { Dot, ICON, Popover, Tooltip } from "../ui";
import { NetworkOverlay } from "./NetworkOverlay";
import { barWord, statWords } from "./networkStatModel.mjs";
import { refreshNetwork, useNetwork } from "./networkStore";

export function NetworkStat() {
  const [open, setOpen] = useState(false);
  const { facts, status, online } = useNetwork();
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("network."))) void refreshNetwork();
  });
  const words = statWords(facts, status, online);
  const word = barWord(words);
  return (
    <Popover
      label={words.title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      // In the type unit, so the panel grows with the text setting rather than trapping its rows.
      className="w-[calc(var(--type-rem)*30)] max-w-[90vw]"
      trigger={
        <Tooltip label={words.title}>
          <span aria-label={t("shell-network-stat-network-value", { value: words.value })} className="anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1.5 text-text-dim hover:bg-surface-2 hover:text-text">
            <ICON.network size={13} aria-hidden />
            <Dot tone={words.tone} title={words.value} />
            {word && <span className="text-2xs text-text">{word}</span>}
          </span>
        </Tooltip>
      }
    >
      {open && <NetworkOverlay close={() => setOpen(false)} />}
    </Popover>
  );
}
