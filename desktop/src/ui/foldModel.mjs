/**
 * Long text, folded to its first sentence.
 *
 * A review body or a thread comment can run to paragraphs, and a panel that
 * shows every one in full is a panel nobody scrolls. So each is drawn folded:
 * the first sentence — or the first line, whichever ends first — cut at a word
 * and marked with an ellipsis when there is more, with one control to show the
 * rest and one to fold it back. The words are decided here, where `node --test`
 * can read them; `FoldedText.tsx` draws.
 */

import { t } from "../i18n/l10n.mjs";

/** Longer than this, a first sentence is cut at a word boundary. */
export const FOLD_MAX = 140;

/** Markdown marks that are noise in a one-line summary. */
function plainish(text) {
  return String(text ?? "")
    .replace(/```[\s\S]*?```\s*/g, "[code] ")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/^\s*(?:[-*+]|\d+\.)\s+/gm, "")
    .replace(/^\s*#{1,6}\s+/gm, "")
    .replace(/^\s*>\s?/gm, "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/[*_]([^*_]+)[*_]/g, "$1")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .trim();
}

/**
 * The first sentence or line of `text`, at most `max` characters, cut at a
 * word and ending in `…` when anything was left out.
 * @param {string | null | undefined} text
 * @param {number} [max]
 */
export function firstSentence(text, max = FOLD_MAX) {
  const plain = plainish(text);
  if (!plain) return "";
  const firstLine = plain.split(/\r?\n/).find((l) => l.trim()) ?? "";
  const m = /^(.*?[.!?])(\s|$)/.exec(firstLine);
  let sentence = (m ? m[1] : firstLine).trim();
  const cutByStructure = sentence.length < plain.length;
  if (sentence.length > max) {
    const cut = sentence.slice(0, max);
    const atWord = cut.lastIndexOf(" ");
    sentence = (atWord > max * 0.6 ? cut.slice(0, atWord) : cut).replace(/[\s,;:—–-]+$/, "");
    return `${sentence}…`;
  }
  return cutByStructure ? `${sentence}…` : sentence;
}

/**
 * Whether folding `text` would hide anything: it has more than one sentence or
 * line, or is longer than `max`.
 * @param {string | null | undefined} text
 * @param {number} [max]
 */
export function needsFold(text, max = FOLD_MAX) {
  const plain = plainish(text);
  if (!plain) return false;
  return firstSentence(text, max) !== plain;
}

/** The words on the fold control. @param {boolean} open */
export function foldLabel(open) {
  return open ? t("ui-fold-show-less") : t("ui-fold-show-all");
}
