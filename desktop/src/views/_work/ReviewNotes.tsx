/**
 * The review notes on a project (ide/04 §6): what a person wrote on
 * hunks, whether the agent has seen each one, and the one button that hands
 * them over.
 *
 * "Send" posts one message per goal the project is attached to, the notes as
 * `DiffHunk` chips — the agent sees exactly what is listed here — and marks
 * them sent. A project attached to no goal has no thread to post in; the
 * notes are still marked sent and an agent working in the project reads them
 * with `review_notes_list`, and the toast says so rather than pretending a
 * message went somewhere.
 *
 * A note's verbs are kit buttons with glyphs, revealed on hover, and *Delete*
 * asks first like every other delete under Git — a note is a person's words.
 */

import { useState } from "react";
import { fingerprint } from "./gitPanelModel.mjs";
import { useSessionDraft } from "./gitPanelStore";
import { ApiError, api } from "../../api";
import type { ReviewNote } from "../../types";
import { Button, Checkbox, ConfirmDialog, EmptyState, ErrorNote, ICON, RelativeTime, SectionHeader, SkeletonRows, TextArea, Tooltip, failureText, useToast } from "../../ui";
import { confirmLabel } from "./gitWords.mjs";
import { reviewNoteGone, scopeLabel, unsentNotes } from "./hunkModel.mjs";
import { useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

function NoteRow({
  note,
  busy,
  onEdit,
  onResolve,
  onDelete,
  onOpen,
}: {
  note: ReviewNote;
  busy: boolean;
  onEdit: (body: string) => Promise<void>;
  onResolve: () => void;
  onDelete: () => void;
  onOpen?: () => void;
}) {
  // The edit in progress lives in the git panel store under the note and its
  // body: a body that changed on the node is a fresh draft, a tab switch is not.
  const draftKey = `note:${note.id}|${fingerprint(note.body)}`;
  const [editing, setEditing] = useSessionDraft(`${draftKey}|editing`, false);
  const [draft, setDraft] = useSessionDraft(`${draftKey}|draft`, note.body);
  const [showHunk, setShowHunk] = useState(false);
  const lines = note.range.start === note.range.end ? `${note.range.start}` : `${note.range.start}–${note.range.end}`;
  const resolved = note.resolved_at != null;
  const verb = "row-actions anim";
  return (
    <li className={`group flex flex-col gap-1 py-1.5 first:pt-0 last:pb-0 ${resolved ? "opacity-70" : ""}`}>
      <div className="flex flex-wrap items-center gap-2 text-2xs">
        <Tooltip label={t("work-review-notes-show-file-s-patch")}>
          <button type="button" onClick={onOpen} className="min-w-0 truncate font-mono text-text hover:underline">
            {note.path}:{lines}
          </button>
        </Tooltip>
        <span className="text-text-dim">{scopeLabel(note.scope)}</span>
        <span className="flex-1" />
        {resolved ? (
          <span className="inline-flex items-center gap-1 text-ok">
            <ICON.check size={11} aria-hidden />{t("work-review-notes-resolved")}</span>
        ) : note.sent_at != null ? (
          <Tooltip label={t("work-review-notes-handed-agents-edit-clears")}>
            <span className="text-text-dim">{t("work-review-notes-sent")}<RelativeTime at={note.sent_at} />
            </span>
          </Tooltip>
        ) : (
          <span className="text-warn">{t("work-review-notes-not-sent")}</span>
        )}
        {note.hunk && (
          <Button size="sm" variant="ghost" className={`${verb} aria-pressed:bg-selected aria-pressed:text-text`} aria-pressed={showHunk} onClick={() => setShowHunk((v) => !v)}>
            <ICON.note size={12} aria-hidden />
            {showHunk ? t("work-review-notes-hide-hunk") : t("work-review-notes-hunk")}
          </Button>
        )}
        {!resolved && !editing && (
          <Button size="sm" variant="ghost" className={verb} disabled={busy} onClick={() => setEditing(true)}>
            <ICON.edit size={12} aria-hidden />{t("work-remote-card-edit")}</Button>
        )}
        {!resolved && (
          <Button size="sm" variant="ghost" className={verb} disabled={busy} onClick={onResolve}>
            <ICON.check size={12} aria-hidden />{t("work-review-notes-resolve")}</Button>
        )}
        <Button size="sm" variant="ghost" className={`${verb} hover:text-danger`} disabled={busy} onClick={onDelete} aria-label={t("work-review-notes-delete-note", { note: note.path, lines })}>
          <ICON.delete size={12} aria-hidden />
        </Button>
      </div>
      {editing ? (
        <div className="flex flex-col gap-1">
          <TextArea value={draft} rows={2} autoFocus onChange={(e) => setDraft(e.target.value)} />
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>{t("work-agent-editor-cancel")}</Button>
            <Button size="sm" variant="primary" disabled={busy || !draft.trim()} onClick={() => void onEdit(draft.trim()).then(() => setEditing(false))}>{t("work-agent-editor-save")}</Button>
          </div>
        </div>
      ) : (
        <p className="whitespace-pre-wrap text-xs text-text">{note.body}</p>
      )}
      {showHunk && note.hunk && <pre className="max-h-40 overflow-auto rounded-control bg-surface-2 p-1.5 font-mono text-2xs leading-relaxed text-text-dim">{note.hunk}</pre>}
    </li>
  );
}

export function ReviewNotes({
  pid,
  wid,
  refreshKey,
  onOpen,
}: {
  /** The project the notes belong to. */
  pid: string;
  /** The workstream whose diff they annotate. */
  wid: string;
  /** Bumped by the caller when a note was written elsewhere on the screen. */
  refreshKey: number;
  /** Show the patch a note is about. */
  onOpen?: (path: string, staged: boolean) => void;
}) {
  const toast = useToast();
  const [showResolved, setShowResolved] = useState(false);
  const [busy, setBusy] = useState<"send" | "note" | null>(null);
  const [deleting, setDeleting] = useState<ReviewNote | null>(null);
  const notes = useAsync((s) => api.reviewNotes(pid, wid, showResolved, s), [pid, wid, showResolved, refreshKey]);
  const list = notes.data?.notes ?? [];
  const pending = unsentNotes(list);

  const act = async (kind: "send" | "note", f: () => Promise<unknown>, done?: string) => {
    if (busy) return;
    setBusy(kind);
    try {
      await f();
      if (done) toast.ok(done);
      notes.reload();
    } catch (e) {
      toast.error(failureText("work", "review-notes-failed", e));
      // A note that is gone (the node's 404) is a row this list should no longer show: said in the node's words, then read again.
      if (e instanceof ApiError && reviewNoteGone(e.status)) notes.reload();
    } finally {
      setBusy(null);
    }
  };

  const send = () =>
    act("send", async () => {
      const r = await api.reviewNotesSend(pid, []);
      const n = r.notes.length;
      toast.ok(
        r.posted_to.length > 0
          ? t("work-review-notes-sent-note-notes-into-goal-thread", { n, posted_to: r.posted_to.length })
          : t("work-review-notes-marked-note-notes-sent-project-attached", { n }),
      );
    });

  if (notes.loading && !notes.data) return <SkeletonRows rows={2} />;
  if (notes.error && !notes.data) return <ErrorNote error={notes.error} retry={notes.reload} />;

  return (
    <div className="flex flex-col gap-1.5">
      <SectionHeader
        title={t("work-review-notes-review-notes")}
        count={list.length}
        showZero
        trailing={pending.length > 0 ? t("work-review-notes-not-sent-2", { pending: pending.length }) : undefined}
        alwaysAction
        action={
          <Tooltip label={t("work-review-notes-post-unsent-notes-their-hunks-goal")}>
            <span className="inline-flex">
              <Button size="sm" variant={pending.length > 0 ? "primary" : "ghost"} disabled={busy !== null || pending.length === 0} onClick={() => void send()}>
                <ICON.send size={12} aria-hidden />
                {busy === "send" ? t("work-review-notes-sending") : pending.length > 0 ? t("work-review-notes-send-agents", { pending: pending.length }) : t("work-review-notes-send-agents-2")}
              </Button>
            </span>
          </Tooltip>
        }
      />
      {list.length === 0 && !showResolved ? (
        <EmptyState
          title={t("work-review-notes-no-review-notes")}
          hint={t("work-review-notes-annotate-hunk-patch-above-leave-one")}
          className="py-3"
          // The door is the Annotate control on each hunk, above; a second one
          // here would need a hunk to point at.
          action={null}
        />
      ) : (
        // One list on hairlines, not a box per note: the note's words are the content, not its frame.
        <ul className="flex flex-col divide-y divide-hairline">
          {list.map((n) => (
            <NoteRow
              key={n.id}
              note={n}
              busy={busy !== null}
              onOpen={onOpen ? () => onOpen(n.path, n.scope.scope === "staged") : undefined}
              onEdit={(body) => act("note", () => api.reviewNoteEdit(pid, n.id, body))}
              onResolve={() => void act("note", () => api.reviewNoteResolve(pid, n.id), t("work-review-notes-toast-resolved"))}
              onDelete={() => setDeleting(n)}
            />
          ))}
        </ul>
      )}
      <Checkbox label={t("work-review-notes-show-resolved")} checked={showResolved} onChange={setShowResolved} />
      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          const n = deleting;
          setDeleting(null);
          if (n) void act("note", () => api.reviewNoteDelete(pid, n.id), t("work-review-notes-toast-deleted"));
        }}
        title={deleting ? t("work-review-notes-delete-note-2", { deleting: deleting.path }) : ""}
        body={t("work-review-notes-note-gone-note-already-sent-stays")}
        confirmLabel={confirmLabel("delete")}
        danger
      />
    </div>
  );
}
