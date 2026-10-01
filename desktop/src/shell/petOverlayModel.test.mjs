/**
 * The Pet read-out's words. Run with `node --test desktop/src/shell/petOverlayModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { positionWords, sizeWords, switchWords, titleWords } from "./petOverlayModel.mjs";

const pets = [
  { id: "bisa-pets.midnight-shipping.moonrice", displayName: "Moonrice" },
  { id: "bisa-pets.midnight-shipping.jolt", displayName: "Jolt" },
];

test("the title names the pet shown, or says it is put away", () => {
  assert.equal(titleWords(null, pets), "Pet · put away");
  assert.equal(titleWords("bisa-pets.midnight-shipping.jolt", pets), "Pet · Jolt");
  assert.equal(titleWords("gone", pets), "Pet · a pet", "a pet the list lost is still a pet");
});

test("the switch reads by its state and names the default", () => {
  assert.equal(switchWords(null).label, "Show the pet");
  assert.ok(switchWords(null).hint.includes("Moonrice"));
  assert.ok(switchWords("x").hint.startsWith("On."));
});

test("the size and the position are said in words", () => {
  assert.equal(sizeWords(100), "104 px tall — the pet on screen is the preview.");
  assert.equal(sizeWords(50), "52 px tall — the pet on screen is the preview.");
  assert.equal(positionWords(true), "Dragged from its corner.");
  assert.equal(positionWords(false), "In its corner.");
});
