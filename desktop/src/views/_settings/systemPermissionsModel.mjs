/**
 * The words of Settings › Capabilities › System (ide/13): what a grant's
 * status reads as beside the platform's own switch, which verb asks for it,
 * and the two sentences the panel owes a person — that Full Disk Access is
 * recommended for productivity, and that the security features protect the
 * machine on a best-effort basis, never as a sandbox. Pure, so `node --test`
 * reads them; the kinds and statuses are the shell's (`permissions.rs`).
 */

import { t } from "../../i18n/l10n.mjs";

/** The grants, in panel order — `permissions.rs`'s `PermissionKind`, snake_case. */
export const KINDS = Object.freeze(["full_disk_access", "microphone"]);

/** The statuses the shell answers — `permissions.rs`'s `PermissionStatus`. */
export const STATUSES = Object.freeze(["granted", "denied", "not_determined", "unsupported"]);

/** The registry key the platform's wish for a grant lives under. */
export function settingKeyOf(kind) {
  return `system.${kind}`;
}

const TITLE = Object.freeze({
  full_disk_access: t("settings-system-permissions-full-disk-access"),
  microphone: t("settings-system-permissions-microphone"),
});

export function titleOf(kind) {
  return TITLE[kind] ?? kind;
}

/**
 * A grant's status beside the switch, as a chip and a sentence. The grant is
 * macOS's and the switch is ours, so *granted · off here* is a state a person
 * can read — and the panel never pretends one implies the other.
 * @param {"full_disk_access" | "microphone"} kind
 * @param {"granted" | "denied" | "not_determined" | "unsupported" | null} status `null` while the shell has not answered
 * @param {boolean} enabled the platform's switch
 */
export function statusWords(kind, status, enabled) {
  const title = titleOf(kind);
  switch (status) {
    case "granted":
      return enabled
        ? { tone: "ok", label: "granted", sentence: t("settings-system-permissions-macos-lets-app-use-here", { title: title.toLowerCase() }) }
        : { tone: "ok", label: t("settings-system-permissions-granted-off-here"), sentence: t("settings-system-permissions-macos-lets-app-use-nothing-ours", { title: title.toLowerCase() }) };
    case "denied":
      return {
        tone: "warn",
        label: t("settings-system-permissions-granted"),
        sentence: enabled
          ? t("settings-system-permissions-turned-here-but-macos-has-granted")
          : t("settings-system-permissions-macos-has-granted-turning-here-opens"),
      };
    case "not_determined":
      return {
        tone: "neutral",
        label: t("settings-system-permissions-asked-yet"),
        sentence: kind === "microphone" ? t("settings-system-permissions-macos-has-been-asked-turning-here") : t("settings-system-permissions-check-could-tell-check-again-open"),
      };
    case "unsupported":
      return { tone: "quiet", label: t("settings-system-permissions-available"), sentence: t("settings-system-permissions-grant-cannot-read-asked-from-here") };
    default:
      return { tone: "quiet", label: t("settings-system-permissions-checking"), sentence: t("settings-system-permissions-asking-macos") };
  }
}

/**
 * The verb that asks for a grant, or none when there is nothing to ask:
 * Full Disk Access has no prompt, so the verb is always the pane; the
 * microphone is asked once by macOS and afterwards only the pane can change it.
 * @param {"full_disk_access" | "microphone"} kind
 * @param {"granted" | "denied" | "not_determined" | "unsupported" | null} status
 */
export function requestVerb(kind, status) {
  if (status === "granted" || status === "unsupported" || status === null) return null;
  if (kind === "microphone" && status === "not_determined") return t("settings-system-permissions-ask-macos");
  return t("settings-system-permissions-open-system-settings");
}

/**
 * What the panel owes a person about each grant, before they decide.
 * @param {"full_disk_access" | "microphone"} kind
 */
export function recommendation(kind) {
  if (kind === "full_disk_access") {
    return {
      why: t("settings-system-permissions-recommended-productivity-without-macos-fences-app"),
      safety:
        t("settings-system-permissions-platform-s-security-features-redactor-tool"),
      how: t("settings-system-permissions-turning-opens-system-settings-privacy-security"),
    };
  }
  return {
    why: t("settings-system-permissions-voice-mode-which-coming-nothing-listens"),
    safety: t("settings-system-permissions-turning-asks-macos-once-app-s"),
    how: null,
  };
}

/**
 * Why the grants cannot be read here, in one sentence.
 * @param {"not_desktop" | "not_macos" | string} reason `not_desktop`, `not_macos`, or the shell's own detail
 */
export function unavailableWords(reason) {
  switch (reason) {
    case "not_desktop":
      return t("settings-system-permissions-these-grants-macos-s-read-asked");
    case "not_macos":
      return t("settings-system-permissions-these-grants-macos-s-system-switches");
    default:
      return reason;
  }
}
