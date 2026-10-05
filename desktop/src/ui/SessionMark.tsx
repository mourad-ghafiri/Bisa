/**
 * One mark for a session's state (`sessionState.mjs`): the state's own glyph
 * in the state's tone, moving as `sessionMotion.mjs` says — a slow spin while
 * a tool runs, a breath while it thinks, a nudge while it waits on you, a pop
 * when it is done, a shake when it failed, still otherwise. The same mark on
 * the rail, the session lists, the Agents screen and the status bar, so
 * *waiting* looks the same everywhere and reads without its colour.
 *
 * The mark always sits in the same box, whatever the state, so a row does not
 * jiggle when a session settles. And it dwells: a working word holds for a
 * beat against another working word (`markDwellModel`), so a harness
 * flipping between *thinking* and *running* on every quick tool call does
 * not cut the breath with a spin and back — a raised hand, a failure or an
 * end shows at once.
 */
import { useEffect, useRef, useState } from "react";
import { ICON } from "./icons";
import type { LucideIcon } from "./icons";
import { shownState } from "./markDwellModel.mjs";
import { motionClass, motionOf } from "./sessionMotion.mjs";
import { iconOf, label, stateOf, toneOf } from "./sessionState.mjs";
import type { SessionTone, SessionWord } from "./sessionState.mjs";
import type { SessionState } from "../types";

type MarkState = SessionState | SessionWord | null | undefined;

/**
 * The state the mark shows for the one the roster says: the same, except
 * that a working word replacing another working word within the dwell is
 * held back until the beat is over, then shown. The rule is the model's; the
 * timer is here, cleared when the state moves again or the mark leaves.
 */
function useDwelled(state: MarkState): MarkState {
  const [shown, setShown] = useState<MarkState>(state);
  // What the mark shows and since when — by word, so another tool under
  // *running* does not restart the beat.
  const held = useRef<{ state: MarkState; since: number }>({ state, since: Date.now() });
  useEffect(() => {
    const show = (next: MarkState) => {
      const since = stateOf(next) === stateOf(held.current.state) ? held.current.since : Date.now();
      held.current = { state: next, since };
      setShown(next);
    };
    const { state: next, holdMs } = shownState(held.current.state, state, Date.now() - held.current.since);
    if (holdMs === 0) {
      show(next);
      return;
    }
    const timer = window.setTimeout(() => show(state), holdMs);
    return () => window.clearTimeout(timer);
  }, [state]);
  return shown;
}

const TONE_CLASS: Record<SessionTone, string> = {
  accent: "text-accent",
  working: "text-accent",
  ok: "text-ok",
  danger: "text-danger",
  dim: "text-text-dim",
};

/** The glyph the vocabulary names for a state — `icon:<key>` resolved against the one glyph map. */
export function sessionGlyph(state: SessionState | SessionWord | null | undefined): LucideIcon {
  const key = iconOf(state).replace(/^icon:/, "") as keyof typeof ICON;
  return ICON[key] ?? ICON.agent;
}

export function SessionMark({ state: said, title, size = 12 }: { state: MarkState; title?: string; size?: number }) {
  const state = useDwelled(said);
  const Glyph = sessionGlyph(state);
  const text = title ?? label(state);
  const motion = motionClass(motionOf(state));
  return (
    <span
      role="img"
      aria-label={text}
      title={text}
      className={`inline-flex shrink-0 items-center justify-center ${TONE_CLASS[toneOf(state)]}`}
      style={{ width: size + 2, height: size + 2 }}
    >
      <Glyph size={size} aria-hidden className={motion || undefined} />
    </span>
  );
}
