/**
 * Settings › Relays & sync (14-collaboration): the switch that turns the
 * wire on — off by default, nothing contacted until it is — the relays this
 * node talks through, each with its health as the pool measures it, a
 * check on any relay whether or not the wire is on, a reconnect, the
 * wire's counts, and the `sync.*` settings beside them.
 *
 * Relays are transport, never authority: they see ciphertext and a
 * recipient. The list is the `sync.relays` setting at the machine scope —
 * four public relays by default, read from the registry so this panel never
 * restates them — written through the settings route like every other
 * setting, so a running node hears it and connects at once. Every word is
 * `relayHealthModel.mjs`'s.
 */

import { useMemo, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { useSettingsRegistry } from "../../shell/settingsStore";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import type { RelayCheck, SyncReport } from "../../types";
import { Button, Card, Chip, ConfirmDialog, CopyText, Dot, EmptyState, ErrorNote, Field, ICON, Pending, RelativeTime, Section, Switch, TextInput, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase } from "./loadModel.mjs";
import { checkAbout, checkAllWords, checkWords, defaultRelays, isDefaultList, relayEntry, relayProblem, relayTone, relayWords, syncSummary, wireMoved } from "./relayHealthModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const RELAYS_KEY = "sync.relays";
const ENABLED_KEY = "sync.enabled";

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-control border border-border px-2 py-1.5">
      <div className="tnum text-sm">{value}</div>
      <div className="text-2xs text-text-dim">{label}</div>
    </div>
  );
}

export function RelaysPanel() {
  const toast = useToast();
  const { resolved, reload: reloadSettings } = useResolvedSettings(null);
  const registry = useSettingsRegistry();
  const sync = useAsync((s) => api.sync(s), []);
  const [url, setUrl] = useState("");
  const [check, setCheck] = useState<RelayCheck | null>(null);
  const [checking, setChecking] = useState(false);
  /** The last answer per configured relay — *Check* on a row, or *Check all*. */
  const [rowChecks, setRowChecks] = useState<Record<string, RelayCheck>>({});
  const [checkingRows, setCheckingRows] = useState<Set<string>>(new Set());
  const [removing, setRemoving] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEngineEvents((e) => {
    if (wireMoved(e.payload)) {
      sync.reload();
      reloadSettings();
    }
  });

  const relays: string[] = (() => {
    const v = (resolved ?? []).find((r) => r.key === RELAYS_KEY)?.value;
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  })();
  const enabled = (resolved ?? []).find((r) => r.key === ENABLED_KEY)?.value === true;
  const defaults = useMemo(() => defaultRelays(registry.data?.settings ?? null), [registry.data]);
  const entry = relayEntry(url, relays);
  const addUrl = entry.ok ? entry.url : null;
  const report: SyncReport | null = sync.data ?? null;
  const summary = syncSummary(report);
  const healthOf = (u: string) => report?.relays.find((r) => r.url === u) ?? null;

  const write = async (next: string[], done: string) => {
    setBusy(true);
    await attempt(() => api.setSettings("machine", { [RELAYS_KEY]: next }), toast.error, () => {
      toast.ok(done);
      setUrl("");
      setCheck(null);
      reloadSettings();
      sync.reload();
    });
    setBusy(false);
  };

  const tryIt = async () => {
    if (!addUrl) return;
    setChecking(true);
    await attempt(() => api.checkRelay(addUrl), toast.error, setCheck);
    setChecking(false);
  };

  /** One relay tried once, on a throwaway connection — on or off. */
  const checkRow = async (u: string) => {
    setCheckingRows((s) => new Set(s).add(u));
    await attempt(() => api.checkRelay(u), toast.error, (r) => setRowChecks((m) => ({ ...m, [u]: r })));
    setCheckingRows((s) => {
      const next = new Set(s);
      next.delete(u);
      return next;
    });
  };
  const checkAll = async () => {
    await Promise.all(relays.map((u) => checkRow(u)));
  };
  const setEnabled = async (on: boolean) => {
    setBusy(true);
    await attempt(() => api.setSettings("machine", { [ENABLED_KEY]: on }), toast.error, () => {
      toast.ok(on ? t("settings-relays-panel-relays-3") : t("settings-relays-panel-relays-off"));
      reloadSettings();
      sync.reload();
    });
    setBusy(false);
  };

  const checked = checkWords(checkAbout(check, addUrl));
  const allChecked = checkAllWords(relays.map((u) => rowChecks[u]).filter((c): c is RelayCheck => !!c));

  return (
    <div className="flex max-w-2xl flex-col gap-4">
      <Card>
        <Switch
          checked={enabled}
          disabled={busy || !resolved}
          onChange={(on) => void setEnabled(on)}
          label={t("settings-relays-panel-sync-over-relays")}
          hint={
            enabled
              ? t("settings-relays-panel-node-talks-relays-below-hosts-people")
              : t("settings-relays-panel-off-default-nothing-contacted-until-turn")
          }
        />
      </Card>

      <Section
        title={t("settings-relays-panel-relays", { relays: relays.length })}
        action={
          relays.length > 0 ? (
            <div className="flex items-center gap-1">
              {!isDefaultList(relays, defaults) && defaults.length > 0 && (
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => void write(defaults, t("settings-relays-panel-default-relays-back"))}>{t("settings-relays-panel-reset-defaults")}</Button>
              )}
              <Button size="sm" disabled={checkingRows.size > 0} onClick={() => void checkAll()}>
                {checkingRows.size > 0 ? t("settings-mcp-panel-checking-2") : t("settings-mcp-panel-check-all")}
              </Button>
            </div>
          ) : undefined
        }
      >
        {allChecked && <p className={`mb-1.5 text-2xs ${allChecked.tone === "ok" ? "text-ok" : allChecked.tone === "danger" ? "text-danger" : "text-warn"}`}>{allChecked.text}</p>}
        {relays.length === 0 ? (
          <EmptyState icon={ICON.sync} title={t("settings-relays-panel-relays-2")} hint={t("settings-relays-panel-alone-works-relay-all-add-one")} action={null} />
        ) : (
          <ul className="flex flex-col gap-1.5">
            {relays.map((r) => {
              const h = healthOf(r);
              const tone = relayTone(h);
              const last = checkWords(rowChecks[r] ?? null);
              const state = !report ? t("settings-relays-panel-read-yet") : `${tone.word}${relayWords(h) ? ` · ${relayWords(h)}` : ""}`;
              return (
                <li key={r} className="flex items-center gap-2 rounded-control border border-border px-2 py-1.5">
                  <Dot tone={last ? (last.tone === "ok" ? "ok" : "danger") : tone.tone === "danger" ? "danger" : tone.tone === "warn" ? "warn" : tone.tone === "ok" ? "ok" : "neutral"} title={last ? last.text : tone.word} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-mono text-2xs">{r}</span>
                    <span className={`block truncate text-2xs ${last ? (last.tone === "ok" ? "text-ok" : "text-danger") : "text-text-dim"}`}>{last ? `${last.text} · ${state}` : state}</span>
                    {/* Why it is not connected — whole, never truncated: the one line a person acts on. */}
                    {relayProblem(h) && <span className="block text-2xs text-danger">{relayProblem(h)}</span>}
                  </span>
                  <Button size="sm" disabled={checkingRows.has(r)} onClick={() => void checkRow(r)}>
                    {checkingRows.has(r) ? t("settings-mcp-panel-checking-2") : t("settings-mcp-panel-check")}
                  </Button>
                  <Button size="sm" variant="danger" disabled={busy} onClick={() => setRemoving(r)}>{t("settings-git-profiles-panel-remove-2")}</Button>
                </li>
              );
            })}
          </ul>
        )}

        <Card className="mt-2">
          <div className="flex flex-col gap-2">
            <Field label={t("settings-relays-panel-add-relay")} /* for the machine */ hint={entry.ok ? t("settings-relays-panel-any-public-nostr-relay-works-only") : entry.reason || "wss://relay.example.com"}>
              <TextInput
                value={url}
                spellCheck={false}
                placeholder="wss://relay.example.com" // for the machine
                onChange={(e) => {
                  setUrl(e.target.value);
                  setCheck(null);
                }}
              />
            </Field>
            {checked && <p className={`text-2xs ${checked.tone === "ok" ? "text-ok" : "text-danger"}`}>{checked.text}</p>}
            <div className="flex gap-2">
              <Button disabled={!entry.ok || checking} onClick={() => void tryIt()}>
                {checking ? t("settings-mcp-panel-checking-2") : t("settings-mcp-panel-check")}
              </Button>
              <Button variant="primary" disabled={!addUrl || busy} onClick={() => addUrl && void write([...relays, addUrl], t("settings-relays-panel-relay-added"))}>{t("settings-relays-panel-add")}</Button>
            </div>
          </div>
        </Card>
      </Section>

      <Section
        title={t("settings-relays-panel-wire")}
        action={
          <Button size="sm" disabled={!report?.running || !report.enabled} onClick={() => void attempt(() => api.reconnectRelays(), toast.error, () => sync.reload())}>
            <ICON.refresh size={12} aria-hidden />{t("settings-relays-panel-reconnect")}</Button>
        }
      >
        {phase(sync) === "failed" && <ErrorNote error={sync.error ?? t("settings-relays-panel-wire-read-refused")} retry={sync.reload} />}
        {phase(sync) === "pending" && <Pending what={t("settings-relays-panel-wire-2")} rows={pendingRows(t("settings-relays-panel-wire-2"))} />}
        {phase(sync) === "ready" && report && (
          <Card>
            <div className="mb-2 flex items-center gap-2">
              <Chip tone={summary.tone === "ok" ? "ok" : summary.tone === "danger" ? "danger" : "quiet"}>{!summary.running ? t("settings-relays-panel-pump") : summary.enabled ? t("settings-relays-panel-on") : t("settings-relays-panel-off")}</Chip>
              <span className="text-2xs text-text-dim">{summary.line}</span>
            </div>
            <div className="grid grid-cols-4 gap-2">
              <Stat label={t("settings-relays-panel-published")} value={report.published} />
              <Stat label={t("settings-relays-panel-ingested")} value={report.ingested} />
              <Stat label={t("settings-relays-panel-people-hosted-here")} value={report.people} />
              <Stat label={t("settings-relays-panel-workspaces-joined")} value={report.hosts} />
            </div>
            <div className="mt-2 flex flex-wrap items-center gap-1.5 text-2xs text-text-dim">
              {report.last_catchup != null && (
                <span>{rich("settings-relays-panel-last-catch-up", { when: <RelativeTime at={report.last_catchup} /> })}</span>
              )}
            </div>
            {report.iroh_node_id && (
              <div className="mt-2 border-t border-border pt-2">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-2xs font-medium text-text-dim">{t("settings-relays-panel-direct-transport")}</span>
                  <Chip tone="quiet">{t("settings-relays-panel-sessions", { iroh_peers_connected: report.iroh_peers_connected })}</Chip>
                </div>
                <CopyText value={report.iroh_node_id} label={`${report.iroh_node_id.slice(0, 16)}…`} />
                <p className="mt-1 text-2xs text-text-dim">{t("settings-relays-panel-node-s-endpoint-id-peer-entered")}</p>
              </div>
            )}
          </Card>
        )}
      </Section>

      <RegistryPanel group="sync" omit={[RELAYS_KEY, ENABLED_KEY]} intro={t("settings-relays-panel-how-node-talks-how-often-catches")} />

      <ConfirmDialog
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={t("settings-relays-panel-remove-relay")}
        danger
        confirmLabel={t("settings-git-profiles-panel-remove-2")}
        body={t("settings-relays-panel-node-stops-publishing-reading-from-once")}
        onConfirm={() => {
          if (!removing) return;
          const target = removing;
          void write(relays.filter((r) => r !== target), t("settings-relays-panel-relay-removed"));
        }}
      />
    </div>
  );
}
