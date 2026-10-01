/**
 * Devices, beside Browser (ide/19): the one button for the simulators,
 * emulators and phones this machine can reach and everything that puts the
 * checkout's Flutter app on one. The main click shows the device the app is
 * running on, else runs on the device that is up, else boots a simulator or
 * an emulator, else opens the setup. The caret lists every device with its
 * verbs by state — run, show, stop, boot, shut down — then *Check the
 * setup…*. The words and the order are `devicesModel.mjs`'s; the node lists
 * the devices and composes the run line, so nothing here names a program or
 * a path. Shown only while mobile development is on here and the checkout
 * holds a Flutter app: the Settings panel is the door to both.
 */

import { useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { navigate } from "../router";
import { ICON, Menu, Tooltip, cn, useToast } from "../ui";
import type { MenuItem } from "../ui";
import { useAsync } from "../views/_work/useAsync";
import { deviceById, deviceMenu, isUp } from "../views/_workbench/devicesModel.mjs";
import { DEFAULT_PLATFORMS, ENABLED_KEY, PLATFORMS, PLATFORMS_KEY } from "../views/_settings/mobileDevelopmentSettingsModel.mjs";
import { settingsSearch } from "../views/_settings/settingsLink.mjs";
import { refreshDevices, useDevices } from "./devicesStore";
import { boolOf, choiceOf } from "./settingsModel.mjs";
import { mobileDevelopmentRunSession, mobileDevelopmentRunSessions } from "./terminalsModel.mjs";
import { useResolvedSettings } from "./useResolvedSettings";
import { openTerminalIn, useTerminals, writeToTerminalTab } from "./useTerminals";
import { t } from "../i18n/l10n.mjs";

export function DeviceLauncher({
  wid,
  label,
  openDevices,
  onOpenDevice,
  disabledReason,
  className,
}: {
  wid: string;
  label?: string | null;
  /** The device ids with a document open in this root. */
  openDevices: readonly string[];
  /** Open (or show) the device's document beside the code. */
  onOpenDevice: (deviceId: string) => void;
  disabledReason?: string | null;
  className?: string;
}) {
  const toast = useToast();
  const { resolved } = useResolvedSettings(null);
  const enabled = boolOf(resolved, ENABLED_KEY, false);
  const platforms = choiceOf(resolved, PLATFORMS_KEY, PLATFORMS, DEFAULT_PLATFORMS);
  const flutter = useAsync<boolean | null>(
    (s) =>
      enabled
        ? api.workstreamMobileDevelopment(wid, s).then(
            (r) => r.flutter,
            () => null,
          )
        : Promise.resolve(null),
    [wid, enabled],
  );
  const { devices } = useDevices(enabled);
  const { sessions } = useTerminals();
  const [busy, setBusy] = useState(false);
  useEngineEvents((e) => {
    if (e.payload.type === "mobile_development_changed") void refreshDevices();
    if (e.payload.type === "file_changed" && e.payload.id === wid && e.payload.path.endsWith("pubspec.yaml")) flutter.reload();
  });
  const running = mobileDevelopmentRunSessions(sessions, wid).map((s) => s.mobileDevelopment?.device ?? "").filter(Boolean);
  const noCheckout = disabledReason !== null && disabledReason !== undefined;
  const menu = deviceMenu({ enabled, flutter: flutter.data, platforms, devices, tabs: openDevices, running, busy: busy || noCheckout });

  const act = async (item: string) => {
    const [verb, id] = item.includes(":") ? [item.slice(0, item.indexOf(":")), item.slice(item.indexOf(":") + 1)] : [item, ""];
    const device = id ? deviceById(devices, id) : null;
    if (verb === "check" || verb === "none") {
      navigate({ name: "settings" }, settingsSearch("mobile-development"));
      return;
    }
    if (!device) return;
    if (verb === "run") {
      openTerminalIn({ scope: "workstream", id: wid, label: device.name, mobileDevelopment: { device: device.id } });
      onOpenDevice(device.id);
    } else if (verb === "view") {
      onOpenDevice(device.id);
    } else if (verb === "stop") {
      const run = mobileDevelopmentRunSession(sessions, wid, device.id);
      if (run) await writeToTerminalTab(run.key, "q").catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e)));
    } else if (verb === "boot" || verb === "shutdown") {
      setBusy(true);
      try {
        const after = verb === "boot" ? await api.mobileDevelopmentDeviceBoot(device.id) : await api.mobileDevelopmentDeviceShutdown(device.id);
        toast.ok(verb === "boot" ? t("shell-device-launcher-up", { after: after.name }) : t("shell-device-launcher-shut-down", { after: after.name }));
        await refreshDevices();
        if (verb === "boot") onOpenDevice(device.id);
      } catch (e) {
        toast.error(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    }
  };

  if (!enabled || flutter.data !== true) return null;
  const items: MenuItem[] = menu.items.map((item) => ({
    label: item.label,
    icon: (ICON as Record<string, typeof ICON.device>)[item.icon],
    disabled: item.disabled,
    separatorBefore: item.separatorBefore,
    danger: item.danger,
    onSelect: () => void act(item.id),
  }));
  const hint = noCheckout ? `${menu.main.hint} — ${disabledReason}` : menu.main.hint;
  const upCount = devices.filter(isUp).length;
  return (
    <span className={cn("inline-flex items-center", className)}>
      <Tooltip label={hint}>
        <button
          type="button"
          disabled={menu.main.disabled}
          onClick={() => void act(menu.main.id)}
          aria-label={label ? t("shell-device-launcher-main-with-device", { main: menu.main.label, label }) : menu.main.label}
          className="anim inline-flex h-6 items-center gap-1.5 rounded-l-control border border-r-0 border-border px-2 text-2xs text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-60"
        >
          <ICON.simulator size={12} aria-hidden />
          {t("shell-device-launcher-devices")}
          {upCount > 0 && <span className="tnum text-accent-ink">{upCount}</span>}
        </button>
      </Tooltip>
      <Menu
        label={t("shell-device-launcher-run-device-boot-one-check-setup")}
        items={items}
        trigger={
          <span className="anim inline-flex h-6 items-center rounded-r-control border border-border px-1 text-text-dim hover:bg-surface-2 hover:text-text">
            <ICON.expanded size={12} aria-hidden />
          </span>
        }
      />
    </span>
  );
}
