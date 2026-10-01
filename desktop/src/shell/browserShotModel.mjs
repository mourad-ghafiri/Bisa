/**
 * A screenshot of a tab (ide/18), as facts: what it is named, how wide it is
 * taken, and what a PNG says about itself. The shell takes it, the node
 * keeps it; this only reads bytes and words. Plain `.mjs`, so `node --test`
 * reads it.
 */

import { t } from "../i18n/l10n.mjs";

/** The setting that says how wide, and the bounds the shell keeps it in. */
export const SHOT_WIDTH_KEY = "browser.screenshot.width";
export const MIN_SHOT_WIDTH = 320;
export const MAX_SHOT_WIDTH = 4096;
export const DEFAULT_SHOT_WIDTH = 1280;

/** The width a screenshot is taken at: the setting's number, within bounds. @param {unknown} value */
export function shotWidth(value) {
  // `Number(null)` is 0 and `Number("")` is 0: an absent setting is the default, never the minimum.
  if (value === null || value === undefined || value === "" || typeof value === "boolean") return DEFAULT_SHOT_WIDTH;
  const n = Number(value);
  if (!Number.isFinite(n)) return DEFAULT_SHOT_WIDTH;
  return Math.min(MAX_SHOT_WIDTH, Math.max(MIN_SHOT_WIDTH, Math.round(n)));
}

/**
 * The file name a screenshot of a tab is kept under, for the person's copy
 * and the upload alike: `browser-<tab>-<stamp>.png`.
 * @param {string} key @param {Date} [at]
 */
export function shotName(key, at = new Date()) {
  const tab = String(key ?? "").replace(/[^a-zA-Z0-9]/g, "").slice(0, 16) || "tab";
  const stamp = at.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  return `browser-${tab}-${stamp}.png`;
}

/**
 * A PNG's pixel size from its header — the IHDR chunk every PNG starts
 * with — or `null` for bytes that are not a PNG.
 * @param {Uint8Array} bytes
 * @returns {{width: number, height: number} | null}
 */
export function pngSize(bytes) {
  if (!bytes || bytes.length < 24) return null;
  const magic = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  for (let i = 0; i < magic.length; i += 1) if (bytes[i] !== magic[i]) return null;
  if (bytes[12] !== 0x49 || bytes[13] !== 0x48 || bytes[14] !== 0x44 || bytes[15] !== 0x52) return null;
  const be = (at) => ((bytes[at] << 24) >>> 0) + (bytes[at + 1] << 16) + (bytes[at + 2] << 8) + bytes[at + 3];
  const width = be(16);
  const height = be(20);
  if (!(width > 0) || !(height > 0)) return null;
  return { width, height };
}

/** The words the camera's toast says. @param {{width: number, height: number}} size */
export function shotWords(size) {
  return t("shell-browser-shot-png", { width: size.width, height: size.height });
}

/** What the person reads when the tab did not show in time for its shot — nothing was asked of a hidden page. */
export const NOT_SHOWN_WORDS = t("shell-browser-shot-page-showing-close-what-over-try");

/** What the person reads when the clipboard would not take the picture. */
export const CLIPBOARD_REFUSED = t("shell-browser-shot-clipboard-refused-picture");
