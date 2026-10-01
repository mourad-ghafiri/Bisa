/**
 * The palette's address row (ide/18): what was typed, when it reads as a
 * page — `example.com`, `localhost:5173/admin`, `https://…` — becomes one
 * row that opens it in the embedded browser where the person is. A word
 * that is not an address is left to the search. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { normalizeUrl, urlWords } from "./browsersModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * The row for what was typed, or `null` when it is not an address: a bare
 * word with no dot, no port and no scheme is a search, not a host.
 * @param {string} typed
 * @returns {{url: string, label: string, hint: string} | null}
 */
export function urlRow(typed) {
  const raw = String(typed ?? "").trim();
  if (!raw || /\s/.test(raw)) return null;
  const explicit = /^https?:\/\//i.test(raw);
  const hostLike = /^[a-zA-Z0-9.-]+(?::\d{1,5})?(?:\/|$)/.test(raw) && (raw.includes(".") || /^(localhost|127\.0\.0\.1|\[::1\])(?::\d+)?(?:\/|$)/.test(raw));
  if (!explicit && !hostLike) return null;
  const norm = normalizeUrl(raw);
  if ("error" in norm) return null;
  return { url: norm.url, label: t("shell-omnibox-url-open-in-browser", { url: urlWords(norm.url) }), hint: t("shell-omnibox-url-browser-tab-beside-screen-ide-s") };
}
