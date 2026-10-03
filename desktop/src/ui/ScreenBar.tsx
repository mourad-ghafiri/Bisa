/**
 * The band at the top of an index screen, under the chrome: the one place a
 * screen says what it shows and what you can do with it.
 *
 * Every index screen was building this by hand, and they had drifted — a
 * tab strip with its own short line here, a toolbar inside the list column
 * there, the create button left of the count on one screen and in the
 * column's corner on the next. One band, so moving between screens moves
 * nothing but the content:
 *
 * - `tabs` (a `Tabs` with `bare`) stand the band's full height at the left,
 *   their underline on the band's hairline — one line, never two;
 * - `children` narrow what is listed (search, filters, a view switch) and
 *   wrap among themselves when the screen is narrow;
 * - `end` holds the count, quiet tools and the screen's one primary
 *   action, which comes last, at the right edge, on every screen.
 *
 * The gutter is the index screens' `px-6`, the height the `row-lg` step's
 * 44px, and the band never scrolls with the list under it. When a narrow
 * screen wraps what narrows onto a second line, the tabs keep to the band's
 * base, their underline still on its hairline, and the count and the
 * primary keep to its first line, where the eye starts. A screen with more
 * to narrow than a narrow band holds beside its tabs (`stack`) gives the
 * tabs a line of their own at the base below `5xl` of the band's width, and
 * what narrows takes the first line from the gutter.
 */

import type { ReactNode } from "react";
import { cn } from "./cn";

export function ScreenBar({
  tabs,
  children,
  end,
  stack = false,
  className,
}: {
  tabs?: ReactNode;
  children?: ReactNode;
  end?: ReactNode;
  /** Narrower than `5xl`, the tabs take the band's base line to themselves. */
  stack?: boolean;
  className?: string;
}) {
  return (
    // A size container: `stack` reads the band's own width, not the window's.
    <div className={cn("@container shrink-0 border-b border-hairline", className)}>
      <div className="flex min-h-11 flex-wrap items-stretch gap-x-3 px-6">
        {/* `-ml-3`: a tab's own inset, given back, so its word starts on the gutter like everything under it. */}
        {tabs && <div className={cn("-ml-3 flex h-11 min-w-0 items-stretch self-end", !children && "flex-1", stack && "@max-5xl:order-last @max-5xl:basis-full")}>{tabs}</div>}
        {children && <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2 py-2">{children}</div>}
        {end && <div className="ml-auto flex h-11 shrink-0 items-center gap-2 self-start">{end}</div>}
      </div>
    </div>
  );
}
