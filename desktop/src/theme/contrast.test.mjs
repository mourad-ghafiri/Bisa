import test from "node:test";
import assert from "node:assert/strict";
import { composite, contrast, deltaL, hueDistance, luminance, oklchToLinearSrgb, parseOklch, toHex } from "./contrast.mjs";

test("an oklch token parses with or without alpha and percentages; anything else is not a theme value", () => {
  assert.deepEqual(parseOklch("oklch(0.5 0.1 200)"), { l: 0.5, c: 0.1, h: 200, alpha: 1 });
  assert.deepEqual(parseOklch("oklch(50% 0.1 200 / 0.4)"), { l: 0.5, c: 0.1, h: 200, alpha: 0.4 });
  assert.deepEqual(parseOklch("  oklch(0 0 0 / 55%) "), { l: 0, c: 0, h: 0, alpha: 0.55 });
  assert.equal(parseOklch("#ffffff"), null);
  assert.equal(parseOklch("rgb(1 2 3)"), null);
  assert.equal(parseOklch(""), null);
});

test("the ends of the lightness axis are black and white, and the middle is grey", () => {
  assert.deepEqual(oklchToLinearSrgb({ l: 0, c: 0, h: 0 }), { r: 0, g: 0, b: 0 });
  const white = oklchToLinearSrgb({ l: 1, c: 0, h: 0 });
  for (const v of Object.values(white)) assert.ok(v > 0.99, `white: ${v}`);
  const grey = oklchToLinearSrgb({ l: 0.6, c: 0, h: 0 });
  assert.ok(Math.abs(grey.r - grey.g) < 0.01 && Math.abs(grey.g - grey.b) < 0.01, "achromatic stays achromatic");
  assert.ok(luminance("oklch(1 0 0)") > 0.99);
  assert.equal(luminance("oklch(0 0 0)"), 0);
  assert.equal(luminance("not a colour"), 0);
  assert.equal(toHex("oklch(1 0 0)"), "#ffffff");
  assert.equal(toHex("oklch(0 0 0)"), "#000000");
  assert.equal(toHex("nope"), null);
});

test("contrast is WCAG's ratio: 21 for black on white, 1 for a colour on itself, symmetric", () => {
  assert.ok(Math.abs(contrast("oklch(0 0 0)", "oklch(1 0 0)") - 21) < 0.05);
  assert.equal(contrast("oklch(0.5 0 0)", "oklch(0.5 0 0)"), 1);
  assert.equal(contrast("oklch(0.2 0 0)", "oklch(0.9 0 0)"), contrast("oklch(0.9 0 0)", "oklch(0.2 0 0)"));
  assert.ok(contrast("oklch(0.22 0 0)", "oklch(0.975 0 0)") > 12, "dark text on an off-white page");
  assert.ok(contrast("oklch(0.72 0 0)", "oklch(0.2 0 0)") > 4.5, "dim text on a dark page still reads");
});

test("lightness steps and hue distances are read straight off the tokens", () => {
  assert.ok(Math.abs(deltaL("oklch(0.975 0 0)", "oklch(0.945 0 0)") - 0.03) < 1e-9);
  assert.equal(hueDistance("oklch(0.5 0.1 350)", "oklch(0.5 0.1 25)"), 35, "across zero");
  assert.equal(hueDistance("oklch(0.5 0.1 58)", "oklch(0.5 0.1 22)"), 36);
  assert.equal(hueDistance("oklch(0.5 0.1 10)", "oklch(0.5 0.1 190)"), 180);
  assert.equal(deltaL("x", "oklch(0.5 0 0)"), 0);
});

test("an opaque colour composites to itself, and a translucent one lands between itself and its ground in linear light", () => {
  assert.equal(composite("oklch(0.5 0.1 200)", "oklch(1 0 0)"), "oklch(0.5 0.1 200)");
  assert.equal(composite("nope", "oklch(1 0 0)"), "nope");
  assert.equal(composite("oklch(0.5 0 0)", "nope"), "oklch(0.5 0 0)");
  const half = composite("oklch(0 0 0 / 0.5)", "oklch(1 0 0)");
  const lum = luminance(half);
  assert.ok(Math.abs(lum - 0.5) < 0.01, `half black over white is mid-grey in linear light: ${lum}`);
  const onBlack = composite("oklch(0 0 0 / 0.5)", "oklch(0 0 0)");
  assert.equal(luminance(onBlack), 0, "black over black stays black");
  const nearlyThere = composite("oklch(0.9 0.02 240 / 0.9)", "oklch(0.2 0 0)");
  assert.ok(luminance(nearlyThere) < luminance("oklch(0.9 0.02 240)"), "a dark ground shows through a light pane");
  assert.ok(parseOklch(nearlyThere), "the composite is a theme value again");
  const hue = parseOklch(composite("oklch(0.7 0.15 240 / 0.6)", "oklch(0.7 0.15 240)"));
  assert.ok(Math.abs(hue.h - 240) < 1 && Math.abs(hue.c - 0.15) < 0.01, "a colour over itself keeps its hue and chroma");
});
