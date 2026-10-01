/**
 * The overlay under the footer's network read-out: the internet — reached
 * in how long, the public IP, whether DNS answers — the VPN's standing and
 * the up tunnel's rows, every interface that is up, this Mac's default
 * route and resolver, the proxy System Settings names, and what the
 * platform's own calls leave through — the same sentences Settings ›
 * Capabilities › Network says — with one door to that panel. Every fact is
 * `networkStatModel.mjs`'s; this file paints.
 */

import { inDesktopShell } from "../api";
import { navigate } from "../router";
import { Button, Chip, cn } from "../ui";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { footnote, overlaySections } from "./networkStatModel.mjs";
import { networkPoll, useNetwork } from "./networkStore";
import { t } from "../i18n/l10n.mjs";

export function NetworkOverlay({ close }: { close: () => void }) {
  const { facts, status, online, readAt } = useNetwork();
  const sections = overlaySections(facts, status, online, { desktop: inDesktopShell() });
  const open = () => {
    navigate({ name: "settings" }, settingsSearch("network"));
    close();
  };
  return (
    <div className="flex min-w-0 flex-col gap-2" role="group" aria-label={t("shell-network-overlay-network")}>
      <div className="flex items-center gap-2 px-1">
        <p className="text-xs font-medium text-text">{t("shell-network-overlay-network")}</p>
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={open}>{settingsPath("network")}</Button>
      </div>
      <div className="flex max-h-80 flex-col gap-2 overflow-y-auto">
        {sections.map((s) => (
          <section key={s.key} className="flex flex-col gap-1 px-1">
            <div className="flex items-center gap-2">
              <span className="text-2xs font-medium text-text">{s.title}</span>
              {s.label && s.tone && <Chip tone={s.tone}>{s.label}</Chip>}
            </div>
            {s.sentence && <p className={cn("text-2xs", s.tone === "warn" ? "text-warn" : s.tone === "danger" ? "text-danger" : "text-text-dim")}>{s.sentence}</p>}
            {s.rows.length > 0 && (
              <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 rounded-control border border-border bg-surface-2 p-2 text-2xs">
                {s.rows.map((row, i) => (
                  <div key={`${row.label}-${i}`} className="contents">
                    <dt className="text-text-dim">{row.label}</dt>
                    <dd className="min-w-0 break-words font-mono">{row.value}</dd>
                  </div>
                ))}
              </dl>
            )}
          </section>
        ))}
      </div>
      <p className="px-1 text-3xs text-text-dim">{footnote(networkPoll(), readAt)}</p>
    </div>
  );
}
