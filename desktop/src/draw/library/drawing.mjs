/**
 * The drawing elements (19 — Drawings): a title banner, a section frame, a
 * legend, a note card — the furniture of a readable picture. Plain `.mjs`,
 * so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { PALETTE, box, caption, frame, title } from "../templates/grid.mjs";

export const DRAWING_ITEMS = Object.freeze([
  { id: "title-banner", name: t("draw-library-title-banner"), skeleton: () => [box("banner", 0, 0, "Title", { width: 480, height: 64, fill: PALETTE.violet, fontSize: 28 })] },
  {
    id: "section-frame",
    name: t("draw-library-section-frame"),
    skeleton: () => [
      caption("section-caption", 20, 20, "Section"),
      box("section-a", 20, 60, "…", { fill: "#ffffff", dashed: true }),
      box("section-b", 240, 60, "…", { fill: "#ffffff", dashed: true }),
      frame("section", "Section", ["section-caption", "section-a", "section-b"]),
    ],
  },
  {
    id: "legend",
    name: t("draw-library-legend"),
    skeleton: () => [
      title("legend-title", 0, 0, "Legend"),
      box("legend-1", 0, 50, "means…", { width: 160, height: 40, fill: PALETTE.blue, fontSize: 14 }),
      box("legend-2", 0, 100, "means…", { width: 160, height: 40, fill: PALETTE.yellow, fontSize: 14 }),
      box("legend-3", 0, 150, "means…", { width: 160, height: 40, fill: PALETTE.green, fontSize: 14 }),
    ],
  },
  { id: "note-card", name: t("draw-library-note-card"), skeleton: () => [box("card", 0, 0, t("draw-shape-note-card-text"), { width: 260, height: 140, fill: "#ffffff", fontSize: 16 })] },
]);
