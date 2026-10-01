/**
 * A section's door — the sidebar's rows, its rail, the palette's *Go to*:
 * where it leads is where the person was in the section
 * (`placeMemoryModel.sectionHash`). From another section, the place they
 * left; from inside one of the section's details, its list as they left
 * it; from its list, nowhere. One rule for every door, so the row, the rail
 * and the palette never disagree.
 */

import { useMemo } from "react";
import { address, navigateTo, section, useAddress, type Route } from "../router";
import { sectionHash } from "./placeMemoryModel.mjs";
import { placesNow, usePlaces } from "./placeMemoryStore";

/** Where the door of `route`'s section leads, as an `href` that follows the person. */
export function useSectionHref(route: Route): string {
  const places = usePlaces();
  const at = useAddress();
  const key = section(route);
  return useMemo(() => sectionHash(places, key, at), [places, key, at]);
}

/** Go through the door of `route`'s section — for a door that is not a link. */
export function goToSection(route: Route): void {
  navigateTo(sectionHash(placesNow(), section(route), address()));
}
