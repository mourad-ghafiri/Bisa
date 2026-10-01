/**
 * The language the window speaks, followed (17 — Internationalisation): the
 * catalog is installed once, before any module says a word (`boot.ts`), and
 * the window speaks that language until it is opened again. `setLocale`
 * mirrors a new tag for the next first paint (`bisa.locale`, read by
 * `index.html`'s inline script and by `boot.ts`) and reloads the window
 * once — a word a model froze into a table at import time is then said again
 * in the new language, like every other. `syncLocaleFromNode` follows the
 * `appearance.language` setting, at boot and on `settings_changed`.
 */

import { api } from "../api";
import { errorFields, log } from "../log";
import { choiceOf } from "../shell/settingsModel.mjs";
import { webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { flushMemories } from "../shell/viewMemoryStore";
import { locale as installed, onMissing } from "./l10n.mjs";
import { LANGUAGE_CHOICES, LANGUAGE_SETTING, LOCALE_KEY, resolveChoice, textDirection } from "./localeModel.mjs";

/** `<html lang dir>`: what the browser shapes text and controls by. */
export function stamp(locale: string): void {
  const root = document.documentElement;
  root.lang = locale;
  root.dir = textDirection(locale);
}

/** A message the catalog lacked: one line in the log per id, with the id that is on screen instead. */
export function installMissingLog(): void {
  onMissing((id) => log.warn("i18n", "a message the catalog lacks; its id is shown", { id }));
}

/** Mirror a language and open the window again in it; the same language again is nothing. */
function setLocale(locale: string): void {
  if (locale === installed()) return;
  writePref(webStorage(), LOCALE_KEY, locale);
  log.info("i18n", "the language changed; the window opens again in it", { from: installed(), to: locale });
  // The window that opens again finds every screen as this one left it.
  flushMemories();
  window.location.reload();
}

/** The `appearance.language` setting, read from the node and applied. */
export async function syncLocaleFromNode(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    const choice = choiceOf(settings, LANGUAGE_SETTING, LANGUAGE_CHOICES, "system");
    setLocale(resolveChoice(choice, navigator.languages));
  } catch (e) {
    log.debug("i18n", "the language setting could not be read; the language stands", errorFields(e));
  }
}
