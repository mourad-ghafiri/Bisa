/**
 * Before any module says a word: the language the last session chose (the
 * `bisa.locale` mirror) or the machine's, negotiated against what ships, its
 * catalog installed synchronously from the bundle and the document stamped.
 * This module runs as `main.tsx`'s first import, so a table a model builds
 * at import time is in the right language; `bootLocale()` is the same work
 * as a call, for the guard that holds it before the first render. Nothing
 * here waits.
 */

import { log } from "../log";
import { webStorage } from "../shell/storedPrefModel.mjs";
import { sourcesFor } from "./catalog";
import { install, locale as installed } from "./l10n.mjs";
import { negotiate, readStoredLocale } from "./localeModel.mjs";
import { installMissingLog, stamp } from "./localeStore";

let booted = false;

export function bootLocale(): void {
  if (booted) return;
  booted = true;
  installMissingLog();
  const wanted = readStoredLocale(webStorage()) ?? negotiate([...navigator.languages]);
  const errors = install(wanted, sourcesFor(wanted));
  if (errors.length > 0) log.warn("i18n", "the catalog has messages that do not parse", { locale: installed(), count: errors.length, first: String(errors[0]) });
  stamp(installed());
}

bootLocale();
