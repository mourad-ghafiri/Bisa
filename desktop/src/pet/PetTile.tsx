/**
 * One pet as a tile: its idle frame standing in a box, its name, the pack's
 * tagline and two chips — the archetype and the mood — on the kit's `Tile`,
 * the one shape a preview-and-name choice wears (the Appearance panel's
 * theme tiles and the *New drawing* gallery are the others). `size="sm"` is
 * the footer overlay's: the preview and the name only.
 *
 * A person's own pack's *Remove* sits beside the tile, never inside it, as
 * the kit's `Card` rule says.
 */

import type { PetDef } from "../types";
import { Chip, Tile } from "../ui";
import { originWords, tileWords } from "./petModel.mjs";
import { PetSprite } from "./PetSprite";
import { t } from "../i18n/l10n.mjs";

export function PetTile({
  pet,
  active,
  onSelect,
  size = "md",
  disabled,
}: {
  pet: PetDef;
  active: boolean;
  onSelect: () => void;
  size?: "sm" | "md";
  disabled?: boolean;
}) {
  const words = tileWords(pet);
  const small = size === "sm";
  return (
    <Tile
      active={active}
      onSelect={onSelect}
      disabled={disabled}
      name={pet.displayName}
      ariaLabel={t("pet-pet-tile-showing", { displayName: pet.displayName, flag: (active) ? "yes" : "no" })}
      title={small ? `${pet.displayName} — ${words.tagline}` : undefined}
      previewClass={small ? "flex h-16 items-center justify-center bg-surface-2" : "flex h-28 items-center justify-center bg-surface-2"}
      blurb={small ? undefined : words.tagline}
      preview={
        <>
          {/* Idle, because a row of pets all running would be a row nobody can read. */}
          <PetSprite pet={pet.id} def={pet} state="idle" height={small ? 44 : 80} className="block" />
          {!small && (
            <span className="absolute right-1.5 top-1.5">
              <Chip tone={pet.origin === "catalog" ? "quiet" : "neutral"}>{originWords(pet.origin)}</Chip>
            </span>
          )}
        </>
      }
    >
      {!small && words.chips.length > 0 && (
        <span className="flex flex-wrap gap-1">
          {words.chips.map((w) => (
            <Chip key={w} tone="quiet">
              {w}
            </Chip>
          ))}
        </span>
      )}
    </Tile>
  );
}
