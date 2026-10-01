/**
 * Which language the desktop speaks (17 — Internationalisation): the
 * languages shipped, the setting behind the choice, the negotiation of a
 * machine's languages against them, the stored mirror the first paint reads,
 * and a language's writing direction. Plain `.mjs`, so `node --test` reads it.
 *
 * `AVAILABLE` is spelt here and in `crates/bisa-i18n/src/locale.rs`; a guard
 * (`crates/bisa-i18n/tests/it/catalog.rs`) holds the two lists equal.
 */

import { negotiateLanguages } from "@fluent/langneg";
import { readPref } from "../shell/storedPrefModel.mjs";

/** The languages a catalog ships for, best first. */
export const AVAILABLE = Object.freeze(["en"]);

/** The language everything falls back to. */
export const DEFAULT_LOCALE = "en";

/** The machine setting: `system`, or one of `AVAILABLE`. */
export const LANGUAGE_SETTING = "appearance.language";

/** The setting's words, in the panel's order. */
export const LANGUAGE_CHOICES = Object.freeze(["system", ...AVAILABLE]);

/** The setting's mirror in this window's storage, for the stamp before the first paint (`index.html`). */
export const LOCALE_KEY = "bisa.locale";

/** The primary language subtags written right to left. */
const RTL = Object.freeze(["ar", "he", "fa", "ur", "ps", "yi", "dv", "ckb", "syr", "ug"]);

/**
 * The best shipped language for what the machine asks for, in order; nothing
 * acceptable is the default.
 * @param {readonly string[] | null | undefined} requested e.g. `navigator.languages`
 */
export function negotiate(requested) {
  const asked = Array.isArray(requested) ? requested.filter((s) => typeof s === "string" && s.length > 0) : [];
  const [best] = negotiateLanguages(asked, [...AVAILABLE], { defaultLocale: DEFAULT_LOCALE, strategy: "lookup" });
  return best ?? DEFAULT_LOCALE;
}

/**
 * The language for a setting's value: `system` negotiates the machine's;
 * a shipped tag is itself; anything else is the machine's too.
 * @param {unknown} choice the resolved `appearance.language`
 * @param {readonly string[] | null | undefined} systemLanguages
 */
export function resolveChoice(choice, systemLanguages) {
  if (typeof choice === "string" && choice !== "system" && AVAILABLE.includes(choice)) return choice;
  return negotiate(systemLanguages);
}

/** Whether a stored word is a shipped language, for the mirror. @param {unknown} raw */
export function isShipped(raw) {
  return typeof raw === "string" && AVAILABLE.includes(raw);
}

/**
 * The mirror's word, or `null` — never a language that is not shipped.
 * @param {Storage | null | undefined} storage
 */
export function readStoredLocale(storage) {
  return readPref(storage, LOCALE_KEY, (raw) => (isShipped(raw) ? raw : null), null);
}

/**
 * The document's writing direction for a language.
 * @param {string | null | undefined} locale
 * @returns {"ltr" | "rtl"}
 */
export function textDirection(locale) {
  const primary = String(locale ?? "")
    .toLowerCase()
    .split("-")[0];
  return RTL.includes(primary) ? "rtl" : "ltr";
}
