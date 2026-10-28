/**
 * The Problems list (ide/10): what the root's language servers have
 * published, above the tree in the Files occupant as the search box is —
 * a file row per path, its problems under it errors first, a row opening
 * the document at its line and revealing the path in the tree. The facts
 * are `problemsModel.mjs`'s; the frames are `diagnosticsStore.ts`'s. It
 * lists what the servers said: one that reports only for open documents
 * leaves the rest unsaid, and the empty state says so.
 */

import { useMemo, useState } from "react";
import type { FileScope } from "../../types";
import { Button, Dot, ICON, VirtualList, cn, requestReveal, useTokenPx } from "../../ui";
import { useDiagnostics } from "./diagnosticsStore";
import { problemCounts, problemRows, problemsWords, severityWord } from "./problemsModel.mjs";
import type { ProblemRow, Severity } from "./problemsModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** A row is one theme row (`--spacing-row-sm`); this is the frame-zero fallback. */
const ROW_H_FALLBACK = 28;

function basename(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? path : path.slice(at + 1);
}

/** The dot's tone for a severity: an error is danger, a warning warn, the rest quiet. */
function toneOf(severity: Severity): "danger" | "warn" | "quiet" {
  return severity === "error" ? "danger" : severity === "warning" ? "warn" : "quiet";
}

export function FilesProblems({
  scope,
  id,
  onOpen,
  onClose,
}: {
  scope: FileScope;
  id: string;
  /** Open a document at the problem's line — a glance is a preview, as a search hit is. */
  onOpen: (path: string, line: number | null, opts: { preview: boolean }) => void;
  onClose: () => void;
}) {
  const rootKey = `${scope}:${id}`;
  const entries = useDiagnostics(scope, id);
  const rows = useMemo(() => problemRows(entries), [entries]);
  const counts = useMemo(() => problemCounts(entries), [entries]);
  const [cursor, setCursor] = useState(-1);
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_H_FALLBACK);

  const open = (row: ProblemRow, i: number) => {
    if (row.kind === "more") return;
    setCursor(i);
    onOpen(row.path, row.kind === "problem" ? row.line : null, { preview: true });
    requestReveal(rootKey, row.path);
  };

  return (
    <div className="mb-2 flex shrink-0 flex-col gap-1.5 rounded-control border border-border bg-surface-2 p-2" role="region" aria-label={tr("workbench-problems-title")}>
      <div className="flex items-center gap-1.5">
        <span className="text-2xs font-medium text-text">{tr("workbench-problems-title")}</span>
        <span className={cn("min-w-0 flex-1 truncate text-2xs", counts.errors > 0 ? "text-danger" : counts.warnings > 0 ? "text-warn" : "text-text-dim")} aria-live="polite">
          {problemsWords(counts)}
        </span>
        <Button size="sm" variant="ghost" aria-label={tr("workbench-problems-close")} onClick={onClose}>
          <ICON.close size={12} aria-hidden />
        </Button>
      </div>
      {rows.length === 0 ? (
        <p className="px-1 text-2xs leading-relaxed text-text-dim">{tr("workbench-problems-none-hint")}</p>
      ) : (
        <VirtualList
          items={rows}
          rowHeight={rowHeight}
          keyOf={(r) => r.key}
          className="max-h-64 rounded-control bg-surface"
          scrollToIndex={cursor}
          render={(row, i) => {
            const at = i === cursor;
            if (row.kind === "more") {
              return <div className="flex h-full items-center pl-6 text-2xs text-text-dim">{tr("workbench-problems-more-file", { more: row.more })}</div>;
            }
            if (row.kind === "file") {
              return (
                <button type="button" tabIndex={-1} onClick={() => open(row, i)} className={cn("flex h-full w-full items-center gap-1.5 px-2 text-left text-2xs hover:bg-surface-2", at && "bg-selected")}>
                  <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
                  <span className="shrink-0 font-mono text-text">{basename(row.path)}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-text-dim">{row.path}</span>
                  {row.errors > 0 && <span className="tnum shrink-0 text-danger">{row.errors}</span>}
                  {row.warnings > 0 && <span className="tnum shrink-0 text-warn">{row.warnings}</span>}
                </button>
              );
            }
            return (
              <button
                type="button"
                tabIndex={-1}
                onClick={() => open(row, i)}
                title={`${severityWord(row.severity)} · ${row.source}${row.code ? ` ${row.code}` : ""}`}
                className={cn("flex h-full w-full items-center gap-2 pl-4 pr-2 text-left text-2xs hover:bg-surface-2", at && "bg-selected")}
              >
                <Dot tone={toneOf(row.severity)} title={severityWord(row.severity)} />
                <span className="min-w-0 flex-1 truncate text-text">{row.message}</span>
                <span className="tnum shrink-0 font-mono text-text-dim">
                  {row.line}:{row.column}
                </span>
              </button>
            );
          }}
        />
      )}
    </div>
  );
}
