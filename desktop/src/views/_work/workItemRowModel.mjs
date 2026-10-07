/**
 * What a work item's row says about it, as words.
 *
 * A work item's instructions are Markdown an agent wrote — a heading, then
 * the steps — and the Details pane is 380 px wide. The row shows the first
 * line as the item's name, because that is what an agent's instructions open
 * with: what to do. The whole body is one click away, in the item's panel.
 * Plain JavaScript so `node --test` reads it without a build.
 */

import { t } from "../../i18n/l10n.mjs";

/** What the row says when the instructions say nothing. */
export const NO_INSTRUCTIONS = t("work-work-item-row-no-instructions");

const LEADING_MARK = /^(?:#{1,6}\s+|[-*+]\s+|\d+[.)]\s+|>\s*)+/;

/**
 * The first non-empty line of the instructions, without its Markdown mark —
 * a heading's `#`, a bullet's `-` or `*`, a numbered item's `1.`, a quote's
 * `>` — and with its whitespace collapsed. Nothing is cut: the row truncates
 * with CSS and its title carries the whole line.
 */
export function headline(instructions) {
  for (const raw of String(instructions ?? "").split(/\r?\n/)) {
    const line = raw.replace(LEADING_MARK, "").replace(/\s+/g, " ").trim();
    if (line) return line;
  }
  return NO_INSTRUCTIONS;
}

/**
 * Whether an engine frame moved one work item — its execution ended, its
 * state changed, a session of it moved — so a panel open on it reads it
 * again. The frame is read by the item it names, never by its type alone.
 * @param {{payload?: {type?: string, work_item?: string | null, presence?: {work_item?: string | null} | null} | null} | null | undefined} event @param {string} itemId
 */
export function movesItem(event, itemId) {
  const p = event?.payload;
  if (!p || !itemId) return false;
  if (p.work_item === itemId) return true;
  return p.type === "session_state" && p.presence?.work_item === itemId;
}
