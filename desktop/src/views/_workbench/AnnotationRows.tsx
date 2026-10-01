/**
 * The lines of an annotation tray (ide/03 §Annotate, ide/18): each
 * annotation as a numbered row — the element as an inspector names it, the
 * change wanted, × to drop it — and the one sentence a tray with nothing in
 * it says. Shared by the checkout's tray (`AnnotationTray`) and the
 * screen's (`PaneAnnotationTray`), which differ only in where the chips go.
 */

import { ICON, Tooltip, cn } from "../../ui";
import { annotationLabel, removeAnnotation, staleWords } from "./annotationModel.mjs";
import type { AnnotationDraft } from "./annotationModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function AnnotationRows({ draft, lost, onDraft }: { draft: AnnotationDraft; lost: readonly number[]; onDraft: (next: AnnotationDraft) => void }) {
  if (draft.annotations.length === 0) {
    return <p className="text-text-dim">{t("workbench-annotation-rows-point-element-page-say-what-should")}</p>;
  }
  return (
    <ul className="flex flex-col gap-1" aria-label={t("workbench-annotation-rows-annotated-elements")}>
      {draft.annotations.map((a, i) => {
        const stale = staleWords(lost, i + 1);
        return (
          <li key={a.id} className="group flex items-start gap-2">
            <span className="mt-px inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-accent px-1 font-semibold text-accent-contrast">{i + 1}</span>
            <span className="min-w-0 flex-1">
              <span className={cn("block truncate", stale ? "text-text-dim" : "text-text")} title={annotationLabel(i + 1, a)}>
                <span className="font-mono text-text-dim">&lt;{a.tag}&gt;</span> {a.note}
              </span>
              {stale && <span className="block text-text-dim">{stale}</span>}
            </span>
            <Tooltip label={t("workbench-annotation-rows-drop-annotation")}>
              <button type="button" aria-label={t("workbench-annotation-rows-drop-annotation-2", { i: i + 1 })} className="anim rounded text-text-dim hover:text-danger" onClick={() => onDraft(removeAnnotation(draft, a.id))}>
                <ICON.close size={11} aria-hidden />
              </button>
            </Tooltip>
          </li>
        );
      })}
    </ul>
  );
}
