/**
 * The English catalog for a test: every `locales/en/**.ftl` read from disk
 * (the CLI's file left out — the webview never speaks it) and installed, so a
 * model's words are the catalog's under `node --test` exactly as in the
 * window. `preload.mjs` calls this for `npm test`; a single test file may
 * call it itself.
 */

import { readdirSync, readFileSync } from "node:fs";
import { install } from "./l10n.mjs";

const EN = new URL("../../../locales/en/", import.meta.url);

/** The webview reads every namespace but the CLI's. */
const NOT_THE_WEBVIEW = new Set(["cli.ftl"]);

/**
 * Every `.ftl` under a folder, recursively, as `[path, text]`.
 * @param {URL} dir
 * @returns {[string, string][]}
 */
export function englishFiles(dir = EN) {
  /** @type {[string, string][]} */
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) out.push(...englishFiles(new URL(`${entry.name}/`, dir)));
    else if (entry.name.endsWith(".ftl") && !NOT_THE_WEBVIEW.has(entry.name)) {
      const url = new URL(entry.name, dir);
      out.push([decodeURIComponent(url.pathname), readFileSync(url, "utf8")]);
    }
  }
  return out.sort(([a], [b]) => a.localeCompare(b));
}

/** Install English from disk; answers the syntax errors, which a test asserts are none. */
export function installEnglish() {
  return install(
    "en",
    englishFiles().map(([, text]) => text),
  );
}
