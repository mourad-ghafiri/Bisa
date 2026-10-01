/**
 * A template's preview, drawn from its skeleton (19 — Drawings): the boxes
 * as boxes, the arrows as lines between their boxes' centres, the words as
 * words, a frame as a dashed rectangle around what it names — enough to
 * recognise a picture in a tile, without loading the canvas. Pure data in,
 * pure shapes out; `TemplatePreview.tsx` paints them as one SVG.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/** The margin around the picture, in skeleton units. */
const PAD = 40;
/** A text's estimated width per character, as a fraction of its font size. */
const GLYPH = 0.55;
/** A frame's inset around what it holds. */
const FRAME_INSET = 16;

const BOXES = new Set(["rectangle", "ellipse", "diamond"]);

/**
 * @typedef {{kind: "rect" | "ellipse" | "diamond", x: number, y: number, w: number, h: number, fill: string | null, dashed: boolean}} BoxShape
 * @typedef {{kind: "line", x1: number, y1: number, x2: number, y2: number, arrow: boolean}} LineShape
 * @typedef {{kind: "text", x: number, y: number, text: string, size: number, anchor: "start" | "middle"}} TextShape
 * @typedef {{kind: "frame", x: number, y: number, w: number, h: number}} FrameShape
 * @typedef {BoxShape | LineShape | TextShape | FrameShape} Shape
 */

/** The first line of a text, or nothing. */
function firstLine(text) {
  const line = String(text ?? "").split("\n")[0].trim();
  return line;
}

/** A box's centre. */
function centre(b) {
  return { x: b.x + (b.width ?? 0) / 2, y: b.y + (b.height ?? 0) / 2 };
}

/**
 * The shapes a skeleton previews as, and the box around them.
 * @param {readonly Record<string, any>[]} skeleton
 * @returns {{viewBox: {x: number, y: number, w: number, h: number}, shapes: Shape[]}}
 */
export function templatePreview(skeleton) {
  const byId = new Map();
  for (const e of skeleton ?? []) if (e && typeof e.id === "string") byId.set(e.id, e);
  /** @type {Shape[]} */
  const shapes = [];
  /** @type {{x: number, y: number}[]} */
  const points = [];
  const bound = (x, y) => points.push({ x, y });
  const boxBounds = (e) => {
    bound(e.x, e.y);
    bound(e.x + (e.width ?? 0), e.y + (e.height ?? 0));
  };

  for (const e of skeleton ?? []) {
    if (!e || typeof e !== "object") continue;
    if (BOXES.has(e.type)) {
      const kind = e.type === "rectangle" ? "rect" : e.type;
      shapes.push({ kind, x: e.x, y: e.y, w: e.width ?? 0, h: e.height ?? 0, fill: e.backgroundColor ?? null, dashed: e.strokeStyle === "dashed" });
      boxBounds(e);
      const words = firstLine(e.label?.text);
      if (words) {
        const c = centre(e);
        shapes.push({ kind: "text", x: c.x, y: c.y, text: words, size: e.label?.fontSize ?? 20, anchor: "middle" });
      }
    } else if (e.type === "text") {
      const words = firstLine(e.text);
      const size = e.fontSize ?? 20;
      shapes.push({ kind: "text", x: e.x, y: e.y + size, text: words, size, anchor: "start" });
      bound(e.x, e.y);
      bound(e.x + words.length * size * GLYPH, e.y + size * 1.2);
    } else if (e.type === "arrow" || e.type === "line") {
      const from = e.start?.id ? byId.get(e.start.id) : null;
      const to = e.end?.id ? byId.get(e.end.id) : null;
      const a = from ? centre(from) : { x: e.x, y: e.y };
      const b = to ? centre(to) : { x: e.x + (e.width ?? 0), y: e.y + (e.height ?? 0) };
      shapes.push({ kind: "line", x1: a.x, y1: a.y, x2: b.x, y2: b.y, arrow: e.type === "arrow" });
      bound(a.x, a.y);
      bound(b.x, b.y);
      const words = firstLine(e.label?.text);
      if (words) shapes.push({ kind: "text", x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 - 6, text: words, size: e.label?.fontSize ?? 16, anchor: "middle" });
    } else if (e.type === "frame") {
      const kids = (e.children ?? []).map((id) => byId.get(id)).filter((k) => k && Number.isFinite(k.x) && Number.isFinite(k.y));
      if (kids.length === 0) continue;
      const xs = kids.flatMap((k) => [k.x, k.x + (k.width ?? 0)]);
      const ys = kids.flatMap((k) => [k.y, k.y + (k.height ?? 0)]);
      const x = Math.min(...xs) - FRAME_INSET;
      const y = Math.min(...ys) - FRAME_INSET;
      const w = Math.max(...xs) - Math.min(...xs) + 2 * FRAME_INSET;
      const h = Math.max(...ys) - Math.min(...ys) + 2 * FRAME_INSET;
      // A frame goes under what it holds.
      shapes.unshift({ kind: "frame", x, y, w, h });
      bound(x, y);
      bound(x + w, y + h);
    }
  }

  if (points.length === 0) return { viewBox: { x: 0, y: 0, w: 1, h: 1 }, shapes: [] };
  const minX = Math.min(...points.map((p) => p.x)) - PAD;
  const minY = Math.min(...points.map((p) => p.y)) - PAD;
  const maxX = Math.max(...points.map((p) => p.x)) + PAD;
  const maxY = Math.max(...points.map((p) => p.y)) + PAD;
  return { viewBox: { x: minX, y: minY, w: Math.max(1, maxX - minX), h: Math.max(1, maxY - minY) }, shapes };
}
