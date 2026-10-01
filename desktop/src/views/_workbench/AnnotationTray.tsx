/**
 * The tray under an annotated page (ide/03 §Annotate): every annotation as
 * a numbered line — the element as an inspector names it, the change wanted,
 * × to drop it — over the doors every tray of chips has (`ContextTray`:
 * Send to an agent, Attach to the Agent panel, Clear). The facts — the
 * chips, the words, the target — are `annotationModel.mjs`'s; a sent file
 * page follows the agent to its end (`recordAgentEdit`). The rows are
 * `AnnotationRows`, shared with the screen's tray (`PaneAnnotationTray`,
 * ide/18); `CheckoutAnnotationTray` is the same tray for a host that knows
 * the checkout and not its project — the Browser pane.
 */

import { useWorkstream } from "../_work/useWorkstream";
import { recordAgentEdit } from "./agentEditsStore";
import { AnnotationRows } from "./AnnotationRows";
import { annotationChips, annotationsContent, annotationsTarget, setMessage } from "./annotationModel.mjs";
import type { AnnotationDraft } from "./annotationModel.mjs";
import { pageWords } from "./contextChips.mjs";
import type { PageRef } from "./contextChips.mjs";
import { ContextTray } from "./ContextTray";
import { t } from "../../i18n/l10n.mjs";

export function AnnotationTray({
  wid,
  pid,
  page,
  draft,
  lost,
  onDraft,
  onDone,
}: {
  wid: string;
  /** The project, for its default agent. */
  pid: string | null;
  /** The page the elements are on: a file of the project, or a URL the browser showed (ide/18). */
  page: PageRef;
  draft: AnnotationDraft;
  /** The numbers whose element the page no longer has. */
  lost: readonly number[];
  onDraft: (next: AnnotationDraft) => void;
  /** The annotations left the tray — sent, attached or cleared. */
  onDone: () => void;
}) {
  const count = draft.annotations.length;
  const chips = annotationChips(page, draft.annotations);
  const file = page.kind === "file" ? (page.path.split("/").pop() ?? page.path) : pageWords(page);
  return (
    <ContextTray
      wid={wid}
      pid={pid}
      chips={chips}
      count={count}
      message={draft.message}
      onMessage={(m) => onDraft(setMessage(draft, m))}
      target={annotationsTarget(count, page)}
      content={annotationsContent(draft.message, count)}
      about={file}
      noun={t("workbench-annotation-tray-annotations")}
      rows={<AnnotationRows draft={draft} lost={lost} onDraft={onDraft} />}
      // The page follows the agent to its end: reloaded and said when it
      // rests — a file's; a served page reloads on its own.
      onSent={(landing, agentId) => {
        if (page.kind === "file") recordAgentEdit({ scope: "conversation", id: landing.id, path: page.path, agentId });
      }}
      onDone={onDone}
    />
  );
}

/** The checkout's tray where only the checkout is known: its project read here. */
export function CheckoutAnnotationTray({ wid, ...rest }: { wid: string; page: PageRef; draft: AnnotationDraft; lost: readonly number[]; onDraft: (next: AnnotationDraft) => void; onDone: () => void }) {
  const { projectId } = useWorkstream(wid);
  return <AnnotationTray wid={wid} pid={projectId} {...rest} />;
}
