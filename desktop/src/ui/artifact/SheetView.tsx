/**
 * A spreadsheet as a grid (ide/12): a header row, lettered columns, a row
 * per record through the kit's `VirtualList`, and one tab per sheet. A
 * delimited text (`.csv`, `.tsv`) is parsed by `sheetModel.mjs`; every other
 * name — `.xlsx`, `.xls`, `.ods` — by SheetJS, loaded when the first one
 * opens and nowhere else.
 */

import { useEffect, useMemo, useState } from "react";
import { Spinner } from "../Card";
import { cn } from "../cn";
import type { Find } from "../find/findModel.mjs";
import { VirtualList } from "../VirtualList";
import { textOf } from "./artifactBytes";
import { cellText, columnLetter, columnWidths, findCells, sheetFacts, sheetFromText, squareRows, type Cell, type Sheet } from "./sheetModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const ROW = 24;

async function sheetsOf(bytes: Uint8Array, kind: "csv" | "xlsx"): Promise<Sheet[]> {
  if (kind === "csv") return [sheetFromText(textOf(bytes))];
  const xlsx = await import("xlsx");
  const book = xlsx.read(bytes, { type: "array", cellDates: true });
  return book.SheetNames.map((name) => ({
    name,
    rows: xlsx.utils.sheet_to_json<Cell[]>(book.Sheets[name], { header: 1, blankrows: false, defval: "" }),
  }));
}

export function SheetView({
  bytes,
  name,
  className,
  onFacts,
  find = null,
  current = -1,
  onFound,
}: {
  bytes: Uint8Array;
  name: string;
  className?: string;
  onFacts?: (words: string) => void;
  /** The find bar's query, searched in the parsed rows; `null` when the bar is closed. */
  find?: Find | null;
  /** The hit the bar is on — scrolled to and marked. */
  current?: number;
  /** How many cells the query lands on. */
  onFound?: (count: number) => void;
}) {
  const kind: "csv" | "xlsx" = /\.(csv|tsv)$/i.test(name) ? "csv" : "xlsx";
  const [sheets, setSheets] = useState<Sheet[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [which, setWhich] = useState(0);
  useEffect(() => {
    let live = true;
    setSheets(null);
    setError(null);
    sheetsOf(bytes, kind)
      .then((s) => {
        if (!live) return;
        setSheets(s);
        setWhich(0);
        onFacts?.(sheetFacts(s));
      })
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [bytes, kind, onFacts]);
  const sheet = sheets?.[which] ?? null;
  const rows = useMemo(() => (sheet ? squareRows(sheet.rows) : []), [sheet]);
  const widths = useMemo(() => columnWidths(rows), [rows]);
  const header = rows[0] ?? [];
  const body = rows.slice(1);
  const hits = useMemo(() => (find && find.query ? findCells(rows, find) : []), [rows, find]);
  useEffect(() => {
    if (find && find.query) onFound?.(hits.length);
  }, [find, hits.length, onFound]);
  const hit = hits[current] ?? null;
  const hitKey = (r: number, c: number) => hits.some((h) => h.row === r && h.col === c);

  if (error) return <p className={cn("p-3 text-2xs text-danger", className)}>{error}</p>;
  if (!sheets) {
    return (
      <div className={cn("p-3", className)}>
        <Spinner label={t("ui-sheet-view-reading-sheet")} />
      </div>
    );
  }
  const cell = (v: Cell, i: number, r: number, head = false) => {
    const found = hits.length > 0 && hitKey(r, i);
    const on = !!hit && hit.row === r && hit.col === i;
    return (
      <div
        key={i}
        aria-current={on ? "true" : undefined}
        className={cn(
          "tnum shrink-0 overflow-hidden text-ellipsis whitespace-nowrap border-r border-border px-1.5 leading-6",
          head ? "font-semibold text-text" : "text-text",
          found && !on && "bg-accent-soft/60",
          on && "bg-warn-soft ring-1 ring-inset ring-warn",
        )}
        style={{ width: `${widths[i] + 2}ch` }}
        title={cellText(v)}
      >
        {cellText(v)}
      </div>
    );
  };
  return (
    <div className={cn("flex h-full flex-col font-mono text-2xs", className)}>
      {sheets.length > 1 && (
        <div data-scroll-keep="sheet-tabs" className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-b border-border px-2">
          {sheets.map((s, i) => (
            <button
              key={s.name}
              type="button"
              onClick={() => setWhich(i)}
              className={cn("anim rounded-control px-2 py-0.5 font-sans", i === which ? "bg-surface-2 text-text" : "text-text-dim hover:text-text")}
            >
              {s.name}
            </button>
          ))}
        </div>
      )}
      {/* Two scrollports, each keeping its place (`useKeptScroll`): the grid sideways, the rows down. */}
      <div data-scroll-keep="sheet" className="min-h-0 flex-1 overflow-auto">
        <div className="min-w-max">
          <div className="flex border-b border-border bg-surface-2 text-text-dim">
            <div className="w-10 shrink-0 border-r border-border" />
            {header.map((_, i) => (
              <div key={i} className="shrink-0 border-r border-border px-1.5 text-center leading-6" style={{ width: `${widths[i] + 2}ch` }}>
                {columnLetter(i)}
              </div>
            ))}
          </div>
          <div className="flex border-b border-border bg-surface-2">
            <div className="w-10 shrink-0 border-r border-border text-center leading-6 text-text-dim">1</div>
            {header.map((v, i) => cell(v, i, 0, true))}
          </div>
          <VirtualList
            items={body}
            rowHeight={ROW}
            keyOf={(_, i) => String(i)}
            keepScroll="sheet-rows"
            className="h-[calc(100%-3rem)]"
            scrollToIndex={hit && hit.row > 0 ? hit.row - 1 : null}
            scrollNonce={current}
            render={(row, i) => (
              <div className="flex border-b border-border/60">
                <div className="w-10 shrink-0 border-r border-border text-center leading-6 text-text-dim">{i + 2}</div>
                {row.map((v, j) => cell(v, j, i + 1))}
              </div>
            )}
          />
        </div>
      </div>
    </div>
  );
}
