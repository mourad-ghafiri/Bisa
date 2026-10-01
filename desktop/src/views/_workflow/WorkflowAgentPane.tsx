/**
 * The Workflow Designer's Agent pane (03-workflows §The designer; 13 —
 * Conversations §Origins): the conversations about this workflow, in the
 * right panel beside the canvas. A conversation with origin `workflow` runs
 * its turns in the agent's scratch, is framed as being about this workflow
 * — its inputs, its steps and its flows — and reaches the Workflow Agent,
 * which reads it, validates and **saves** it at the revision it read
 * (`save_workflow`); the canvas beside the pane adopts the new revision, or
 * offers the conflict banner when the person was mid-edit.
 *
 * Painted as every owner's surface is (`ConversationSurface`), from
 * `useConversationSurface` with the screen's own selection, remembered: the
 * pick is `?conversation=` and the unfolded list `?conversations=1`, so a
 * link lands on a conversation — and the pick is kept for the workflow
 * (`conversationPickStore.ts`), so every door back to the designer, which
 * opens a bare `#/workflows/<id>`, comes back to the conversation the person
 * left.
 */

import type { Workflow } from "../../types";
import { ICON } from "../../ui";
import { ConversationSurface } from "../_studio/ConversationSurface";
import { useConversationSurface } from "../_studio/useConversationSurface";
import { t } from "../../i18n/l10n.mjs";

export function WorkflowAgentPane({ workflow }: { workflow: Workflow }) {
  const c = useConversationSurface({ owner: { kind: "workflow", id: workflow.id } }, "remembered");
  return (
    <ConversationSurface
      surface={c}
      icon={<ICON.workflow size={14} aria-hidden />}
      subject={workflow.name}
      hint={t("workflow-workflow-agent-pane-mention-workflow-agent-analyse-fix-finish")}
    />
  );
}
