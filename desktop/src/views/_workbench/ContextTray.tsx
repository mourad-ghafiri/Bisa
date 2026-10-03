/**
 * The doors under a tray of chips (ide/03 §Annotate, ide/18, ide/19): an
 * optional message, and the two doors the editor's selection toolbar has
 * too — **Send**, to the agent the chip beside it names (the project's
 * default, the General Agent unless the project says otherwise, chosen up
 * front so one click sends; another is one pick away on the same
 * `AgentPicker`), the request going to the checkout's conversation as an
 * edit with one chip per row, and the Agent pane opening; and **Attach**,
 * which puts the same chips in the pane's tray and sends nothing. Either
 * empties the draft; *Clear* does too. A send remembers its agent for the
 * document hand-offs that lead with the last one used.
 *
 * The rows are the caller's — `AnnotationRows` for an annotated page,
 * `CaptureRows` for a device's screen — and so are the chips, the words and
 * the target: `AnnotationTray` and `CaptureTray` are the two wrappers. This
 * file draws and asks; it computes nothing.
 */

import { useState } from "react";
import type { ReactNode } from "react";
import { api } from "../../api";
import type { ContextRef } from "../../types";
import { Button, ICON, TextInput, Tooltip, useToast } from "../../ui";
import { AgentPicker, useAgentChoice } from "../_work/AgentPicker";
import { attempt } from "../_work/useAsync";
import { attachContext } from "./agentPaneStore";
import { ensureConversation } from "./conversationsStore";
import { handOffWords } from "./conversationPaneModel.mjs";
import { fitsBudget } from "./contextChips.mjs";
import { messageBody, sentWords } from "./editorAgentModel.mjs";
import { rememberAgent } from "./rememberedAgent";
import { showRightPanel } from "./rightPanelStore";
import { t } from "../../i18n/l10n.mjs";

export function ContextTray({
  wid,
  pid,
  chips,
  count,
  message,
  onMessage,
  target,
  content,
  about,
  noun,
  rows,
  onSent,
  onDone,
}: {
  wid: string;
  /** The project, for its default agent. */
  pid: string | null;
  /** One chip per row, in row order. */
  chips: readonly ContextRef[];
  count: number;
  message: string;
  onMessage: (next: string) => void;
  /** What the edit is about, for the request's target. */
  target: string;
  /** The request's text: the person's words, then what the chips are. */
  content: string;
  /** What the agent edits, for the Send tooltip: a file's name, a device's. */
  about: string;
  /** The rows' noun, plural: *annotations*, *captures*. */
  noun: string;
  rows: ReactNode;
  /** The request landed in a conversation, as this agent. */
  onSent?: (landing: { id: string }, agentId: string) => void;
  /** The rows left the tray — sent, attached or cleared. */
  onDone: () => void;
}) {
  const toast = useToast();
  const [sending, setSending] = useState(false);
  const scope = `workstream:${wid}`;
  const over = !fitsBudget([...chips]);
  // The project's default agent up front — the General Agent unless the
  // project says otherwise — and whatever the person picks on the chip after.
  const [wanted, setWanted] = useState<string | null>(null);
  const { chosen } = useAgentChoice(pid, wanted);

  // A request is only ever sent to a conversation — the one this checkout is
  // on, or a new one about it — never to a session.
  const send = () => {
    if (sending || count === 0 || over || !chosen || !pid) return;
    const agentId = chosen.id;
    rememberAgent(agentId);
    const agentName = chosen.name;
    setSending(true);
    let started = false;
    void attempt(
      async () => {
        const landing = await ensureConversation(wid, pid);
        started = landing.started;
        await api.postConversationMessage(landing.id, messageBody({ mode: "edit", agentId, text: content, target, chips: [...chips] }));
        onSent?.(landing, agentId);
      },
      (why) => {
        setSending(false);
        toast.error(why);
      },
      () => {
        setSending(false);
        toast.ok(started ? handOffWords(agentName, true) : sentWords("edit", agentName));
        onDone();
        showRightPanel("agents", scope);
      },
    );
  };

  const attach = () => {
    if (count === 0) return;
    for (const chip of chips) attachContext(chip, scope);
    toast.ok(count === 1 ? t("workbench-context-tray-attached-write-message-agent-panel") : t("workbench-context-tray-attached-write-message-agent-panel-2", { count, noun }));
    onDone();
  };

  return (
    <section aria-label={noun[0].toUpperCase() + noun.slice(1)} className="flex shrink-0 flex-col gap-1.5 border-t border-hairline px-3 py-2 text-2xs">
      {rows}
      <div className="flex flex-wrap items-center gap-2">
        <TextInput value={message} placeholder={t("workbench-context-tray-word-agent-optional")} aria-label={t("workbench-context-tray-message-send", { noun })} className="min-w-0 flex-1 text-2xs" onChange={(e) => onMessage(e.target.value)} />
        <AgentPicker agentId={wanted} pid={pid} onChoose={setWanted} disabled={sending} />
        <Tooltip label={chosen ? t("workbench-context-tray-send-edits-keep-undo-change-conversation", { noun, chosen: chosen.name, about }) : t("workbench-context-tray-no-agent-can-reached-from-workstream")}>
          <Button size="sm" variant="primary" disabled={count === 0 || over || sending || !chosen} onClick={send}>
            <ICON.agent size={12} aria-hidden />{t("workbench-context-tray-send")}</Button>
        </Tooltip>
        <Tooltip label={t("workbench-context-tray-attach-them-agent-panel-nothing-sent")}>
          <Button size="sm" variant="ghost" disabled={count === 0 || over || sending} onClick={attach}>
            <ICON.attach size={12} aria-hidden />{t("workbench-context-tray-attach")}</Button>
        </Tooltip>
        {count > 0 && (
          <Button size="sm" variant="ghost" disabled={sending} onClick={onDone}>{t("workbench-context-tray-clear")}</Button>
        )}
        {over && <span className="text-danger">{t("workbench-context-tray-over-64-kib-drop-one", { noun })}</span>}
      </div>
    </section>
  );
}
