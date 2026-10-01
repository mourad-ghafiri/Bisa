/**
 * Settings › Capabilities › Draw, as facts (19 — Drawings): the keys, the
 * three words `draw.agents` takes and what each means, the snapshot's width
 * within its bounds. The registry panel draws the group; the panel's own
 * switches are `drawStore.ts`'s. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

export const ENABLED_KEY = "draw.enabled";
export const AGENTS_KEY = "draw.agents";
export const SNAPSHOT_WIDTH_KEY = "draw.snapshot.width";

/** Who may draw through the tools, in the order the switch shows. */
export const POLICIES = Object.freeze(["everyone", "assigned", "nobody"]);
export const DEFAULT_POLICY = "everyone";

export const MIN_SNAPSHOT_WIDTH = 320;
export const MAX_SNAPSHOT_WIDTH = 4096;
export const DEFAULT_SNAPSHOT_WIDTH = 1280;

/** The width a snapshot is rendered at: the setting's number, within bounds. @param {unknown} value */
export function snapshotWidth(value) {
  if (value === null || value === undefined || value === "" || typeof value === "boolean") return DEFAULT_SNAPSHOT_WIDTH;
  const n = Number(value);
  if (!Number.isFinite(n)) return DEFAULT_SNAPSHOT_WIDTH;
  return Math.min(MAX_SNAPSHOT_WIDTH, Math.max(MIN_SNAPSHOT_WIDTH, Math.round(n)));
}

/** What a policy means, in a sentence. @param {string} policy */
export function policyWords(policy) {
  switch (policy) {
    case "assigned":
      return t("draw-settings-only-agents-carrying-drawing-skill");
    case "nobody":
      return t("draw-settings-drawing-tools-refuse-every-agent");
    default:
      return t("draw-settings-any-agent-may-draw");
  }
}
