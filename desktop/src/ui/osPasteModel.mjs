/**
 * Pasting what the machine's clipboard holds into the explorer (ide/03 §The
 * explorer), as facts: where the files land — the tree's root, or the
 * folder under the cursor, by absolute path, since the shell copies and the
 * node never learns a path outside the root — what the toast says about
 * what landed, what was renamed and what was left out, and the sentence for
 * a paste with nothing to paste. The copying itself — and the writing of a
 * picture under the name the person confirmed — is the shell's
 * (`src-tauri/src/pasteboard.rs`). Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * The absolute folder a paste lands in: the root for the tree's root (`""`),
 * else the folder under it. Null with no root known yet.
 * @param {string | null} root the tree's absolute root
 * @param {string} dir the folder, relative to the root; `""` is the root
 * @returns {string | null}
 */
export function pasteDestination(root, dir) {
  if (!root) return null;
  const rel = String(dir ?? "").replace(/^\/+|\/+$/g, "");
  return rel ? `${root.replace(/\/+$/, "")}/${rel}` : root;
}

/**
 * What the toast says after a paste.
 * @param {{pasted: {from: string, name: string}[], skipped: {path: string, why: string}[]}} report
 * @param {string} dir the folder pasted into, relative to the root; `""` is the root
 * @returns {{text: string, tone: "ok" | "warn" | "error"}}
 */
/** Whether a pasted item came from a file on this machine — the shell hands an absolute path — rather than the clipboard. @param {unknown} from */
const fromAFile = (from) => typeof from === "string" && from.startsWith("/");

export function pasteWords(report, dir) {
  const pasted = report?.pasted ?? [];
  const skipped = report?.skipped ?? [];
  const where = dir ? t("ui-os-paste-into-dir", { dir: dir.replace(/\/+$/, "") }) : t("ui-os-paste-into-root");
  // A rename is a fact about a file the person copied — `from` a path on this
  // machine. A pasted picture comes from the clipboard under a name they
  // confirmed, so it is never counted as renamed.
  const renamed = pasted.filter((p) => fromAFile(p.from) && p.name !== basename(p.from)).length;
  if (pasted.length === 0) {
    const text = skipped.length === 0 ? t("ui-os-paste-nothing-pasted") : skipped.length === 1 ? t("ui-os-paste-nothing-pasted-why", { why: skipped[0].why }) : t("ui-os-paste-nothing-pasted-left-out", { n: skipped.length });
    return { text, tone: "error" };
  }
  // One sentence from its pieces: what was pasted and where, then the notes — renamed, left out — each a message.
  const pieces = [pasted.length === 1 ? t("ui-os-paste-pasted-one", { name: pasted[0].name, where }) : t("ui-os-paste-pasted-items", { pasted: pasted.length, where })];
  if (renamed > 0) pieces.push(t("ui-os-paste-renamed-note", { renamed }));
  if (skipped.length === 1) pieces.push(t("ui-os-paste-left-out-one", { why: skipped[0].why }));
  else if (skipped.length > 1) pieces.push(t("ui-os-paste-left-out-many", { n: skipped.length }));
  return { text: t("ui-os-paste-sentence", { body: pieces.join(" ") }), tone: skipped.length > 0 ? "warn" : "ok" };
}

/** The sentence when neither the tree's clipboard nor the machine's holds anything a paste can take. */
export function pasteRefusal() {
  return t("ui-os-paste-nothing-paste-copy-files-here-file");
}

/**
 * What a paste would take from, for the menu's word: the tree's clipboard
 * first, else the file manager's files, else the picture on the clipboard,
 * else nothing. Files before a picture: a copy of an image file in the file
 * manager is a file, and lands under its own name.
 * @param {object | null} clip the tree's clipboard
 * @param {{files: boolean, image: boolean}} held what the machine's clipboard holds
 * @returns {"tree" | "os" | "image" | null}
 */
export function pasteSource(clip, held) {
  if (clip) return "tree";
  if (held?.files) return "os";
  return held?.image ? "image" : null;
}

function basename(path) {
  const s = String(path ?? "").replace(/\/+$/, "");
  return s.slice(s.lastIndexOf("/") + 1);
}
