/**
 * A file's patch, hunk by hunk, with the three acts a reviewer performs on a
 * hunk (ide/04 §6): stage or unstage it whole, stage a few of its lines, and
 * annotate it.
 *
 * Picking lines is a click on a `+` or `-` row, or Space on it once the hunk
 * has focus — the hunk's lines are one tab stop, not one each, so a long
 * hunk is not two hundred stops between its buttons and the next hunk's.
 * The patch that stages the picks is built in `hunkModel.mjs` and applied to
 * the index only — the working tree is never written from here, which is
 * what lets the buttons say "stage" without a confirmation. Discarding is
 * consented and says so with the one `SafetyNote`. Annotating opens a
 * composer under the hunk; the note is pinned to this hunk's text, so if the
 * diff moves on, the note knows.
 *
 * A hunk is never its own scrollport: the document that holds the patch
 * scrolls, and a hunk's lines box scrolls sideways only — a diff never wraps.
 */

import { useId, useMemo, useState, type ReactNode } from "react";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { fileDraftKey, fingerprint } from "./gitPanelModel.mjs";
import { useSessionDraft } from "./gitPanelStore";
import { api } from "../../api";
import type { GitFileRow } from "../../types";
import { Button, CURSOR_RING, ConfirmDialog, EmptyState, ICON, KeyHint, TextArea, Tooltip, failureText, hunkDrag, useDragSource, useToast } from "../../ui";
import type { DragData } from "../../ui";
import { shortRef } from "./gitDiscardModel.mjs";
import { VERB, confirmLabel } from "./gitWords.mjs";
import { hunkPatch, hunkPosition, hunkRange, linesPatch, noteScope, parseHunks, pickable, type Hunk, type HunkLine } from "./hunkModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { t } from "../../i18n/l10n.mjs";

/**
 * A hunk's header row, draggable onto the agent pane when it carries a
 * payload (`hunkDrag`): the drag is the kit's, so it works in the desktop
 * webview and shows the hunk's name as it moves.
 */
function HunkHeader({ data, children }: { data: DragData | null; children: ReactNode }) {
  const drag = useDragSource(data);
  return (
    <header ref={drag.ref} {...drag.props} style={drag.style} className="flex flex-wrap items-center gap-1.5 border-b border-hairline px-2 py-1 text-2xs">
      {children}
    </header>
  );
}

/** Kept in one place so the plain and the pickable view cannot drift on the ramp. */
function lineTone(line: HunkLine): string {
  if (line.kind === "add") return "text-ok";
  if (line.kind === "del") return "text-danger";
  if (line.kind === "meta") return "text-text-dim italic";
  return "";
}

const EMPTY_PICKS: Map<number, Set<number>> = new Map();

export function HunkDiff({
  pid,
  wid,
  path,
  staged,
  diff,
  busy,
  onApplied,
  onNoted,
}: {
  /** The project a note is filed under. */
  pid: string;
  /** The checkout the hunk is in. */
  wid: string;
  path: string;
  /** The patch is the index against HEAD (true) or the tree against the index. */
  staged: boolean;
  diff: string;
  busy: boolean;
  /** A write landed: the fresh file list. */
  onApplied: (files: GitFileRow[]) => void;
  /** A note was written; the notes list is stale. */
  onNoted: () => void;
}) {
  const toast = useToast();
  const parsed = useMemo(() => parseHunks(diff), [diff]);
  // The picked lines and the note being composed live in the checkout's
  // session (`gitPanelStore`) under a key that carries the patch: a new patch
  // is a new set of hunks and yesterday's picks name nothing in it, so the
  // key changes and the draft starts clean — and a tab switch keeps both.
  const draftKey = fileDraftKey(rootKey("workstream", wid), path, `hunks:${staged}:${fingerprint(diff)}`);
  const [picks, setPicks] = useSessionDraft<Map<number, Set<number>>>(`${draftKey}|picks`, EMPTY_PICKS);
  const [composing, setComposing] = useSessionDraft<number | null>(`${draftKey}|composing`, null);
  const [note, setNote] = useSessionDraft(`${draftKey}|note`, "");
  const [working, setWorking] = useState<string | null>(null);
  /** The line the keyboard is on, per hunk — Space picks it, ↑↓ move. */
  const [cursor, setCursor] = useState<Record<number, number>>({});
  /** The lines' ids, so each hunk's list can name the line its cursor is on (`aria-activedescendant`). */
  const lineBase = useId();
  const lineId = (hunk: number, line: number) => `${lineBase}-h${hunk}-l${line}`;
  /** A discard waiting on the person's word: the whole hunk, or only the picked lines. */
  const [discarding, setDiscarding] = useState<{ hunk: Hunk; lines: Set<number> | null } | null>(null);

  const toggle = (hunk: number, line: number) => {
    setPicks((m) => {
      const next = new Map(m);
      const set = new Set(next.get(hunk) ?? []);
      if (set.has(line)) set.delete(line);
      else set.add(line);
      next.set(hunk, set);
      return next;
    });
  };

  const apply = async (key: string, patch: string | null) => {
    if (!patch || busy || working) return;
    setWorking(key);
    try {
      // On the staged side the same hunk is *removed* from the index.
      const r = await api.gitStageHunk(wid, patch, staged);
      onApplied(r.files);
    } catch (e) {
      toast.error(failureText("work", "hunk-diff-failed", e));
    } finally {
      setWorking(null);
    }
  };

  /// Discard the hunk's working-tree change — the whole hunk, or only the
  /// picked lines — consented, recorded first.
  const discard = async (hunk: Hunk, lines: Set<number> | null) => {
    setDiscarding(null);
    if (busy || working || !wid) return;
    const patch = lines ? linesPatch(parsed.header, hunk, lines) : hunkPatch(parsed.header, hunk);
    if (!patch) return;
    setWorking(`discard:${hunk.index}`);
    try {
      const r = await api.gitDiscard(wid, { patch });
      toast.ok(t("work-hunk-diff-discarded-what-there-saved", { ref_name: shortRef(r.recovery.ref_name) }));
      onApplied(r.files);
    } catch (e) {
      toast.error(failureText("work", "hunk-diff-failed", e));
    } finally {
      setWorking(null);
    }
  };

  const annotate = async (hunk: Hunk) => {
    if (!note.trim() || working) return;
    setWorking(`note:${hunk.index}`);
    try {
      const range = hunkRange(hunk);
      await api.reviewNoteCreate(pid, {
        workstream: wid,
        path,
        start: range.start,
        end: range.end,
        scope: noteScope(staged),
        hunk: hunk.text,
        body: note.trim(),
      });
      toast.ok(t("work-hunk-diff-note-saved-send-when-done-reviewing"));
      setNote("");
      setComposing(null);
      onNoted();
    } catch (e) {
      toast.error(failureText("work", "hunk-diff-failed", e));
    } finally {
      setWorking(null);
    }
  };

  if (parsed.hunks.length === 0) {
    return <EmptyState title={t("work-hunk-diff-no-hunks-patch")} hint={t("work-hunk-diff-file-changed-way-has-no-lines")} className="py-3" action={null} />;
  }

  const verb = staged ? VERB.unstage : VERB.stage;
  const doing = (w: string) => `${w.replace(/e$/, "")}ing…`;
  const total = parsed.hunks.length;

  return (
    <div className="flex flex-col gap-2">
      <ConfirmDialog
        open={discarding !== null}
        onClose={() => setDiscarding(null)}
        onConfirm={() => discarding && void discard(discarding.hunk, discarding.lines)}
        title={discarding?.lines ? t("work-hunk-diff-discard-picked-line-lines", { lines: discarding.lines.size }) : t("work-hunk-diff-discard-hunk")}
        body={
          <div className="flex flex-col gap-2">
            <p>{discarding?.lines ? t("work-hunk-diff-picked-lines-working-tree-change-replaced") : t("work-hunk-diff-working-tree-change-hunk-replaced-index")}</p>
            <SafetyNote kind="tree" />
          </div>
        }
        confirmLabel={confirmLabel("discard")}
        danger
      />
      {parsed.hunks.map((hunk) => {
        const picked = picks.get(hunk.index) ?? new Set<number>();
        const choosable = pickable(hunk);
        const pickedChanges = choosable.filter((i) => picked.has(i)).length;
        const isComposing = composing === hunk.index;
        const at = cursor[hunk.index] ?? -1;
        const isWorking = (key: string) => working === `${key}:${hunk.index}`;
        return (
          <section key={hunk.index} className="min-w-0 overflow-hidden rounded-control border border-border bg-surface-2" aria-label={t("work-hunk-diff-hunk", { index: hunk.index + 1, total })}>
            <HunkHeader data={wid ? hunkDrag({ path, staged, text: hunk.text, id: `${path}@${hunk.newStart}` }) : null}>
              <Tooltip label={wid ? t("work-hunk-diff-drag-onto-agent-pane-attach-hunk") : ""}>
                <span className={`min-w-0 flex-1 truncate font-mono text-text-dim ${wid ? "cursor-grab" : ""}`}>
                  <span className="mr-2 text-text-dim">{hunkPosition(hunk.index, total)}</span>
                  {hunk.header}
                </span>
              </Tooltip>
              {pickedChanges > 0 && (
                <>
                  <Tooltip label={t("work-hunk-diff-only-picked-line-lines-rest-stays", { pickedChanges })}>
                    <span className="inline-flex">
                      <Button size="sm" variant="default" disabled={busy || working !== null} onClick={() => void apply(`lines:${hunk.index}`, linesPatch(parsed.header, hunk, picked))}>
                        {isWorking("lines") ? doing(verb) : t("work-hunk-diff-line-lines", { verb, pickedChanges })}
                      </Button>
                    </span>
                  </Tooltip>
                  {!staged && wid && (
                    <Button size="sm" variant="ghost" disabled={busy || working !== null} onClick={() => setDiscarding({ hunk, lines: picked })}>
                      <ICON.discard size={12} aria-hidden />
                      {isWorking("discard") ? t("work-hunk-diff-discarding") : t("work-hunk-diff-line-lines-2", { discard: VERB.discard, pickedChanges })}
                    </Button>
                  )}
                </>
              )}
              <Button size="sm" disabled={busy || working !== null} onClick={() => void apply(`hunk:${hunk.index}`, hunkPatch(parsed.header, hunk))}>
                {staged ? <ICON.unstage size={12} aria-hidden /> : <ICON.stage size={12} aria-hidden />}
                {isWorking("hunk") ? doing(verb) : t("work-hunk-diff-hunk-2", { verb })}
              </Button>
              {!staged && wid && pickedChanges === 0 && (
                <Button size="sm" variant="ghost" disabled={busy || working !== null} onClick={() => setDiscarding({ hunk, lines: null })}>
                  <ICON.discard size={12} aria-hidden />
                  {isWorking("discard") ? t("work-hunk-diff-discarding") : VERB.discard}
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                aria-pressed={isComposing}
                disabled={working !== null}
                onClick={() => {
                  setComposing(isComposing ? null : hunk.index);
                  setNote("");
                }}
              >
                <ICON.note size={12} aria-hidden />{t("work-hunk-diff-annotate")}</Button>
            </HunkHeader>
            {/* One tab stop per hunk: the arrows move a cursor over the lines that
                can be picked, Space picks the one under it. */}
            <div
              className="overflow-x-auto focus:outline-none"
              tabIndex={0}
              role="listbox"
              aria-multiselectable
              aria-label={t("work-hunk-diff-lines-hunk", { index: hunk.index + 1 })}
              aria-activedescendant={at >= 0 ? lineId(hunk.index, at) : undefined}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget || choosable.length === 0) return;
                const pos = Math.max(0, choosable.indexOf(at));
                if (e.key === "ArrowDown") setCursor((c) => ({ ...c, [hunk.index]: choosable[Math.min(choosable.length - 1, at < 0 ? 0 : pos + 1)] }));
                else if (e.key === "ArrowUp") setCursor((c) => ({ ...c, [hunk.index]: choosable[Math.max(0, pos - 1)] }));
                else if (e.key === " " || e.key === "Enter") {
                  if (at >= 0) toggle(hunk.index, at);
                } else return;
                e.preventDefault();
              }}
              onFocus={(e) => {
                if (e.target === e.currentTarget && at < 0 && choosable.length > 0) setCursor((c) => ({ ...c, [hunk.index]: choosable[0] }));
              }}
            >
              <pre className="w-max min-w-full p-2 font-mono text-2xs leading-relaxed">
                {hunk.lines.map((line, i) => {
                  const can = line.kind === "add" || line.kind === "del";
                  const on = picked.has(i);
                  const numbers = `${line.oldLine ?? ""}`.padStart(4) + " " + `${line.newLine ?? ""}`.padStart(4);
                  return (
                    <div
                      key={i}
                      id={can ? lineId(hunk.index, i) : undefined}
                      role={can ? "option" : undefined}
                      aria-selected={can ? on : undefined}
                      onClick={can ? () => toggle(hunk.index, i) : undefined}
                      className={`flex gap-2 ${lineTone(line)} ${can ? "cursor-pointer hover:bg-surface" : ""} ${on ? "bg-selected" : ""} ${at === i ? CURSOR_RING : ""}`}
                    >
                      <span className="tnum select-none text-text-dim">{numbers}</span>
                      {/* The pick, drawn rather than typed: a filled dot picked, an empty one pickable. */}
                      <span aria-hidden className="flex w-3 shrink-0 select-none items-center justify-center">
                        {can && <span className={`h-1.5 w-1.5 rounded-full ${on ? "bg-text" : "border border-text-dim/70"}`} />}
                      </span>
                      <span>{line.text || " "}</span>
                    </div>
                  );
                })}
              </pre>
            </div>
            {isComposing && (
              <div className="flex flex-col gap-1.5 border-t border-hairline p-2">
                <TextArea
                  value={note}
                  rows={2}
                  autoFocus
                  placeholder={t("work-hunk-diff-what-should-change-here-why")}
                  onChange={(e) => setNote(e.target.value)}
                  onKeyDown={(e) => {
                    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") void annotate(hunk);
                  }}
                />
                <div className="flex items-center gap-2 text-2xs text-text-dim">
                  <span>
                    {t("work-hunk-diff-lines-range-staged", { start: hunkRange(hunk).start, end: hunkRange(hunk).end, flag: staged ? "yes" : "no" })}
                  </span>
                  <span className="flex-1" />
                  <KeyHint combo="Mod+Enter" />
                  <Button size="sm" variant="ghost" onClick={() => setComposing(null)}>{t("work-agent-editor-cancel")}</Button>
                  <Button size="sm" variant="primary" disabled={!note.trim() || working !== null} onClick={() => void annotate(hunk)}>
                    {isWorking("note") ? t("work-agent-editor-saving") : t("work-hunk-diff-save-note")}
                  </Button>
                </div>
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
