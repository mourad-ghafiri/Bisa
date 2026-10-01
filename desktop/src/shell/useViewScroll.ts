/**
 * A screen's scrollports keep their place across leaving the screen and
 * across a restart: the kit's `useKeptScroll` over the view memory
 * (`viewMemoryStore.ts`). A screen puts this on its root and marks what
 * scrolls — `data-scroll-keep="<name>"` — and each marked scrollport comes
 * back where it was left.
 *
 * In `shell/`, not in the kit: the kit keeps no store and knows no place. A
 * scroll is kept quietly — it is read once, at mount, so writing it
 * re-renders nobody.
 */

import { useMemo, type RefObject } from "react";
import { useKeptScroll, type KeptScroll } from "../ui";
import { parsePlace } from "../ui/keptScrollModel.mjs";
import { viewState } from "./viewMemoryStore";

/** The name a scrollport's place is kept under, beside the place's other values. */
const scrollName = (name: string): string => `scroll:${name}`;

/**
 * @param root the screen's root element
 * @param identity what is on screen — a change of it restores again; the place itself, unless `place` says otherwise
 * @param place where the scroll is kept, when what is on screen is more than the place: a pane that comes and goes beside one
 */
export function useViewScroll(root: RefObject<HTMLElement | null>, identity: string, place: string = identity): void {
  const kept = useMemo<KeptScroll>(
    () => ({
      read: (name) => parsePlace(viewState.read(place, scrollName(name))),
      write: (name, at) => void viewState.keepQuietly(place, scrollName(name), at),
    }),
    [place],
  );
  useKeptScroll(root, kept, identity);
}
