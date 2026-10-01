/**
 * Blame rows → what the editor's gutter shows (ide/04 §6).
 *
 * One line of text per source line: the short hash and the author, padded
 * so the column lines up, with the full story — author, when, the commit
 * subject — on hover. Uncommitted lines say so instead of showing a hash of
 * zeros, because seven zeros is a puzzle and "not committed" is an answer.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * @param {import("../../types").BlameLine[]} lines
 * @param {(secs: number) => string} when  formats a timestamp for the hover
 * @returns {{line: number, text: string, hover: string}[]}
 */
export function gutterFor(lines, when) {
  const width = Math.min(
    16,
    lines.reduce((w, l) => Math.max(w, (l.uncommitted ? "" : l.author).length), 0),
  );
  return lines.map((l) => {
    if (l.uncommitted) {
      return {
        line: l.line,
        text: `${"·······"} ${t("work-blame-not-committed").slice(0, width).padEnd(width)}`,
        hover: t("work-blame-not-committed-yet-line-working-tree"),
      };
    }
    const author = l.author.length > width ? l.author.slice(0, width - 1) + "…" : l.author;
    return {
      line: l.line,
      text: `${l.short} ${author.padEnd(width)}`,
      hover: `**${l.short}** ${l.summary}\n\n${l.author} · ${when(l.timestamp)}`,
    };
  });
}
