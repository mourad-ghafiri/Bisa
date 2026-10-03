/**
 * The small card a link opens at the pointer (ide/17): a title, a line under
 * it, and a few verbs — *Open in browser · Copy the URL* for a URL, *Open
 * src/a.rs:42 · Reveal in Finder · Copy the path* for a path. A peek with an
 * act, not a dialog: *never mind* is the common answer, so Esc or a click
 * anywhere else is enough to leave it. Focus lands on the first verb.
 */

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { cn } from "./cn";
import { useSurface } from "./openSurfaces";

export interface LinkCardVerb {
  label: string;
  onSelect: () => void;
  primary?: boolean;
  danger?: boolean;
  disabled?: boolean;
}

const MARGIN = 8;

export function LinkCard({
  at,
  title,
  subtitle,
  verbs,
  onClose,
}: {
  at: { x: number; y: number };
  title: string;
  subtitle?: string | null;
  verbs: readonly LinkCardVerb[];
  onClose: () => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);
  // Mounted only while open: a surface the browser tab's native page yields to (`openSurfaces.ts`).
  useSurface(true);
  // Placed under the pointer, and pulled back inside the window when it would
  // run off an edge — measured once painted, so the width is the real one.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const x = Math.max(MARGIN, Math.min(at.x, window.innerWidth - width - MARGIN));
    const y = at.y + height + MARGIN > window.innerHeight ? Math.max(MARGIN, at.y - height - MARGIN) : at.y + MARGIN;
    setPos({ x, y });
    el.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus();
  }, [at]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown, true);
    };
  }, [onClose]);
  return createPortal(
    <div
      ref={box}
      role="dialog"
      aria-label={title}
      style={{ left: pos.x, top: pos.y }}
      className="fixed z-50 flex w-72 max-w-[calc(100vw-1rem)] flex-col gap-1.5 rounded-card border border-border bg-surface p-2 shadow-lg"
    >
      <div className="min-w-0">
        <p className="truncate text-xs font-semibold text-text" title={title}>
          {title}
        </p>
        {subtitle && (
          <p className="truncate font-mono text-2xs text-text-dim" title={subtitle}>
            {subtitle}
          </p>
        )}
      </div>
      <div className="flex flex-col gap-0.5">
        {verbs.map((v) => (
          <button
            key={v.label}
            type="button"
            disabled={v.disabled}
            onClick={() => {
              onClose();
              v.onSelect();
            }}
            className={cn(
              "anim rounded-control px-2 py-1 text-left text-2xs disabled:opacity-45",
              v.primary ? "bg-accent text-accent-contrast hover:opacity-90" : "text-text hover:bg-surface-2",
              v.danger && "text-danger",
            )}
          >
            {v.label}
          </button>
        ))}
      </div>
    </div>,
    document.body,
  );
}
