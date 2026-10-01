/**
 * Where a floating dock sits, and where it is allowed to be painted.
 *
 * Plain `.mjs` with a `.d.mts` beside it — `node --test` imports it with no
 * build step, and TypeScript reads the declarations. It lived in the notes
 * model until a second dock wanted it; a pure function used by two callers is
 * the one thing that must not be copied, because the copies diverge silently.
 *
 * # A placement is anchored to the nearest edges
 *
 * A dock's **placement** is `{h, x, v, y}`: `x` px from the `left` or `right`
 * edge to the dock's near side, `y` px from the `top` or `bottom`. It is
 * measured from whichever edge is nearer on each axis, so a dock keeps its
 * distance from the edges it lives near whatever the window does: a maximize
 * and a restore put it back exactly, and a dock parked 24px from a corner is
 * 24px from that corner at every size. A single corner as the origin — the
 * bottom right, which this replaced — slid a dock parked top-left across the
 * screen on every maximize, and a fraction of the window drifts a corner-
 * parked dock away from its corner as the window grows.
 *
 * # The paint is clamped; the placement is not
 *
 * `dockBox` is the only thing painted, and it is clamped inside the window and
 * under the chrome — a dock dragged to the edge of a wide window would
 * otherwise be off the screen entirely once the window narrows, with no way
 * to get it back. The clamp is never written back: persisting it would lose
 * the person's placement, so growing the window again could never restore
 * it. A drag begins from the painted box (`placementOf` of the box the pointer
 * moved), so a clamped dock does not jump when it is grabbed.
 *
 * `top` is the height of the window chrome, and it is **required**: that
 * strip is `data-tauri-drag-region`, where a pointer gesture moves the OS
 * window instead of the thing you grabbed, and its height is the theme's
 * (`--spacing-chrome`), not a number to keep a second copy of here.
 *
 * `size` is the square case; `sizeX`/`sizeY` override it per axis, because
 * **not every dock is square**: a pet is taller than it is wide, and clamping
 * its height against its width let the bottom of it hang off the screen.
 */

const EDGES_H = new Set(["left", "right"]);
const EDGES_V = new Set(["top", "bottom"]);

/** The extents a viewport gives, with the square fallback applied. */
function extents(viewport) {
  const { width, height, size = 40, sizeX = size, sizeY = size, top, margin = 8 } = viewport;
  return { width, height, sizeX, sizeY, top, margin };
}

/** The box, kept inside the window and under the chrome. */
function clampBox(box, viewport) {
  const { width, height, sizeX, sizeY, top, margin } = extents(viewport);
  const maxLeft = Math.max(margin, width - sizeX - margin);
  const maxTop = Math.max(top, height - sizeY - margin);
  return {
    left: Math.round(Math.min(Math.max(box.left, margin), maxLeft)),
    top: Math.round(Math.min(Math.max(box.top, top), maxTop)),
  };
}

/**
 * Where a placement is painted in this viewport — `{left, top}` px, clamped.
 * Never store what this returns: the placement is the fact, the box is its
 * projection into the window of the moment.
 * @param {{h: "left" | "right", x: number, v: "top" | "bottom", y: number}} placement
 * @param {{width: number, height: number, size?: number, sizeX?: number, sizeY?: number, top: number, margin?: number}} viewport
 * @returns {{left: number, top: number}}
 */
export function dockBox(placement, viewport) {
  const { width, height, sizeX, sizeY } = extents(viewport);
  return clampBox(
    {
      left: placement.h === "left" ? placement.x : width - sizeX - placement.x,
      top: placement.v === "top" ? placement.y : height - sizeY - placement.y,
    },
    viewport,
  );
}

/**
 * The placement of a box — clamped first, then anchored to the nearer edge on
 * each axis. A tie anchors right and bottom, the corner the defaults live in.
 * @param {{left: number, top: number}} box
 * @param {{width: number, height: number, size?: number, sizeX?: number, sizeY?: number, top: number, margin?: number}} viewport
 * @returns {{h: "left" | "right", x: number, v: "top" | "bottom", y: number}}
 */
export function placementOf(box, viewport) {
  const { width, height, sizeX, sizeY, top, margin } = extents(viewport);
  const at = clampBox(box, viewport);
  const fromRight = width - sizeX - at.left;
  const fromBottom = height - sizeY - at.top;
  const h = at.left - margin < fromRight - margin ? "left" : "right";
  const v = at.top - top < fromBottom - margin ? "top" : "bottom";
  // Never below zero: a window smaller than the dock puts the far edge inside
  // it, and a negative offset is not a placement (`placementFrom` refuses it).
  return {
    h,
    x: Math.max(0, h === "left" ? at.left : fromRight),
    v,
    y: Math.max(0, v === "top" ? at.top : fromBottom),
  };
}

/**
 * A stored value as a placement, or the fallback. Exactly the four fields,
 * the two edges, finite offsets at or past zero — anything else, an earlier
 * shape included, is not a placement and takes the fallback. The result is a
 * fresh object, never the value handed in.
 * @template T
 * @param {unknown} value
 * @param {T} fallback
 * @returns {{h: "left" | "right", x: number, v: "top" | "bottom", y: number} | T}
 */
export function placementFrom(value, fallback) {
  if (typeof value !== "object" || value === null) return fallback;
  const { h, x, v, y } = /** @type {Record<string, unknown>} */ (value);
  const offset = (n) => typeof n === "number" && Number.isFinite(n) && n >= 0;
  if (!EDGES_H.has(h) || !EDGES_V.has(v) || !offset(x) || !offset(y)) return fallback;
  return { h, x, v, y };
}

/** Whether two placements are the same place from the same edges. */
export function samePlacement(a, b) {
  return a.h === b.h && a.x === b.x && a.v === b.v && a.y === b.y;
}
