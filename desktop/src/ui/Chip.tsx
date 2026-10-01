/**
 * Status chips, one component per namespace.
 *
 * The old version keyed every status through one flat record, so a goal
 * state and a session status could collide on a shared word and silently
 * borrow each other's colour. Each domain now owns its own mapping.
 *
 * The glyphs come from `ui/icons`, which splits its maps the same way and for
 * the same reason. A chip carries both because colour alone is not a signal
 * anyone can rely on — colour-blind readers, a projector, a screenshot in a
 * ticket — and the two together are what make "blocked" legible without
 * reading the word.
 */

import { sessionGlyph } from "./SessionMark";
import { label, toneOf } from "./sessionState.mjs";
import type { SessionState } from "../types";
import type { ReactNode } from "react";
import type { StepState, WorkItemState } from "../types";
import { cn } from "./cn";
import { STEP_STATE_ICON, WORK_ITEM_STATE_ICON, type LucideIcon } from "./icons";
import { t as tr } from "../i18n/l10n.mjs";

export type Tone = "neutral" | "accent" | "warn" | "danger" | "ok" | "quiet";

const TONE: Record<Tone, string> = {
  neutral: "border-border bg-surface-2 text-text-dim",
  accent: "border-transparent bg-accent-soft text-accent-ink",
  warn: "border-transparent bg-warn-soft text-warn",
  danger: "border-transparent bg-danger-soft text-danger",
  ok: "border-transparent bg-ok-soft text-ok",
  quiet: "border-border bg-transparent text-text-dim",
};

export function Chip({
  tone = "neutral",
  children,
  title,
  icon: Icon,
  className,
}: {
  tone?: Tone;
  children: ReactNode;
  title?: string;
  /** From `ui/icons`, never from `lucide-react` directly. */
  icon?: LucideIcon;
  className?: string;
}) {
  return (
    <span
      title={title}
      className={cn(
        "inline-flex h-5 items-center gap-1 rounded-full border px-2 text-2xs font-medium whitespace-nowrap",
        TONE[tone],
        className,
      )}
    >
      {Icon && <Icon size={11} aria-hidden className="shrink-0" />}
      {children}
    </span>
  );
}

/** A step's state inside a run, the same ramp at step scale. A divert is a detour, not a failure. */
const STEP_TONE: Record<StepState["state"], Tone> = {
  pending: "quiet",
  running: "accent",
  waiting: "warn",
  done: "ok",
  skipped: "quiet",
  failed: "danger",
  cancelled: "quiet",
  diverted: "warn",
};

/** The chip's words: the state, with the branches a gateway chose or the boundary that diverted the step. */
function stepStateWords(state: StepState): string {
  if (state.state === "done" && Array.isArray(state.branches) && state.branches.length > 0) return tr("ui-chip-done-branches", { branches: state.branches.join(", ") });
  if (state.state === "diverted") return tr("ui-chip-diverted-by", { by: state.by });
  return state.state;
}

export function StepStateChip({ state }: { state: StepState }) {
  const name = state.state;
  return (
    <Chip tone={STEP_TONE[name] ?? "quiet"} icon={STEP_STATE_ICON[name]}>
      {stepStateWords(state)}
    </Chip>
  );
}

export function WorkItemStateChip({ state }: { state: WorkItemState }) {
  const label = state.state;
  const tone: Tone =
    label === "accepted"
      ? "ok"
      : label === "rejected" || label === "blocked"
        ? "danger"
        : label === "in_progress" || label === "claimed"
          ? "accent"
          : label === "review"
            ? "warn"
            : "quiet";
  return (
    <Chip tone={tone} icon={WORK_ITEM_STATE_ICON[label]}>
      {label.replace(/_/g, " ")}
    </Chip>
  );
}

/** A session's state as a chip, in the vocabulary's tone, glyph and words. */
export function SessionStateChip({ state }: { state: SessionState }) {
  const t = toneOf(state);
  const tone: Tone = t === "working" ? "accent" : t === "dim" ? "quiet" : t;
  return (
    <Chip tone={tone} icon={sessionGlyph(state)}>
      {label(state)}
    </Chip>
  );
}

/**
 * "Waiting on you" — the one badge that should always catch the eye, so it
 * wears the accent. Attention is this app's primary call to action; `warn`
 * is for a condition you should know about, not one that is blocking you.
 */
export function WaitBadge({ count, label }: { count?: number; label?: string }) {
  return (
    <span className="inline-flex h-5 items-center gap-1 rounded-full bg-accent-soft px-2 text-2xs font-semibold text-accent-ink">
      {label ?? tr("ui-chip-waiting")}
      {count !== undefined && count > 1 ? <span className="tnum">{count}</span> : null}
    </span>
  );
}
