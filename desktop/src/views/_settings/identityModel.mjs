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

/**
 * Whether two drafts are the same profile: the name as typed, trimmed, and
 * the face by its content.
 * @param {{label: string, photo: {sha256: string} | null}} a
 * @param {{label: string, photo: {sha256: string} | null}} b
 */
export function sameProfile(a, b) {
  return a.label.trim() === b.label.trim() && (a.photo?.sha256 ?? null) === (b.photo?.sha256 ?? null);
}

/**
 * The form after the owner's row was read again. The row is read anew by
 * every reload of the workspace — a run starting is enough — and a form
 * that followed the *object* was re-seeded each time: a name being typed
 * was wiped mid-word. The form follows the row's **value**, and only while
 * it is untouched: a draft that differs from what it was seeded with is the
 * person's, and stays; what the row says now is kept as the seed, so
 * *Discard* goes back to it and a save that lands leaves nothing dirty.
 * @template {{label: string, photo: {sha256: string} | null}} D
 * @param {{seeded: D, draft: D}} form
 * @param {{label?: string | null, photo?: {sha256: string} | null} | null | undefined} row
 * @returns {{seeded: D, draft: D}}
 */
export function profileSeed(form, row) {
  const now = /** @type {D} */ (profileDraft(row));
  const untouched = sameProfile(form.draft, form.seeded);
  if (sameProfile(now, form.seeded) && !untouched) return form;
  return { seeded: now, draft: untouched ? now : form.draft };
}

/** The row as one value an effect can follow: it changes when the profile does, not when the object is made anew. */
export function profileKey(row) {
  const d = profileDraft(row);
  return `${d.label}\u0000${d.photo?.sha256 ?? ""}`;
}
