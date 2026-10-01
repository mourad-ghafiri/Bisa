import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { PUBLISH_LABEL, PUBLISH_MEANING, PUBLISH_POLICIES, PUBLISH_TONE, publishOf } from "./publishPolicy.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");

/** snake_case variants of a Rust enum, read from the source (recipe 7). */
function variantsOf(file, name) {
  const src = readFileSync(join(CORE, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) =>
    m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase(),
  );
}

const ROLES = readFileSync(join(HERE, "../../theme/tokens.css"), "utf8");
const CHIP = readFileSync(join(HERE, "../../ui/Chip.tsx"), "utf8");
const CHIP_TONES = [...CHIP.match(/export type Tone = ([^;]+);/)[1].matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
/** A tone is a kit chip tone; each of those names a theme role in `Chip.tsx`. */
const isRole = (t) => CHIP_TONES.includes(t) || ROLES.includes(`--color-${t}`) || ROLES.includes(`--${t}`);

test("every core publish policy has a label, a meaning and a theme-role tone", () => {
  const policies = variantsOf("project.rs", "PublishPolicy");
  assert.deepEqual([...PUBLISH_POLICIES].sort(), policies.sort());
  for (const p of policies) {
    assert.ok(PUBLISH_LABEL[p], `${p} has a label`);
    assert.ok(PUBLISH_MEANING[p].length > 20, `${p} says what it means`);
    assert.ok(isRole(PUBLISH_TONE[p]), `${p} paints a chip tone or a role`);
  }
});

test("a project with no policy publishes auto, the core's default", () => {
  assert.equal(publishOf({ publish: null }), "auto");
  assert.equal(publishOf({}), "auto");
  assert.equal(publishOf(null), "auto");
  assert.equal(publishOf({ publish: "gated" }), "gated");
  assert.equal(publishOf({ publish: "nonsense" }), "auto");
});
