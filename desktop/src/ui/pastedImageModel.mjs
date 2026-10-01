/**
 * A picture pasted from the clipboard, and the name it gets (guide/the-
 * desktop.md §Conversations, ide/03 §The explorer): the suggested name for
 * a picture with no file behind it — the web engine calls one `image.png`,
 * which says nothing — the extension a typed name keeps, the rule a name
 * must pass, and how a paste event's contents sort into what is named,
 * what is taken as it is, and when the shell is asked. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { renameError, splitExt } from "./fileTreeMutations.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * The name a clipboard picture is offered under: `pasted-image-<stamp>.png`,
 * the stamp as the browser's snapshot uses (`shotName`), so two pastes in
 * one second still differ by the second they were made.
 * @param {Date} [at]
 * @returns {string}
 */
export function pastedImageName(at = new Date()) {
  const stamp = at.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  return `pasted-image-${stamp}.png`;
}

/**
 * The name a pasted picture is offered under: its own, when it has a file
 * behind it (a copy made in the file manager), else the stamped name — the
 * web engine calls a picture with no file behind it `image.png` (or
 * `image.jpeg`…), a placeholder, not a name.
 * @param {string} name the file's name as the paste event gave it
 * @param {Date} [at]
 * @returns {string}
 */
export function suggestedName(name, at = new Date()) {
  const own = String(name ?? "").trim();
  const [stem] = splitExt(own);
  if (!own || stem.toLowerCase() === "image") {
    const ext = extensionOf(own) || "png";
    return pastedImageName(at).replace(/\.png$/, `.${ext}`);
  }
  return own;
}

/**
 * The extension of a file's name, without the dot and in lower case; `""`
 * for none (a dot-file has none).
 * @param {string} name
 * @returns {string}
 */
export function extensionOf(name) {
  const [, ext] = splitExt(String(name ?? ""));
  return ext.slice(1).toLowerCase();
}

/**
 * The name the person typed, trimmed, given `ext` when it has no extension
 * of its own: `login-bug` becomes `login-bug.png`; `login-bug.jpeg` stays.
 * @param {string} value
 * @param {string} ext without the dot
 * @returns {string}
 */
export function withExtension(value, ext) {
  const v = String(value ?? "").trim();
  if (!v || !ext) return v;
  return extensionOf(v) ? v : `${v}.${ext}`;
}

/**
 * Why a name will not do for a picture, or null: the tree's rule (a name,
 * no slash, not `.` or `..`, not already among `taken`) and the store's —
 * a dot-file is not a picture's name (`sanitise_file_name`).
 * @param {string} value
 * @param {readonly string[]} taken the names already there
 * @returns {string | null}
 */
export function imageNameError(value, taken) {
  const problem = renameError(value, "", taken);
  if (problem) return problem;
  return String(value).trim().startsWith(".") ? t("ui-pasted-image-picture-s-name-does-start-dot") : null;
}

/**
 * How a paste event's contents sort: `pictures` (the `image/*` entries) are
 * named before they are taken; `files` are taken as they are; `askShell` is
 * a paste carrying neither files nor text, when the web engine may be
 * holding back a picture the shell can read.
 * @template {{name: string, type: string}} T
 * @param {readonly T[]} entries the event's files
 * @param {boolean} hasText whether the event carries plain text
 * @returns {{pictures: T[], files: T[], askShell: boolean}}
 */
export function pasteIntake(entries, hasText) {
  const all = Array.from(entries ?? []);
  const pictures = all.filter((f) => isPicture(f));
  const files = all.filter((f) => !isPicture(f));
  return { pictures, files, askShell: all.length === 0 && !hasText };
}

/** @param {{type?: string}} entry */
function isPicture(entry) {
  return String(entry?.type ?? "").toLowerCase().startsWith("image/");
}
