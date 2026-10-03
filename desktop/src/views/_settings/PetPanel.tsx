/**
 * Choosing a pet, sizing it, and getting your own in.
 *
 * # Why this is not part of Appearance
 *
 * It started there, on the reasoning that a pet changes how the app looks and
 * nothing about what it does. That was wrong about what Appearance *is*.
 * Appearance is four dials over the whole surface — palette, accent, density,
 * text size — that compose, and every one of them applies to every pixel. A
 * pet is a discrete object you choose between, drag around, size and — your
 * own — import and remove. Filing it under Appearance buried a feature nobody
 * would think to look for behind the text-size control.
 *
 * # Nine ship with the platform
 *
 * The *midnight-shipping* pack — Bracket, Buffer, Fathom, Jolt, Kernel, Loop,
 * Lumen, Moonrice, Nocturn — is in the binary, drawn against the sheet's nine
 * states with each pet's own pace, so the panel never opens empty: the tiles
 * are the offer, Moonrice is the one shown when the pet is turned on with
 * none chosen, and *Import a folder* is for a pack of your own in Codex's
 * format. A built-in has no *Remove*: it ships, it is put away.
 */

import { useEffect, useState } from "react";
import { ApiError, api, inDesktopShell, pickFolder } from "../../api";
import { Button, Card, Pending, Section, Slider, Switch } from "../../ui";
import { PetTile } from "../../pet/PetTile";
import { pendingRows } from "./loadModel.mjs";
import { positionWords, sizeWords, switchWords } from "../../shell/petOverlayModel.mjs";
import {
  petMoved,
  refreshPets,
  resetPetPosition,
  setActivePet,
  setPetSize,
  togglePet,
  usePet,
} from "../../pet/petStore";
import { PET_SIZE_MAX, PET_SIZE_MIN, PET_SIZE_STEP } from "../../pet/petModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function PetPanel() {
  // The same list the companion and the footer read: one list, one answer.
  const { active, dock, pets, size, loaded } = usePet();
  const moved = petMoved(dock);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const words = switchWords(active);

  useEffect(() => {
    void refreshPets();
  }, []);

  const install = async () => {
    // The picker returns a path; the node reads it. The webview has no
    // filesystem access of its own and deliberately never gains one.
    const path = await pickFolder(t("settings-pet-panel-choose-pet-package"));
    if (!path) return;
    setBusy(true);
    try {
      const { pet } = await api.installPet(path);
      await refreshPets();
      setActivePet(pet.id);
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("settings-pet-panel-folder-pet-package"));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    if (removing) return;
    setRemoving(id);
    try {
      await api.deletePet(id);
      if (active === id) setActivePet(null);
      await refreshPets();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("settings-pet-panel-could-remove-pet"));
    } finally {
      setRemoving(null);
    }
  };

  const builtIn = pets.filter((p) => p.origin === "catalog");
  const mine = pets.filter((p) => p.origin !== "catalog");

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-pet-panel-pet")}>
        <Card>
          <p className="mb-3 max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-pet-panel-blurb", { code: (inner) => <code>{inner}</code> })}</p>
          <Switch checked={!!active} onChange={togglePet} label={words.label} hint={words.hint} disabled={!loaded || pets.length === 0} />
        </Card>
      </Section>

      <Section title={t("settings-pet-panel-which-pet")}>
        <Card>
          {error && <p className="mb-2 text-2xs text-danger">{error}</p>}
          {!loaded ? (
            <Pending what={t("settings-pet-panel-pets")} rows={pendingRows(t("settings-pet-panel-pets"))} />
          ) : (
            <>
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {builtIn.map((pet) => (
                  <PetTile key={pet.id} pet={pet} active={active === pet.id} onSelect={() => setActivePet(pet.id)} />
                ))}
                {mine.map((pet) => (
                  <div key={pet.id} className="flex flex-col gap-1">
                    <PetTile pet={pet} active={active === pet.id} onSelect={() => setActivePet(pet.id)} />
                    {/* Beside the tile, never inside it: a tile is one control. */}
                    <Button size="sm" variant="ghost" className="self-start" disabled={removing !== null} onClick={() => void remove(pet.id)}>
                      {removing === pet.id ? t("settings-pet-panel-removing") : t("settings-git-profiles-panel-remove-2")}
                    </Button>
                  </div>
                ))}
              </div>
              <div className="mt-3 flex items-center justify-between gap-3">
                <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
                  {mine.length === 0 ? t("settings-pet-panel-own-pack-imports-here-codex-s") : t("settings-pet-panel-own-after-nine", { mine: mine.length })}
                </p>
                {inDesktopShell() ? (
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => void install()}>{t("settings-pet-panel-import-folder")}</Button>
                ) : (
                  // The picker is a desktop capability; in a browser session
                  // there is nothing to offer, and saying so beats a dead button.
                  <p className="text-2xs text-text-dim">{t("settings-pet-panel-importing-needs-desktop-app")}</p>
                )}
              </div>
            </>
          )}
        </Card>
      </Section>

      {/* Only once there is something to size. A dial that moves nothing is a
          control the reader has to test to discover is inert. */}
      {active && (
        <Section title={t("settings-logging-panel-size")}>
          <Card>
            <Slider
              label={t("settings-pet-panel-how-big-stands")}
              value={size}
              min={PET_SIZE_MIN}
              max={PET_SIZE_MAX}
              step={PET_SIZE_STEP}
              onChange={setPetSize}
              format={(v) => `${v}%`}
              hint={sizeWords(size)}
            />
          </Card>
        </Section>
      )}

      {/* Anchored to the edges it is nearest, a pet parked top-left stays
          there for good — so the way back to its corner is here, as the notes
          dock's is. */}
      {active && (
        <Section title={t("settings-notes-panel-layout")}>
          <Card>
            <div className="flex items-center justify-between gap-3">
              <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
                {positionWords(moved)} {t("settings-pet-panel-drag-anywhere-keeps-distance-from-edges")}
              </p>
              <Button size="sm" variant="ghost" disabled={!moved} onClick={resetPetPosition}>{t("settings-notes-panel-reset-position")}</Button>
            </div>
          </Card>
        </Section>
      )}
    </div>
  );
}
