/**
 * The devices of the Project IDE (ide/19), as facts with no React in them:
 * what a device is called in a strip, a tooltip and a menu, which glyph it
 * wears, and the Devices button's menu — the main click and the caret's
 * items, in the order they are offered. The node lists the devices
 * (`GET /mobile-development/devices`) and composes the run line; nothing here names a
 * program or a path. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** The words a device's kind and state wear. */
const KIND_WORD = Object.freeze({ simulator: t("workbench-devices-kind-simulator"), emulator: t("workbench-devices-kind-emulator"), physical: t("workbench-devices-kind-physical") });
const STATE_WORD = Object.freeze({ booted: t("workbench-devices-state-booted"), shutdown: t("workbench-devices-shut-down"), running: t("workbench-devices-state-running"), offline: t("workbench-devices-state-offline") });

/**
 * @typedef {{id: string, name: string, platform: "ios" | "android", kind: "simulator" | "emulator" | "physical", state: "booted" | "shutdown" | "running" | "offline", os?: string | null}} Device
 */

/** @param {readonly Device[]} devices @param {string} id */
export function deviceById(devices, id) {
  return devices.find((d) => d.id === id) ?? null;
}

/** Whether an app can run on it now. @param {Device} device */
export function isUp(device) {
  return device.state === "booted" || device.state === "running";
}

/** Whether the platforms setting lets a device through. @param {string} platforms @param {string} platform */
export function platformAllows(platforms, platform) {
  return platforms === "both" || platforms === platform;
}

/** The `ICON` name a device wears: a simulator or an emulator is a phone on this screen, a physical one a phone in hand. @param {Device} device */
export function deviceIcon(device) {
  return device.kind === "physical" ? "device" : "simulator";
}

/** The device as a tooltip says it: `iPhone 16 · iOS 18.2 · simulator · booted`. @param {Device} device */
export function deviceTitle(device) {
  const os = device.os ? ` · ${device.os}` : "";
  return `${device.name}${os} · ${KIND_WORD[device.kind] ?? device.kind} · ${STATE_WORD[device.state] ?? device.state}`;
}

/** A device's state as a strip note, or nothing while it is up. @param {Device} device */
export function deviceNote(device) {
  return isUp(device) ? null : (STATE_WORD[device.state] ?? device.state);
}

/**
 * The Devices button's menu.
 *
 * The main click is the one thing most worth doing: show the device the app
 * runs on; else run on the device that is up; else boot a simulator or an
 * emulator; else the setup. The caret lists every device of the platforms
 * that are on — up first — with its verbs by state, then the setup.
 *
 * @param {{enabled: boolean, flutter: boolean | null, platforms: string, devices: readonly Device[], tabs: readonly string[], running: readonly string[], busy: boolean}} facts
 *   `tabs` the device ids with a document open here, `running` the ones with a run terminal live
 * @returns {{main: {id: string, label: string, hint: string, icon: string, disabled?: boolean}, items: {id: string, label: string, hint?: string, icon: string, disabled?: boolean, danger?: boolean, separatorBefore?: boolean}[]}}
 */
export function deviceMenu({ enabled, flutter, platforms, devices, tabs, running, busy }) {
  const setup = { id: "check", label: t("workbench-devices-check-setup"), hint: t("workbench-devices-settings-capabilities-mobile-development-what-installed"), icon: "settings" };
  if (!enabled) {
    return {
      main: { id: "check", label: t("workbench-devices-set-up-mobile-development"), hint: t("workbench-devices-mobile-development-off-machine-turn-settings"), icon: "device" },
      items: [setup],
    };
  }
  if (flutter === false) {
    return {
      main: { id: "none", label: t("workbench-devices-no-flutter-app-here"), hint: t("workbench-devices-no-pubspec-naming-flutter"), icon: "device", disabled: true },
      items: [setup],
    };
  }
  const listed = devices.filter((d) => platformAllows(platforms, d.platform));
  const ordered = [...listed.filter(isUp), ...listed.filter((d) => !isUp(d))];
  const runningOn = new Set(running);
  const open = new Set(tabs);
  const items = [];
  for (const [i, d] of ordered.entries()) {
    const first = i === 0 ? {} : { separatorBefore: true };
    if (isUp(d)) {
      if (runningOn.has(d.id)) {
        items.push({ id: `view:${d.id}`, label: open.has(d.id) ? t("workbench-devices-show", { d: d.name }) : t("workbench-devices-open-beside-code", { d: d.name }), hint: t("workbench-devices-app-running-screen-mirrored-beside-code"), icon: deviceIcon(d), ...first });
        items.push({ id: `stop:${d.id}`, label: t("workbench-devices-stop-app", { d: d.name }), hint: t("workbench-devices-sends-q-run-terminal"), icon: "terminate", danger: true, disabled: busy });
      } else {
        items.push({ id: `run:${d.id}`, label: t("workbench-devices-run", { d: d.name }), hint: t("workbench-devices-flutter-run-terminal-screen-beside-code", { d: d.name }), icon: "play", disabled: busy, ...first });
        items.push({ id: `view:${d.id}`, label: open.has(d.id) ? t("workbench-devices-show", { d: d.name }) : t("workbench-devices-open-beside-code", { d: d.name }), hint: t("workbench-devices-screen-mirrored-beside-code-without-running"), icon: deviceIcon(d) });
      }
      if (d.kind !== "physical") {
        items.push({ id: `shutdown:${d.id}`, label: t("workbench-devices-shut-down-2", { d: d.name }), icon: "stop", disabled: busy || runningOn.has(d.id) });
      }
    } else if (d.kind === "physical") {
      items.push({ id: `offline:${d.id}`, label: t("workbench-devices-offline", { d: d.name }), hint: t("workbench-devices-plugged-not-answering-unlock-trust-computer"), icon: "device", disabled: true, ...first });
    } else {
      items.push({ id: `boot:${d.id}`, label: t("workbench-devices-boot", { d: d.name }), hint: `${KIND_WORD[d.kind]}${d.os ? ` · ${d.os}` : ""}`, icon: "boot", disabled: busy, ...first });
    }
  }
  items.push({ ...setup, separatorBefore: items.length > 0 });
  const up = ordered.find(isUp);
  const runningDevice = ordered.find((d) => isUp(d) && runningOn.has(d.id));
  const bootable = ordered.find((d) => !isUp(d) && d.kind !== "physical");
  let main;
  if (runningDevice) {
    main = { id: `view:${runningDevice.id}`, label: t("workbench-devices-show-2", { runningDevice: runningDevice.name }), hint: t("workbench-devices-app-running-screen-beside-code", { runningDevice: runningDevice.name }), icon: deviceIcon(runningDevice) };
  } else if (up) {
    main = { id: `run:${up.id}`, label: t("workbench-devices-run-2", { up: up.name }), hint: t("workbench-devices-flutter-run-terminal-screen-beside-code-2", { up: up.name }), icon: "play", disabled: busy };
  } else if (bootable) {
    main = { id: `boot:${bootable.id}`, label: t("workbench-devices-boot-2", { bootable: bootable.name }), hint: t("workbench-devices-boot-then-run-app", { kind: KIND_WORD[bootable.kind] }), icon: "boot", disabled: busy };
  } else {
    main = { id: "check", label: t("workbench-devices-no-device-check-setup"), hint: listed.length === 0 && devices.length > 0 ? t("workbench-devices-every-device-here-platform-off-settings") : t("workbench-devices-no-simulator-emulator-phone-here-settings"), icon: "device" };
  }
  return { main, items };
}
