/**
 * The one find bar: the query, where it stands (`i/n`), the two modes
 * (`.*` regex, `Aa` match case), next and previous, close — and, when the
 * surface can take a replacement, the replacement field with *Replace* and
 * *Replace all*. The bar knows nothing of what it searches: a terminal
 * hands it to the search addon, a rendering to its highlights, a buffer to
 * `replaceAll`. `findModel.mjs` is the fact behind it; this draws.
 *
 * Enter is next, Shift+Enter previous, Escape closes; the bar takes focus
 * when it opens and hands it back on close through `onClose`. A surface
 * that opens the bar on the replace chord asks for the replacement field
 * with `focus` — a request with a nonce, so asking twice moves the focus
 * twice.
 */

import { useEffect, useRef, type KeyboardEvent } from "react";
import { cn } from "../cn";
import { ICON } from "../icons";
import { countWords, type Find } from "./findModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

export interface FindBarProps {
  find: Find;
  onChange: (next: Find) => void;
  /** The match the bar is on, `-1` for none; `null` before any search ran. */
  index: number | null;
  count: number | null;
  onStep: (dir: "next" | "previous") => void;
  onClose: () => void;
  /** The surface takes a replacement: the second field and its two verbs. */
  replace?: {
    onReplace: () => void;
    onReplaceAll: () => void;
    /** The replacement is refused — a read-only buffer, say. */
    disabled?: boolean;
  };
  /** A sentence under the fields — *Find works over the source of an SVG*. */
  note?: string;
  /** What the bar searches, for the screen reader. */
  label?: string;
  /** Which field takes the focus, and a nonce so a repeated request is honoured. */
  focus?: { field: "find" | "replace"; nonce: number };
  className?: string;
}

export function FindBar({ find, onChange, index, count, onStep, onClose, replace, note, label = tr("ui-find-bar-find"), focus, className }: FindBarProps) {
  const input = useRef<HTMLInputElement>(null);
  const replaceInput = useRef<HTMLInputElement>(null);
  const wantsReplace = focus?.field === "replace" && replace !== undefined;
  useEffect(() => {
    const target = wantsReplace ? replaceInput : input;
    const t = window.setTimeout(() => target.current?.select(), 0);
    return () => window.clearTimeout(t);
  }, [wantsReplace, focus?.nonce]);

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter") {
      e.preventDefault();
      onStep(e.shiftKey ? "previous" : "next");
    }
  };
  const toggle = (key: "regex" | "caseSensitive", title: string, word: string) => (
    <button
      type="button"
      aria-pressed={find[key]}
      title={title}
      className={cn("rounded px-1 font-mono", find[key] ? "bg-accent-soft text-accent-ink" : "text-text-dim hover:text-text")}
      onClick={() => onChange({ ...find, [key]: !find[key] })}
    >
      {word}
    </button>
  );
  const field = "h-5 rounded border border-border bg-surface-2 px-1 font-mono text-2xs text-text outline-none focus:border-accent";

  return (
    <div role="search" aria-label={label} className={cn("flex flex-col gap-1 rounded-control border border-border bg-surface px-1.5 py-1 text-2xs shadow-sm", className)}>
      <div className="flex items-center gap-1">
        <input ref={input} value={find.query} placeholder={tr("ui-find-bar-find")} aria-label={tr("ui-find-bar-find")} className={cn(field, "w-40")} onChange={(e) => onChange({ ...find, query: e.target.value })} onKeyDown={onKey} />
        <span className="tnum w-14 text-center text-text-dim" aria-live="polite">
          {countWords(index ?? -1, count)}
        </span>
        {toggle("regex", tr("ui-find-bar-regular-expression"), ".*")}
        {toggle("caseSensitive", tr("ui-find-bar-match-case"), "Aa")}
        <button type="button" aria-label={tr("ui-find-bar-previous-match")} className="rounded px-1 text-text-dim hover:text-text" onClick={() => onStep("previous")}>
          ↑
        </button>
        <button type="button" aria-label={tr("ui-find-bar-next-match")} className="rounded px-1 text-text-dim hover:text-text" onClick={() => onStep("next")}>
          ↓
        </button>
        <button type="button" aria-label={tr("ui-find-bar-close-find")} className="rounded px-1 text-text-dim hover:text-text" onClick={onClose}>
          <ICON.close size={11} aria-hidden />
        </button>
      </div>
      {replace && (
        <div className="flex items-center gap-1">
          <input
            ref={replaceInput}
            value={find.replacement}
            placeholder={tr("ui-find-bar-replace")}
            aria-label={tr("ui-find-bar-replace-2")}
            className={cn(field, "w-40")}
            onChange={(e) => onChange({ ...find, replacement: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              } else if (e.key === "Enter") {
                e.preventDefault();
                if (e.shiftKey || e.metaKey || e.ctrlKey) replace.onReplaceAll();
                else replace.onReplace();
              }
            }}
          />
          <button type="button" disabled={replace.disabled || !count} className="rounded px-1.5 text-text-dim hover:text-text disabled:opacity-45" onClick={replace.onReplace}>{tr("ui-find-bar-replace")}</button>
          <button type="button" disabled={replace.disabled || !count} className="rounded px-1.5 text-text-dim hover:text-text disabled:opacity-45" onClick={replace.onReplaceAll}>{tr("ui-find-bar-replace-all")}</button>
        </div>
      )}
      {note && <p className="text-text-dim">{note}</p>}
    </div>
  );
}
