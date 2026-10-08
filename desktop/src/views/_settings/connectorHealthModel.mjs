/**
 * What a connector account's health reads as (03 § Connectors): the chip's
 * tone and words for a row, from the node's `AccountHealthView` on every
 * account row — what the last check found, kept for the engine's lifetime
 * and forgotten when the way in changes — the platform's own reason under a
 * failing row, and which accounts *Check all* dials. The facts are the
 * node's; this only puts them in words. No DOM, no fetch.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The kit's tone for a health: fine, a refused credential (the person's to
 * fix), a platform that did not answer, or nothing asked yet.
 * @param {{state: string, check?: string | null} | null | undefined} health
 */
export function healthTone(health) {
  switch (health?.state) {
    case "ok":
      return "ok";
    case "failing":
      return health.check === "refused" ? "danger" : "warn";
    default:
      return "quiet";
  }
}

/**
 * The row's words: *connected (200)*, *refused (401)*, *unreachable*, the
 * connector names no check, or *not checked yet*. A check word this desktop
 * does not know — a newer node's — is said, never read as nothing.
 * @param {{state: string, check?: string | null, status?: number | null} | null | undefined} health
 * @param {string} [name] the connector's name
 */
export function healthWords(health, name = t("settings-connectors-platform")) {
  if (!health || health.state === "unknown") return t("settings-code-host-accounts-checked-yet");
  const status = health.status ? ` (${health.status})` : "";
  switch (health.check) {
    case "connected":
      return t("settings-connectors-health-connected", { status });
    case "refused":
      return t("settings-connectors-health-refused", { status });
    case "unreachable":
      return t("settings-connectors-health-unreachable");
    case "no_check":
      return t("settings-connectors-s-definition-names-check-operation-first", { name });
    default:
      return t("settings-connectors-check-answered-state-desktop-does-know", { state: String(health.check ?? health.state) });
  }
}

/**
 * The platform's own sentence under a failing row — already redacted by the
 * node — and nothing under a row that is fine or not yet checked.
 * @param {{state: string, reason?: string | null} | null | undefined} health
 */
export function healthDetail(health) {
  return health?.state === "failing" && health.reason ? health.reason : "";
}

/**
 * The accounts *Check all* dials for one connector: every account that can
 * answer — one with a secret set, or any account of a scheme that needs
 * none — of a connector that names a check operation. An account with no
 * secret yet would only be refused for want of one.
 * @param {import("../../types").ConnectorDetail} detail
 * @returns {{connector: string, account: string}[]}
 */
export function checkTargets(detail) {
  const def = detail?.connector;
  if (!def?.check) return [];
  const auth = def.auth;
  const scheme = typeof auth === "string" ? auth : (auth?.scheme ?? "none");
  return (detail.accounts ?? [])
    .filter((a) => scheme === "none" || (a.secrets_set ?? []).length > 0)
    .map((a) => ({ connector: def.id, account: a.id }));
}

/** How many accounts *Check all* dials at once — the engine joins a second ask to a running check, so more buys nothing. */
export const CHECK_ALL_AT_ONCE = 4;
