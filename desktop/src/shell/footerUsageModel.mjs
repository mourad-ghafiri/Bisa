/**
 * The footer's usage stat, as facts: which harness it shows, and its words.
 *
 * One harness at a time — the one the person pinned, while it is still an
 * installed harness that can be launched; else the first installed one; else
 * none, and the footer's left is simply empty. The picker in the overlay
 * lists the same rows, so what can be pinned is exactly what can be shown.
 */

import { t } from "../i18n/l10n.mjs";
import { percent } from "../i18n/format.mjs";

/**
 * The harnesses the picker offers: the installed ones, in the catalog's
 * order (installed first, then by label — `useLaunchableHarnesses`).
 * @template {{ id: string, installed: boolean }} R
 * @param {readonly R[]} rows
 * @returns {R[]}
 */
export function pickerRows(rows) {
  return rows.filter((r) => r.installed);
}

/**
 * The harness the footer shows: the pinned one when it can be, else the
 * first the picker offers, else null.
 * @param {string | null | undefined} pinned
 * @param {readonly { id: string, installed: boolean }[]} rows
 * @returns {string | null}
 */
export function pinnedHarness(pinned, rows) {
  const offered = pickerRows(rows);
  if (pinned && offered.some((r) => r.id === pinned)) return pinned;
  return offered[0]?.id ?? null;
}

/**
 * The stat's accessible name and tooltip, the line's words in its order:
 * *Claude Code usage · 5h 23% · resets in 2 h · Weekly 41% · Fable 9%* — the
 * first window's reset after it, the credits left out as on the line — or
 * the harness's sentence when there is no report. Ends with the door.
 * @param {string} label the harness's label
 * @param {{ meters: { label: string, percent: number }[], inline: string | null, note: string | null }} words
 */
export function usageStatWords(label, words) {
  const meter = (m) => t("shell-footer-usage-meter", { label: m.label, percent: percent(m.percent / 100) });
  const facts = words.note ? [words.note] : words.meters.flatMap((m, i) => (i === 0 && words.inline ? [meter(m), words.inline] : [meter(m)]));
  return [t("shell-footer-usage-harness-usage", { label }), ...facts, t("shell-footer-usage-click-every-harness")].join(" · ");
}
