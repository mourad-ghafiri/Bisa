/**
 * The two controls a commit wears wherever it is drawn — a graph row, a
 * commit document (ide/05): a **ref chip** (a branch, a remote branch, a
 * tag, `HEAD`) with its menu when the ref can be acted on, and an **action
 * button** with the hint, or the reason it is off, as its tooltip. One file
 * so the row and the document cannot drift on what a chip or a button is.
 */

import { ICON, Menu, Tooltip } from "../../ui";
import type { MenuItem } from "../../ui";
import { refActions } from "./commitActionsModel.mjs";
import type { ActionContext, CommitAction } from "./commitActionsModel.mjs";
import type { ActionRef, ActionTarget, CommitActions } from "./useCommitActions";

// HEAD is where you stand, not a summons: full ink, never the accent.
const CHIP_TONE: Record<string, string> = {
  head: "border-text/40 text-text",
  tag: "border-warn/60 text-warn",
  remote: "border-border text-text-dim",
};

/** The graph's context, with the branch HEAD is on — what a ref's menu needs to say *this is where you are*. */
export type RefActionContext = ActionContext & { currentBranch: string | null };

/**
 * A decoration on a commit — a branch, a remote branch, a tag, `HEAD` — and,
 * when the ref can be acted on, its menu (`refActions`): switch to a branch,
 * delete a tag, copy the name. `HEAD` is a fact, not a handle.
 */
export function RefChip({ commit, refName, actions, ctx }: { commit: ActionTarget; refName: ActionRef; actions: CommitActions; ctx: RefActionContext }) {
  const items = refActions(refName, { busy: ctx.busy, inProgress: ctx.inProgress, currentBranch: ctx.currentBranch });
  if (items.length === 0) {
    // A ref with nothing to do to it — HEAD — is a fact, drawn as a plain
    // label so it does not read as a menu beside the chips that are.
    return <span className="shrink-0 font-mono text-3xs leading-tight font-semibold text-text">{refName.name}</span>;
  }
  const tone = CHIP_TONE[refName.kind] ?? "border-ok/60 text-ok";
  const chip = (
    <span className={`anim shrink-0 rounded border px-1 font-mono text-3xs leading-tight hover:bg-surface-2 ${tone}`}>
      {refName.name}
      <ICON.collapsed size={9} aria-hidden className="ml-0.5 inline rotate-90 opacity-70" />
    </span>
  );
  const menu: MenuItem[] = items.map((a) => ({
    label: a.disabled && a.reason ? `${a.label} — ${a.reason}` : a.label,
    icon: ICON[a.icon],
    danger: a.danger,
    disabled: a.disabled,
    onSelect: () => actions.startRef(a.id, commit, refName),
  }));
  return (
    <Menu
      label={`${refName.kind} ${refName.name}`}
      align="start"
      items={menu}
      trigger={
        <span className="inline-flex cursor-pointer" onClick={(e) => e.stopPropagation()}>
          {chip}
        </span>
      }
    />
  );
}

/** One icon button for an action, with the hint or the reason it is off as its tooltip. */
export function ActionButton({ action, target, actions, withLabel = false }: { action: CommitAction; target: ActionTarget; actions: CommitActions; withLabel?: boolean }) {
  const Glyph = ICON[action.icon];
  return (
    <Tooltip label={action.reason ?? action.hint}>
      <span className="inline-flex">
        <button
          type="button"
          disabled={action.disabled}
          aria-label={action.label}
          onClick={(e) => {
            e.stopPropagation();
            actions.start(action.id, target);
          }}
          className={`anim inline-flex h-6 shrink-0 items-center gap-1 rounded px-1 text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45 ${withLabel ? "text-2xs" : ""}`}
        >
          <Glyph size={13} aria-hidden />
          {withLabel && action.label}
        </button>
      </span>
    </Tooltip>
  );
}
