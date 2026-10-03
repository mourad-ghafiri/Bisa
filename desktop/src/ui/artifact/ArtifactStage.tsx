/**
 * An artifact over the whole window (ide/12): the viewer at `z-50` with a
 * dim ground, Esc to leave, ← and → to move between the conversation's
 * artifacts when the host hands them over. A surface while mounted, so the
 * browser tab's native page yields to it (`openSurfaces.ts`).
 */

import { useEffect } from "react";
import { createPortal } from "react-dom";
import type { ArtifactRef } from "../../types";
import { Button } from "../Button";
import { ICON } from "../icons";
import { useSurface } from "../openSurfaces";
import { ArtifactView } from "./ArtifactView";
import { t } from "../../i18n/l10n.mjs";

export function ArtifactStage({
  artifact,
  present,
  onClose,
  onPrev,
  onNext,
  onRequest,
  libraries,
  onOpenInBrowser = null,
}: {
  artifact: ArtifactRef;
  present: boolean;
  onClose: () => void;
  onPrev?: (() => void) | null;
  onNext?: (() => void) | null;
  onRequest?: () => Promise<void>;
  libraries?: boolean;
  onOpenInBrowser?: ((artifact: ArtifactRef) => void) | null;
}) {
  // Mounted only while open: a surface for as long as it stands.
  useSurface(true);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowLeft" && onPrev) onPrev();
      else if (e.key === "ArrowRight" && onNext) onNext();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, onPrev, onNext]);
  return createPortal(
    <div role="dialog" aria-label={artifact.title} className="fixed inset-0 z-50 flex flex-col bg-surface-2/95 p-4">
      <ArtifactView
        artifact={artifact}
        present={present}
        onRequest={onRequest}
        libraries={libraries}
        onOpenInBrowser={onOpenInBrowser}
        className="min-h-0 flex-1 rounded-card border border-border bg-surface shadow-lg"
        actions={
          <>
            {onPrev && (
              <Button size="sm" variant="ghost" onClick={onPrev} aria-label={t("ui-artifact-stage-previous-artifact")}>
                <ICON.back size={13} aria-hidden />
              </Button>
            )}
            {onNext && (
              <Button size="sm" variant="ghost" onClick={onNext} aria-label={t("ui-artifact-stage-next-artifact")}>
                <ICON.forward size={13} aria-hidden />
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={onClose} aria-label={t("ui-dialog-close")}>
              <ICON.collapse size={13} aria-hidden />
            </Button>
          </>
        }
      />
    </div>,
    document.body,
  );
}
