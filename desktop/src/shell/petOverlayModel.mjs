/**
 * The words of the footer's Pet read-out and the Pet panel — what the switch
 * says, what the size means, whether the pet was moved — so the two never
 * disagree. Plain `.mjs`, so `node --test` reads it.
 */

import { petHeight } from "../pet/petModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** The read-out's tooltip and the overlay's title line. */
export function titleWords(active, pets) {
  if (!active) return t("shell-pet-overlay-pet-put-away");
  const name = pets.find((p) => p.id === active)?.displayName ?? t("shell-pet-overlay-a-pet");
  return t("shell-pet-overlay-pet", { name });
}

/** The switch's label and its hint, by whether a pet shows. */
export function switchWords(active) {
  return active
    ? { label: t("shell-pet-overlay-show-pet"), hint: t("shell-pet-overlay-put-away-footer-keeps-way-back") }
    : { label: t("shell-pet-overlay-show-pet"), hint: t("shell-pet-overlay-off-moonrice-comes-when-pet-chosen") };
}

/** What the size slider says beside its value. */
export function sizeWords(size) {
  return t("shell-pet-overlay-px-tall", { px: petHeight(size) });
}

/** Whether the pet was dragged from its corner, in a sentence. */
export function positionWords(moved) {
  return moved ? t("shell-pet-overlay-dragged-from-corner") : t("shell-pet-overlay-corner");
}
