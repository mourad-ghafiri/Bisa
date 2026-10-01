/**
 * An agent's thinking, folded above its words (13 — Conversations §The
 * reply streams): a disclosure line — *Thinking*, the length, a chevron;
 * *Thinking… 4s* while live — and, open, the reasoning as dim prose, parsed
 * block by block as it streams (`StreamedMarkdown`) with a caret at the end
 * of what has arrived. Folded while live, one dim line shows the tail of
 * the thinking moving, so a folded block still says the agent is at work
 * without the prose's cost. The block is controlled: the timeline decides
 * whether it is open (the control's mode, or this one block's own press) —
 * this file only paints. The words are `views/_studio/liveTurnModel.mjs`'s.
 */

import { useEffect, useState } from "react";
import { ICON } from "./icons";
import { StreamedMarkdown } from "./StreamedMarkdown";
import { cn } from "./cn";
import { glimpse, thinkingWords } from "../views/_studio/liveTurnModel.mjs";

/** The caret at the end of what is still being written. */
export function StreamCaret() {
  return <span aria-hidden className="ml-0.5 inline-block h-3 w-0.5 translate-y-0.5 animate-pulse rounded-sm bg-text-dim motion-reduce:animate-none" />;
}

/** Whole seconds since `since` (unix seconds), ticking once a second while `live`; zero when unknown. */
function useElapsedSeconds(since: number, live: boolean): number {
  const now = () => (since > 0 ? Math.max(0, Math.floor(Date.now() / 1000) - since) : 0);
  const [elapsed, setElapsed] = useState(now);
  useEffect(() => {
    if (!live || since <= 0) return;
    setElapsed(now());
    const timer = window.setInterval(() => setElapsed(now()), 1000);
    return () => window.clearInterval(timer);
    // `now` closes over `since` alone.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [live, since]);
  return live ? elapsed : 0;
}

export function ThinkingBlock({
  text,
  open,
  onToggle,
  live = false,
  since = 0,
  className,
}: {
  text: string;
  open: boolean;
  onToggle: () => void;
  /** Still being written: the line says so and counts, the prose ends in a caret, a folded block shows its tail moving. */
  live?: boolean;
  /** Unix seconds the turn began, for the count while live. */
  since?: number;
  className?: string;
}) {
  const elapsed = useElapsedSeconds(since, live);
  const words = thinkingWords(text.length, open, live, elapsed);
  const Chevron = open ? ICON.expanded : ICON.collapsed;
  return (
    <div className={cn("mb-1 min-w-0", className)}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        title={words.hint}
        className="anim inline-flex items-center gap-1 rounded px-1 text-2xs text-text-dim hover:bg-surface-2 hover:text-text"
      >
        <Chevron size={11} aria-hidden />
        <span className={live ? "italic" : undefined}>{words.label}</span>
        <span className="tnum">{words.length}</span>
      </button>
      {open ? (
        <div className="mt-0.5 border-l-2 border-border pl-2 text-2xs text-text-dim">
          <StreamedMarkdown text={text} />
          {live && <StreamCaret />}
        </div>
      ) : (
        live && text && <p className="mt-0.5 truncate pl-1 text-2xs text-text-dim italic">{glimpse(text)}</p>
      )}
    </div>
  );
}
