/**
 * The stickers (19 — Drawings): sticky notes in the palette's colours, a
 * callout, badges for a card's state, a checkmark and a warning made of
 * shapes and text — never an image. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { INK, PALETTE, box, note } from "../templates/grid.mjs";

function badge(id, text, fill) {
  return [box(id, 0, 0, text, { width: 120, height: 36, fill, fontSize: 16 })];
}

export const STICKER_ITEMS = Object.freeze([
  { id: "sticky-yellow", name: t("draw-library-sticky-yellow"), skeleton: () => [note("sticky", 0, 0, "…", PALETTE.yellow)] },
  { id: "sticky-blue", name: t("draw-library-sticky-blue"), skeleton: () => [note("sticky", 0, 0, "…", PALETTE.blue)] },
  { id: "sticky-green", name: t("draw-library-sticky-green"), skeleton: () => [note("sticky", 0, 0, "…", PALETTE.green)] },
  { id: "sticky-pink", name: t("draw-library-sticky-pink"), skeleton: () => [note("sticky", 0, 0, "…", PALETTE.pink)] },
  { id: "sticky-violet", name: t("draw-library-sticky-violet"), skeleton: () => [note("sticky", 0, 0, "…", PALETTE.violet)] },
  {
    id: "callout",
    name: t("draw-library-callout"),
    skeleton: () => [
      box("callout", 0, 0, t("draw-shape-say-it-here"), { width: 220, height: 80, fill: "#ffffff" }),
      { id: "callout-tail", type: "line", x: 30, y: 80, width: 30, height: 40, strokeColor: INK, strokeWidth: 1 },
    ],
  },
  { id: "badge-done", name: t("draw-library-badge-done"), skeleton: () => badge("done", "✓ Done", PALETTE.green) },
  { id: "badge-blocked", name: t("draw-library-badge-blocked"), skeleton: () => badge("blocked", "✕ Blocked", PALETTE.red) },
  { id: "badge-idea", name: t("draw-library-badge-idea"), skeleton: () => badge("idea", "💡 Idea", PALETTE.yellow) },
  { id: "badge-question", name: t("draw-library-badge-question"), skeleton: () => badge("question", "? Question", PALETTE.violet) },
  { id: "checkmark", name: t("draw-library-checkmark"), skeleton: () => [box("check", 0, 0, "✓", { type: "ellipse", width: 56, height: 56, fill: PALETTE.green, fontSize: 28 })] },
  { id: "warning", name: t("draw-library-warning"), skeleton: () => [box("warn", 0, 0, "!", { type: "diamond", width: 72, height: 72, fill: PALETTE.orange, fontSize: 28 })] },
]);
