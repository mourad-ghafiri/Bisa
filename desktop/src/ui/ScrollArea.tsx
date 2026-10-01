/**
 * A scrolling region with a scrollbar that matches the theme.
 *
 * Native overlay scrollbars are drawn by the OS and ignore the palette, which
 * is fine on a page and wrong inside a dark popover on a light system. This
 * is for those places — a long menu, a picker, a pane of chips — and not for
 * the main content areas, which keep native scrolling because it is smoother
 * on a trackpad and because {@link VirtualList} needs a real scroll container
 * to measure.
 */

import * as S from "@radix-ui/react-scroll-area";
import type { ReactNode } from "react";
import { cn } from "./cn";

export function ScrollArea({
  children,
  className,
  viewportClassName,
}: {
  children: ReactNode;
  /** Sizes the region; the viewport inside it is what scrolls. */
  className?: string;
  viewportClassName?: string;
}) {
  return (
    <S.Root className={cn("relative overflow-hidden", className)}>
      <S.Viewport className={cn("h-full w-full", viewportClassName)}>{children}</S.Viewport>
      <S.Scrollbar
        orientation="vertical"
        className="anim flex w-1.5 touch-none p-px select-none"
      >
        <S.Thumb className="flex-1 rounded-full bg-text-dim/40" />
      </S.Scrollbar>
      <S.Corner />
    </S.Root>
  );
}
