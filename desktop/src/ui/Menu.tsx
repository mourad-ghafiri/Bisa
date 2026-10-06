/**
 * A dropdown menu.
 *
 * The previous version implemented roving focus, outside-click dismissal and
 * Escape by hand, and its trigger was a `<div onClick>` — reachable with a
 * mouse and invisible to the keyboard. Radix brings typeahead, arrow-key
 * navigation that skips disabled items, focus return to the trigger on close,
 * and collision-aware placement, and it makes the trigger a real button.
 *
 * `trigger` stays a `ReactNode` rather than becoming an `asChild` element,
 * because most call sites pass a styled `<span>`. Wrapping it in the trigger
 * button is what turns those into keyboard-reachable controls without
 * touching a single view. A call site that passes a button — a `<button>`,
 * or the kit's `Button` — gets the opposite treatment: that button *is* the
 * trigger (`isButtonElement`, `asChild`), never a button inside the one
 * Radix renders, which is invalid HTML and two controls to a screen reader.
 */

import * as M from "@radix-ui/react-dropdown-menu";
import type { ReactNode } from "react";
import { useOpenSurface } from "./openSurfaces";
import { useChosenOnClose } from "./useChosenOnClose";
import { cn } from "./cn";
import { FOCUS_RING } from "./rings";
import type { LucideIcon } from "./icons";
import type { Mark } from "./harnessMarks";
import { isButtonElement } from "./triggers";
import { keyLabel } from "./KeyHint";
import { MENU_ITEM, MENU_ITEM_TONE, POPOVER_SURFACE } from "./surfaces";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** From `ui/icons`, never from `lucide-react` directly. */
  /** A kit icon, or a harness's own mark — both take `size` and `className`. */
  icon?: LucideIcon | Mark;
  /** Draw a rule above this item — a group boundary, not decoration. */
  separatorBefore?: boolean;
  /** The chord that means the same thing, from the keymap — shown, never bound here. */
  shortcut?: string | null;
  /**
   * Run inside the pointer's own event, not after the menu has left — for an
   * action the webview allows only in a user gesture: a file input's click,
   * which opens the file dialog. An action that mounts a field must not say
   * this: the menu's focus return would blur the field it opened.
   */
  immediate?: boolean;
}

export function Menu({
  trigger,
  items,
  align = "end",
  label,
}: {
  trigger: ReactNode;
  items: MenuItem[];
  align?: "start" | "center" | "end";
  /** An accessible name for the trigger, when the trigger is a glyph. */
  label?: string;
}) {
  // The chosen action runs once the menu has left (`useChosenOnClose`).
  const chosen = useChosenOnClose();
  // A native layer (the browser tab) yields while the menu shows.
  const [, setOpen] = useOpenSurface();
  return (
    <M.Root onOpenChange={setOpen}>
      {isButtonElement(trigger) ? (
        // The button given is the trigger: Radix merges its own props onto
        // it (the child's `aria-label`, `disabled` and `className` win).
        <M.Trigger asChild aria-label={label} disabled={items.length === 0} className={FOCUS_RING}>
          {trigger}
        </M.Trigger>
      ) : (
        <M.Trigger
          aria-label={label}
          disabled={items.length === 0}
          className={cn("inline-flex items-center rounded disabled:opacity-45", FOCUS_RING)}
        >
          {trigger}
        </M.Trigger>
      )}
      <M.Portal>
        <M.Content
          data-pane
          align={align}
          sideOffset={4}
          collisionPadding={8}
          className={cn(POPOVER_SURFACE, "min-w-40 py-1")}
          onCloseAutoFocus={chosen.onCloseAutoFocus}
        >
          {items.map((item, i) => (
            <div key={`${i}:${item.label}`}>
              {item.separatorBefore && <M.Separator className="my-1 h-px bg-hairline" />}
              <M.Item
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
              </M.Item>
            </div>
          ))}
        </M.Content>
      </M.Portal>
    </M.Root>
  );
}
