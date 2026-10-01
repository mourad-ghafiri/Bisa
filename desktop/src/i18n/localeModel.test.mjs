/**
 * Which language: negotiation falls back to English, the setting's word
 * resolves, the mirror reads only a shipped tag, direction by script.
 * Run with `node --test desktop/src/i18n/localeModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { AVAILABLE, DEFAULT_LOCALE, LANGUAGE_CHOICES, LANGUAGE_SETTING, LOCALE_KEY, isShipped, negotiate, readStoredLocale, resolveChoice, textDirection } from "./localeModel.mjs";

test("what is shipped is English alone, and the choices are system and each shipped language", () => {
  assert.deepEqual([...AVAILABLE], ["en"]);
  assert.equal(DEFAULT_LOCALE, "en");
  assert.deepEqual([...LANGUAGE_CHOICES], ["system", "en"]);
  assert.equal(LANGUAGE_SETTING, "appearance.language");
  assert.equal(LOCALE_KEY, "bisa.locale");
});

test("negotiation takes the best shipped language and falls back to English", () => {
  assert.equal(negotiate(["fr-FR", "en-GB"]), "en");
  assert.equal(negotiate(["fr-FR"]), "en", "a language not shipped is English");
  assert.equal(negotiate([]), "en");
  assert.equal(negotiate(null), "en");
  assert.equal(negotiate(["en-US"]), "en");
});

test("the setting's word resolves: system negotiates the machine's, a shipped tag stands, junk is the machine's", () => {
  assert.equal(resolveChoice("system", ["fr"]), "en");
  assert.equal(resolveChoice("en", ["fr"]), "en");
  assert.equal(resolveChoice("xx", ["fr"]), "en");
  assert.equal(resolveChoice(undefined, null), "en");
});

test("the mirror reads only a shipped tag", () => {
  const storage = (v) => ({ getItem: () => v });
  assert.equal(readStoredLocale(storage("en")), "en");
  assert.equal(readStoredLocale(storage("fr")), null);
  assert.equal(readStoredLocale(storage(null)), null);
  assert.equal(readStoredLocale(null), null);
  assert.ok(isShipped("en") && !isShipped("EN") && !isShipped(3));
});

test("direction follows the primary subtag", () => {
  assert.equal(textDirection("en"), "ltr");
  assert.equal(textDirection("ar-EG"), "rtl");
  assert.equal(textDirection("he"), "rtl");
  assert.equal(textDirection(null), "ltr");
});

test("the setting is registered with the same choices, machine scope", () => {
  const registry = readFileSync(new URL("../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  const at = registry.indexOf(`"${LANGUAGE_SETTING}",`);
  assert.ok(at >= 0, "appearance.language is registered");
  const def = registry.slice(at, at + 400);
  assert.ok(def.includes(`Choice(&[${LANGUAGE_CHOICES.map((c) => `"${c}"`).join(", ")}])`), "the same words, in the same order");
  assert.ok(def.includes('json!("system")') && /S::M\s*\)/.test(def), "system by default, this machine's");
});
