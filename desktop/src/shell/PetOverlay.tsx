/**
 * The overlay under the footer's Pet read-out: whether the pet shows, how big
 * it stands, which of the nine (and your own) it is, and the way back to its
 * corner — the resource overlays' shape (`ResourceOverlay`): a title line,
 * the controls, a footnote. Every fact is the pet store's and every sentence
 * `petOverlayModel.mjs`'s; this file draws. The pet itself is
 * `pet/PetCompanion.tsx`, floating over the app.
 */

import { navigate } from "../router";
import { Button, Slider, Switch } from "../ui";
import { PET_SIZE_MAX, PET_SIZE_MIN, PET_SIZE_STEP } from "../pet/petModel.mjs";
import { petMoved, resetPetPosition, setActivePet, setPetSize, togglePet, usePet } from "../pet/petStore";
import { PetTile } from "../pet/PetTile";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { positionWords, sizeWords, switchWords, titleWords } from "./petOverlayModel.mjs";
import { t } from "../i18n/l10n.mjs";

export function PetOverlay({ close }: { close: () => void }) {
  const { active, dock, pets, size, loaded } = usePet();
  const words = switchWords(active);
  const moved = petMoved(dock);
  return (
    <div className="flex min-w-0 flex-col gap-3" role="group" aria-label={titleWords(active, pets)}>
      <p className="px-1 text-xs font-medium text-text">{titleWords(active, pets)}</p>
      <Switch checked={!!active} onChange={togglePet} label={words.label} hint={words.hint} disabled={!loaded || pets.length === 0} />
      <Slider
        label={t("shell-pet-overlay-size")}
        value={size}
        min={PET_SIZE_MIN}
        max={PET_SIZE_MAX}
        step={PET_SIZE_STEP}
        onChange={setPetSize}
        format={(v) => `${v}%`}
        hint={sizeWords(size)}
        disabled={!active}
      />
      {/* The nine, and your own: a small tile each; choosing one shows it. */}
      <div className="grid max-h-64 grid-cols-3 gap-2 overflow-y-auto px-1 pb-1">
        {pets.map((pet) => (
          <PetTile key={pet.id} pet={pet} active={active === pet.id} size="sm" onSelect={() => setActivePet(pet.id)} />
        ))}
      </div>
      <div className="flex items-center justify-between gap-2 px-1">
        <span className="text-2xs text-text-dim">{positionWords(moved)}</span>
        <span className="flex items-center gap-1">
          <Button size="sm" variant="ghost" disabled={!moved} onClick={resetPetPosition}>{t("shell-pet-overlay-reset-position")}</Button>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              navigate({ name: "settings" }, settingsSearch("pet"));
              close();
            }}
          >{settingsPath("pet")}</Button>
        </span>
      </div>
      <p className="max-w-measure px-1 text-2xs leading-relaxed text-text-dim">{t("shell-pet-overlay-drag-the-pet-anywhere")}</p>
    </div>
  );
}
