/**
 * The recovery promise, said once (ide/04): a shield and *Saved to Safety
 * first*, under a consented control or inside its dialog, in place of the
 * sentence about `refs/bisa/safety/` that used to be restated in every
 * confirmation, tooltip and toast. The whole sentence is the tooltip; the
 * note opens Git › Branches › Safety when the caller hands it a door.
 */
import { ICON, Tooltip } from "../../ui";
import { recoveryWords } from "./gitDiscardModel.mjs";
import { SAFETY_LINE, SAFETY_SENTENCE } from "./gitWords.mjs";

export function SafetyNote({ kind, onOpenSafety, className = "" }: { kind?: "commit" | "tree" | "stash"; onOpenSafety?: () => void; className?: string }) {
  const words = kind ? `${SAFETY_LINE} — ${recoveryWords(kind)}` : SAFETY_LINE;
  const body = (
    <span className="inline-flex items-center gap-1 text-2xs text-text-dim">
      <ICON.safety size={11} aria-hidden />
      {words}
    </span>
  );
  return (
    <Tooltip label={SAFETY_SENTENCE}>
      {onOpenSafety ? (
        <button type="button" className={`anim inline-flex rounded hover:text-text ${className}`} onClick={onOpenSafety}>
          {body}
        </button>
      ) : (
        <span className={`inline-flex ${className}`}>{body}</span>
      )}
    </Tooltip>
  );
}
