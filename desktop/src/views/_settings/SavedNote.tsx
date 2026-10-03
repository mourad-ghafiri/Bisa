/**
 * The quiet *Saved* beside a control that writes as you go: a write that
 * lands says so for a moment (`SAVED_NOTE_MS`) and is gone — no toast, no
 * colour, the row's own meta ink. `role="status"`, so it is heard once.
 * A failed write is the toast's; this only ever says the write landed.
 */

import { useCallback, useEffect, useState } from "react";
import { SAVED_NOTE_MS } from "./settingDraftModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** When the last write landed, cleared once the note has been read; and the mark a landed write calls. */
export function useSavedNote(): [number | null, () => void] {
  const [at, setAt] = useState<number | null>(null);
  useEffect(() => {
    if (at === null) return;
    const id = setTimeout(() => setAt(null), SAVED_NOTE_MS);
    return () => clearTimeout(id);
  }, [at]);
  const mark = useCallback(() => setAt(Date.now()), []);
  return [at, mark];
}

export function SavedNote({ at }: { at: number | null }) {
  return (
    <span role="status" className="text-2xs text-text-dim">
      {at !== null && (
        // Keyed by the write, so a second save fades in again rather than just staying.
        <span key={at} className="motion-overlay" data-state="open">
          {t("settings-saved-note-saved")}
        </span>
      )}
    </span>
  );
}
