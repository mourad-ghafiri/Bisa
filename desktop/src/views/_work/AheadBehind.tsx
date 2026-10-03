/**
 * Where a branch stands against its upstream, drawn: the kit's up and down
 * glyphs beside the counts — never a typed arrow — and the same fact in
 * words for a screen reader, which the glyphs alone would not give it.
 * `all` keeps a zero (a reading of the checkout); without it only what moved
 * is drawn (a branch row).
 */

import { ICON } from "../../ui";

export function AheadBehind({ ahead, behind, words, all = false }: { ahead: number; behind: number; words: string; all?: boolean }) {
  return (
    <span className="tnum inline-flex shrink-0 items-center">
      <span className="sr-only">{words}</span>
      <span aria-hidden className="inline-flex items-center gap-1">
        {(all || ahead > 0) && (
          <span className="inline-flex items-center">
            <ICON.up size={11} />
            {ahead}
          </span>
        )}
        {(all || behind > 0) && (
          <span className="inline-flex items-center">
            <ICON.down size={11} />
            {behind}
          </span>
        )}
      </span>
    </span>
  );
}
