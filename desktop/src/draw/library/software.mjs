/**
 * The software-engineering shapes (19 — Drawings): the parts a system is
 * drawn from, each a small skeleton the canvas lays out into a library item.
 * Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { INK, PALETTE, box } from "../templates/grid.mjs";

/** A cylinder-ish store: an ellipse lid over a rectangle body, grouped by the label on the body. */
function store(id, text) {
  return [
    { id: `${id}-lid`, type: "ellipse", x: 0, y: 0, width: 160, height: 36, backgroundColor: PALETTE.grey, fillStyle: "solid", strokeColor: INK, strokeWidth: 1, roughness: 1 },
    box(`${id}-body`, 0, 18, text, { width: 160, height: 70, fill: PALETTE.grey, fontSize: 16 }),
  ];
}

/** A person: a head over a body, with the name under. */
function actor(id, text) {
  return [
    { id: `${id}-head`, type: "ellipse", x: 50, y: 0, width: 40, height: 40, backgroundColor: PALETTE.pink, fillStyle: "solid", strokeColor: INK, strokeWidth: 1, roughness: 1 },
    box(`${id}-body`, 20, 44, text, { width: 100, height: 60, fill: PALETTE.pink, fontSize: 16 }),
  ];
}

export const SOFTWARE_ITEMS = Object.freeze([
  { id: "service", name: t("draw-library-service"), skeleton: () => [box("service", 0, 0, "Service", { fill: PALETTE.green })] },
  { id: "database", name: t("draw-library-database"), skeleton: () => store("database", "Database") },
  { id: "queue", name: t("draw-library-queue"), skeleton: () => [box("queue", 0, 0, "▮▮▮ Queue", { fill: PALETTE.orange, height: 56 })] },
  { id: "cache", name: t("draw-library-cache"), skeleton: () => [box("cache", 0, 0, "Cache", { type: "ellipse", fill: PALETTE.grey })] },
  { id: "client", name: t("draw-library-client"), skeleton: () => [box("client", 0, 0, "Client", { fill: PALETTE.blue })] },
  { id: "api-gateway", name: t("draw-library-api-gateway"), skeleton: () => [box("gateway", 0, 0, t("draw-shape-api-gateway"), { fill: PALETTE.yellow })] },
  { id: "load-balancer", name: t("draw-library-load-balancer"), skeleton: () => [box("lb", 0, 0, t("draw-shape-load-balancer"), { type: "diamond", width: 200, height: 100, fill: PALETTE.yellow })] },
  { id: "external-system", name: t("draw-library-external-system"), skeleton: () => [box("external", 0, 0, t("draw-shape-external-system"), { fill: PALETTE.grey, dashed: true })] },
  { id: "actor", name: t("draw-library-actor"), skeleton: () => actor("actor", "Person") },
]);
