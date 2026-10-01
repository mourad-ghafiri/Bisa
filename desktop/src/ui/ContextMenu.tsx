/**
 * Right-click on a row.
 *
 * The same `MenuItem` shape as {@link Menu}, on purpose: a row's context menu
 * and the `⋮` menu in its hover actions should offer the same verbs in the
 * same order, and sharing the type is what makes passing the same array the
 * obvious thing to do. It is a surface like the dropdown, so the browser
 * tab's native page yields while it shows (`openSurfaces.ts`) — a menu from
 * the explorer or the Details pane is drawn over the page, never under it.
 *
 * Radix opens this on the keyboard's context-menu key and on a long press as
 * well as on right-click, so the actions are not mouse-only.
 */

import * as C from "@radix-ui/react-context-menu";
import type { ReactNode } from "react";
import { cn } from "./cn";
import { keyLabel } from "./KeyHint";
import type { MenuItem } from "./Menu";
import { useOpenSurface } from "./openSurfaces";
import { MENU_ITEM, MENU_ITEM_TONE, POPOVER_SURFACE } from "./surfaces";
import { useChosenOnClose } from "./useChosenOnClose";

export function ContextMenu({
  items,
  children,
  className,
  selected,
}: {
  items: MenuItem[];
  /** The region that owns the menu — usually one row. */
  children: ReactNode;
  className?: string;
  /** The row is the one in hand: `data-selected` on the region, so its `row-actions` stay revealed (`styles.css`). */
  selected?: boolean;
}) {
  // The chosen action runs once the menu has left (`useChosenOnClose`).
  const chosen = useChosenOnClose();
  // A native layer (the browser tab) yields while the menu shows.
  const [, setOpen] = useOpenSurface();
  // No verbs, no menu — but the region keeps its box: a caller's sizing
  // (`min-h-0 flex-1` on a tree that fills its column) must not depend on
  // whether a menu has items today.
  if (items.length === 0) {
    return (
      <div className={className} data-selected={selected || undefined}>
        {children}
      </div>
    );
  }
  return (
    <C.Root onOpenChange={setOpen}>
      <C.Trigger className={className} data-selected={selected || undefined}>
        {children}
      </C.Trigger>
      <C.Portal>
        <C.Content
          data-pane
          collisionPadding={8}
          className={cn(POPOVER_SURFACE, "min-w-44 py-1")}
          onCloseAutoFocus={chosen.onCloseAutoFocus}
        >
          {items.map((item, i) => (
            <div key={`${i}:${item.label}`}>
              {item.separatorBefore && <C.Separator className="my-1 h-px bg-border" />}
              <C.Item
                disabled={item.disabled}
                onSelect={() => chosen.select(item)}
                className={cn(
                  MENU_ITEM,
                  item.danger ? MENU_ITEM_TONE.danger : MENU_ITEM_TONE.default,
                )}
              >
                {item.icon && <item.icon size={14} className="shrink-0 opacity-70" />}
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.shortcut && <span className="ml-3 shrink-0 text-2xs text-text-dim">{keyLabel(item.shortcut)}</span>}
              </C.Item>
            </div>
          ))}
        </C.Content>
      </C.Portal>
    </C.Root>
  );
}
