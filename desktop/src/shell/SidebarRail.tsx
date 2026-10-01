/**
 * The sidebar collapsed: a rail of icons (guide/the-desktop.md §The
 * sidebar). The seven destinations, then Channels and Messages — the rail
 * has no room for the lists, so each is one door with its unread summed —
 * each an icon with its name and its marks in the tooltip and the accessible
 * name, the Inbox with its count in the icon's corner: everything not yet
 * dealt with, accented while any of it needs the person
 * (`sidebarModel.inboxBadge`) — `9+` past nine, the exact numbers in the
 * tooltip; the mark hangs a few pixels outside the door, into the rail's
 * gutter, which is why the doors' scrollport is the rail's full width. The doors are `railDoors`' facts; this draws.
 * The travelling highlight is the expanded sidebar's alone (`SidebarSection`
 * is the one file outside the kit that imports `motion`); the rail marks the
 * active door with a bar, as the occupant rail does. The toggle that expands
 * the sidebar is the top chrome's alone: one door for one state.
 *
 * The destinations' doors stand in the person's order and can be dragged to
 * a new one here as in the expanded sidebar — one `SortableList` over the
 * same store (`navOrderStore`), so the rail and the rows never disagree.
 * Channels and Messages are the rail's own doors and stay where they are.
 */

import { section, useRoute } from "../router";
import { CountBadge, FOCUS_RING, ICON, SortableList, Tooltip, WorkingDot, cn, navRowDrag } from "../ui";
import type { LucideIcon, SortableHandle } from "../ui";
import type { NavEntry } from "./nav";
import { placeNav, usePrimaryNav } from "./navOrderStore";
import { useSectionHref } from "./sectionDoor";
import { onArrowKeys } from "./SidebarSection";
import { railDoors, railTooltip } from "./sidebarModel.mjs";
import type { RailDoor } from "./sidebarModel.mjs";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

/** The glyph the rail's own two doors wear. */
function fixedGlyph(key: string): LucideIcon {
  return key === "channels" ? ICON.channel : ICON.dm;
}

function RailDoorLink({ door, active, icon: Icon, handle }: { door: RailDoor; active: boolean; icon: LucideIcon; handle?: SortableHandle }) {
  const words = railTooltip(door);
  // Where the person was in the section, not its bare index (`sectionDoor.ts`).
  const to = useSectionHref(door.route);
  return (
    <Tooltip label={words} side="right">
      <a
        ref={handle?.ref}
        style={handle?.style}
        {...handle?.props}
        role={undefined}
        href={to}
        aria-label={words}
        aria-current={active ? "page" : undefined}
        data-nav-item
        className={cn(
          "anim relative flex h-8 w-8 shrink-0 items-center justify-center rounded-control",
          FOCUS_RING,
          active ? "bg-accent-soft text-accent-ink" : "text-text-dim hover:bg-surface-2 hover:text",
          handle?.dragging && "cursor-grabbing",
        )}
      >
        {/* The mark sits on the rail's outer edge — the right — where the sidebar's column ends. */}
        {active && <span aria-hidden className="absolute inset-y-1.5 -right-1 w-0.5 rounded-full bg-accent" />}
        <Icon size={15} aria-hidden />
        {(door.badge || door.live) && (
          <span className="pointer-events-none absolute -right-1 -top-1 inline-flex items-center gap-0.5">
            {door.badge && <CountBadge count={door.badge.count} tone={door.badge.tone} title={door.badge.title ?? undefined} size="sm" />}
            {door.live && <WorkingDot title={door.live} />}
          </span>
        )}
      </a>
    </Tooltip>
  );
}

export function SidebarRail() {
  const route = useRoute();
  const active = section(route);
  const ws = useWorkspace();
  const nav = usePrimaryNav();
  const doors = railDoors(ws, nav);
  // The destinations, draggable; then the rail's own two doors, fixed.
  const destinations = doors.slice(0, nav.length).map((door, i) => ({ id: door.key, door, entry: nav[i] }));
  const fixed = doors.slice(nav.length);
  return (
    <div data-pane className="flex h-full min-h-0 flex-col items-center bg-surface">
      {/* The scrollport is the rail's whole width, never the doors' column: a
          box that scrolls on one axis clips on both, and a count hangs
          `RAIL_OVERHANG` outside its door — into a gutter that must be inside
          this box to be drawn (`sidebarModel.railGutter`). */}
      <nav aria-label={t("shell-sidebar-workspace")} className="flex min-h-0 w-full flex-1 flex-col items-center gap-0.5 overflow-y-auto pt-2 pb-2" onKeyDown={onArrowKeys}>
        <SortableList<{ id: string; door: RailDoor; entry: NavEntry }>
          items={destinations}
          direction="vertical"
          dragData={({ entry }) => navRowDrag(entry.key, entry.label, entry.glyph)}
          onReorder={placeNav}
        >
          {({ door, entry }, handle) => (
            <div className="flex flex-col items-center">
              <RailDoorLink door={door} active={active === door.key} icon={entry.icon} handle={handle} />
            </div>
          )}
        </SortableList>
        {fixed.map((door, i) => (
          <div key={door.key} className={cn("flex flex-col items-center", i === 0 && "mt-2 border-t border-border pt-2")}>
            <RailDoorLink door={door} active={active === door.key} icon={fixedGlyph(door.key)} />
          </div>
        ))}
      </nav>
      {ws.offline && (
        <Tooltip label={t("shell-sidebar-rail-node-unreachable", { offline: ws.offline })} side="right">
          <span className="mb-2 flex h-8 w-8 shrink-0 items-center justify-center text-danger" aria-label={t("shell-sidebar-rail-node-unreachable", { offline: ws.offline })}>
            <ICON.warn size={14} aria-hidden />
          </span>
        </Tooltip>
      )}
    </div>
  );
}
