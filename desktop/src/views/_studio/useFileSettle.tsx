/**
 * How a change is settled from a surface that lists files — the turn card
 * under a reply and the bar above the composer (ide/20): one owner of the
 * settle call, the busy word, the *also edited by someone else* confirm
 * before a forced Undo, and *Undo with a note* — the undo, then the note
 * posted as a message — and the bar's bulk word over every pending file,
 * which says what it reached once it lands. The caller mounts `dialogs`
 * once; the bus's `changes_moved` / `changes_settled` frames refetch, so
 * nothing here reloads.
 */

import { useState } from "react";
import type { ReactNode } from "react";
import { ApiError, api } from "../../api";
import type { SettleTarget, Settled } from "../../types";
import { ConfirmDialog, PromptDialog, failureText, useToast } from "../../ui";
import { settledAllWords } from "./changedFilesModel.mjs";
import { skippedWords } from "./turnChangesModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** A verb in flight, keyed by what it targets. */
export type SettleBusy = { kind: "file" | "turn" | "all"; path?: string } | null;

export interface FileSettle {
  busy: SettleBusy;
  keepFile: (path: string) => void;
  /** An overlapped file asks first, then undoes with `force`. */
  undoFile: (path: string, overlapped: boolean) => void;
  keepTurn: (turn: string) => void;
  undoTurn: (turn: string) => void;
  /** Every pending file at once — the bar's *Keep all* / *Undo all*; resolves once the word landed or failed. */
  settleAll: (verdict: "keep" | "undo") => Promise<void>;
  /** Open the note dialog for a file, or for the whole turn. */
  askNote: (target: { file: string } | { turn: string }) => void;
  /** The confirm and the note dialogs — mounted once by the caller. */
  dialogs: ReactNode;
}

export function useFileSettle(conversationId: string, onPostNote?: (text: string) => Promise<void>): FileSettle {
  const toast = useToast();
  const [busy, setBusy] = useState<SettleBusy>(null);
  const [confirmOverlap, setConfirmOverlap] = useState<string | null>(null);
  const [noteFor, setNoteFor] = useState<{ file: string } | { turn: string } | null>(null);

  /** The one settle call; the result once it landed, `null` when it failed (the toast said why). */
  const settle = async (verdict: "keep" | "undo", target: SettleTarget, key: SettleBusy, force = false): Promise<Settled | null> => {
    setBusy(key);
    try {
      const res = await api.settleChanges(conversationId, { verdict, target, ...(force ? { force: true } : {}) });
      const words = skippedWords(res.skipped);
      if (words) toast.info(t("studio-use-file-settle-left-alone", { words }));
      return res;
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) toast.error(t("studio-use-file-settle-somebody-moved-since-refresh-try-again"));
      else toast.error(failureText("studio", "use-file-settle-failed", e));
      return null;
    } finally {
      setBusy(null);
    }
  };

  const settleAll = async (verdict: "keep" | "undo") => {
    const res = await settle(verdict, { grain: "all" }, { kind: "all" });
    const words = res ? settledAllWords(verdict, res.files) : null;
    if (words) toast.ok(words);
  };

  const submitNote = async (target: { file: string } | { turn: string }, text: string) => {
    setNoteFor(null);
    const settleTarget: SettleTarget = "turn" in target ? { grain: "turn", turn: target.turn } : { grain: "file", path: target.file };
    await settle("undo", settleTarget, "turn" in target ? { kind: "turn" } : { kind: "file", path: target.file });
    if (text.trim() && onPostNote) {
      try {
        await onPostNote(text.trim());
      } catch (e) {
        toast.error(e instanceof Error ? e.message : t("studio-use-file-settle-could-not-post-note"));
      }
    }
  };

  const dialogs = (
    <>
      <ConfirmDialog
        open={confirmOverlap !== null}
        onClose={() => setConfirmOverlap(null)}
        onConfirm={() => {
          const path = confirmOverlap;
          setConfirmOverlap(null);
          if (path) void settle("undo", { grain: "file", path }, { kind: "file", path }, true);
        }}
        title={t("studio-use-file-settle-also-edited-someone-else")}
        body={t("studio-use-file-settle-somebody-else-person-s-save-terminal")}
        confirmLabel={t("studio-use-file-settle-undo-anyway")}
        danger
      />
      <PromptDialog
        open={noteFor !== null}
        onClose={() => setNoteFor(null)}
        onSubmit={(text) => {
          const target = noteFor;
          if (target) void submitNote(target, text);
        }}
        title={t("studio-changed-file-row-undo-note")}
        description={t("studio-use-file-settle-change-undone-then-posted-conversation-message")}
        label={t("studio-use-file-settle-note")}
        placeholder={t("studio-use-file-settle-say-why-what-try-instead")}
        submitLabel={t("studio-use-file-settle-undo-post")}
      />
    </>
  );

  return {
    busy,
    keepFile: (path) => void settle("keep", { grain: "file", path }, { kind: "file", path }),
    undoFile: (path, overlapped) => {
      if (overlapped) setConfirmOverlap(path);
      else void settle("undo", { grain: "file", path }, { kind: "file", path });
    },
    keepTurn: (turn) => void settle("keep", { grain: "turn", turn }, { kind: "turn" }),
    undoTurn: (turn) => void settle("undo", { grain: "turn", turn }, { kind: "turn" }),
    settleAll,
    askNote: setNoteFor,
    dialogs,
  };
}
