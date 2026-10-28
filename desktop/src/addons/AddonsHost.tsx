/**
 * The addons' mount (18 — Addons), in `App.tsx` beside the pet: the list's
 * one sync in a boundary of its own, and the layer in another. A window
 * that throws costs that window alone (`AddonLayer` puts it away); a layer
 * that throws costs the layer until the route moves or the layer is
 * switched off and on (`⌘⇧X` — `layerShown` is in the boundary's reset
 * key); and neither ever costs the footer's and the panel's reads of the
 * list, which the sync keeps current from outside both. The layer is a
 * chunk of its own; the store is the footer's already.
 */

import { Suspense, lazy } from "react";
import { OverlayBoundary } from "../ui";
import { useAddons, useAddonsSync } from "./addonsStore";

const AddonLayer = lazy(() => import("./AddonLayer").then((m) => ({ default: m.AddonLayer })));

/** The one mount of the sync: the footer, the panel and the layer read the same store. */
function AddonsSync() {
  useAddonsSync();
  return null;
}

export function AddonsHost({ resetKey, onError }: { resetKey: string; onError: (error: Error, info: { componentStack?: string | null }) => void }) {
  const { layerShown } = useAddons();
  return (
    <>
      <OverlayBoundary name="addons sync" resetKey={resetKey} onError={onError}>
        <AddonsSync />
      </OverlayBoundary>
      <OverlayBoundary name="addons" resetKey={`${resetKey}:${layerShown}`} onError={onError}>
        <Suspense fallback={null}>
          <AddonLayer />
        </Suspense>
      </OverlayBoundary>
    </>
  );
}
