import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { CHIP_SIZE, TONE_CLASS, TONE_FILL, chipClasses } from "./stepChipTones.mjs";
import { STEP_TONE_TOKENS } from "../_workflow/runView.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const TOKENS = readFileSync(join(HERE, "../../theme/tokens.css"), "utf8");

/** A utility class names a role when the token sheet declares that colour. */
function namesARole(cls) {
  const role = cls.replace(/^(bg|text|border)-/, "");
  return role === "surface" || role === "surface-2" || role === "border" || TOKENS.includes(`--color-${role}`) || TOKENS.includes(`--${role}`);
}

test("every step-state role has a border tone and a fill, and every class names a theme role", () => {
  const roles = new Set(Object.values(STEP_TONE_TOKENS));
  for (const r of roles) {
    assert.ok(TONE_CLASS[r], `${r} has a tone`);
    assert.ok(TONE_FILL[r], `${r} has a fill`);
  }
  for (const cls of [...Object.values(TONE_CLASS), ...Object.values(TONE_FILL)].flatMap((c) => c.split(" "))) {
    assert.ok(namesARole(cls), `${cls} names a role`);
  }
});

test("an unknown role paints as dim rather than unstyled", () => {
  assert.equal(chipClasses("nonsense"), chipClasses("text-dim"));
  assert.match(chipClasses("ok"), /bg-ok-soft/);
});

test("the three sizes grow monotonically and the current chip is never smaller than its neighbours", () => {
  const px = (cls) => Number(cls.replace("size-", ""));
  assert.ok(px(CHIP_SIZE.compact.chip) < px(CHIP_SIZE.md.chip) && px(CHIP_SIZE.md.chip) < px(CHIP_SIZE.lg.chip));
  for (const s of Object.values(CHIP_SIZE)) assert.ok(px(s.current) >= px(s.chip));
});
