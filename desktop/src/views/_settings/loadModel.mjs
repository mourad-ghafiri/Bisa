/**
 * How a Settings panel reads (ide/13 §Every panel reads the same way).
 *
 * A panel is drawn at once with its shape; each read decides one thing for
 * the place its answer goes — `phase`: *pending* (nothing yet, being read),
 * *failed* (nothing yet, and the read refused) or *ready* (an answer is on
 * screen, whatever a later re-read did). Beside a section that re-reads,
 * `readWords` is the `role="status"` sentence: *reading the GitHub CLI…*
 * while the first answer is on its way, *checking again…* while a re-read
 * is, *could not read …: <reason> — showing the last answer* when a re-read
 * refused, *read 12 s ago* once settled. `pendingRows` is how many rows a
 * section's skeleton draws, from one table, so a panel never lies about its
 * shape.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** @typedef {{ data?: unknown, loading?: boolean, error?: string | null, refreshing?: boolean, at?: number | null }} Read */

/**
 * What the place a read's answer goes draws.
 * @param {Read} read
 * @returns {"pending" | "failed" | "ready"}
 */
export function phase(read) {
  const has = read.data !== null && read.data !== undefined;
  if (has) return "ready";
  if (read.error) return "failed";
  return "pending";
}

/**
 * One phase for a place that waits on several reads: it cannot draw until
 * every one answered, and one that refused with nothing to show is a
 * failure the place says — so *failed* beats *pending* beats *ready*.
 * @param {readonly Read[]} reads
 * @returns {"pending" | "failed" | "ready"}
 */
export function phaseOf(reads) {
  const phases = reads.map(phase);
  if (phases.includes("failed")) return "failed";
  if (phases.includes("pending")) return "pending";
  return "ready";
}

/** The first message among reads that failed with nothing to show. */
export function firstFailure(reads) {
  const failed = reads.find((r) => phase(r) === "failed");
  return failed?.error ?? null;
}

/** *12 s ago*, *3 min ago*, *2 h ago*. */
export function agoWords(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return t("settings-load-s-ago", { s });
  const m = Math.floor(s / 60);
  if (m < 60) return t("settings-load-min-ago", { m });
  const h = Math.floor(m / 60);
  if (h < 24) return t("settings-load-h-ago", { h });
  return t("settings-load-d-ago", { h: Math.floor(h / 24) });
}

/**
 * The status sentence beside a section that reads `what`, and whether it
 * says a failure. `null` when there is nothing worth a line: settled, with
 * no time to tell.
 * @param {{ what: string, loading?: boolean, refreshing?: boolean, error?: string | null, at?: number | null, data?: unknown }} read
 * @param {number} [now] unix seconds
 * @returns {{ text: string, failed: boolean } | null}
 */
export function readWords(read, now = Date.now() / 1000) {
  const has = read.data !== null && read.data !== undefined;
  if (read.refreshing) return { text: t("settings-load-checking-again"), failed: false };
  if (!has && read.loading) return { text: t("settings-load-reading", { what: read.what }), failed: false };
  if (read.error && has) return { text: t("settings-load-could-read-showing-last-answer", { what: read.what, error: read.error }), failed: true };
  if (read.error) return { text: t("settings-load-could-not-read", { what: read.what, error: read.error }), failed: true };
  if (read.at != null) return { text: t("settings-load-read-ago", { ago: agoWords(now - read.at) }), failed: false };
  return null;
}

/**
 * The rows a section's skeleton draws while its read is pending — one table,
 * so a shape is never invented at a call site. A section is named by the
 * words its `Pending` line says (*reading the registry…*), and each key here
 * is that message — never a sentence spelt in this file, which would match
 * in English alone.
 */
export const PENDING_ROWS = Object.freeze({
  [t("settings-registry-panel-settings")]: 4,
  [t("settings-browser-access-panel-resolved-values")]: 1,
  [t("settings-code-host-panel-github-cli")]: 3,
  [t("settings-load-how-sign")]: 1,
  [t("settings-git-profiles-panel-stored-accounts")]: 2,
  [t("settings-load-governance")]: 3,
  [t("settings-load-teams")]: 1,
  [t("settings-harnesses-panel-harnesses")]: 3,
  [t("settings-load-keymap")]: 8,
  [t("settings-cache-panel-caches")]: 4,
  [t("settings-node-panel-node-2")]: 1,
  [t("settings-system-panel-grant")]: 1,
  [t("settings-security-panels-rules-2")]: 3,
  [t("settings-security-panels-the-built-in-rules")]: 4,
  [t("settings-decisions-panel-security-status")]: 2,
  [t("settings-security-panels-agents")]: 1,
  [t("settings-pet-panel-pets")]: 2,
  [t("settings-catalog-panel-catalog")]: 6,
  [t("settings-skills-panel-library")]: 4,
  [t("settings-mcp-panel-registry")]: 4,
  [t("settings-connectors-panel-connectors")]: 3,
  [t("settings-connectors-panel-accounts-2")]: 2,
  [t("settings-connectors-panel-definition")]: 2,
  [t("settings-connectors-panel-callback-port")]: 1,
  [t("settings-git-profiles-panel-ssh-keys")]: 4,
  [t("settings-git-profiles-panel-git-profiles")]: 3,
  [t("settings-global-git-panel-global-git-config-2")]: 4,
  [t("settings-load-log-files")]: 2,
  [t("settings-load-workspace")]: 4,
  [t("settings-network-panel-mac-s-network")]: 4,
  [t("settings-network-panel-proxy-2")]: 2,
});

/** @param {string} what */
export function pendingRows(what) {
  return PENDING_ROWS[what] ?? 3;
}
