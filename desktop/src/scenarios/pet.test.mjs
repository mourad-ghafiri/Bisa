/**
 * The pet as the sources show it: nine ship with the platform and the store
 * bundles every folder; the footer's pet icon opens an overlay like the
 * resource read-outs, with a switch, a slider and the pets as tiles; the
 * panel draws tiles; Moonrice is the default; the sprite plays the
 * manifest's plan. Source assertions, as `conversations.test.mjs` makes them
 * — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/pet.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("the nine pets in library/pets are the nine the store bundles", () => {
  const dir = new URL("../../../library/pets/", import.meta.url);
  const folders = readdirSync(dir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name)
    .sort();
  assert.deepEqual(folders, ["bracket", "buffer", "fathom", "jolt", "kernel", "loop", "lumen", "moonrice", "nocturn"]);
  for (const f of folders) {
    assert.ok(existsSync(new URL(`${f}/pet.json`, dir)), `${f} has a manifest`);
    assert.ok(existsSync(new URL(`${f}/spritesheet.webp`, dir)), `${f} has a sheet`);
    const manifest = JSON.parse(readFileSync(new URL(`${f}/pet.json`, dir), "utf8"));
    assert.equal(manifest.id, `bisa-pets.midnight-shipping.${f}`, `${f}'s id ends in its folder`);
  }
  const catalog = src("../../../crates/bisa-store/src/catalog.rs");
  const bundled = [...catalog.matchAll(/pet!\("([a-z]+)"\)/g)].map((m) => m[1]).sort();
  assert.deepEqual(bundled, folders, "every folder is a row of CATALOG_PETS, and nothing else is");
});

test("the footer's pet icon opens an overlay, not a bare toggle", () => {
  const bar = src("../shell/StatusBar.tsx");
  assert.ok(bar.includes("<PetStat />"), "the read-out is mounted");
  assert.ok(!bar.includes("icon={ICON.pet}"), "no toggle for the pet remains");
  const stat = src("../shell/PetStat.tsx");
  assert.ok(stat.includes("<Popover") && stat.includes('side="top"'), "a popover above the footer, as the resource read-outs are");
  assert.ok(stat.includes("open && <PetOverlay"), "content mounted only while open");
  const overlay = src("../shell/PetOverlay.tsx");
  for (const piece of ["<Switch", "<Slider", "<PetTile", "resetPetPosition", 'settingsSearch("pet")']) {
    assert.ok(overlay.includes(piece), `the overlay has ${piece}`);
  }
  assert.ok(overlay.includes("togglePet"), "the switch speaks the store's verb");
});

test("the panel draws tiles, the default is Moonrice, and the sprite plays the manifest's plan", () => {
  const panel = src("../views/_settings/PetPanel.tsx");
  assert.ok(panel.includes("<PetTile") && panel.includes("grid gap-3 sm:grid-cols-2 lg:grid-cols-3"), "tiles in a grid");
  assert.ok(!panel.includes("<EmptyState"), "nine always exist: no empty state");
  assert.ok(panel.includes("togglePet"), "one verb to show the pet");
  const tile = src("../pet/PetTile.tsx");
  assert.ok(tile.includes("<Tile") && tile.includes("active={active}"), "on the kit's Tile — the frame the theme tiles wear too");
  assert.ok(src("../ui/Tile.tsx").includes("aria-pressed") && src("../ui/Tile.tsx").includes("ring-2 ring-accent/40"), "the selected ring is written once, in the kit");
  assert.ok(tile.includes("tileWords(pet)") && tile.includes("originWords(pet.origin)"), "the words are the model's");
  const model = src("../pet/petModel.mjs");
  assert.ok(model.includes('DEFAULT_PET_ID = "bisa-pets.midnight-shipping.moonrice"'));
  const store = src("../pet/petStore.ts");
  assert.ok(store.includes("defaultPet(state.pets, state.lastActive)"), "turning the pet on shows the last one, else Moonrice");
  const sprite = src("../pet/PetSprite.tsx");
  assert.ok(sprite.includes("framePlan(def, state,"), "the frames and their durations are the plan's");
  assert.ok(!sprite.includes("setInterval("), "one timer per frame, each its own length");
  assert.ok(src("../App.tsx").includes("<PetCompanion />"), "the companion floats over the app");
});
