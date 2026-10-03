/**
 * The identicon's ground holds white initials to the app's 4.5:1 in every
 * hue. Run with `node --test desktop/src/ui/avatarModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { AVATAR_CHROMA, AVATAR_HUES, AVATAR_LIGHTNESS, principalColor, principalHue } from "./avatarModel.mjs";

/** OKLCH to linear sRGB (Björn Ottosson's matrices), clamped to the gamut. */
function linearRgb(L, C, h) {
  const a = C * Math.cos((h * Math.PI) / 180);
  const b = C * Math.sin((h * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const clamp = (x) => Math.min(1, Math.max(0, x));
  return [
    clamp(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    clamp(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    clamp(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

const whiteOn = (L, C, h) => {
  const [r, g, b] = linearRgb(L, C, h);
  return 1.05 / (0.2126 * r + 0.7152 * g + 0.0722 * b + 0.05);
};

test("white initials read at 4.5:1 or better on every hue's ground", () => {
  for (const hue of AVATAR_HUES) {
    const ratio = whiteOn(AVATAR_LIGHTNESS, AVATAR_CHROMA, hue);
    assert.ok(ratio >= 4.5, `hue ${hue}: ${ratio.toFixed(2)}:1`);
  }
  assert.ok(whiteOn(0.62, AVATAR_CHROMA, 130) < 4.5, "the old ground failed: the measure is real");
});

test("an id keeps its colour, and the ground is the model's", () => {
  assert.equal(principalColor("ab12"), principalColor("ab12"));
  assert.ok(AVATAR_HUES.includes(principalHue("ab12")));
  assert.equal(principalColor("ab12"), `oklch(${AVATAR_LIGHTNESS} ${AVATAR_CHROMA} ${principalHue("ab12")})`);
  assert.ok(AVATAR_HUES.every((h) => h < 270 || h > 330), "no violet");
  const avatar = readFileSync(new URL("./Avatar.tsx", import.meta.url), "utf8");
  assert.ok(avatar.includes('from "./avatarModel.mjs"') && !avatar.includes("oklch("), "the component draws the model's ground");
});
