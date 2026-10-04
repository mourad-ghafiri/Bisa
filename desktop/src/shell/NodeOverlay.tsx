/**
 * The overlay under the footer's node read-out: the connection (the address,
 * who runs the node, how long up, restarts), the node (its version beside
 * the desktop's, paused or running, the sessions live, the socket, where it
 * listens, the data and logs folders), its sync and its own load — with one
 * door to Settings › Node. Read on open, never at rest: the node's own
 * description, the paused flag and the shell's status are fetched here; the
 * workspace and the load come from the stores already running. Every fact is
 * `nodeStatModel.mjs`'s; this file paints.
 */

import { useState } from "react";
import { api, apiBaseSync } from "../api";
import type { ConnState } from "../bus";
import type { RelayCheck } from "../types";
import { useEngineEvents } from "../bus";
import { navigate } from "../router";
import { Button, Chip, cn } from "../ui";
import { wireMoved } from "../views/_settings/relayHealthModel.mjs";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { useAsync } from "../views/_work/useAsync";
import { useClock } from "./clock";
import { nodeStatus } from "./nodeApi";
import { footnote, nodeShare, overlaySections } from "./nodeStatModel.mjs";
import { useProcessShares } from "./statsStore";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

export function NodeOverlay({ conn, close }: { conn: ConnState; close: () => void }) {
  const read = useAsync(
    async (s) => {
      const [info, paused, status, sync] = await Promise.all([
        api.nodeInfo(s).catch(() => null),
        api.paused(s).catch(() => null),
        nodeStatus().catch(() => null),
        api.sync(s).catch(() => null),
      ]);
      return { info, paused: paused ? paused.paused : null, status, sync };
    },
    [],
  );
  useEngineEvents((e) => {
    // The wire's rows follow the relays and the switch (`wireMoved`), as the panel's do.
    if (e.payload.type === "paused" || e.payload.type === "resumed" || wireMoved(e.payload)) read.reload();
  });
  const ws = useWorkspace();
  const { processes } = useProcessShares();
  // The last round of relay checks, kept while the panel is open: each
  // configured relay tried once on a throwaway connection, on or off.
  const [checks, setChecks] = useState<RelayCheck[] | null>(null);
  const [checking, setChecking] = useState(false);
  const relayUrls = read.data?.sync?.relays.map((r) => r.url) ?? [];
  const checkRelays = async () => {
    setChecking(true);
    const results = await Promise.all(relayUrls.map((url) => api.checkRelay(url).catch(() => ({ url, ok: false, error: t("shell-node-overlay-node-did-answer") }) as RelayCheck)));
    setChecks(results);
    setChecking(false);
  };
  // A second's tick, so *Up* and the footnote move while the panel is open.
  useClock(1000);
  const now = Date.now() / 1000;
  const sections = overlaySections(conn, {
    info: read.data?.info ?? null,
    status: read.data?.status ?? null,
    workspace: ws.info,
    sync: read.data?.sync ?? null,
    checks,
    paused: read.data?.paused ?? null,
    share: nodeShare(processes),
    appVersion: __APP_VERSION__,
    apiBase: apiBaseSync(),
    now,
  });
  const open = () => {
    navigate({ name: "settings" }, settingsSearch("node"));
    close();
  };
  const openRelays = () => {
    navigate({ name: "settings" }, settingsSearch("sync"));
    close();
  };
  return (
    <div className="flex min-w-0 flex-col gap-2" role="group" aria-label={t("shell-node-overlay-node")}>
      <div className="flex items-center gap-2 px-1">
        <p className="text-xs font-medium text-text">{t("shell-node-overlay-node")}</p>
        <span className="flex-1" />
        {relayUrls.length > 0 && (
          <Button size="sm" variant="ghost" disabled={checking} onClick={() => void checkRelays()} title={t("shell-node-overlay-try-every-configured-relay-once-whether")}>
            {checking ? t("shell-node-overlay-checking") : t("shell-node-overlay-check-relays")}
          </Button>
        )}
        <Button size="sm" variant="ghost" onClick={openRelays}>{t("shell-node-overlay-relays-sync")}</Button>
        <Button size="sm" variant="ghost" onClick={open}>{settingsPath("node")}</Button>
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
              <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 rounded-control bg-surface-2/50 p-2 text-2xs">
                {s.rows.map((row, i) => (
                  <div key={`${row.label}-${i}`} className="[display:contents]">
                    <dt className="text-text-dim">{row.label}</dt>
                    <dd className="min-w-0 break-words font-mono">{row.value}</dd>
                  </div>
                ))}
              </dl>
            )}
          </section>
        ))}
      </div>
      <p className="px-1 text-2xs text-text-dim">{footnote(read.at ?? null, now)}</p>
    </div>
  );
}
