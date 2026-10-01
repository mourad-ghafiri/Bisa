/**
 * One place that runs an action on a commit (ide/05) — for the row's hover
 * buttons, its context menu, the commit document's toolbar and a ref chip's menu
 * alike — so a consented action confirms once, writes its recovery ref once
 * and toasts once, whichever door it came through. The vocabulary — what an
 * action is, when it is off, its words — is `commitActionsModel.mjs`; the
 * dialogs that ask are `CommitActionDialogs.tsx`, which reads this hook's
 * `pending`. Every act runs through `gitOps` against the checkout's session,
 * so it is one of the Git tab's verbs: off while another runs, and a
 * conflict it stops on is the Resolve card's.
 */
import { useState } from "react";
import { copyText, useToast } from "../../ui";
import type { GraphRow } from "../../types";
import { attachContext } from "../_workbench/agentPaneStore";
import { commitChip } from "../_workbench/contextChips.mjs";
import { showRightPanel } from "../_workbench/rightPanelStore";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { NEW_WORKSTREAM, fire } from "../../shell/shortcuts";
import { doneWords } from "./commitActionsModel.mjs";
import type { CommitActionId, ConsentedKind, NamedKind, RefActionId } from "./commitActionsModel.mjs";
import * as ops from "./gitOps";
import { useGitSession } from "./gitPanelStore";
import { t } from "../../i18n/l10n.mjs";

/** The commit an action is about — a graph row, or the commit document's commit read as one. */
export type ActionTarget = Pick<GraphRow, "id" | "short"> & { subject?: string; parents?: number };
/** A decoration on the commit a ref action is about. */
export type ActionRef = { name: string; kind: string };

/** A question waiting on the person: which action, on which commit, about which ref. */
export type PendingAction = { kind: ConsentedKind | NamedKind; row: ActionTarget; ref: ActionRef | null };

/** What the person chose in the dialog before saying yes. */
export type ActionOptions = {
  /** The parent kept when a merge commit is picked or reverted (1-based). */
  mainline?: number | null;
  /** `-x`: the picked commit's message names where it came from. */
  recordOrigin?: boolean;
};

export interface CommitActions {
  /** The question the dialogs draw, or none. */
  readonly pending: PendingAction | null;
  /** A verb of the Git tab is running; every other one waits. */
  readonly busy: boolean;
  /** An action pressed on a commit: the immediate ones run, the rest become `pending`. */
  start(id: CommitActionId, row: ActionTarget): void;
  /** An action pressed on a ref chip. */
  startRef(id: RefActionId, row: ActionTarget, ref: ActionRef): void;
  /** The person said no, or closed the dialog. */
  cancel(): void;
  /** The person said yes to a consented `pending`, with the dialog's options. */
  confirm(options?: ActionOptions): void;
  /** The person named the branch or the tag `pending` asks for. */
  submitName(name: string, message?: string): void;
}

export function useCommitActions(wid: string, { current = null, onOpen }: { current?: string | null; onChanged?: () => void; onOpen?: (sha: string) => void }): CommitActions {
  const toast = useToast();
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const busy = session.busy !== null;
  const [pending, setPending] = useState<PendingAction | null>(null);
  const on = current ?? "HEAD";

  const copy = (text: string) => {
    void copyText(text).then((ok) => (ok ? toast.ok(t("work-git-panel-copied")) : toast.error(t("work-git-panel-clipboard-refused"))));
  };

  const start = (id: CommitActionId, row: ActionTarget) => {
    switch (id) {
      case "copy_sha":
        return copy(row.id);
      case "copy_short":
        return copy(row.short);
      case "inspect":
        return onOpen?.(row.id);
      case "attach":
        attachContext(commitChip(row.id));
        showRightPanel("agents", `workstream:${wid}`);
        return;
      default:
        setPending({ kind: id, row, ref: null });
    }
  };

  const startRef = (id: RefActionId, row: ActionTarget, ref: ActionRef) => {
    if (id === "copy_ref") return copy(ref.name);
    // A door, not an act: the New workstream dialog opens on this ref, and
    // nothing here moves.
    if (id === "open_workstream") {
      return fire(NEW_WORKSTREAM, { preset: ref.kind === "tag" ? { kind: "tag", tag: ref.name } : { kind: "branch", branch: ref.name } });
    }
    setPending({ kind: id, row, ref });
  };

  const confirm = (options: ActionOptions = {}) => {
    const p = pending;
    setPending(null);
    if (!p || busy) return;
    const { row, ref } = p;
    const mainline = (row.parents ?? 1) > 1 ? (options.mainline ?? 1) : null;
    switch (p.kind) {
      case "checkout":
        return void ops.checkout(scope, wid, row.id, doneWords("checkout", { short: row.short }));
      case "cherry_pick":
        return void ops.cherryPick(scope, wid, { commits: [row.id], record_origin: options.recordOrigin ?? false, mainline }, on, doneWords("cherry_pick", { short: row.short }));
      case "revert":
        return void ops.revert(scope, wid, { commits: [row.id], mainline }, on, doneWords("revert", { short: row.short }));
      case "switch":
        if (!ref) return;
        return void ops.checkout(scope, wid, ref.name, doneWords("switch", { short: row.short, name: ref.name }));
      case "delete_tag":
        if (!ref) return;
        return void ops.tagDelete(scope, wid, ref.name, doneWords("delete_tag", { short: row.short, name: ref.name }));
      default:
        return;
    }
  };

  const submitName = (name: string, message?: string) => {
    const p = pending;
    setPending(null);
    if (!p || busy) return;
    const { row } = p;
    if (p.kind === "branch") {
      void ops.branchCreate(scope, wid, { name, start: row.id, switch: false }, doneWords("branch", { short: row.short, name }));
    } else if (p.kind === "tag") {
      void ops.tagCreate(scope, wid, name, row.id, message?.trim() || null, doneWords("tag", { short: row.short, name }));
    }
  };

  return { pending, busy, start, startRef, cancel: () => setPending(null), confirm, submitName };
}
