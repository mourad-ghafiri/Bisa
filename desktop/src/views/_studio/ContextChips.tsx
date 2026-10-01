/**
 * Context chips as they appear on screen (ide/09): above the composer,
 * removable, before a message is sent; on a sent message, read-only, so the
 * transcript shows what each message carried.
 */

import { useState } from "react";
import type { ContextRef } from "../../types";
import { ICON, Tooltip, cn, useLinkHandler, useLinkRoots } from "../../ui";
import { chipLabel } from "../_workbench/contextChips.mjs";
import { markWords } from "../_workbench/captureModel.mjs";
import type { FrameChip } from "../_workbench/contextChips.mjs";
import { t } from "../../i18n/l10n.mjs";

function detail(ref: ContextRef): string | null {
  switch (ref.kind) {
    case "selection":
      return ref.text;
    case "diff_hunk":
      return ref.patch;
    case "terminal":
      return ref.tail;
    case "annotation":
      // The change wanted, then where, then the element as it was — text, never markup.
      return `${ref.note}\n\n${ref.selector}\n${ref.excerpt}`;
    case "capture":
      // The change wanted, then where on which device's screen, then the picture's name (ide/19).
      return `${ref.note}\n\n${markWords(ref.mark ?? null)} of ${ref.label}\n${ref.shot.name}`;
    default:
      return null;
  }
}

function glyph(ref: ContextRef) {
  switch (ref.kind) {
    case "file":
    case "selection":
      return ICON.file;
    case "diff_hunk":
      return ICON.stage;
    case "terminal":
      return ICON.harness;
    case "work_item":
      return ICON.workItem;
    case "commit":
      return ICON.journal;
    case "annotation":
      return ICON.annotate;
    case "capture":
      return ICON.device;
    default:
      return ICON.warn;
  }
}

/** The document a chip stands for, and the line to land on — a selection at its first line, an annotated page at its top. */
function doorOf(item: ContextRef): { path: string; line: number | null } | null {
  switch (item.kind) {
    case "file":
    case "diff_hunk":
      return { path: item.path, line: null };
    case "annotation":
      // A page served from a file opens that file; a page the browser showed from a URL is no document.
      return item.page.kind === "file" ? { path: item.page.path, line: null } : null;
    case "selection":
      return { path: item.path, line: item.range.start };
    default:
      return null;
  }
}

function ContextChip({ item, onRemove }: { item: ContextRef; onRemove?: () => void }) {
  const [open, setOpen] = useState(false);
  const Glyph = glyph(item);
  const body = detail(item);
  const door = doorOf(item);
  const link = useLinkHandler();
  const roots = useLinkRoots();
  // A chip on a sent message is a door (ide/17): its name opens the file
  // through the link handler, with the thread's roots; the chevron beside it
  // is what expands the text.
  const opens = door && link ? door : null;
  return (
    <span className="inline-flex max-w-full flex-col">
      <span className={cn("inline-flex items-center gap-1 rounded-full border border-border bg-surface-2 px-2 py-0.5 text-2xs text-text")}>
        <Glyph size={11} aria-hidden className="shrink-0 text-text-dim" />
        <button
          type="button"
          className={cn("min-w-0 truncate", (body || opens) && "cursor-pointer", opens && "hover:underline")}
          title={"path" in item ? String(item.path) : chipLabel(item)}
          onClick={(e) => {
            if (opens) link?.onLink({ kind: "path", path: opens.path, line: opens.line, col: null, raw: opens.path }, { x: e.clientX, y: e.clientY }, roots, { direct: e.metaKey || e.ctrlKey });
            else if (body) setOpen((v) => !v);
          }}
          aria-expanded={body && !opens ? open : undefined}
        >
          {chipLabel(item)}
        </button>
        {body && opens && (
          <button
            type="button"
            aria-label={open ? t("studio-context-chips-hide-text") : t("studio-context-chips-show-text")}
            aria-expanded={open}
            className="anim rounded text-text-dim hover:text-text"
            onClick={() => setOpen((v) => !v)}
          >
            {open ? <ICON.expanded size={10} aria-hidden /> : <ICON.collapsed size={10} aria-hidden />}
          </button>
        )}
        {onRemove && (
          <Tooltip label={t("studio-context-chips-remove-agent-will-not-see")}>
            <button type="button" aria-label={t("studio-context-chips-remove", { item: chipLabel(item) })} className="anim rounded text-text-dim hover:text-danger" onClick={onRemove}>
              <ICON.close size={10} aria-hidden />
            </button>
          </Tooltip>
        )}
      </span>
      {open && body && (
        <pre className="mt-1 max-h-40 max-w-md overflow-auto rounded-control border border-border bg-surface p-1.5 font-mono text-3xs leading-relaxed text-text-dim">
          {body}
        </pre>
      )}
    </span>
  );
}

/** The chips on a sent message. */
export function ContextChipRow({ refs }: { refs: ContextRef[] | undefined }) {
  if (!refs || refs.length === 0) return null;
  return (
    <div className="mt-1.5 flex flex-wrap gap-1.5">
      {refs.map((r, i) => (
        <ContextChip key={i} item={r} />
      ))}
    </div>
  );
}

/** The removable tray above the composer, with the placement frame in front. */
export function ContextTray({
  frame,
  refs,
  onRemove,
  onClear,
  overBudget,
}: {
  frame: FrameChip[];
  refs: readonly ContextRef[];
  onRemove: (index: number) => void;
  onClear: () => void;
  overBudget: boolean;
}) {
  if (frame.length === 0 && refs.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5 border-t border-border px-3 py-1.5">
      {frame.map((f, i) => (
        <span
          key={`f${i}`}
          className={cn(
            "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-2xs",
            f.kind === "project" && "bg-accent-soft text-accent-ink",
            f.kind === "goal" && "border border-border text-text-dim",
          )}
          title={f.kind === "goal" ? t("studio-context-chips-goal-project-attached-agent-told-which") : t("studio-context-chips-project-agent-works")}
        >
          {f.kind === "goal" ? (
            <a href={`#/goals/${f.id}`} className="hover:underline">
              {t("studio-context-chips-arrow-label", { label: f.label })}
            </a>
          ) : (
            f.label
          )}
        </span>
      ))}
      {refs.map((r, i) => (
        <ContextChip key={`c${i}`} item={r} onRemove={() => onRemove(i)} />
      ))}
      {refs.length > 1 && (
        <button type="button" className="anim ml-auto rounded px-1 text-2xs text-text-dim hover:text-text" onClick={onClear}>{t("studio-context-chips-clear")}</button>
      )}
      {overBudget && <span className="text-2xs text-danger">{t("studio-context-chips-over-64-kib-context-remove-chip")}</span>}
    </div>
  );
}
