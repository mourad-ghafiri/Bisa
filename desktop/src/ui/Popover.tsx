/**
 * A panel anchored to the control that opened it.
 *
 * Use it for something you edit or read and then dismiss — a filter, a
 * picker, a detail peek. If the content demands a decision before the reader
 * can carry on, it is a {@link Dialog}: a popover closes on an outside click,
 * which is the right behaviour for "never mind" and the wrong one for
 * "confirm".
 *
 * Placement is Radix's, which means it flips and shifts to stay on screen
 * rather than being clipped at the edge of a pane — the failure the old
 * absolutely-positioned dropdowns had whenever a row sat near the bottom.
 *
 * A popover that opens onto a read shows the read at once, never an empty
 * frosted box: its content provides the immediate indicator beat
 * (`ImmediateIndicators`).
 */

import * as P from "@radix-ui/react-popover";
import { useState, type ReactNode } from "react";
import { useSurface } from "./openSurfaces";
import { cn } from "./cn";
import { ImmediateIndicators } from "./indicatorBeat";
import { POPOVER_SURFACE } from "./surfaces";

export function Popover({
  trigger,
  children,
  open,
  onOpenChange,
  side = "bottom",
  align = "start",
  className,
  label,
  asChild = false,
}: {
  trigger: ReactNode;
  children: ReactNode;
  /** Omit both to let the popover own its own open state. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  side?: "top" | "right" | "bottom" | "left";
  align?: "start" | "center" | "end";
  className?: string;
  label?: string;
  /**
   * The trigger is already a focusable control (a `<button>`/`<Button>`): make
   * it the trigger itself rather than nesting it inside the one Radix renders,
   * the same rule `Tooltip` follows. Leave it off — the default — and pass a
   * styled `<span>`, which is how the popover earns a keyboard-reachable button.
   */
  asChild?: boolean;
}) {
  // Open as told, or as Radix says when the popover owns its state — either
  // way a native layer yields while it shows.
  const [own, setOwn] = useState(false);
  useSurface(open ?? own);
  return (
    <P.Root
      open={open}
      onOpenChange={(next) => {
        setOwn(next);
        onOpenChange?.(next);
      }}
    >
      {asChild ? (
        <P.Trigger asChild>{trigger}</P.Trigger>
      ) : (
        <P.Trigger aria-label={label} className="inline-flex items-center outline-none">
          {trigger}
        </P.Trigger>
      )}
      <P.Portal>
        <P.Content
          data-pane
          side={side}
          align={align}
          sideOffset={6}
          collisionPadding={8}
          className={cn(POPOVER_SURFACE, "overflow-visible p-3", className)}
        >
          <ImmediateIndicators>{children}</ImmediateIndicators>
        </P.Content>
      </P.Portal>
    </P.Root>
  );
}
