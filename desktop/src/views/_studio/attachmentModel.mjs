/**
 * What a file in a conversation looks like: which glyph, whether it can be
 * shown, and what to say when its bytes are not here.
 *
 * A plain `.mjs` so `node --test` reaches it without a DOM, per the rule in
 * `desktop/README.md`. It qualifies because a wrong answer here makes a wrong
 * *fact* on screen: an attachment marked present when the bytes have not
 * arrived is a broken image with no explanation, and one marked absent when
 * they have is a Request button that does nothing.
 */

import { t } from "../../i18n/l10n.mjs";

/** Bytes → a short human size. Matches the file tree's vocabulary. */
export function formatBytes(size) {
  if (!Number.isFinite(size) || size < 0) return "";
  if (size < 1024) return `${size} B`;
  const units = ["KB", "MB", "GB"];
  let n = size / 1024;
  let unit = 0;
  while (n >= 1024 && unit < units.length - 1) {
    n /= 1024;
    unit += 1;
  }
  return `${n < 10 ? n.toFixed(1) : Math.round(n)} ${units[unit]}`;
}

/**
 * The formats the byte route will actually serve inline.
 *
 * **Kept in step with `image_type` in `bisa-node/src/attachments.rs`**,
 * which decides the same question from the file's magic number. This is only a
 * hint about what to *try*: asking for a mislabelled file as an image gets an
 * octet-stream back, and the chip renders instead of a broken picture.
 */
const RENDERABLE = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

/** Whether to attempt an inline `<img>` for this attachment. */
export function isRenderableImage(file, present) {
  return !!present && RENDERABLE.has((file?.mime ?? "").toLowerCase());
}

/**
 * A glyph name for a file, by media type.
 *
 * `FILE_KIND_ICON` in `ui/icons.ts` is keyed by *domain* kind — what a thing is
 * in the workspace — and has no answer for "a PDF somebody sent". This is the
 * other question, and the names it returns are `ICON` keys.
 */
export function glyphFor(mime) {
  const m = (mime ?? "").toLowerCase();
  if (m.startsWith("image/")) return "image";
  if (m.startsWith("audio/")) return "audio";
  if (m.startsWith("video/")) return "video";
  if (m === "application/pdf") return "document";
  if (m.startsWith("text/") || m === "application/json") return "document";
  if (m.includes("zip") || m.includes("tar") || m.includes("compressed")) return "archive";
  return "file";
}

/**
 * What to say about an attachment, in one line.
 *
 * The absent case is the one that matters. A file's bytes do not travel with
 * the message — the descriptor syncs and the bytes are fetched from whoever has
 * them — so "not on this machine" is an ordinary state and not an error, and
 * the label says so rather than implying something is broken.
 */
export function describe(file, present) {
  const size = formatBytes(file?.size ?? 0);
  return present ? size : t("studio-attachment-not-machine", { size });
}
