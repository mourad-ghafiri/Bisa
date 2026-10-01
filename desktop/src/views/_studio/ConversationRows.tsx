/**
 * A list of conversations (13 — Conversations), as every owner's surface
 * draws it — a goal's tab, a workflow's Agent pane, the drawers beside a note
 * or a drawing, the IDE's Agent pane (`ConversationsBar.tsx`): one row per
 * conversation — its name,
 * what it is about, who is in it, how much was said — the selected one
 * marked, and a menu with the verbs a record takes: rename, put away or
 * take back out, delete. The rows are the caller's, already ordered and
 * narrowed (`conversationsModel.mjs`); this file only paints.
 */

import { useState } from "react";
import type { ReactNode } from "react";
import { api } from "../../api";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { Button, Dialog, EmptyState, ICON, Menu, TextInput, cn, useToast } from "../../ui";
import type { MenuItem } from "../../ui";
import type { ConversationView } from "../../types";
import { attempt, useAsync } from "../_work/useAsync";
import { originWords, rowWords } from "./conversationsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The names an origin is said with, from what the workspace store holds and the workflow library. */
export function useOriginNames() {
  const ws = useWorkspace();
  const workflows = useAsync((s) => api.workflows({}, s), []);
  // A drawing's and a note's titles, for the chip that says what a
  // conversation is about — read the way the workflows are.
  const drawings = useAsync((s) => api.drawings("", s), []);
  const notes = useAsync((s) => api.notes("", s), []);
  return {
    drawing: (id: string) => (drawings.data?.drawings ?? []).find((d) => d.id === id)?.title ?? null,
    note: (id: string) => (notes.data?.notes ?? []).find((n) => n.id === id)?.title ?? null,
    goal: (id: string) => {
      const g = ws.goals.find((x) => x.id === id);
      return g?.title ?? g?.statement?.split("\n")[0]?.slice(0, 60) ?? null;
    },
    workflow: (id: string) => (workflows.data?.workflows ?? []).find((w) => w.workflow.id === id)?.workflow.name ?? null,
    project: (id: string) => ws.projects.find((p) => p.project.id === id)?.project.name ?? null,
    workstream: (id: string) => {
      const w = ws.workstreams.find((x) => x.workstream.id === id);
      if (!w) return ws.projects.find((p) => p.project.id === id)?.project.name ?? null;
      return w.workstream.name ?? (w.workstream.kind.kind === "worktree" ? w.workstream.kind.branch : w.workstream.kind.kind === "primary" ? t("studio-conversation-rows-main-tree") : null);
    },
  };
}

function RenameDialog({ row, onClose, onDone }: { row: ConversationView; onClose: () => void; onDone: () => void }) {
  const toast = useToast();
  const [title, setTitle] = useState(row.title ?? "");
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    const trimmed = title.trim();
    const ok = await attempt(() => api.patchConversation(row.id, { title: trimmed ? trimmed : null }), toast.error);
    setBusy(false);
    if (ok) {
      onDone();
      onClose();
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("studio-conversation-rows-name-conversation")}
      description={t("studio-conversation-rows-name-list-reads-leave-empty-go")}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("studio-conversation-rows-cancel")}</Button>
          <Button variant="primary" disabled={busy} onClick={() => void save()}>
            {busy ? t("studio-conversation-rows-saving") : t("studio-conversation-rows-save")}
          </Button>
        </>
      }
    >
      <TextInput
        autoFocus
        value={title}
        maxLength={120}
        placeholder={t("studio-conversation-rows-dark-mode-settings-screen")}
        onChange={(e) => setTitle(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void save();
        }}
      />
    </Dialog>
  );
}

/** The verbs a conversation's record takes, for a row's menu or a header's. */
export function useConversationVerbs(onChanged?: (what: "renamed" | "archived" | "deleted") => void) {
  const toast = useToast();
  const [renaming, setRenaming] = useState<ConversationView | null>(null);
  const items = (row: ConversationView): MenuItem[] => [
    { label: row.title ? t("studio-conversation-rows-rename") : t("studio-conversation-rows-name"), icon: ICON.edit, onSelect: () => setRenaming(row) },
    {
      label: row.archived ? t("studio-conversation-rows-take-back-out") : t("studio-conversation-rows-archive"),
      icon: ICON.archive,
      onSelect: () =>
        void attempt(
          () => api.patchConversation(row.id, { archived: !row.archived }),
          toast.error,
          () => {
            toast.ok(row.archived ? t("studio-conversation-rows-back-among-live-conversations") : t("studio-conversation-rows-archived-reads-message-will-not-go"));
            onChanged?.("archived");
          },
        ),
    },
    {
      label: t("studio-conversation-rows-delete"),
      icon: ICON.delete,
      danger: true,
      separatorBefore: true,
      onSelect: () =>
        void attempt(
          () => api.deleteConversation(row.id),
          toast.error,
          () => {
            toast.ok(t("studio-conversation-rows-deleted-messages"));
            onChanged?.("deleted");
          },
        ),
    },
  ];
  const dialog: ReactNode = renaming ? (
    <RenameDialog
      row={renaming}
      onClose={() => setRenaming(null)}
      onDone={() => {
        toast.ok(t("studio-conversation-rows-named"));
        onChanged?.("renamed");
      }}
    />
  ) : null;
  return { items, dialog };
}

export function ConversationRows({
  rows,
  selected,
  onSelect,
  onChanged,
  dense = false,
  empty,
}: {
  rows: readonly ConversationView[];
  selected: string | null;
  onSelect: (row: ConversationView) => void;
  /** A record moved through a row's menu; the caller re-reads its list. */
  onChanged?: (what: "renamed" | "archived" | "deleted") => void;
  /** The IDE's panel form: one line per row. */
  dense?: boolean;
  empty?: ReactNode;
}) {
  const ws = useWorkspace();
  const names = useOriginNames();
  const verbs = useConversationVerbs(onChanged);
  const agentName = (id: string) => ws.agents.find((a) => a.id === id)?.name ?? null;
  if (rows.length === 0) return <>{empty ?? <EmptyState icon={ICON.dm} title={t("studio-conversation-rows-no-conversations-yet")} hint={t("studio-conversation-rows-start-one-message-agent-about-what")} action={null} />}</>;
  return (
    <>
      <ul className={cn("flex flex-col", dense ? "gap-0" : "gap-1")} aria-label={t("studio-conversation-rows-conversations")}>
        {rows.map((row) => {
          const w = rowWords(row, { names, agentName });
          const active = row.id === selected;
          const busy = (ws.working[row.id] ?? []).length > 0;
          return (
            <li key={row.id} className="group flex items-stretch">
              <button
                type="button"
                aria-current={active ? "true" : undefined}
                onClick={() => onSelect(row)}
                title={`${w.title} — ${originWords(row.origin, names)}`}
                className={cn(
                  "anim flex min-w-0 flex-1 flex-col gap-0.5 rounded-control px-2 text-left hover:bg-surface-2",
                  dense ? "py-1" : "py-1.5",
                  active && "bg-surface-2",
                )}
              >
                <span className="flex min-w-0 items-center gap-1.5">
                  <span className={cn("min-w-0 truncate text-xs", active ? "font-semibold text-text" : "text-text")}>{w.title}</span>
                  {busy && <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-accent" title={t("studio-conversation-rows-agent-writing-here")} />}
                  <span className="ml-auto shrink-0 text-2xs tnum text-text-dim">{row.message_count}</span>
                </span>
                {!dense && (
                  <span className="flex min-w-0 items-center gap-1 text-2xs text-text-dim">
                    <span className="truncate">{w.about}</span>
                    {w.agents && <span className="truncate">{t("studio-conversation-rows-dot-agents", { agents: w.agents })}</span>}
                    {row.archived && <span>{t("studio-conversation-rows-archived")}</span>}
                  </span>
                )}
              </button>
              <Menu
                label={t("studio-conversation-rows-conversation-actions")}
                items={verbs.items(row)}
                trigger={
                  <span className="anim my-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-text-dim opacity-0 hover:bg-surface-2 hover:text-text group-hover:opacity-100 focus:opacity-100">
                    <ICON.more size={13} aria-hidden />
                  </span>
                }
              />
            </li>
          );
        })}
      </ul>
      {verbs.dialog}
    </>
  );
}
