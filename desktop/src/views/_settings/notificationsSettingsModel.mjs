/**
 * The words of Settings › Capabilities › System › Notifications (ide/13):
 * which switches the card draws and in what order — the master first, then
 * one per category of what happened — what the OS's permission reads as
 * beside them, and the one fact about the icon a person would otherwise
 * puzzle over. Pure, so `node --test` reads them; the keys and their defaults
 * are `shell/notificationsModel.mjs`'s.
 */

import { CATEGORIES, NOTIFY_KEYS } from "../../shell/notificationsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The master's registry key. */
export const MASTER_KEY = NOTIFY_KEYS.enabled;

/** The category switches' registry keys, in the card's order. */
export const CATEGORY_KEYS = Object.freeze(CATEGORIES.map((c) => NOTIFY_KEYS[c]));

/** The permission answers the plugin gives: macOS's word, or none yet. */
export const PERMISSIONS = Object.freeze(["granted", "denied", "default"]);

/**
 * The OS's permission, from what the webview knows: *granted* when the
 * plugin says so; else the webview's own word — `denied` after a refusal,
 * which only System Settings can change, `default` while macOS was never
 * asked. Anything else reads as never asked: asking is what finds out.
 * @param {unknown} granted the plugin's `isPermissionGranted()`
 * @param {unknown} word the webview's `Notification.permission`, or what `requestPermission()` answered
 * @returns {"granted" | "denied" | "default"}
 */
export function permissionOf(granted, word) {
  if (granted === true || word === "granted") return "granted";
  return word === "denied" ? "denied" : "default";
}

/**
 * The OS's permission beside the switches, as a chip and a sentence: the
 * permission is the OS's and the switches are ours, so *granted · off here*
 * is a state a person can read. `null` while the plugin has not answered;
 * `"unavailable"` off the desktop app, where nothing can be read or asked.
 * @param {"granted" | "denied" | "default" | "unavailable" | null} permission
 * @param {boolean} enabled the master switch
 */
export function permissionWords(permission, enabled) {
  switch (permission) {
    case "granted":
      return enabled
        ? { tone: "ok", label: t("settings-notifications-granted"), sentence: t("settings-notifications-macos-lets-bisa-notify-on-here") }
        : { tone: "ok", label: t("settings-notifications-granted-off-here"), sentence: t("settings-notifications-macos-lets-bisa-notify-nothing-sent") };
    case "denied":
      return { tone: "warn", label: t("settings-notifications-not-allowed"), sentence: t("settings-notifications-macos-has-refused-open-system-settings") };
    case "default":
      return { tone: "neutral", label: t("settings-notifications-not-asked-yet"), sentence: t("settings-notifications-macos-has-not-been-asked-allow") };
    case "unavailable":
      return { tone: "quiet", label: t("settings-notifications-not-available"), sentence: t("settings-notifications-permission-read-from-desktop-app") };
    default:
      return { tone: "quiet", label: t("settings-notifications-checking"), sentence: t("settings-notifications-asking-macos") };
  }
}

/** The verb that asks, when asking can change the answer: only before macOS has been asked. */
export function askVerb(permission) {
  return permission === "default" ? t("settings-notifications-allow") : null;
}

/**
 * The one fact about the icon: macOS draws a notification with the icon of
 * the bundle it comes from, and a development build posts as Terminal.
 */
export function iconWords() {
  return t("settings-notifications-development-build-wears-terminal-icon");
}
