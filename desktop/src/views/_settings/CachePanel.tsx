/**
 * The cache panel: the tunable `cache.*` settings, the live hit
 * rates of every in-process cache, and a button to clear them all.
 *
 * The knobs render through the generic `RegistryPanel` — adding a `cache.*` key
 * in `bisa-core` grows a control here with no change to this file. The
 * stats table above them polls `GET /cache/stats` on the shared, visibility-gated
 * clock, so a hidden window costs nothing.
 */

import { useEffect } from "react";
import { api } from "../../api";
import { Button, Card, ErrorNote, Pending, ReadLine, Section, useToast } from "../../ui";
import { useClock } from "../../shell/clock";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase, readWords } from "./loadModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { t } from "../../i18n/l10n.mjs";
import { percent } from "../../i18n/format.mjs";

/** How often the stats table refreshes while the window is visible. */
const STATS_TICK_MS = 2_000;

function pct(rate: number): string {
  return percent(rate);
}

export function CachePanel() {
  const toast = useToast();
  const stats = useAsync((s) => api.cacheStats(s), []);
  // Refresh on the shared clock — paused when the window is hidden. The
  // reload is a stable door (`useAsync`); the tick is what changes.
  const tick = useClock(STATS_TICK_MS);
  const reloadStats = stats.reload;
  useEffect(() => {
    reloadStats();
  }, [tick, reloadStats]);

  const clear = async () => {
    await attempt(async () => {
      await api.clearCache();
      toast.ok(t("settings-cache-panel-caches-cleared"));
      stats.reload();
    }, toast.error);
  };

  const rows = stats.data ?? [];
  const state = phase(stats);
  const status = readWords({ what: t("settings-cache-panel-caches"), refreshing: stats.refreshing, error: stats.error, at: stats.at, data: stats.data }, Date.now() / 1000);

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-cache-panel-live-caches")}>
        <Card>
          <div className="mb-2 flex items-center gap-2">
            <span className="text-2xs text-text-dim">{t("settings-cache-panel-hits-served-without-recomputing-since-node")}</span>
            <span className="ml-auto">
              <ReadLine words={status} busy={stats.loading || stats.refreshing} onReload={() => stats.reload()} reloadLabel={t("settings-catalog-panel-refresh")} />
            </span>
            <Button size="sm" onClick={() => void clear()}>{t("settings-cache-panel-clear-caches")}</Button>
          </div>
          {/* *No caches yet* is an answer, said only once the node has given it. */}
          {state === "pending" && <Pending what={t("settings-cache-panel-caches")} rows={pendingRows(t("settings-cache-panel-caches"))} />}
          {state === "failed" && <ErrorNote error={stats.error ?? t("settings-cache-panel-stats-read-refused")} retry={stats.reload} />}
          {state === "ready" && rows.length === 0 ? (
            <p className="text-2xs text-text-dim">{t("settings-cache-panel-caches-have-been-touched-yet")}</p>
          ) : state === "ready" ? (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-2xs tabular-nums">
                <thead className="text-text-dim">
                  <tr>
                    <th className="py-1 pr-3 font-medium">{t("settings-cache-panel-cache")}</th>
                    <th className="py-1 pr-3 font-medium">{t("settings-cache-panel-hit-rate")}</th>
                    <th className="py-1 pr-3 font-medium">{t("settings-cache-panel-hits")}</th>
                    <th className="py-1 pr-3 font-medium">{t("settings-cache-panel-misses")}</th>
                    <th className="py-1 font-medium">{t("settings-cache-panel-entries")}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr key={r.name} className="border-t border-hairline">
                      <td className="py-1 pr-3 font-mono">{r.name}</td>
                      <td className="py-1 pr-3">{pct(r.hit_rate)}</td>
                      <td className="py-1 pr-3">{r.hits}</td>
                      <td className="py-1 pr-3">{r.misses}</td>
                      <td className="py-1">{r.entries}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </Card>
      </Section>

      <Section title={t("settings-cache-panel-tuning")}>
        <RegistryPanel group="cache" />
      </Section>
    </div>
  );
}
