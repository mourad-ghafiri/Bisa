/**
 * A strip of tabs you can close, reorder and right-click.
 *
 * Separate from {@link Tabs}, which is Radix and deliberately stays that way:
 * that one is a *view switcher* over a fixed set — Details / Work / Projects —
 * where Radix's roving tabindex, Home/End and arrow-wrap are exactly right and
 * nothing is ever added or removed. This one is a *document strip*: the set
 * changes as you work, every tab can be closed, and each carries a glyph and
 * sometimes a status note. Two behaviours under one component would mean a
 * `closeable` prop threaded through Radix's collection model and a close control
 * that has to sit outside `Tabs.Trigger` to avoid nesting a button in a button.
 *
 * There is one of these rather than two because the terminal panel and the
 * workbench both need it, and a second copy is how the two drift — which is the
 * rule `ui/index.ts` exists to state.
 *
 * # The close control is a `<span>`, and that is not laziness
 *
 * A `<button>` inside a `<button>` is invalid, and in practice the outer one
 * takes every click. So the `×` is a span with a pointer handler for the mouse, and
 * the keyboard gets **Delete or Backspace on the focused tab**, announced with
 * `aria-keyshortcuts`. That is the ARIA pattern for closeable tabs, and it keeps
 * a strip of eight terminals at eight tab stops instead of sixteen.
 *
 * Middle-click closes too, because every tab strip in every editor does.
 *
 * # Reorder
 *
 * With `dragData` and `onReorder`, the strip is a `SortableList`: a tab
 * dragged along it slides its neighbours aside and takes the slot it is
 * dropped on; from the keyboard, Space lifts the focused tab, the arrows move
 * it and Space drops it. A tab from *another* strip dropped here goes to
 * `onDropForeign`, which is how a pane takes its neighbour's tab.
 *
 * # Menus
 *
 * With `menuFor`, each tab has a context menu (right-click, the menu key, a
 * long press) whose items the caller decides — the strip knows nothing about
 * pins or panes, it only draws what it is handed.
 */

import type { KeyboardEvent, ReactNode } from "react";
import { cn } from "./cn";
import { ContextMenu } from "./ContextMenu";
import { SortableList, type DragData, type SortableHandle } from "./dnd";
import { ICON, type LucideIcon } from "./icons";
import type { Mark } from "./harnessMarks";
import type { MenuItem } from "./Menu";
import { FOCUS_RING } from "./rings";
import { t as tr } from "../i18n/l10n.mjs";

/**
 * A control in a strip's trailing slot — pin, split, close pane, new shell:
 * one shape for the document strip's and the terminal strip's, so the two
 * slots read as one bar.
 */
export function StripControlButton({
  label,
  title,
  onClick,
  disabled = false,
  active = false,
  children,
}: {
  label: string;
  title?: string;
  onClick: () => void;
  disabled?: boolean;
  /** The control is on — a pinned tab's pin. */
  active?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title ?? label}
      disabled={disabled}
      onClick={onClick}
      className={cn("anim shrink-0 rounded-control p-1 text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45", FOCUS_RING, active && "text-accent-ink")}
    >
      {children}
    </button>
  );
}

export interface StripTab {
  id: string;
  label: string;
  /** The full thing the label is short for — tooltip and accessible name. */
  title?: string;
  /** A kit icon, or a harness's own mark on a harness's tab. */
  icon?: LucideIcon | Mark;
  /** A short status after the label: `exited (137)`, `3 ahead`. */
  note?: string;
  noteTone?: "dim" | "danger";
  /**
   * Belongs somewhere other than what is on screen.
   *
   * Dimmed rather than hidden: a terminal you cannot find is worse than a strip
   * that is honest about what is running.
   */
  dimmed?: boolean;
  /** False for a pinned tab — which then has no `×` and ignores Delete. */
  closeable?: boolean;
  /** Unsaved changes: a dot where the close control sits until the pointer arrives. */
  dirty?: boolean;
  /** A preview (ide/03): opened by a glance, in italics, replaced by the next glance until it is kept. */
  preview?: boolean;
}

export function TabStrip({
  tabs,
  active,
  onSelect,
  onDoubleClick,
  onClose,
  onReorder,
  dragData,
  onDropForeign,
  menuFor,
  label,
  size = "md",
  trailing,
  className,
}: {
  tabs: StripTab[];
  active: string | null;
  onSelect: (id: string) => void;
  /** A double-click on a tab. */
  onDoubleClick?: (id: string) => void;
  /** Absent means nothing in this strip closes. */
  onClose?: (id: string) => void;
  /** A tab dropped on its own strip: put `id` at `index`. Needs `dragData`. */
  onReorder?: (id: string, index: number) => void;
  /** What a tab carries when dragged; null for a tab that stays put. Absent means no drag. */
  dragData?: (id: string) => DragData | null;
  /** A drag from elsewhere — another strip's tab — ended on this strip. */
  onDropForeign?: (data: DragData) => void;
  /** The context menu of one tab; empty means none. */
  menuFor?: (id: string) => MenuItem[];
  /** Names the strip for assistive technology. */
  label: string;
  size?: "sm" | "md";
  /** Controls after the tabs — a `+`, a count, a collapse toggle. */
  trailing?: ReactNode;
  className?: string;
}) {
  const renderTab = (t: StripTab, handle: SortableHandle) => {
    const on = t.id === active;
    const closes = t.closeable !== false && onClose ? () => onClose(t.id) : null;
    const Glyph = t.icon;
    const items = menuFor ? menuFor(t.id) : [];
    const dragKey = handle.props.onKeyDown as ((e: KeyboardEvent) => void) | undefined;
    const button = (
      <button
        ref={handle.ref}
        style={handle.style}
        {...handle.props}
        type="button"
        role="tab"
        tabIndex={on ? 0 : -1}
        aria-selected={on}
        aria-keyshortcuts={closes ? "Delete" : undefined}
        title={t.title ?? t.label}
        onClick={() => onSelect(t.id)}
        onDoubleClick={onDoubleClick ? () => onDoubleClick(t.id) : undefined}
        onAuxClick={(e) => {
          if (e.button === 1 && closes) {
            e.preventDefault();
            closes();
          }
        }}
        onKeyDown={(e) => {
          dragKey?.(e);
          if (e.defaultPrevented) return;
          if (!closes || (e.key !== "Delete" && e.key !== "Backspace")) return;
          e.preventDefault();
          closes();
        }}
        className={cn(
          "anim group flex shrink-0 items-center gap-1.5 border-b-2",
          FOCUS_RING,
          // The density tokens, as every row uses them — a compact setting compacts the strip.
          size === "sm" ? "h-row-sm max-w-52 px-2 text-2xs" : "h-row max-w-56 px-3 text-xs",
          on ? "border-accent text-text" : "border-transparent text-text-dim hover:bg-surface-2 hover:text-text",
          t.dimmed && !on && "opacity-55",
          handle.dragging && "cursor-grabbing",
        )}
      >
        {Glyph && <Glyph size={11} aria-hidden className="shrink-0" />}
        <span className={cn("min-w-0 truncate", t.preview && "italic")}>{t.label}</span>
        {t.note && <span className={cn("shrink-0 text-3xs", t.noteTone === "danger" ? "text-danger" : "text-text-dim")}>{t.note}</span>}
        {t.dirty && (
          <span aria-label={tr("ui-file-tree-unsaved-changes")} className="shrink-0 text-accent group-hover:hidden">
            ●
          </span>
        )}
        {closes && (
          <span
            aria-hidden
            onPointerDown={(e) => {
              // Ahead of the tab's own click, so closing does not first
              // select the thing it is about to remove.
              e.preventDefault();
              e.stopPropagation();
              closes();
            }}
            // The kit's one reveal: `row-actions` fades in on hover *and* on
            // keyboard focus within the tab, so a keyboard user sees the ×.
            className={cn("row-actions anim shrink-0 rounded px-0.5 hover:text-danger", t.dirty && "group-hover:inline")}
          >
            <ICON.close size={11} />
          </span>
        )}
      </button>
    );
    return items.length > 0 ? (
      <ContextMenu key={t.id} items={items} className="inline-flex shrink-0">
        {button}
      </ContextMenu>
    ) : (
      button
    );
  };

  return (
    <div role="tablist" aria-label={label} data-tab-strip className={cn("flex min-w-0 items-center gap-1", className)}>
      <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
        {dragData && onReorder ? (
          <SortableList items={tabs} direction="horizontal" dragData={(t) => dragData(t.id)} onReorder={onReorder} onDropForeign={onDropForeign}>
            {renderTab}
          </SortableList>
        ) : (
          tabs.map((t) => renderTab(t, { ref: () => undefined, props: {}, style: undefined, dragging: false }))
        )}
      </div>
      {/* Outside the tab list: a non-`tab` child of a `tablist` is invalid ARIA. */}
      {trailing}
    </div>
  );
}
