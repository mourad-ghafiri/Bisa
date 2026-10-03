/**
 * One value out of a few, each with a reason attached — the dropdown
 * sibling of `SegmentedControl`, for the foot of a composer where three tabs
 * would crowd the row, or where a choice's meaning is a sentence rather than
 * a tooltip.
 *
 * Built on Radix's `RadioGroup`/`RadioItem` over `Menu`'s surface
 * (`POPOVER_SURFACE`, `MENU_ITEM`) and its open-state accounting
 * (`useOpenSurface`), so it reads as the same family and the embedded
 * browser yields while it is open. Not a `Menu`: a `MenuItem` is an action,
 * this is a value — choosing runs at once, nothing is deferred to the close.
 *
 * A disabled choice cannot be hovered for a `Tooltip` (Radix mutes pointer
 * events on `data-disabled`), so its `hint` is drawn as its second line, in
 * the place the `description` would take.
 */

import * as M from "@radix-ui/react-dropdown-menu";
import type { ReactNode } from "react";
import { useOpenSurface } from "./openSurfaces";
import { cn } from "./cn";
import { FOCUS_RING } from "./rings";
import { ICON } from "./icons";
import type { LucideIcon } from "./icons";
import { MENU_ITEM, MENU_ITEM_TONE, POPOVER_SURFACE } from "./surfaces";

export interface MenuChoice<T extends string> {
  id: T;
  label: string;
  /** The one-sentence meaning, drawn under the label. */
  description?: string;
  icon?: LucideIcon;
  /** Offered but not choosable — `hint` says why. */
  disabled?: boolean;
  /** Drawn in place of `description` while `disabled`. */
  hint?: string;
}

export function ChoiceMenu<T extends string>({
  value,
  onChange,
  choices,
  trigger,
  label,
  align = "start",
  disabled = false,
}: {
  value: T;
  onChange: (value: T) => void;
  choices: ReadonlyArray<MenuChoice<T>>;
  trigger: ReactNode;
  /** Names the group for assistive technology. */
  label: string;
  align?: "start" | "center" | "end";
  disabled?: boolean;
}) {
  const [, setOpen] = useOpenSurface();
  return (
    <M.Root onOpenChange={setOpen}>
      <M.Trigger aria-label={label} disabled={disabled || choices.length === 0} className={cn("inline-flex items-center rounded-control disabled:opacity-45", FOCUS_RING)}>
        {trigger}
      </M.Trigger>
      <M.Portal>
        <M.Content data-pane align={align} sideOffset={4} collisionPadding={8} className={cn(POPOVER_SURFACE, "min-w-56 py-1")}>
          <M.RadioGroup value={value} onValueChange={(next) => onChange(next as T)}>
            {choices.map((c) => (
              <M.RadioItem key={c.id} value={c.id} disabled={c.disabled} className={cn(MENU_ITEM, MENU_ITEM_TONE.default, "flex-col items-stretch gap-0.5 py-2")}>
                <span className="flex items-center gap-2">
                  {c.icon && <c.icon size={14} aria-hidden className="shrink-0 opacity-70" />}
                  <span className="min-w-0 flex-1 truncate font-medium">{c.label}</span>
                  <M.ItemIndicator>
                    <ICON.check size={12} aria-hidden className="shrink-0 text-text" />
                  </M.ItemIndicator>
                </span>
                {c.disabled && c.hint ? <span className="pl-5 text-2xs text-warn">{c.hint}</span> : c.description ? <span className="pl-5 text-2xs text-text-dim">{c.description}</span> : null}
              </M.RadioItem>
            ))}
          </M.RadioGroup>
        </M.Content>
      </M.Portal>
    </M.Root>
  );
}
