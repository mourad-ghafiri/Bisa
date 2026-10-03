/**
 * A rail of icon tabs at a panel's edge — the vertical strip the Project
 * IDE's right panel and the Workflow Designer's panel both wear: every
 * occupant a glyph at one spacing, the showing one marked on the rail's outer
 * edge, on screen whether or not the column is. Icon only; the name (and a
 * chord, a reason it is muted) is the tooltip and the accessible name. A
 * muted tab is drawn as absent — a dashed edge, the kit's mark for "not
 * here" — and is still pressed: the owner answers the press as it sees fit,
 * the way a chord would be answered. Its glyph keeps full `text-dim` ink:
 * nothing on glass is drawn through alpha. The rules — one press
 * rule, focus by ↑↓/Home/End, one tab stop — are `iconRailModel.mjs`; this
 * file only paints.
 */

import type { KeyboardEvent, ReactNode } from "react";
import { cn } from "./cn";
import type { LucideIcon } from "./icons";
import type { Mark } from "./harnessMarks";
import { nextRailIndex } from "./iconRailModel.mjs";
import { FOCUS_RING } from "./rings";
import { Tooltip } from "./Tooltip";

export interface IconRailItem<Id extends string = string> {
  id: Id;
  /** A kit glyph, or a tool's mark (the Git tab's). */
  icon: LucideIcon | Mark;
  /** The accessible name — one word or two. */
  label: string;
  /** What the tooltip says; the label when absent. */
  tooltip?: string;
  showing: boolean;
  /** Drawn as absent (a dashed edge), `aria-disabled`; still pressed. */
  muted?: boolean;
  /** A mark in the tab's corner — a dot with its sentence. */
  badge?: ReactNode;
}

function onArrowKeys(e: KeyboardEvent<HTMLElement>): void {
  const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("[data-rail-item]"));
  const next = nextRailIndex(e.key, items.indexOf(document.activeElement as HTMLElement), items.length);
  if (next === null) return;
  e.preventDefault();
  items[next]?.focus();
}

export function IconRail<Id extends string>({
  label,
  items,
  anchor,
  onPress,
  className,
}: {
  /** The rail's accessible name. */
  label: string;
  items: readonly IconRailItem<Id>[];
  /** The one tab stop (`iconRailModel.railAnchor`); the first item when absent. */
  anchor?: Id | null;
  onPress: (id: Id) => void;
  className?: string;
}) {
  const stop = anchor ?? items[0]?.id ?? null;
  return (
    <nav
      aria-label={label}
      role="tablist"
      aria-orientation="vertical"
      onKeyDown={onArrowKeys}
      className={cn("flex w-10 shrink-0 flex-col items-center gap-1 border-l border-border bg-surface py-1.5", className)}
    >
      {items.map((item) => {
        const Icon = item.icon;
        return (
          <Tooltip key={item.id} label={item.tooltip ?? item.label} side="left">
            <button
              type="button"
              role="tab"
              aria-selected={item.showing}
              aria-disabled={!!item.muted}
              aria-label={item.label}
              data-rail-item
              tabIndex={item.id === stop ? 0 : -1}
              onClick={() => onPress(item.id)}
              className={cn(
                "anim relative flex h-8 w-8 shrink-0 items-center justify-center rounded-control",
                FOCUS_RING,
                item.showing ? "bg-selected text-text" : item.muted ? "border border-dashed border-border text-text-dim hover:bg-surface-2" : "text-text-dim hover:bg-surface-2 hover:text-text",
              )}
            >
              {/* The mark sits on the rail's outer edge — the right — where the tab's column ends. */}
              {item.showing && <span aria-hidden className="absolute inset-y-1.5 -right-1 w-0.5 rounded-full bg-text/70" />}
              <Icon size={15} aria-hidden />
              {/* The mark sits in the button's own corner, inside the showing bar's column. */}
              {item.badge && <span className="pointer-events-none absolute right-1 top-1 inline-flex">{item.badge}</span>}
            </button>
          </Tooltip>
        );
      })}
    </nav>
  );
}
