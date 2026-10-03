import type { ReactNode } from "react";
import { cn } from "./cn";
import { ICON } from "./icons";
import { useShowAfter } from "./useShowAfter";
import { t } from "../i18n/l10n.mjs";

/**
 * A bounded surface.
 *
 * When `onClick` is given the whole card becomes one `<button>`, which is why
 * a card that is a link must not also contain buttons: nesting interactive
 * elements is invalid, and the browser resolves it by giving the outer one
 * the click no matter where you aimed. A card with its own actions is a
 * `<div>` with an explicit link inside it.
 */
export function Card({
  children,
  className,
  onClick,
}: {
  children: ReactNode;
  className?: string;
  onClick?: () => void;
}) {
  // Raised: the family's `shadow-raised` is what a card lifts by — a light
  // edge on glass, nothing on a matte family — so a card and the page it
  // sits on are two layers without a heavier border to say so.
  const cls = cn(
    "rounded-card border border-border bg-surface p-3 shadow-sm",
    onClick && "anim w-full cursor-pointer text-left hover:border-text-dim/30 hover:bg-surface-2",
    className,
  );
  return onClick ? (
    <button type="button" className={cls} onClick={onClick}>
      {children}
    </button>
  ) : (
    <div className={cls}>{children}</div>
  );
}

export function Section({
  title,
  action,
  children,
  className,
}: {
  title?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {(title || action) && (
        <header className="mb-2 flex items-center justify-between gap-2">
          {/* Sentence case at full ink: a section's name is a heading, not a
              label shouted in capitals at the same grey as the meta under it. */}
          {typeof title === "string" ? (
            <h2 className="text-sm font-semibold text-text">{title}</h2>
          ) : (
            title
          )}
          {action}
        </header>
      )}
      {children}
    </section>
  );
}

/**
 * Work is happening and we cannot say how much is left.
 *
 * `role="status"` with the label inside means a screen reader hears "loading"
 * once, when it starts — not on every re-render, and not silently.
 */
/** A turning glyph and a word, in place — after the beat (`useShowAfter`), never at once. */
export function Spinner({ label }: { label?: string }) {
  const due = useShowAfter();
  if (!due) return null;
  return (
    <div className="flex items-center gap-2 text-2xs text-text-dim" role="status">
      <ICON.working size={12} aria-hidden className="motion-safe:animate-spin" />
      {label ?? t("ui-card-loading")}
    </div>
  );
}

/**
 * The line beside a section that reads and reads again: what the read is
 * doing — *reading …*, *checking again…*, *read 12 s ago*, or what failed
 * with the last answer kept (`loadModel.readWords`) — and the door to read
 * now. `role="status"`, so a re-read is heard once; the icon turns while one
 * is in flight and never under reduced motion.
 */
export function ReadLine({
  words,
  busy,
  onReload,
  reloadLabel,
}: {
  words: { text: string; failed: boolean } | null;
  /** A read is in flight: the door turns and is not offered twice. */
  busy?: boolean;
  onReload?: () => void;
  reloadLabel?: string;
}) {
  if (!words && !onReload) return null;
  return (
    <div className={cn("flex items-center gap-1.5 text-2xs", words?.failed ? "text-warn" : "text-text-dim")} role="status">
      {words && <span className="min-w-0 truncate">{words.text}</span>}
      {onReload && (
        <button
          type="button"
          aria-label={reloadLabel ?? t("ui-card-read-again")}
          title={reloadLabel ?? t("ui-card-read-again")}
          disabled={busy}
          className="anim flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-60"
          onClick={onReload}
        >
          <ICON.refresh size={11} aria-hidden className={busy ? "motion-safe:animate-spin" : undefined} />
        </button>
      )}
    </div>
  );
}

export function ErrorNote({ error, retry }: { error: string; retry?: () => void }) {
  return (
    <div role="alert" className="flex items-center justify-between gap-3 rounded-control border border-border bg-danger-soft px-3 py-2 text-2xs text-danger">
      <span className="flex items-start gap-1.5">
        <ICON.danger size={12} aria-hidden className="mt-px shrink-0" />
        {error}
      </span>
      {retry && (
        <button type="button" className="shrink-0 underline underline-offset-2" onClick={retry}>{t("ui-card-retry")}</button>
      )}
    </div>
  );
}
