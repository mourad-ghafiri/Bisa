/**
 * Your profile as Settings › Identity edits it (14-collaboration): the owner's
 * own row of the members list — its label and its face — as a draft, and the
 * words that say where both travel. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The owner's row of the workspace's members, by the workspace's own key.
 * @template {{pubkey: string}} M
 * @param {{pubkey: string, members?: readonly M[]} | null | undefined} ws
 * @returns {M | null}
 */
export function ownerRow(ws) {
  return (ws?.members ?? []).find((m) => m.pubkey === ws?.pubkey) ?? null;
}

/**
 * The draft the panel opens on: the row's label as text, its face as the ref.
 * @param {{label?: string | null, photo?: {sha256: string} | null} | null | undefined} row
 */
export function profileDraft(row) {
  return { label: (row?.label ?? "").trim(), photo: row?.photo ?? null };
}

/** What the panel says above the two fields. */
export function profileWords() {
  return t("settings-identity-name-photo-workspace-s-they-travel");
}
