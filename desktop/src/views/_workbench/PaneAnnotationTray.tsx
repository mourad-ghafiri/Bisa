/**
 * The tray under an annotated page whose tab is not a checkout's (ide/18
 * §Annotating a page): the Browser pane beside a goal, a channel or a
 * message, or a goal's tab in the IDE. The rows are `AnnotationRows`; the
 * one door is **Attach to the message** — every annotation as an
 * `annotation` chip in the tray of the conversation on screen
 * (`chatTargetStore`), where the person writes the message and sends it —
 * and *Clear*. Nothing is sent from here: the composer beside the page is
 * the one that sends, so the words and the chips leave together. With no
 * conversation on screen the door is held and says what to open.
 */

import { Button, ICON, Tooltip, useToast } from "../../ui";
import { attachTo } from "./agentPaneStore";
import { annotationChips } from "./annotationModel.mjs";
import type { AnnotationDraft } from "./annotationModel.mjs";
import { AnnotationRows } from "./AnnotationRows";
import { fitsBudget } from "./contextChips.mjs";
import type { PageRef } from "./contextChips.mjs";
import { attachWords, attachedWords, chatKey } from "../_studio/chatScopeModel.mjs";
import { useChatTarget } from "../_studio/chatTargetStore";
import { t } from "../../i18n/l10n.mjs";

export function PaneAnnotationTray({ page, draft, lost, onDraft, onDone }: { page: PageRef; draft: AnnotationDraft; lost: readonly number[]; onDraft: (next: AnnotationDraft) => void; onDone: () => void }) {
  const toast = useToast();
  const target = useChatTarget();
  const count = draft.annotations.length;
  const chips = annotationChips(page, draft.annotations);
  const over = !fitsBudget(chips);
  const door = attachWords(target);

  const attach = () => {
    if (!target || count === 0 || over) return;
    const scope = chatKey(target.kind, target.id);
    for (const chip of chips) attachTo(scope, chip);
    toast.ok(attachedWords(count));
    onDone();
  };

  return (
    <section aria-label={t("workbench-pane-annotation-tray-annotations")} className="flex shrink-0 flex-col gap-1.5 border-t border-hairline px-3 py-2 text-2xs">
      <AnnotationRows draft={draft} lost={lost} onDraft={onDraft} />
      <div className="flex flex-wrap items-center gap-2">
        <Tooltip label={door.hint}>
          <span className="inline-flex">
            <Button size="sm" variant="primary" disabled={!door.enabled || count === 0 || over} onClick={attach}>
              <ICON.attach size={12} aria-hidden />
              {door.label}
            </Button>
          </span>
        </Tooltip>
        {count > 0 && (
          <Button size="sm" variant="ghost" onClick={onDone}>{t("workbench-context-tray-clear")}</Button>
        )}
        {over && <span className="text-danger">{t("workbench-pane-annotation-tray-over-64-kib-annotations-drop-one")}</span>}
      </div>
    </section>
  );
}
