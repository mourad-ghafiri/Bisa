/**
 * One mark for a session's state (`sessionState.mjs`): the state's own glyph
 * in the state's tone, moving as `sessionMotion.mjs` says — a slow spin while
 * a tool runs, a breath while it thinks, a nudge while it waits on you, a pop
 * when it is done, a shake when it failed, still otherwise. The same mark on
 * the rail, the session lists, the Agents screen and the status bar, so
 * *waiting* looks the same everywhere and reads without its colour.
 *
 * The mark always sits in the same box, whatever the state, so a row does not
 * jiggle when a session settles.
 */
import { ICON } from "./icons";
import type { LucideIcon } from "./icons";
import { motionClass, motionOf } from "./sessionMotion.mjs";
import { iconOf, label, toneOf } from "./sessionState.mjs";
import type { SessionTone, SessionWord } from "./sessionState.mjs";
import type { SessionState } from "../types";

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

export function SessionMark({ state, title, size = 12 }: { state: SessionState | SessionWord | null | undefined; title?: string; size?: number }) {
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
