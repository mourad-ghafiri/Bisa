/**
 * Settings › Capabilities › Mobile Development (ide/19): the switch, which
 * platforms this machine develops for, the setup — one row per component
 * with what was found and the official way to install what was not, and
 * Flutter's own doctor lines —, the devices with their verbs and a door to
 * make a simulator, and which agents may drive the devices. The two paths
 * (`mobile_development.flutter.path`, `mobile_development.android.sdk`) are the registry panel's
 * under this one.
 *
 * `mobile_development.enabled` and `mobile_development.platforms` are written at the machine scope
 * — facts about this Mac —, the policy at the workspace's, as the origin
 * badge says. The setup is read whether or not the switch is on: what to
 * install comes before turning it on.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { MobileDevelopmentStatus } from "../../types";
import { refreshDevices, useDevices } from "../../shell/devicesStore";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { boolOf, choiceOf } from "../../shell/settingsModel.mjs";
import { Button, Card, Chip, CopyText, Dialog, ErrorNote, ExternalLink, Field, ICON, Pending, ReadLine, SegmentedControl, Section, Select, Switch, TextInput, Tooltip, failureText, isMac, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, readWords } from "./loadModel.mjs";
import {
  AGENTS_KEY,
  DEFAULT_PLATFORMS,
  DEFAULT_POLICY,
  ENABLED_KEY,
  PLATFORMS,
  PLATFORMS_KEY,
  POLICIES,
  checkedWords,
  componentRows,
  deviceRows,
  doctorRows,
  platformSegments,
  platformShown,
  platformWords,
  policySegments,
  policyWords,
  simulatorProblem,
  statusWords,
} from "./mobileDevelopmentSettingsModel.mjs";
import type { MobileDevelopmentPolicy, Platforms } from "./mobileDevelopmentSettingsModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";
import { OriginBadge } from "./OriginBadge";

const TONE = { ok: "ok", warn: "warn", quiet: "quiet", neutral: "neutral" } as const;

export function MobileDevelopmentPanel() {
  const toast = useToast();
  const { resolved, error, reload } = useResolvedSettings(null);
  const [busy, setBusy] = useState(false);
  const enabled = boolOf(resolved, ENABLED_KEY, false);
  const platforms = platformShown(choiceOf(resolved, PLATFORMS_KEY, PLATFORMS, DEFAULT_PLATFORMS), isMac);
  const policy = choiceOf(resolved, AGENTS_KEY, POLICIES, DEFAULT_POLICY);
  const origin = resolved?.find((s) => s.key === AGENTS_KEY)?.origin ?? "default";

  // The setup: the node's last look, read again on the window's focus —
  // a person installs a tool and comes back — and on *Check again*.
  const status = useAsync<MobileDevelopmentStatus>((s) => api.mobileDevelopmentStatus(s), []);
  useEffect(() => {
    const again = () => status.reload();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [status]);
  const [checking, setChecking] = useState(false);
  const check = () => {
    if (checking) return;
    setChecking(true);
    void attempt(
      () => api.mobileDevelopmentCheck(),
      (why) => {
        setChecking(false);
        toast.error(why);
      },
      () => {
        setChecking(false);
        status.reload();
      },
    );
  };
  const words = readWords({ what: tr("settings-mobile-development-panel-setup-2"), refreshing: status.refreshing || checking, error: status.error, at: status.at, data: status.data }, Date.now() / 1000);
  const toolchain = status.data?.toolchain ?? null;
  const flutter = toolchain ? toolchain.flutter.installed : null;
  const rows = componentRows(toolchain, { platforms, mac: isMac });
  const doctor = doctorRows(toolchain?.doctor);
  const card = statusWords({ enabled, platforms, flutter });

  const write = async (scope: "machine" | "workspace", key: string, value: unknown) => {
    if (busy) return;
    setBusy(true);
    try {
      await api.setSettings(scope, { [key]: value });
      reload();
    } catch (e) {
      toast.error(failureText("settings", "mobile-development-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };
  const reset = async () => {
    setBusy(true);
    try {
      await api.unsetSetting("workspace", AGENTS_KEY);
      reload();
    } catch (e) {
      toast.error(failureText("settings", "mobile-development-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };
  const platformOptions = platformSegments(isMac).map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon as keyof typeof ICON], hint: s.hint, disabled: s.disabled }));
  const policyOptions = policySegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon as keyof typeof ICON] }));

  return (
    <div className="flex flex-col gap-6">
      <Section title={tr("settings-mobile-development-panel-mobile-development")}>
        <Card>
          {!resolved && !error && <Pending what={tr("settings-browser-access-panel-resolved-values")} rows={pendingRows(tr("settings-browser-access-panel-resolved-values"))} />}
          {!resolved && error && <ErrorNote error={error} retry={reload} />}
          {resolved && (
            <div className="flex flex-col gap-3">
              <div className="min-w-0">
                <div className="mb-1 flex items-center gap-2">
                  <Chip tone={card.tone}>{card.label}</Chip>
                  <span className="text-2xs text-text-dim">{card.sentence}</span>
                </div>
                <Switch
                  checked={enabled}
                  disabled={busy}
                  onChange={(v) => void write("machine", ENABLED_KEY, v)}
                  label={tr("settings-mobile-development-panel-ai-assisted-mobile-development-machine")}
                  hint={tr("settings-mobile-development-panel-devices-below-listed-flutter-app-runs")}
                />
              </div>
              <div className="min-w-0">
                <span className="mb-1 block text-2xs font-medium text-text-dim">{tr("settings-mobile-development-panel-develops")}</span>
                <SegmentedControl options={platformOptions} value={platforms} onChange={(v: Platforms) => void write("machine", PLATFORMS_KEY, v)} label={tr("settings-mobile-development-panel-which-platforms-machine-develops")} size="sm" />
                <span className="mt-1 block text-2xs text-text-dim">
                  {platformWords(platforms)}
                  {!isMac && ` ${tr("settings-mobile-development-panel-ios-needs-xcode-which-runs-macos")}`}
                </span>
              </div>
            </div>
          )}
        </Card>
      </Section>

      <Section title={tr("settings-mobile-development-panel-setup")} action={<ReadLine words={words} busy={status.loading || status.refreshing || checking} onReload={check} reloadLabel={tr("settings-code-host-panel-check-again")} />}>
        <Card>
          {status.loading && !status.data && <Pending what={tr("settings-mobile-development-panel-setup-2")} rows={4} />}
          {status.error && !status.data && <ErrorNote error={status.error} retry={status.reload} />}
          {status.data && (
            <div className="flex flex-col gap-2 text-2xs">
              <div className="flex items-center justify-between gap-2 text-text-dim">
                <span>{rich("settings-mobile-development-panel-components-blurb")}</span>
                <span className="shrink-0">{checkedWords(status.data.checked_at)}</span>
              </div>
              <ul className="flex flex-col divide-y divide-hairline" aria-label={tr("settings-mobile-development-panel-setup")}>
                {rows.map((r) => (
                  <li key={r.id} className="flex flex-col gap-1 py-2">
                    <div className="flex items-center gap-2">
                      <Chip tone={TONE[r.tone]}>{r.tone === "ok" ? tr("settings-mobile-development-panel-found") : r.tone === "warn" ? tr("settings-mobile-development-panel-missing") : tr("settings-mobile-development-panel-held")}</Chip>
                      <span className="font-medium text-text">{r.name}</span>
                      <span className="min-w-0 truncate text-text-dim" title={r.found}>
                        {r.found}
                      </span>
                    </div>
                    {r.install && (
                      <div className="flex flex-wrap items-center gap-2 pl-1 text-text-dim">
                        <span>{r.install.text}</span>
                        {r.install.command && <CopyText value={r.install.command} label={tr("settings-mobile-development-panel-copy-command")} />}
                        <ExternalLink href={r.install.url}>{tr("settings-mobile-development-panel-official-guide")}</ExternalLink>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
              {doctor.length > 0 && (
                <div>
                  <span className="mb-1 block font-semibold text-text-dim">{tr("settings-mobile-development-panel-flutter-s-own-doctor")}</span>
                  <ul className="flex flex-col gap-0.5" aria-label={tr("settings-mobile-development-panel-flutter-doctor")}>
                    {doctor.map((d, i) => (
                      <li key={i} className="flex items-center gap-2">
                        <Chip tone={TONE[d.tone]}>{d.tone === "ok" ? tr("settings-mobile-development-panel-ok") : d.tone === "warn" ? tr("settings-mobile-development-panel-missing") : tr("settings-mobile-development-panel-note")}</Chip>
                        <span className="text-text">{d.name}</span>
                        {d.detail && <span className="min-w-0 truncate text-text-dim">{d.detail}</span>}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}
        </Card>
      </Section>

      {enabled && <DevicesSection platforms={platforms} status={status.data} />}

      <Section title={tr("settings-mobile-development-panel-which-agents-may-use-devices")}>
        <Card>
          {resolved && (
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <span className="mb-1 block text-2xs font-medium text-text-dim">{tr("settings-mobile-development-panel-mobile-tools-answer")}</span>
                <SegmentedControl options={policyOptions} value={policy} onChange={(v: MobileDevelopmentPolicy) => void write("workspace", AGENTS_KEY, v)} label={tr("settings-mobile-development-panel-which-agents-may-use-mobile-tools")} size="sm" />
                <span className="mt-1 block text-2xs text-text-dim">{policyWords(policy)}</span>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <OriginBadge origin={origin} />
                <span className="text-2xs text-text-dim">{tr("settings-browser-access-panel-set-workspace")}</span>
                {origin === "workspace" && (
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => void reset()}>{tr("settings-appearance-panel-reset")}</Button>
                )}
              </div>
            </div>
          )}
        </Card>
      </Section>
    </div>
  );
}

/** The devices this machine can reach, with their verbs, and the door to make a simulator. */
function DevicesSection({ platforms, status }: { platforms: Platforms; status: MobileDevelopmentStatus | null }) {
  const toast = useToast();
  const { devices, read, error } = useDevices();
  useEngineEvents((e) => {
    if (e.payload.type === "mobile_development_changed") void refreshDevices();
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const rows = deviceRows(devices, platforms);
  const canMake = isMac && platforms !== "android";

  const act = async (id: string, verb: "boot" | "shutdown" | "show") => {
    if (busy) return;
    setBusy(id);
    try {
      const after = verb === "boot" ? await api.mobileDevelopmentDeviceBoot(id) : verb === "shutdown" ? await api.mobileDevelopmentDeviceShutdown(id) : await api.mobileDevelopmentDeviceShow(id);
      if (verb === "boot") toast.ok(tr("settings-mobile-development-panel-up", { after: after.name }));
      if (verb === "shutdown") toast.ok(tr("settings-mobile-development-panel-shut-down-2", { after: after.name }));
      await refreshDevices();
    } catch (e) {
      toast.error(failureText("settings", "mobile-development-panel-failed", e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Section
      title={tr("settings-mobile-development-panel-devices")}
      action={
        <div className="flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => void refreshDevices()}>
            <ICON.refresh size={12} aria-hidden />{tr("settings-mobile-development-panel-look-again")}</Button>
          {canMake && (
            <Button size="sm" onClick={() => setMaking(true)}>
              <ICON.simulator size={12} aria-hidden />{tr("settings-mobile-development-panel-create-simulator")}</Button>
          )}
        </div>
      }
    >
      <Card>
        {!read && <Pending what={tr("settings-mobile-development-panel-devices-2")} rows={3} />}
        {read && error && <ErrorNote error={error} retry={() => void refreshDevices()} />}
        {read && !error && rows.length === 0 && <p className="text-2xs text-text-dim">{tr("settings-mobile-development-panel-simulator-emulator-phone-here-make-simulator")}</p>}
        {read && !error && rows.length > 0 && (
          <ul className="flex flex-col divide-y divide-hairline text-2xs" aria-label={tr("settings-mobile-development-panel-devices")}>
            {rows.map((r) => (
              <li key={r.id} className="flex items-center gap-2 py-1.5">
                <ICON.simulator size={12} aria-hidden className="text-text-dim" />
                <span className="font-medium text-text">{r.name}</span>
                <span className="min-w-0 flex-1 truncate text-text-dim">{r.words}</span>
                <span className="flex items-center gap-1">
                  {r.verbs.includes("boot") && (
                    <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void act(r.id, "boot")}>{tr("settings-mobile-development-panel-boot")}</Button>
                  )}
                  {r.verbs.includes("show") && (
                    <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void act(r.id, "show")}>{tr("settings-mobile-development-panel-show")}</Button>
                  )}
                  {r.verbs.includes("shutdown") && (
                    <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => void act(r.id, "shutdown")}>{tr("settings-mobile-development-panel-shut-down")}</Button>
                  )}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Card>
      <CreateSimulatorDialog open={making} onClose={() => setMaking(false)} status={status} />
    </Section>
  );
}

/** A new iOS simulator: a name, a device type and a runtime the status lists. */
function CreateSimulatorDialog({ open, onClose, status }: { open: boolean; onClose: () => void; status: MobileDevelopmentStatus | null }) {
  const toast = useToast();
  const types = status?.toolchain.ios_devicetypes ?? [];
  const runtimes = (status?.toolchain.ios_runtimes ?? []).filter((r) => r.available);
  const [name, setName] = useState("");
  const [devicetype, setDevicetype] = useState("");
  const [runtime, setRuntime] = useState("");
  const [saving, setSaving] = useState(false);
  const problem = simulatorProblem({ name, devicetype, runtime });
  const submit = () => {
    if (problem || saving) return;
    setSaving(true);
    void attempt(
      () => api.createSimulator({ name: name.trim(), devicetype, runtime }),
      (why) => {
        setSaving(false);
        toast.error(why);
      },
      (made) => {
        setSaving(false);
        toast.ok(tr("settings-mobile-development-panel-made-boot-from-list", { made: made.name }));
        setName("");
        onClose();
        void refreshDevices();
      },
    );
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={tr("settings-mobile-development-panel-create-simulator-2")}
      description={tr("settings-mobile-development-panel-device-type-ios-runtime-xcode-has")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{tr("settings-connectors-panel-cancel")}</Button>
          <Tooltip label={problem ?? tr("settings-mobile-development-panel-make")}>
            <span className="inline-flex">
              <Button variant="primary" disabled={!!problem || saving} onClick={submit}>{tr("settings-mobile-development-panel-create")}</Button>
            </span>
          </Tooltip>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={tr("settings-git-profiles-panel-name")}>
          <TextInput autoFocus value={name} placeholder={tr("settings-mobile-development-panel-iphone-16-tests")} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={tr("settings-mobile-development-panel-device-type")} hint={types.length === 0 ? tr("settings-mobile-development-panel-device-type-listed-check-setup-xcode") : undefined}>
          <Select value={devicetype} onChange={(e) => setDevicetype(e.target.value)}>
            <option value="">{tr("settings-mobile-development-panel-choose")}</option>
            {types.map((t) => (
              <option key={t.identifier} value={t.identifier}>
                {t.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label={tr("settings-mobile-development-panel-runtime")} hint={runtimes.length === 0 ? tr("settings-mobile-development-panel-ios-runtime-downloaded-xcode-settings-components") : undefined}>
          <Select value={runtime} onChange={(e) => setRuntime(e.target.value)}>
            <option value="">{tr("settings-mobile-development-panel-choose")}</option>
            {runtimes.map((r) => (
              <option key={r.identifier} value={r.identifier}>
                {r.name}
              </option>
            ))}
          </Select>
        </Field>
      </div>
    </Dialog>
  );
}
