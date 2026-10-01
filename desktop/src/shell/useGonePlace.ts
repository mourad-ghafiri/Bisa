/**
 * A remembered place never leads to a dead page (`crates/desktop.md`
 * §Routing). A detail screen hands this what its read said: when the node
 * answers that there is no such thing, the place is forgotten — with what
 * was kept of it — so no door returns there. Then:
 *
 * - a place the person **had been on** — they came by a section's door, by
 *   a launch, by a card drawn before the thing went — is left for its
 *   section's list, with one line saying why;
 * - a place they had **never been on** — a link they followed — keeps the
 *   screen's own *not found* note: the link is what is wrong, and leaving
 *   would hide that.
 *
 * What went while the app was closed raises no fact on the bus, and a
 * deleted channel raises none at all; this is what hears them.
 */

import { useEffect, useRef } from "react";
import { href, indexRouteOf, replace, type Route } from "../router";
import { useToast } from "../ui";
import { placeWasKnown } from "./placeMemoryStore";
import { forgetPlaceMemory } from "./viewMemoryStore";
import { t } from "../i18n/l10n.mjs";

/**
 * @param missing whether the screen's read answered *not found*
 * @param route the place the screen shows
 */
export function useGonePlace(missing: boolean, route: Route): void {
  const toast = useToast();
  const path = href(route).slice(1);
  // Once a place: a second read that fails the same way leaves nothing more to do.
  const left = useRef<string | null>(null);
  useEffect(() => {
    if (!missing || left.current === path) return;
    left.current = path;
    const known = placeWasKnown(path);
    forgetPlaceMemory(path);
    if (!known) return;
    toast.info(t("shell-gone-place-no-longer-here"));
    replace(indexRouteOf(route));
    // The route is the path's: a fresh object per render is the same place.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [missing, path, toast]);
}
