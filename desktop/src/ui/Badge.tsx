/**
 * Unread and count badges.
 *
 * Two shapes on purpose: a number when you need to know how many, a dot when
 * you only need to know *that*. Muted rows keep the dot but drop the number,
 * the way a muted channel still shows life without demanding attention.
 */

import { badgeText } from "./badgeModel.mjs";
import { cn } from "./cn";
import { t } from "../i18n/l10n.mjs";

type Tone = "accent" | "neutral" | "quiet" | "danger";

/**
 * `accent-contrast` is the ink a theme picks for text on a saturated fill,
 * and it serves `danger` as well as the accent: the two sit at the same
 * lightness in every theme by construction — both dark against a light
 * surface, both light against a dark one — so one role covers both, and a
 * theme cannot get one right while getting the other wrong.
 */
const TONE: Record<Tone, string> = {
  accent: "bg-accent text-accent-contrast",
  neutral: "border border-border bg-surface-2 text-text-dim",
  /** A count that is a fact, not a call — the rail's section headings — with no border to read as a control. */
  quiet: "bg-surface-2/80 text-text-dim",
  danger: "bg-danger text-accent-contrast",
};

export function CountBadge({
  count,
  tone = "accent",
  max,
  title,
  size = "md",
}: {
  count: number;
  tone?: Tone;
  /** The most it counts to before *and more*; the size's own when absent (`badgeModel.BADGE_MAX`: 99, or 9 in an icon's corner). */
  max?: number;
  /** The words for the number — the tooltip, and the accessible name where the badge is the only sign (a rail icon's corner). */
  title?: string;
  /** `sm` is the mark in an icon's corner. */
  size?: "md" | "sm";
}) {
  const text = badgeText(count, size, max);
  if (text === null) return null;
  return (
    <span
      title={title}
      aria-label={title}
      role={title ? "img" : undefined}
      className={cn(
        "tnum inline-flex items-center justify-center rounded-full font-semibold",
        size === "sm" ? "h-3.5 min-w-3.5 px-0.5 text-3xs leading-none" : "h-4 min-w-4 px-1 text-2xs",
        TONE[tone],
      )}
    >
      {text}
    </span>
  );
}

const DOT_TONE: Record<Tone | "ok" | "warn", string> = {
  accent: "bg-accent",
  danger: "bg-danger",
  neutral: "bg-text-dim",
  quiet: "bg-text-dim/70",
  ok: "bg-ok",
  warn: "bg-warn",
};

export function Dot({ tone = "accent", title }: { tone?: Tone | "ok" | "warn"; title?: string }) {
  return (
    <span
      title={title}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      className={cn("inline-block h-1.5 w-1.5 shrink-0 rounded-full", DOT_TONE[tone])}
    />
  );
}

/**
 * A live/working pulse — an agent is mid-turn in this conversation.
 *
 * The ping is the one animation in the kit that never stops, so it is the one
 * that most needs the reduced-motion escape: `motion-safe` drops it to a
 * static dot, which still says "working" because the dot itself is only
 * present while something is.
 */
export function WorkingDot({ title = t("ui-badge-writing") }: { title?: string }) {
  return (
    <span title={title} role="img" className="relative inline-flex h-2 w-2 shrink-0" aria-label={title}>
      <span className="absolute inline-flex h-full w-full rounded-full bg-accent opacity-60 motion-safe:animate-ping" />
      <span className="relative inline-flex h-2 w-2 rounded-full bg-accent" />
    </span>
  );
}
