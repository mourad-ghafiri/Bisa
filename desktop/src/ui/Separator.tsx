/**
 * A rule between groups.
 *
 * `decorative` defaults to true, which is the common case and the one people
 * get wrong: a line that is only there because two groups look better apart
 * must not be announced, or a screen reader reads "separator" between every
 * pair of rows. Pass `decorative={false}` only when the line is the only
 * thing saying that what follows is a different kind of thing.
 */

import * as S from "@radix-ui/react-separator";
import { cn } from "./cn";

export function Separator({
  orientation = "horizontal",
  decorative = true,
  className,
}: {
  orientation?: "horizontal" | "vertical";
  decorative?: boolean;
  className?: string;
}) {
  return (
    <S.Root
      orientation={orientation}
      decorative={decorative}
      className={cn(
        "shrink-0 bg-border",
        orientation === "horizontal" ? "h-px w-full" : "h-full w-px",
        className,
      )}
    />
  );
}
