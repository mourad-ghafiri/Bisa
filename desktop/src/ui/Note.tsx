/**
 * A fact a pane needs the person to know, said once in a quiet box: why a
 * form is read-only, that a step has already run, that an operation writes
 * outside. Two tones — `neutral` for a plain fact, `warn` for a condition
 * worth caution — and never the accent: a note asks for nothing (DESIGN.md
 * §The One Summons Rule). A failure is not a note; it is the kit's
 * `ErrorNote`, in the danger roles.
 *
 * An optional `door` is the one thing to do about it — *Edit the steps*,
 * *Open in the designer* — drawn as a small raised button under the
 * sentence, so a read-only state is never a dead end (`PRODUCT.md`: doors,
 * not prose). Under, not beside: a note lives in narrow columns, where a
 * button on the sentence's line left the words one per row.
 */

import type { ReactNode } from "react";
import { Button } from "./Button";
import { cn } from "./cn";
import { ICON } from "./icons";

export function Note({
  tone = "neutral",
  door,
  children,
  className,
}: {
  tone?: "neutral" | "warn";
  door?: { label: string; onClick: () => void } | null;
  children: ReactNode;
  className?: string;
}) {
  const Glyph = tone === "warn" ? ICON.warn : ICON.info;
  return (
    <div
      role={tone === "warn" ? "status" : undefined}
      className={cn(
        "flex flex-col items-start gap-2 rounded-control border px-3 py-2 text-2xs leading-relaxed",
        // The plain edge on both tones: a status colour never carries alpha (DESIGN.md §Don't), as `ErrorNote` keeps `border` over `danger-soft`.
        tone === "warn" ? "border-border bg-warn-soft text-text" : "border-border bg-surface-2/70 text-text",
        className,
      )}
    >
      <span className="flex min-w-0 items-start gap-1.5 self-stretch">
        <Glyph size={12} aria-hidden className={cn("mt-0.5 shrink-0", tone === "warn" ? "text-warn" : "text-text-dim")} />
        <span className="min-w-0">{children}</span>
      </span>
      {door && (
        <Button size="sm" onClick={door.onClick} className="ml-4.5">
          {door.label}
        </Button>
      )}
    </div>
  );
}
