/**
 * A tooltip, for a label that would not fit.
 *
 * It is not a place for information: it is invisible on a touch screen,
 * unreadable from a keyboard until focus lands, and gone the moment the
 * pointer moves. Anything the reader needs in order to decide belongs in the
 * row. What belongs here is the name of a glyph-only button, and Radix gives
 * that to assistive technology as an accessible name rather than as a
 * floating box nobody but a sighted mouse user ever sees.
 *
 * Each tooltip carries its own `Provider`. A single provider at the app root
 * would be better — it is what lets a second tooltip skip the open delay
 * while you sweep along a toolbar — but the shell does not mount one yet, and
 * a tooltip that silently does nothing because its provider is missing is a
 * worse trade than a lost skip-delay. `TooltipProvider` is exported so the
 * shell can hoist it, at which point the nested providers become harmless
 * no-ops rather than a breaking change.
 *
 * A tooltip is read, never hovered: its content takes no pointer and never
 * keeps itself open under one (`disableHoverableContent`), so a tooltip can
 * never sit between the pointer and a control — the one that could sat over
 * a menu's own list and blocked the pick. A menu trigger wears no floating
 * tooltip at all: the menu carries its accessible `label` and the trigger a
 * native `title` (`AgentMenu`, `SelectionAgentBar`).
 */

import * as T from "@radix-ui/react-tooltip";
import type { ReactNode } from "react";
import { cn } from "./cn";

export const TooltipProvider = T.Provider;

export function Tooltip({
  label,
  children,
  side = "top",
  className,
  active = true,
}: {
  label: ReactNode;
  /** A single element that can hold a ref — usually a button. */
  children: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  className?: string;
  /**
   * Whether the tip shows. False keeps the trigger mounted and draws
   * nothing — for a tip that comes and goes with a state (a disabled
   * button's reason) without remounting what it wraps, which would drop
   * keyboard focus.
   */
  active?: boolean;
}) {
  // A tip with nothing to say is no tip — unless it was asked to stand by
  // (`active={false}`), which keeps the trigger mounted for when it has.
  if (!label && active) return <>{children}</>;
  return (
    <T.Provider delayDuration={400} skipDelayDuration={200}>
      <T.Root disableHoverableContent>
        <T.Trigger asChild>{children}</T.Trigger>
        {active && <T.Portal>
          <T.Content
            data-pane
            side={side}
            sideOffset={6}
            collisionPadding={8}
            className={cn(
              "motion-pop pointer-events-none z-50 max-w-64 rounded-control border border-border bg-surface px-2 py-1 text-2xs text-text shadow-lg",
              className,
            )}
          >
            {label}
          </T.Content>
        </T.Portal>}
      </T.Root>
    </T.Provider>
  );
}
