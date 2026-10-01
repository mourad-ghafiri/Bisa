/**
 * The few words a template or a shape library is drawn with (19 —
 * Drawings): a grid, a box, an arrow, a sticky note, a title, a frame — each
 * a **skeleton** element the canvas lays out (`convertToExcalidrawElements`),
 * so a template is data and never a scene. Every id is chosen here so an
 * arrow can bind to its boxes in the same skeleton; the canvas regenerates
 * them on *New*, so two drawings from one template never collide.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/** The column pitch, the row pitch, and a box's size — a 20 px grid, boxes at least 160×80, gaps at least 40. */
const COL = 260;
const ROW = 140;
const BOX_W = 180;
const BOX_H = 80;
const MARGIN = 40;

/** The open-color tints that read well on the canvas, light or dark. */
export const PALETTE = Object.freeze({
  blue: "#e7f5ff",
  yellow: "#fff3bf",
  green: "#d3f9d8",
  red: "#ffe3e3",
  violet: "#f3f0ff",
  grey: "#f1f3f5",
  orange: "#ffe8cc",
  teal: "#c3fae8",
  pink: "#ffdeeb",
});
export const INK = "#1e1e1e";

export function col(i) {
  return MARGIN + i * COL;
}

export function row(j) {
  return MARGIN + j * ROW;
}

/**
 * A labelled shape.
 * @param {string} id
 * @param {number} x
 * @param {number} y
 * @param {string} text
 * @param {{type?: "rectangle" | "ellipse" | "diamond", width?: number, height?: number, fill?: string, dashed?: boolean, fontSize?: number}} [opts]
 */
export function box(id, x, y, text, opts = {}) {
  return {
    id,
    type: opts.type ?? "rectangle",
    x,
    y,
    width: opts.width ?? BOX_W,
    height: opts.height ?? BOX_H,
    label: { text, fontSize: opts.fontSize ?? 20 },
    backgroundColor: opts.fill ?? PALETTE.blue,
    fillStyle: "solid",
    strokeColor: INK,
    strokeWidth: 1,
    strokeStyle: opts.dashed ? "dashed" : "solid",
    roughness: 1,
  };
}

/**
 * An arrow from one box to another, bound at both ends; its place is the
 * midpoint between them, which the canvas refines against the bindings.
 * @param {string} id
 * @param {{id: string, x: number, y: number, width: number, height: number}} from
 * @param {{id: string, x: number, y: number, width: number, height: number}} to
 * @param {string} [text]
 */
export function arrow(id, from, to, text) {
  const fx = from.x + from.width / 2;
  const fy = from.y + from.height / 2;
  const tx = to.x + to.width / 2;
  const ty = to.y + to.height / 2;
  return {
    id,
    type: "arrow",
    x: Math.min(fx, tx),
    y: Math.min(fy, ty),
    width: Math.abs(tx - fx),
    height: Math.abs(ty - fy),
    start: { id: from.id },
    end: { id: to.id },
    strokeColor: INK,
    strokeWidth: 1,
    endArrowhead: "arrow",
    ...(text ? { label: { text, fontSize: 16 } } : {}),
  };
}

/** A sticky note: a warm, filled rectangle with the words inside. */
export function note(id, x, y, text, fill = PALETTE.yellow) {
  return box(id, x, y, text, { width: 200, height: 120, fill, fontSize: 16 });
}

/** A title: one text, large. */
export function title(id, x, y, text) {
  return { id, type: "text", x, y, text, fontSize: 28, strokeColor: INK };
}

/** A caption: one text, small and dim. */
export function caption(id, x, y, text) {
  return { id, type: "text", x, y, text, fontSize: 16, strokeColor: "#5c5f66" };
}

/** A frame around the elements named. */
export function frame(id, name, children) {
  return { id, type: "frame", name, children };
}
