/**
 * The `⋯` — one square, the same everywhere a row keeps its verbs behind a
 * menu: the Changes tree, a branch, a tag, a remote. It was drawn inline in
 * four places, four times the same span; a row's `⋯` should look alike
 * wherever it is, and a change to its hover should land once. A card wears
 * the `⋮` (`vertical`): the same square, the dots stacked.
 */

import { cn } from "./cn";
import { ICON } from "./icons";
import { Menu, type MenuItem } from "./Menu";

export function MoreMenu({ label, items, className, vertical = false }: { label: string; items: MenuItem[]; className?: string; vertical?: boolean }) {
  const Glyph = vertical ? ICON.moreVertical : ICON.more;
  return (
    <Menu
      label={label}
      items={items}
      trigger={
        <span className={cn("anim flex h-6 w-6 items-center justify-center rounded text-text-dim hover:bg-surface hover:text-text", className)}>
          <Glyph size={13} aria-hidden />
        </span>
      }
    />
  );
}
