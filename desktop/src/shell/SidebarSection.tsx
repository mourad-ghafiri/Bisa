/**
 * The parts a sidebar is made of: a collapsible group, and the row inside it.
 *
 * Every row is the same height whatever it points at — a screen, a goal, a
 * channel, a person, an agent — because in a workspace they are all just places
 * you go. Two things are load-bearing beyond the shape:
 *
 * Every row is a real `<a>` or `<button>`. A `div` with an `onClick` is
 * unreachable by keyboard, invisible to a screen reader's link list, and
 * cannot be middle-clicked or opened in a second window — and a sidebar whose
 * rows are not links is a sidebar you can only use with a mouse.
 *
 * The active indicator is one element that travels between rows rather than a
 * background class on whichever row happens to be current. That is why it can
 * animate at all: `layoutId` lets `motion` recognise the highlight in its new
 * position as the same object, so moving between two nav items reads as one
 * marker sliding rather than two independent flashes. `useMotionTiming` reads
 * the duration from `--motion-*`, which collapses to zero under reduced
 * motion, so the marker jumps instead of travelling without this file having
 * to know the preference exists.
 *
 * `motion/react` is imported here directly, which no *view* may do. The kit
 * deliberately exports the timing rather than the library, and the shell is a
 * peer of the kit rather than a consumer of it; `ui/motion.ts` names this
 * exact case — "an indicator sliding between two elements" — as the thing
 * JavaScript is still needed for.
 */

import { motion } from "motion/react";
import { type KeyboardEvent, type ReactNode } from "react";
import { ICON, SectionHeader, cn, useCollapsed, useMotionTiming, type LucideIcon, type SortableHandle } from "../ui";

// `useCollapsed` used to live here. It moved into the kit beside
// `SectionHeader` when the Goals screen needed it: a view may not import
// from `shell/`, so a screen could have the disclosure control and no memory
// of it. Re-exported so the shell's own callers keep one import site.
export { useCollapsed };

/**
 * Up and down move within one group; Tab still leaves it.
 *
 * A sidebar with forty rows is forty Tab stops between the search box and the
 * screen, so the arrow keys have to be the way through a section and Tab has
 * to be the way past it. Focus clamps at each end rather than wrapping,
 * because a wrap is indistinguishable from having gone nowhere.
 */
export function onArrowKeys(e: KeyboardEvent<HTMLElement>): void {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "Home" && e.key !== "End") return;
  const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("[data-nav-item]"));
  if (items.length === 0) return;
  const at = items.indexOf(document.activeElement as HTMLElement);
  const last = items.length - 1;
  let next: number;
  if (e.key === "Home") next = 0;
  else if (e.key === "End") next = last;
  else if (at === -1) next = e.key === "ArrowDown" ? 0 : last;
  else next = Math.min(last, Math.max(0, at + (e.key === "ArrowDown" ? 1 : -1)));
  e.preventDefault();
  items[next]?.focus();
}

/**
 * The travelling highlight. One per document — that is what makes it travel.
 * It is the neutral `selected`, never the accent: the row you stand on is
 * where you are, and the accent in this sidebar is kept for what waits on
 * you — the Inbox's count, an agent writing — so it is never lost among
 * the places.
 */
const ACTIVE_LAYOUT_ID = "sidebar-active";

function ActiveMarker() {
  const timing = useMotionTiming("fast");
  return (
    <motion.span
      layoutId={ACTIVE_LAYOUT_ID}
      // Position only. Dragging the sidebar's edge re-renders on every
      // pointer move, and animating the marker's *width* would leave it
      // trailing the column it sits in for the length of the drag — the
      // resize would read as a drag against resistance.
      layout="position"
      transition={timing}
      aria-hidden
      className="absolute inset-0 rounded-control bg-selected"
    />
  );
}

export function SidebarRow({
  href,
  active,
  icon: Icon,
  leading,
  label,
  sub,
  trailing,
  prominent,
  title,
  handle,
}: {
  href: string;
  active?: boolean;
  /** From `ui/icons`; use `leading` instead when the row wants an avatar. */
  icon?: LucideIcon;
  leading?: ReactNode;
  label: ReactNode;
  sub?: ReactNode;
  trailing?: ReactNode;
  /**
   * A destination rather than a thing inside one. Primary nav sits at full
   * text weight so the eight places you can go do not read as eight more
   * items in a list of forty.
   */
  prominent?: boolean;
  title?: string;
  /**
   * From a `SortableList`, when the row can be dragged to a new place in its
   * list — the primary nav. The handle's props make the row the drag
   * source (pointer past the kit's distance; Space lifts, arrows move,
   * Space drops) and its style slides the row aside while another passes.
   * The row stays a link: dnd-kit's `role="button"` is not spread over it.
   */
  handle?: SortableHandle;
}) {
  return (
    <a
      ref={handle?.ref}
      style={handle?.style}
      {...handle?.props}
      role={undefined}
      href={href}
      title={title}
      data-nav-item
      aria-current={active ? "page" : undefined}
      className={cn(
        "group anim relative flex h-row shrink-0 items-center gap-2 rounded-control px-2 text-xs",
        prominent && "font-medium",
        handle?.dragging && "cursor-grabbing",
        active
          ? "font-medium text-text"
          : prominent
            ? "text-text hover:bg-surface-2"
            : "text-text-dim hover:bg-surface-2 hover:text-text",
      )}
    >
      {active && <ActiveMarker />}
      {Icon && <Icon size={14} aria-hidden className="relative shrink-0" />}
      {leading && <span className="relative flex shrink-0 items-center">{leading}</span>}
      <span className="relative min-w-0 flex-1 truncate">
        {label}
        {sub && <span className="ml-1 text-2xs text-text-dim">{sub}</span>}
      </span>
      {trailing && <span className="relative flex shrink-0 items-center gap-1.5">{trailing}</span>}
    </a>
  );
}

/**
 * A group of rows under a heading that collapses.
 *
 * `empty` is a door rather than prose, on the same principle as the kit's
 * `EmptyState`: a section that says "no channels" and stops has told the
 * reader nothing they could not see.
 */
export function SidebarSection({
  id,
  title,
  count,
  action,
  children,
  empty,
}: {
  id: string;
  title: string;
  count?: number;
  action?: ReactNode;
  children: ReactNode;
  empty?: ReactNode;
}) {
  const [collapsed, toggle] = useCollapsed(id);
  const isEmpty = Array.isArray(children) ? children.length === 0 : !children;
  return (
    <section className="mt-4" onKeyDown={onArrowKeys}>
      <SectionHeader
        title={title}
        count={count}
        open={!collapsed}
        onToggle={toggle}
        action={action}
      />
      {!collapsed && (
        <div className="mt-0.5 flex flex-col">
          {isEmpty && empty ? <div className="px-0.5">{empty}</div> : children}
        </div>
      )}
    </section>
  );
}

/**
 * A section's empty door: the one verb that fills it, drawn as a quiet row
 * with a "+" so it stands where the first row will — the same height, the
 * same inset — rather than as a link in the accent, which in this sidebar
 * means something is waiting on you. A door that goes somewhere rather than
 * adds wears that place's glyph, and a `note` beneath says why the section
 * is empty.
 */
export function EmptyDoor({ onClick, label, icon: Icon = ICON.add, note }: { onClick: () => void; label: string; icon?: LucideIcon; note?: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="anim flex min-h-row w-full items-center gap-2 rounded-control px-1.5 py-1 text-left text-xs text-text-dim hover:bg-surface-2 hover:text-text"
    >
      <Icon size={14} aria-hidden className="shrink-0" />
      <span className="flex min-w-0 flex-col">
        <span className="min-w-0 truncate">{label}</span>
        {note && <span className="text-2xs text-text-dim">{note}</span>}
      </span>
    </button>
  );
}

/**
 * A "+" that belongs to a section header.
 *
 * It is a real button with a real accessible name; `row-actions` fades it in
 * on hover *and* on focus-within, so reaching it with the keyboard is not
 * reaching for something invisible.
 */
export function AddButton({
  onClick,
  label,
  icon: Icon,
}: {
  onClick: () => void;
  label: string;
  icon: LucideIcon;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="anim flex h-5 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
    >
      <Icon size={12} aria-hidden />
    </button>
  );
}
