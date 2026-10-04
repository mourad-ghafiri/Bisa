/**
 * Settings › Capabilities › Network (ide/13, ide/01): what this Mac's
 * network is, and how the platform reaches the internet.
 *
 * Two halves over two authorities. **This Mac** is the shell's: the internet
 * — reached or down, the probe's latency, the public IP the echo service
 * answers — every interface that is up, the VPN — up or not, its tunnel,
 * the route, its resolvers — the machine's own resolver and default route,
 * and the proxy System Settings names, read through the OS's own readers
 * (`src-tauri/src/network.rs`) and never written, through the footer's one
 * store (`shell/networkStore.ts`) on its one cadence; the two doors are
 * *Public IP service* — the registry's own row — and *Use in Bisa*, which
 * copies the Mac's proxy into the platform's own settings. **How the
 * platform reaches the internet** is the node's: the `network.*` keys — the
 * mode as the three-way switch, the manual URLs and the bypass as the
 * registry's own controls, HTTP/1.1 as a registry row — with what is in
 * force now from `GET /network` and one check the person asks for. The
 * words are `networkModel.mjs`'s.
 */

import { useMemo, useState } from "react";
import { api, inDesktopShell } from "../../api";
import { useEngineEvents } from "../../bus";
import { refreshNetwork, useNetwork } from "../../shell/networkStore";
import { useResolvedSettingsRead, useSettingsRegistry } from "../../shell/settingsStore";
import type { NetworkCheck, ProxyMode, SettingDef } from "../../types";
import { Button, Card, Chip, Dot, ErrorNote, Field, Pending, ReadLine, Section, SegmentedControl, TextInput, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { firstFailure, pendingRows, phaseOf, readWords } from "./loadModel.mjs";
import {
  CHECK_URL_DEFAULT,
  KEYS,
  checkWords,
  dnsWords,
  inForceWords,
  interfaceRows,
  internetRows,
  internetWords,
  macProxyWords,
  manualFrom,
  modeSegments,
  modeWords,
  routeWords,
  tunnelRows,
  unavailableWords,
  vpnWords,
} from "./networkModel.mjs";
import { RegistryPanel } from "./RegistryPanel";
import { SavedNote, useSavedNote } from "./SavedNote";
import { SettingControl } from "./SettingControl";
import { t as tr } from "../../i18n/l10n.mjs";
import { OriginBadge } from "./OriginBadge";

export function NetworkPanel() {
  return (
    <div className="flex flex-col gap-6">
      <Section title={tr("settings-network-panel-mac")}>
        <ThisMac />
      </Section>
      <Section title={tr("settings-network-panel-how-platform-reaches-internet")}>
        <div className="flex flex-col gap-3">
          <ProxyCard />
          <RegistryPanel group="network" only={[KEYS.http1Only]} />
        </div>
      </Section>
    </div>
  );
}

/** Rows as a definition list — the shape every card here draws its facts in. */
function Rows({ rows }: { rows: readonly { label: string; value: string }[] }) {
  if (rows.length === 0) return null;
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 rounded-control bg-surface-2/50 p-2 text-2xs">
      {rows.map((row, i) => (
        <div key={`${row.label}-${i}`} className="[display:contents]">
          <dt className="text-text-dim">{row.label}</dt>
          <dd className="min-w-0 break-words font-mono">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ThisMac() {
  const desktop = inDesktopShell();
  // The footer's store: one read of the Mac, one cadence, and the window's
  // own word; *Check again* is its `refreshNetwork`.
  const { facts, error, online, readAt } = useNetwork();
  const [busy, setBusy] = useState(false);
  const again = () => {
    setBusy(true);
    void refreshNetwork().finally(() => setBusy(false));
  };

  if (!desktop) return <p className="text-2xs text-text-dim">{unavailableWords("not_desktop")}</p>;
  // The read answered `null` on a shell with no readers: said, not spun on.
  if (facts === null && !error && readAt != null) {
    return <p className="text-2xs text-text-dim">{unavailableWords("no_reader")}</p>;
  }
  const loading = facts === null && readAt == null && !error;
  const line = readWords({ what: tr("settings-network-panel-mac-s-network"), refreshing: busy, error, at: readAt, data: facts, loading }, Date.now() / 1000);
  const net = internetWords(facts, online);
  const vpn = vpnWords(facts);
  const proxy = facts ? macProxyWords(facts.proxy) : null;
  const interfaces = facts ? interfaceRows(facts.interfaces) : [];

  return (
    <div className="flex flex-col gap-3">
      <Card className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <Dot tone={net.tone} title={net.label} />
          <span className="text-xs font-medium">{tr("settings-network-panel-internet")}</span>
          {/* The sentence is the line under the row, not a tooltip a keyboard never reaches. */}
          <Chip tone={net.tone}>{net.label}</Chip>
          <span className="flex-1" />
          <ReadLine words={line} busy={busy} onReload={again} reloadLabel={tr("settings-code-host-panel-check-again")} />
        </div>
        {loading && <Pending what={tr("settings-network-panel-mac-s-network")} rows={pendingRows(tr("settings-network-panel-mac-s-network"))} />}
        {!facts && error && <ErrorNote error={error} retry={again} />}
        {facts && (
          <>
            <p className={net.tone === "danger" ? "max-w-measure text-2xs leading-relaxed text-danger" : "max-w-measure text-2xs leading-relaxed text-text-dim"}>{net.sentence}</p>
            {net.kind === "up" && <Rows rows={internetRows(facts)} />}
          </>
        )}
      </Card>
      <RegistryPanel group="network" only={[KEYS.publicIpUrl]} />
      {facts && (
        <Card className="flex flex-col gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <Dot tone={vpn.tone} title={vpn.label} />
            <span className="text-xs font-medium">{tr("settings-network-panel-vpn")}</span>
            <Chip tone={vpn.tone}>{vpn.label}</Chip>
          </div>
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{vpn.sentence}</p>
          {facts.vpn.tunnels
            .filter((t) => t.up)
            .map((t) => (
              <Rows key={t.interface} rows={tunnelRows(t, facts.internet.public_ip ?? null)} />
            ))}
        </Card>
      )}
      {facts && (
        <Card className="flex flex-col gap-2">
          <span className="text-xs font-medium">{tr("settings-network-panel-interfaces")}</span>
          {interfaces.length > 0 ? <Rows rows={interfaces} /> : <p className="text-2xs text-text-dim">{tr("settings-network-panel-interface-up")}</p>}
          <p className="text-2xs leading-relaxed text-text-dim">
            <span className="font-medium text-text">{tr("settings-network-panel-default-route")}</span> {routeWords(facts)}{" "}
            <span className="font-medium text-text">{tr("settings-network-panel-dns")}</span> {dnsWords(facts)}
          </p>
        </Card>
      )}
      {facts && proxy && <MacProxyCard sentence={proxy.sentence} values={proxy.followable ? manualFrom(facts.proxy) : null} />}
    </div>
  );
}

/** The proxy System Settings names, and the door that copies it. */
function MacProxyCard({ sentence, values }: { sentence: string; values: Record<string, string> | null }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const use = async () => {
    if (!values) return;
    setBusy(true);
    await attempt(() => api.setSettings("machine", values), toast.error, () => toast.ok(tr("settings-network-panel-platform-now-follows-mac-s-proxy")));
    setBusy(false);
  };
  return (
    <Card className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium">{tr("settings-network-panel-proxy-system-settings")}</span>
        <span className="flex-1" />
        {values && (
          <Button size="sm" disabled={busy} onClick={() => void use()}>
            {busy ? tr("settings-connectors-panel-saving") : tr("settings-network-panel-use-bisa")}
          </Button>
        )}
      </div>
      <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{sentence}</p>
      {values && <p className="text-2xs text-text-dim">{tr("settings-network-panel-copies-into-platform-s-own-settings")}</p>}
    </Card>
  );
}

/** The proxy the platform follows: the mode, the manual fields, what is in force, one check. */
function ProxyCard() {
  const toast = useToast();
  const registry = useSettingsRegistry();
  const resolved = useResolvedSettingsRead(null);
  const status = useAsync((s) => api.network(s), []);
  // The settings store re-reads the values itself; what is in force is this
  // card's own read, asked again when a network key moves.
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("network."))) status.reload();
  });
  const defs = useMemo(() => new Map((registry.data?.settings ?? []).map((d) => [d.key, d])), [registry.data]);
  const values = useMemo(() => new Map((resolved.data?.settings ?? []).map((r) => [r.key, r])), [resolved.data]);
  const [savedAt, markSaved] = useSavedNote();

  // Never disabled under its own write: a box that greys out between keystrokes loses the caret.
  const write = async (patch: Record<string, unknown>) => {
    await attempt(() => api.setSettings("machine", patch), toast.error, () => {
      markSaved();
      resolved.reload();
      status.reload();
    });
  };

  // Both reads gate the switch: a mode drawn before the values land would be a lie for a beat.
  const state = phaseOf([registry, resolved]);
  if (state === "failed") {
    return (
      <ErrorNote
        error={firstFailure([registry, resolved]) ?? tr("settings-keymap-panel-settings-read-refused")}
        retry={() => {
          registry.reload();
          resolved.reload();
        }}
      />
    );
  }
  if (state === "pending") return <Pending what={tr("settings-network-panel-proxy-2")} rows={pendingRows(tr("settings-network-panel-proxy-2"))} />;

  const mode = (values.get(KEYS.mode)?.value as ProxyMode | undefined) ?? "environment";
  const origin = values.get(KEYS.mode)?.origin ?? "default";
  const field = (key: string): SettingDef | undefined => defs.get(key);
  const inForce = status.data ? inForceWords(status.data) : null;
  const statusLine = readWords({ what: tr("settings-network-panel-what-force"), refreshing: status.refreshing, error: status.error, at: status.at, data: status.data, loading: status.loading }, Date.now() / 1000);

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <span className="mb-1 block text-2xs font-medium text-text-dim">{tr("settings-network-panel-proxy")}</span>
          <SegmentedControl options={modeSegments()} value={mode} onChange={(v) => void write({ [KEYS.mode]: v })} label={tr("settings-network-panel-proxy-mode")} size="sm" />
          <span className="mt-1 block text-2xs text-text-dim">{modeWords(mode)}</span>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1.5">
          <div className="flex items-center gap-1.5">
            <SavedNote at={savedAt} />
            <OriginBadge origin={origin} />
          </div>
          <span className="text-2xs text-text-dim">{tr("settings-network-panel-set-machine")}</span>
        </div>
      </div>

      {mode === "manual" &&
        [KEYS.https, KEYS.http, KEYS.noProxy].map((key) => {
          const def = field(key);
          if (!def) return null;
          return (
            <Field key={key} label={def.label} hint={def.help}>
              <SettingControl def={def} value={values.get(key)?.value ?? ""} onChange={(v) => void write({ [key]: v })} disabled={false} />
            </Field>
          );
        })}

      <div className="flex flex-col gap-1 rounded-control bg-surface-2/50 p-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-2xs font-medium">{tr("settings-network-panel-force")}</span>
          {inForce && <Chip tone={inForce.tone}>{inForce.label}</Chip>}
          <span className="flex-1" />
          <ReadLine words={statusLine} busy={status.loading || status.refreshing} onReload={status.reload} reloadLabel={tr("settings-connectors-panel-read-again")} />
        </div>
        {!status.data && !status.error && <Pending what={tr("settings-network-panel-proxy-2")} rows={1} />}
        {!status.data && status.error && <ErrorNote error={status.error} retry={status.reload} />}
        {inForce && <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{inForce.sentence}</p>}
        {status.data?.problems.map((p) => (
          <p key={p} className="text-2xs text-warn" role="alert">
            {p}
          </p>
        ))}
      </div>

      <CheckRow />
    </Card>
  );
}

/** One request the person asks for, to see whether the way out works. */
function CheckRow() {
  const toast = useToast();
  const [url, setUrl] = useState(CHECK_URL_DEFAULT);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<NetworkCheck | null>(null);
  const words = checkWords(result);
  const check = async () => {
    setBusy(true);
    setResult(null);
    await attempt(() => api.networkCheck(url), toast.error, setResult);
    setBusy(false);
  };
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-2">
        <TextInput value={url} className="min-w-0 flex-1" aria-label={tr("settings-network-panel-url-check")} disabled={busy} onChange={(e) => setUrl(e.target.value)} />
        <Button size="sm" disabled={busy || url.trim().length === 0} onClick={() => void check()}>
          {busy ? tr("settings-mcp-panel-checking-2") : tr("settings-mcp-panel-check")}
        </Button>
        {words && <Chip tone={words.tone}>{words.label}</Chip>}
      </div>
      <p className="text-2xs text-text-dim">{words ? words.sentence : tr("settings-network-panel-one-request-through-platform-s-own")}</p>
    </div>
  );
}
