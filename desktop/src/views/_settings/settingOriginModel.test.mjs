/**
 * A setting's origin badge and the scope a row writes to (ide/13 — *Visible
 * in the UI*). Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_settings/settingOriginModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { sourceFiles } from "../../testWalk.mjs";
import { SETTINGS_SCOPES, defaultTarget, mayReset, originBadge, scopeWord, setAtWords, shadowWords, shadowedBy, writableScopes } from "./settingOriginModel.mjs";

const here = new URL(".", import.meta.url);
const read = (name) => readFileSync(new URL(name, here), "utf8");

test("the badge says the scope the value came from: default is quiet, a scope says itself and the sentence behind it", () => {
  assert.deepEqual(originBadge("default"), { word: "default", tone: "quiet", hint: "The compiled default — no scope holds a value." });
  assert.deepEqual(originBadge("machine"), { word: "machine", tone: "neutral", hint: "The value on screen comes from the machine scope." });
  assert.deepEqual(originBadge("workspace"), { word: "workspace", tone: "neutral", hint: "The value on screen comes from the workspace scope." });
  assert.equal(originBadge("project").word, "project");
  assert.equal(scopeWord("galaxy"), "galaxy", "a scope a newer node names is drawn as it came, never as nothing");
});

test("the scope words are the ones a person types and the node names when it refuses a scope", () => {
  // `bisa settings set <scope> …` and the core's refusal both say these words (ide/13 §Resolution).
  const scope = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const word of ["machine", "workspace", "project"]) {
    assert.equal(scopeWord(word), word);
    assert.ok(scope.includes(`"${word}"`), `${word} is the registry's own word for the scope`);
  }
  // The wire's origins, read from the generated types: every one has a badge.
  const types = readFileSync(new URL("../../types.gen.ts", import.meta.url), "utf8");
  const origin = types.match(/export type SettingOrigin = ([^;]+);/);
  assert.ok(origin, "SettingOrigin is declared in types.gen.ts");
  const words = [...origin[1].matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...words].sort(), ["default", "machine", "project", "workspace"]);
  for (const word of words) assert.equal(originBadge(word).word, word);
});

test("Settings writes the workspace and this machine, narrowest first, and never a project's layer", () => {
  assert.deepEqual([...SETTINGS_SCOPES], ["workspace", "machine"]);
  assert.deepEqual(writableScopes({ scopes: ["machine", "workspace", "project"] }), ["workspace", "machine"]);
  assert.deepEqual(writableScopes({ scopes: ["machine"] }), ["machine"]);
  assert.deepEqual(writableScopes({ scopes: ["project"] }), []);
  assert.equal(defaultTarget({ scopes: ["machine", "workspace"] }), "workspace");
  assert.equal(defaultTarget({ scopes: ["machine"] }), "machine");
  assert.equal(setAtWords("machine"), "set at machine");
});

test("Reset removes the value where the row writes; a value held nearer than the row's scope is said to win", () => {
  assert.equal(mayReset("workspace", "workspace"), true);
  assert.equal(mayReset("machine", "workspace"), false, "the workspace holds nothing to remove");
  assert.equal(mayReset("default", "machine"), false);
  assert.equal(shadowedBy("workspace", "machine"), "workspace", "a write at the machine lands under the workspace's value");
  assert.equal(shadowedBy("project", "workspace"), "project");
  assert.equal(shadowedBy("machine", "workspace"), null, "the nearer scope is the one written: the write shows");
  assert.equal(shadowedBy("workspace", "workspace"), null);
  assert.equal(shadowedBy("default", "machine"), null);
  assert.equal(shadowWords("workspace", "machine"), "The workspace scope holds a value, and it wins over the machine one.");
  assert.equal(shadowWords("machine", "machine"), null);
});

test("one badge, drawn by every panel that shows a key's origin — none spells the wire's word or the sentence itself", () => {
  const panels = sourceFiles(new URL(".", import.meta.url).pathname, (p) => p.endsWith(".tsx"));
  const drawing = panels.filter((p) => readFileSync(p, "utf8").includes("<OriginBadge origin={"));
  assert.ok(drawing.length >= 6, `the registry's rows and the hand-written panels: ${drawing.length}`);
  for (const p of panels) {
    if (p.endsWith("OriginBadge.tsx")) continue;
    const text = readFileSync(p, "utf8");
    assert.ok(!text.includes("compiled-default-scope-holds-value") && !text.includes("value-screen-comes-from-scope"), `${p}: the badge's sentence is the model's`);
    assert.ok(!/<Chip[^>]*>\{\w*[oO]rigin\}<\/Chip>/.test(text), `${p}: no chip draws the wire's origin word`);
  }
  assert.ok(read("OriginBadge.tsx").includes("originBadge(origin)"));
  const row = read("RegistryPanel.tsx");
  assert.ok(row.includes("writableScopes(def)") && row.includes("mayReset(resolved.origin, target)") && row.includes("shadowWords(resolved.origin, target)") && row.includes("{scopeWord(s)}") && row.includes("{setAtWords(target)}"), "the registry row decides nothing itself");
});
