import { strict as assert } from "node:assert";
import { test } from "node:test";

import { BASE_PX, SPECIMEN, familyCards, isDefaultScale, sampleAttributes, typeSpecimen } from "./appearanceModel.mjs";

test("the specimen's pixels are the stylesheet's steps times the scale, and the percentage a person reads", () => {
  assert.deepEqual(typeSpecimen(1), { percent: 100, body: 14, secondary: 12, meta: 11 });
  assert.deepEqual(typeSpecimen(1.5), { percent: 150, body: 21, secondary: 18, meta: 16.5 });
  assert.deepEqual(typeSpecimen(0.9), { percent: 90, body: 12.6, secondary: 10.8, meta: 9.9 });
  assert.deepEqual(typeSpecimen(Number.NaN), typeSpecimen(1), "an unreadable scale is the default");
  assert.equal(BASE_PX.body, 14, "the body is 14px at scale 1 — the number the docs promise");
});

test("the default scale is recognised through the float a slider hands back", () => {
  assert.equal(isDefaultScale(1), true);
  assert.equal(isDefaultScale(1.0000001), true);
  assert.equal(isDefaultScale(1.05), false);
  assert.equal(isDefaultScale(1.2, 1.2), true);
  assert.equal(isDefaultScale("x"), false);
});

test("a sample stamps what the DOM would carry: absent for the defaults, the scheme always", () => {
  assert.deepEqual(sampleAttributes({ theme: "system", scheme: "dark", accent: "amber" }), {
    "data-theme": null,
    "data-scheme": "dark",
    "data-accent": null,
  });
  assert.deepEqual(sampleAttributes({ theme: "dune-dark", scheme: "dark", accent: "teal" }), {
    "data-theme": "dune-dark",
    "data-scheme": "dark",
    "data-accent": "teal",
  });
  assert.equal(sampleAttributes({ scheme: "sideways" })["data-scheme"], "light", "an unknown side is the light one");
});

test("the cards are System first with the default family's halves, then one card per family with two tiles", () => {
  const families = [
    { id: "harbor", label: "Harbor", mood: "cool", light: "harbor", dark: "harbor-dark" },
    { id: "dune", label: "Dune", mood: "warm", light: "dune", dark: "dune-dark" },
  ];
  const cards = familyCards(families, families[0], "dune-dark");
  assert.deepEqual(
    cards.map((c) => c.id),
    ["system", "harbor", "dune"],
  );
  assert.deepEqual(cards[0].halves, { light: "harbor", dark: "harbor-dark" });
  assert.equal(cards[0].tiles.length, 1);
  assert.equal(cards[0].tiles[0].active, false);
  assert.match(cards[0].mood, /Harbor/);
  assert.equal(cards[2].halves, null);
  assert.deepEqual(
    cards[2].tiles.map((t) => [t.id, t.active]),
    [
      ["dune", false],
      ["dune-dark", true],
    ],
  );
  assert.equal(familyCards(families, families[0], "system")[0].tiles[0].active, true);
});

test("the specimens carry the glyphs that tell faces apart", () => {
  assert.match(SPECIMEN.mono, /Il1\|/, "capital I, lowercase l, one and a bar");
  assert.match(SPECIMEN.mono, /O0o/, "capital O, zero, lowercase o");
  assert.match(SPECIMEN.ui, /\d/, "numerals, for the tabular question");
  assert.ok(SPECIMEN.body.length > 80, "a body specimen has to wrap to show a line height");
});
