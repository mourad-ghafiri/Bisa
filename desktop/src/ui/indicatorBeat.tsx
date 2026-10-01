/**
 * Whether the indicators under here stand inside a surface that has just
 * opened — a dialog, a popover — and so show at once: the surface has
 * already appeared, and a blank body for the beat would be the flash the
 * beat exists to avoid (`loadingModel.beatMs`). The surface provides it; the
 * kit's `Pending`, `SkeletonRows` and `Spinner` read it through
 * `useShowAfter`, so no call site has to say where it stands.
 */

import { createContext, type ReactNode } from "react";

export const IndicatorBeat = createContext(false);

/** The surface that just opened: everything inside shows its status at once. */
export function ImmediateIndicators({ children }: { children: ReactNode }) {
  return <IndicatorBeat.Provider value={true}>{children}</IndicatorBeat.Provider>;
}
