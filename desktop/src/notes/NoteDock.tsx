/**
 * The collapsed overlay: a button you can drag anywhere.
 *
 * The drag itself is `ui/Dock`'s, which is where it moved when the pet became
 * its second caller — the click-versus-drag threshold, the anchoring to the
 * nearest edges, the resize re-clamp and the keep-out over the window chrome
 * are correctness rules, and two copies of a correctness rule is one copy that
 * will not learn the next thing.
 *
 * # Why it is styled unlike every other button
 *
 * It floats over arbitrary content with nothing around it, which is the
 * opposite of the situation the rest of the kit is tuned for: a button in a
 * toolbar is found by its neighbours, and this one has none. It was drawn as
 * `bg-surface` + `text-text-dim` — the ordinary quiet-control pair — and in a
 * dark theme that is a `0.22` fill on a `0.18` background, a four-hundredths
 * lightness step, with a dim glyph inside it. It read as a smudge.
 *
 * So: the raised fill rather than the flat one, the full-strength text colour,
 * and **two edges instead of a shadow**. A drop shadow is how a light theme
 * says "above"; on a dark ground a black shadow is invisible, so the ring
 * carries the separation there and the shadow carries it in the light. Neither
 * is load-bearing alone, which is what makes this hold across all four themes.
 *
 * Still not accent, and that is not an oversight — `theme/tokens.css` reserves
 * accent for *your attention*, and a scratchpad you left open is never a
 * summons. Contrast is the fix here; colour would be a lie.
 */

import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import { DOCK_SIZE, dockBox, dockStyle, useDockDrag, useDockViewport } from "../ui/Dock";
import { moveDock, toggleNotes, useNotesOverlay } from "./notesStore";
import { t } from "../i18n/l10n.mjs";

export function NoteDock({
  count,
  showCount = true,
}: {
  count?: number;
  /**
   * Whether to *paint* the count. The accessible name uses `count` either
   * way — hiding the badge is about pixels, and a screen reader losing the
   * number would be a different, worse change than the one being asked for.
   */
  showCount?: boolean;
}) {
  const { dock } = useNotesOverlay();
  const viewport = useDockViewport(DOCK_SIZE);
  // The paint: the stored placement projected into this window and clamped
  // inside it. The drag starts from it, so a clamped dock does not jump.
  const box = dockBox(dock, viewport);
  const drag = useDockDrag(box, viewport, moveDock);

  return (
    <button
      type="button"
      aria-label={count ? t("notes-note-dock-notes-2", { count }) : t("notes-note-dock-notes")}
      aria-keyshortcuts="Alt+N"
      title={t("notes-note-dock-notes")}
      style={dockStyle(box)}
      onPointerDown={drag.onPointerDown}
      onClick={() => {
        // A gesture that moved was a drag, and finishing a drag must not also
        // open the panel.
        if (drag.wasClick()) toggleNotes();
      }}
      className={cn(
        "anim fixed z-40 flex items-center justify-center rounded-full",
        // The raised fill and the full-strength glyph: see the note above.
        "border border-border bg-surface-2 text-text",
        // Both separations, so neither theme depends on the one that fails in
        // it. `ring-inset` keeps the ring inside the 48px box the clamp knows
        // about, rather than growing the thing it is keeping on screen.
        "shadow-lg ring-1 ring-inset ring-text/10",
        // Hover strengthens the *edge*, not the fill. `surface` is lighter
        // than `surface-2` in a light theme and darker in a dark one, so a
        // fill swap would brighten on one and recede on the other.
        "hover:border-text-dim hover:ring-text/25 active:scale-95",
        drag.dragging ? "cursor-grabbing" : "cursor-grab",
      )}
    >
      <ICON.note size={20} strokeWidth={2} aria-hidden />
      {showCount && Boolean(count) && (
        <span
          aria-hidden
          // Reads against the button's own fill, not the page's — it sits on
          // `surface-2` now, so the old `surface-2` badge would have vanished
          // into it.
          className="tnum absolute -top-0.5 -right-0.5 min-w-4 rounded-full border border-border bg-bg px-1 text-3xs leading-tight text-text"
        >
          {count}
        </span>
      )}
    </button>
  );
}
