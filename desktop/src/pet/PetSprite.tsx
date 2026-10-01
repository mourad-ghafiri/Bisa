/**
 * One cell of a sprite sheet, animated.
 *
 * The sheet arrives over HTTP — `GET /pets/{id}/sprite` — the same way every
 * other byte reaches this webview, and shows through a window the size of one
 * cell with the background moved under it. No library, no bundled asset, and
 * nothing decoded twice: the same URL the browser already has in its cache is
 * what the canvas below reads.
 *
 * # Where the frames come from
 *
 * A pack drawn for this platform says in its manifest how each state plays —
 * its frames and each frame's duration (`petModel.framePlan`) — and that is
 * what is played, so a quick pet is quick and a slow one slow. A pet with no
 * `animations` leaves its unused cells fully transparent and says nothing of
 * their number; playing all eight would show it vanishing five times a
 * second, so its sheet is decoded once into a canvas and each row scanned
 * for its last cell with art in it (`frameCounts`). Only that pet pays for
 * the canvas.
 *
 * # Why this animates in JavaScript
 *
 * `desktop/README.md` reserves JS animation for the two cases CSS cannot do.
 * This is a third, and the reason is concrete rather than aesthetic: CSS
 * `steps(n)` needs one duration and `n` at authoring time, and here each
 * frame has its own duration and `n` differs per row. A chain of one timer
 * per frame is the honest shape of that.
 *
 * **Under reduced motion no timer runs at all** and the pet holds its first
 * frame. `theme/motion.css` says a looping animation is the one kind that has
 * to stop dead rather than merely get shorter, and a pet loops forever.
 */

import { prefersReducedMotion } from "../shell/motion";
import { errorFields, log } from "../log";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { PetDef } from "../types";
import { ICON } from "../ui/icons";
import {
  SHEET,
  alphaProbe,
  cellOffset,
  frameCounts,
  framePlan,
  PET_HEIGHT,
  rowOf,
  spriteBox,
  type PetState,
  type SpriteBox,
} from "./petModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * Count the frames in each row of a sheet, once — and say whether the sheet
 * loaded at all.
 *
 * `failed` matters more than it looks. Everything about this component is
 * transparent by construction: a window one cell wide over a background image.
 * If the image never arrives there is nothing to see and nothing to say so,
 * and a pet that is installed, selected and invisible is indistinguishable
 * from a pet that is broken.
 *
 * A decode that fails is different from a load that fails: every row falls
 * back to one frame, so a sheet that arrives but will not decode stands still
 * rather than disappearing.
 */
function useSheet(url: string, height: number, probe: boolean): { counts: number[]; failed: boolean; box: SpriteBox } {
  const [counts, setCounts] = useState<number[]>(() => new Array(SHEET.rows).fill(1));
  const [failed, setFailed] = useState(false);
  // Until the image has loaded there is nothing to measure, so the reference
  // sheet stands in — right for the common case, corrected for the rest.
  const [box, setBox] = useState<SpriteBox>(() => spriteBox(0, 0, height));
  // The natural size, kept so a resize re-measures without re-fetching: the
  // sheet has not changed, only how big it is drawn.
  const natural = useRef({ w: 0, h: 0 });

  useEffect(() => {
    let live = true;
    setFailed(false);
    const img = new Image();
    img.onerror = () => {
      if (live) setFailed(true);
    };
    // The sheet is same-origin with the node, and the canvas is read back —
    // without this the read taints and throws.
    img.crossOrigin = "anonymous";
    img.onload = () => {
      if (!live) return;
      // What the pet is actually drawn at, measured from the file rather than
      // assumed from the spec. The grid is the contract; the resolution is not.
      natural.current = { w: img.naturalWidth, h: img.naturalHeight };
      setBox(spriteBox(img.naturalWidth, img.naturalHeight, height));
      // A manifest that says its frames spares the sheet the canvas.
      if (!probe) return;
      try {
        const canvas = document.createElement("canvas");
        canvas.width = SHEET.width;
        canvas.height = SHEET.height;
        const ctx = canvas.getContext("2d", { willReadFrequently: false });
        if (!ctx) return;
        // Drawn at the sheet's own size whatever the file's dimensions are, so
        // a sheet that is not exactly 1536×1872 is scaled into the grid rather
        // than read off the end of it.
        ctx.drawImage(img, 0, 0, SHEET.width, SHEET.height);
        const { data } = ctx.getImageData(0, 0, SHEET.width, SHEET.height);
        setCounts(frameCounts(alphaProbe(data)));
      } catch (e) {
        // A tainted or oversized canvas: one frame per row is a pet that
        // stands still, which is better than one that flickers through blanks.
        log.debug("pet", "the sprite sheet could not be probed; the pet stands still", errorFields(e));
      }
    };
    img.src = url;
    return () => {
      live = false;
    };
    // `height` is deliberately absent: re-running this would re-decode the
    // sheet on every step of a size drag. The effect below rescales instead.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, probe]);

  useEffect(() => {
    setBox(spriteBox(natural.current.w, natural.current.h, height));
  }, [height]);

  return { counts, failed, box };
}

export function PetSprite({
  pet,
  def,
  state,
  className,
  height = PET_HEIGHT,
  onSize,
}: {
  pet: string;
  /** The manifest, when the caller holds it: its animations decide the frames. */
  def?: PetDef | null;
  state: PetState;
  className?: string;
  /** How tall to stand, in px. The width follows the sheet's cell aspect. */
  height?: number;
  /**
   * The box this pet occupies, once its sheet has been measured.
   *
   * The overlay needs it: whatever wraps the pet has to be the pet's own size,
   * or only part of it is grabbable and the rest hangs outside the thing that
   * is supposed to keep it on screen.
   */
  onSize?: (box: SpriteBox) => void;
}) {
  const url = api.petSpriteUrl(pet);
  const animated = Boolean(def?.animations && Object.keys(def.animations).length > 0);
  const { counts, failed, box } = useSheet(url, height, !animated);
  const [frame, setFrame] = useState(0);
  const row = rowOf(state);
  const plan = framePlan(def, state, counts[row] ?? 1);
  const frames = plan.frames;

  // A new state starts at its first frame rather than wherever the last one
  // happened to be — landing on frame 6 of a 2-frame row would show a blank.
  useEffect(() => setFrame(0), [state]);

  const still = prefersReducedMotion();
  // One timer per frame, each as long as that frame is shown.
  const durations = plan.durations.join(",");
  useEffect(() => {
    if (still || frames <= 1) return;
    const held = durations.split(",").map(Number);
    let at = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const tick = () => {
      at = (at + 1) % frames;
      setFrame(at);
      timer = setTimeout(tick, held[at] ?? held[0]);
    };
    timer = setTimeout(tick, held[0]);
    return () => {
      if (timer) clearTimeout(timer);
    };
  }, [durations, frames, state, still]);

  useEffect(() => onSize?.(box), [box, onSize]);

  const at = cellOffset(row, Math.min(frame, frames - 1), box);

  // The sheet never arrived. Say so at the size the pet would have been, rather
  // than leaving a transparent box that reads as nothing being there at all.
  if (failed) {
    return (
      <span
        title={t("pet-pet-sprite-pet-s-sprite-sheet-could-not")}
        className="flex items-center justify-center rounded-card border border-dashed border-border bg-surface/80 text-text-dim"
        style={{ width: box.width, height: box.height }}
      >
        <ICON.note size={16} aria-hidden />
      </span>
    );
  }

  return (
    <span
      aria-hidden
      className={className}
      style={{
        width: box.width,
        height: box.height,
        backgroundImage: `url("${url}")`,
        backgroundSize: t("pet-pet-sprite-px-px", { sheetWidth: box.sheetWidth, sheetHeight: box.sheetHeight }),
        backgroundPosition: t("pet-pet-sprite-px-px-2", { x: at.x, y: at.y }),
        backgroundRepeat: "no-repeat",
        imageRendering: "pixelated",
      }}
    />
  );
}
