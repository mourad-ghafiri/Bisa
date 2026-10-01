/**
 * The library's one card (03-workflows §The library): a workflow of yours and
 * a catalog template wear the same shape — the graph's thumbnail on top,
 * the picture the designer opens on; the title row with the thing's glyph
 * and its name; **one state line** under it (a dot in the state's tone and
 * the words: *ready to run*, *2 problems*, *running in one goal*,
 * *archived*, *installed*); the description clamped to two lines; a dimmed
 * line of facts (*3 steps · 2 inputs · yours*); and a footer for what is a
 * badge or a verb. A workflow that listens wears its mark at the end of the
 * state line — *On*, or *Paused* — for the switch is not one of its states:
 * it is On *and* ready to run, On *and* running. A card that is a door is one `<a>` (the workflow's); the
 * `⋮` a host gives it sits **beside** the door, over the thumbnail's corner,
 * never inside it — an interactive thing inside an anchor is not a thing —
 * and shows on hover, on focus, and while its menu is open. `dimmed` is an
 * archived workflow's look.
 */

import type { ReactNode } from "react";
import type { Definition } from "./workflowGraph.mjs";
import { FOCUS_RING, ICON, cn } from "../../ui";
import type { CardStatus, CardTone, OnMark } from "./workflowCardModel.mjs";
import { WorkflowThumbnail } from "./WorkflowThumbnail";

const MARK: Record<OnMark["tone"], string> = {
  ok: "border-ok/40 text-ok",
  warn: "border-warn/40 text-warn",
};

const DOT: Record<CardTone, string> = {
  ok: "bg-ok",
  danger: "bg-danger",
  accent: "bg-accent",
  quiet: "bg-text-dim",
};

const WORDS: Record<CardTone, string> = {
  ok: "text-ok",
  danger: "text-danger",
  accent: "text-accent-ink",
  quiet: "text-text-dim",
};

export function LibraryCard({
  definition,
  href,
  icon,
  title,
  status,
  mark,
  description,
  meta,
  footer,
  dimmed,
  menu,
}: {
  /** What the thumbnail draws — the steps and their flows. */
  definition: Definition;
  /** The door the whole card is, when it is one. */
  href?: string;
  /** The thing's glyph, before its name. */
  icon: ReactNode;
  title: string;
  /** The one state the card leads with. */
  status?: CardStatus;
  /** While it listens: *On* or *Paused*, at the end of the state line. */
  mark?: OnMark | null;
  description?: string | null;
  /** The dimmed facts under the description, joined with ` · `. */
  meta?: readonly string[];
  /** The footer's badges, and a verb beside them. */
  footer?: ReactNode;
  /** An archived workflow's look. */
  dimmed?: boolean;
  /** The card's `⋮`, laid beside the door over the thumbnail's corner. */
  menu?: ReactNode;
}) {
  const Glyph = status ? (ICON[status.icon as keyof typeof ICON] as typeof ICON.workflow | undefined) : undefined;
  const body = (
    <>
      <span className="block h-32 shrink-0 overflow-hidden border-b border-border bg-gradient-to-b from-surface-2 to-surface p-2" aria-hidden>
        <WorkflowThumbnail definition={definition} />
      </span>
      <span className="flex min-h-0 flex-1 flex-col gap-1 p-3">
        <span className="flex items-center gap-2">
          <span className="shrink-0 text-text-dim">{icon}</span>
          <span className="min-w-0 flex-1 truncate text-xs font-medium">{title}</span>
        </span>
        {(status || mark) && (
          <span className="flex min-w-0 items-center gap-1.5 text-2xs">
            {status && (
              <span className={cn("flex min-w-0 items-center gap-1.5", WORDS[status.tone])}>
                <span aria-hidden className={cn("h-1.5 w-1.5 shrink-0 rounded-full", DOT[status.tone], status.live && "animate-pulse motion-reduce:animate-none")} />
                {Glyph && <Glyph size={11} aria-hidden className="shrink-0" />}
                <span className="min-w-0 truncate">{status.words}</span>
              </span>
            )}
            {mark && (
              <span className={cn("ml-auto inline-flex shrink-0 items-center gap-1 rounded-full border px-1.5", MARK[mark.tone])} data-on-mark={mark.tone}>
                <ICON.signal size={10} aria-hidden />
                {mark.words}
              </span>
            )}
          </span>
        )}
        <span className="line-clamp-2 min-h-[2.6em] text-2xs text-text-dim">{description}</span>
        {meta && meta.length > 0 && <span className="tnum truncate text-2xs text-text-dim">{meta.join(" · ")}</span>}
        {footer && <span className="mt-auto flex flex-wrap items-center gap-1.5 pt-1">{footer}</span>}
      </span>
    </>
  );
  const cls = cn(
    "anim flex h-full min-h-0 flex-col overflow-hidden rounded-card border border-border bg-surface",
    href && cn("hover:border-accent/40 hover:shadow-md", FOCUS_RING),
    dimmed && "opacity-60",
  );
  return (
    <div className="group relative flex min-h-0 flex-col">
      {href ? (
        <a href={href} className={cls} data-library-card>
          {body}
        </a>
      ) : (
        <div className={cls} data-library-card>
          {body}
        </div>
      )}
      {menu && (
        <span className="anim absolute top-2 right-2 z-10 rounded-control bg-surface/85 shadow-sm backdrop-blur opacity-0 group-hover:opacity-100 focus-within:opacity-100 has-[[data-state=open]]:opacity-100">
          {menu}
        </span>
      )}
    </div>
  );
}
