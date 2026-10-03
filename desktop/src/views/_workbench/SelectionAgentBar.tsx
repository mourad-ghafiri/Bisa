/**
 * The toolbar that floats beside an editor selection: **Ask**, **Edit** or
 * **Attach**, without leaving the file (ide/09).
 *
 * Three verbs, and they are not the same weight. *Ask* is a question and
 * changes nothing. *Edit* writes your working tree — the agent is told not to
 * commit, and the change is kept or undone per hunk in the conversation (ide/20) —
 * so it is the accented one and says so. *Attach* sends nothing at all: it
 * drops the selection into the Agents pane's chip tray, for a message you want
 * to write properly.
 *
 * It sits **above** the selection when there is room and below it otherwise,
 * clamped to the editor — the arithmetic is `editorAgentModel.toolbarPlacement`,
 * so where a floating bar goes is a tested fact rather than two pixel offsets.
 * The agent it posts to is the one every other addressing surface would offer
 * (`AgentPicker` over `agentChoices`), remembered between selections.
 */

import { useLayoutEffect, useRef, useState } from "react";
import { Button, FOCUS_RING, ICON, TextInput, Tooltip, cn, useToast } from "../../ui";
import type { EditorSelection } from "../../ui";
import { api } from "../../api";
import { AgentPicker, useAgentChoice } from "../_work/AgentPicker";
import { attempt } from "../_work/useAsync";
import { attachContext } from "./agentPaneStore";
import { recordAgentEdit } from "./agentEditsStore";
import { selectionChip } from "./contextChips.mjs";
import { messageBody, rangeLabel, sentWords, toolbarPlacement } from "./editorAgentModel.mjs";
import { showRightPanel } from "./rightPanelStore";
import { rememberAgent, rememberedAgent } from "./rememberedAgent";
import { ensureConversation } from "./conversationsStore";
import { handOffWords } from "./conversationPaneModel.mjs";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { settingOf } from "../../shell/settingsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** What the bar is before it has been measured — enough to place it without a flash. */
const ESTIMATE = { width: 260, height: 28 };

export function SelectionAgentBar({
  wid,
  pid,
  path,
  sel,
  box,
  mode,
  onMode,
  onClose,
}: {
  wid: string;
  /** The project whose `agents.default` and `agents.context.selection_max_lines` apply. */
  pid: string | null;
  path: string;
  sel: EditorSelection;
  /** The editor's own rectangle, so the bar cannot be placed outside it. */
  box: { width: number; height: number };
  /** `null` shows the verbs; a mode shows that input (a keyboard command sets it). */
  mode: null | "ask" | "edit";
  onMode: (mode: null | "ask" | "edit") => void;
  onClose: () => void;
}) {
  const toast = useToast();
  const [agentId, setAgentId] = useState(rememberedAgent);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const bar = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState(ESTIMATE);

  // Measured after paint, because where it goes depends on how big it is and
  // the input mode is twice the width of the verbs.
  useLayoutEffect(() => {
    const el = bar.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    setSize((prev) => (Math.abs(prev.width - rect.width) < 1 && Math.abs(prev.height - rect.height) < 1 ? prev : { width: rect.width, height: rect.height }));
  }, [mode, text]);

  const { resolved } = useResolvedSettings(pid);
  const maxLines = settingOf(resolved ?? [], "agents.context.selection_max_lines", undefined);
  const selectionLines = typeof maxLines === "number" ? maxLines : undefined;
  const { chosen } = useAgentChoice(pid, agentId);
  const agentName = chosen?.name ?? agentId;

  const place = toolbarPlacement({ anchors: sel.anchors, box, bar: size });
  const pickAgent = (nextId: string) => {
    setAgentId(nextId);
    rememberAgent(nextId);
  };

  const target = t("workbench-selection-agent-bar-selection", { end: rangeLabel(path, sel.start, sel.end) });
  const chip = () => selectionChip(path, sel.start, sel.end, sel.text, selectionLines);

  /** Put the selection in the pane's tray and leave the writing to the person. */
  const attach = () => {
    attachContext(chip(), `workstream:${wid}`);
    toast.ok(t("workbench-context-tray-attached-write-message-agent-panel"));
    onClose();
  };

  // A question or an edit goes to a conversation — the one this checkout is
  // on, or a new one about it — never to a session.
  const send = () => {
    if (sending || !chosen || !pid) return;
    setSending(true);
    let started = false;
    void attempt(
      async () => {
        const landing = await ensureConversation(wid, pid);
        started = landing.started;
        await api.postConversationMessage(landing.id, messageBody({ mode: mode ?? "ask", agentId: chosen.id, text, target, chips: [chip()] }));
        // An edit is followed to the agent's end: the document reloads and says so.
        if (mode === "edit") recordAgentEdit({ scope: "conversation", id: landing.id, path, agentId: chosen.id });
      },
      (message) => {
        setSending(false);
        toast.error(message);
      },
      () => {
        setSending(false);
        toast.ok(started ? handOffWords(agentName, true) : sentWords(mode ?? "ask", agentName));
        showRightPanel("agents", `workstream:${wid}`);
        onClose();
      },
    );
  };

  const agentMenu = <AgentPicker agentId={agentId} pid={pid} onChoose={pickAgent} />;

  if (!place) return null;
  return (
    <div
      ref={bar}
      role="toolbar"
      aria-label={t("workbench-selection-agent-bar-hand-agent", { end: rangeLabel(path, sel.start, sel.end) })}
      // The kit's floating surface, told it is open so its entrance animates —
      // it keys on the attribute a Radix popover would set, and this is not one.
      data-state="open"
      className="motion-pop absolute z-20 flex items-center gap-1 rounded-control border border-border bg-surface p-1 shadow-lg"
      style={{ top: place.top, left: place.left }}
      // Keep a mousedown inside the bar from clearing the editor's selection.
      onMouseDown={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }}
    >
      {mode === null ? (
        <>
          <Tooltip label={t("workbench-selection-agent-bar-ask-about-selection-changes-nothing")}>
            <Button size="sm" variant="ghost" className="h-6" onClick={() => onMode("ask")}>
              <ICON.question size={12} aria-hidden />{t("workbench-selection-agent-bar-ask")}</Button>
          </Tooltip>
          <Tooltip label={t("workbench-selection-agent-bar-ask-edit-writes-file-keep-undo")}>
            <Button size="sm" variant="primary" className="h-6" onClick={() => onMode("edit")}>
              <ICON.edit size={12} aria-hidden />{t("workbench-selection-agent-bar-edit")}</Button>
          </Tooltip>
          <Tooltip label={t("workbench-selection-agent-bar-attach-agent-panel-nothing-sent-so")}>
            <Button size="sm" variant="ghost" className="h-6" onClick={attach}>
              <ICON.attach size={12} aria-hidden />{t("workbench-context-tray-attach")}</Button>
          </Tooltip>
          <span className="mx-0.5 h-4 w-px bg-hairline" aria-hidden />
          {agentMenu}
          <Tooltip label={t("workbench-selection-agent-bar-dismiss")}>
            <button type="button" aria-label={t("workbench-selection-agent-bar-dismiss")} onClick={onClose} className={cn("anim flex h-6 w-6 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text", FOCUS_RING)}>
              <ICON.close size={12} aria-hidden />
            </button>
          </Tooltip>
        </>
      ) : (
        <>
          {agentMenu}
          <TextInput
            autoFocus
            value={text}
            aria-label={mode === "edit" ? t("workbench-selection-agent-bar-ask-edit", { agentName, end: rangeLabel(path, sel.start, sel.end) }) : t("workbench-selection-agent-bar-ask-about", { agentName, end: rangeLabel(path, sel.start, sel.end) })}
            placeholder={mode === "edit" ? t("workbench-selection-agent-bar-edit-2", { end: rangeLabel(path, sel.start, sel.end) }) : t("workbench-selection-agent-bar-ask-about-2", { end: rangeLabel(path, sel.start, sel.end) })}
            className={cn("w-80 text-2xs", sending && "opacity-60")}
            onMouseDown={(e) => e.stopPropagation()}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") {
                e.preventDefault();
                send();
              } else if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              }
            }}
          />
          <Button size="sm" variant="primary" disabled={sending} onClick={send}>
            {sending ? t("workbench-selection-agent-bar-sending") : mode === "edit" ? t("workbench-selection-agent-bar-edit") : t("workbench-selection-agent-bar-ask")}
          </Button>
        </>
      )}
    </div>
  );
}
