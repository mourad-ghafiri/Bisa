/**
 * The Files tab's search box (ide/03): one field, two modes.
 * **Names** ranks the root's path index as you type — instant, the palette's
 * scorer. **Contents** streams the node's ripgrep (`GET /ide/search`) with a
 * hit per line as it lands, debounced, the previous run aborted. A hit opens
 * the document at its line and shows the path in the tree below. The facts —
 * what the box parses to, how hits fold, what the footer says — are
 * `fileSearchModel.mjs`'s. What was asked — the text, the mode, the three
 * switches — is the root's view, kept across closing the box, leaving the
 * IDE and a restart; the hits are the disk's, and are read again.
 */

import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../api";
import { usePathIndex } from "../../shell/pathIndexStore";
import { useViewState } from "../../shell/viewMemoryStore";
import { flagValue, textValue, wordOf } from "../../shell/viewValuesModel.mjs";
import type { FileScope, SearchHit, SearchSummary } from "../../types";
import { Button, ICON, SegmentedControl, TextInput, VirtualList, cn, requestReveal, useTokenPx } from "../../ui";
import { groupHits, nameResults, nextOpenable, parseQuery, resultRows, searchStatus, splitHit } from "./fileSearchModel.mjs";
import type { ResultRow } from "./fileSearchModel.mjs";
import { idePlace } from "./idePlacesModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

type Mode = "names" | "contents";
const MODES = [
  { id: "names" as Mode, label: tr("workbench-files-search-names") },
  { id: "contents" as Mode, label: tr("workbench-files-search-contents") },
];
/** A kept mode the box does not offer is the box's own beginning. */
const modeValue = wordOf<Mode>(["names", "contents"]);
const DEBOUNCE_MS = 180;
/** A result row is one theme row (`--spacing-row-sm`); this is the frame-zero fallback. */
const ROW_H_FALLBACK = 28;

function basename(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? path : path.slice(at + 1);
}

export function FilesSearch({
  scope,
  id,
  focusNonce,
  onOpen,
  onClose,
}: {
  scope: FileScope;
  id: string;
  /** Bumped by the opener: focus the box again. */
  focusNonce: number;
  /** Open a document; `line` when a content hit says where. A glance — a click, an arrow — is a preview; Enter keeps it. */
  onOpen: (path: string, line: number | null, opts: { preview: boolean }) => void;
  onClose: () => void;
}) {
  const rootKey = `${scope}:${id}`;
  const place = idePlace(rootKey);
  const [text, setText] = useViewState(place, "search.q", "", textValue);
  const [mode, setMode] = useViewState<Mode>(place, "search.mode", "names", modeValue);
  const [regex, setRegex] = useViewState(place, "search.regex", false, flagValue);
  const [caseSensitive, setCaseSensitive] = useViewState(place, "search.case", false, flagValue);
  const [word, setWord] = useViewState(place, "search.word", false, flagValue);
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [summary, setSummary] = useState<SearchSummary | null>(null);
  const [live, setLive] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cursor, setCursor] = useState(-1);
  /** Bumped on every step, so re-selecting the same row scrolls it back into view. */
  const [scrollNonce, setScrollNonce] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const index = usePathIndex(scope, id);
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_H_FALLBACK);

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusNonce]);

  const query = useMemo(() => parseQuery(text), [text]);

  // Names: instant, over the cached index.
  const names = useMemo(() => (mode === "names" && index ? nameResults(index.paths, query.needle) : []), [mode, index, query.needle]);

  // Contents: the node's stream, debounced; the previous run aborted.
  useEffect(() => {
    if (mode !== "contents" || !query.needle) {
      setHits([]);
      setSummary(null);
      setLive(false);
      setError(null);
      return;
    }
    const ac = new AbortController();
    const collected: SearchHit[] = [];
    let flush: number | null = null;
    const t = window.setTimeout(() => {
      setHits([]);
      setSummary(null);
      setError(null);
      setLive(true);
      api
        .ideSearch(
          scope,
          id,
          { q: query.needle, regex, case: caseSensitive ? "sensitive" : "smart", word, include: query.include, exclude: query.exclude },
          (hit) => {
            collected.push(hit);
            // Paint in batches: one state update per frame, not per line.
            if (flush === null) {
              flush = window.requestAnimationFrame(() => {
                flush = null;
                if (!ac.signal.aborted) setHits([...collected]);
              });
            }
          },
          ac.signal,
        )
        .then((s) => {
          if (ac.signal.aborted) return;
          setHits([...collected]);
          setSummary(s);
          setLive(false);
        })
        .catch((e: unknown) => {
          if (ac.signal.aborted) return;
          setError(e instanceof Error ? e.message : String(e));
          setLive(false);
        });
    }, DEBOUNCE_MS);
    return () => {
      window.clearTimeout(t);
      if (flush !== null) window.cancelAnimationFrame(flush);
      ac.abort();
    };
    // The include and exclude lists are keyed by their text: a fresh array
    // with the same names is the same search, and following the arrays
    // would cancel the one in flight on every render.
  }, [mode, query.needle, query.include.join(","), query.exclude.join(","), regex, caseSensitive, word, scope, id]); // eslint-disable-line react-hooks/exhaustive-deps

  const rows: ResultRow[] = useMemo(
    () => (mode === "names" ? names.map((path) => ({ kind: "file" as const, key: `f:${path}`, path, count: 0 })) : resultRows(groupHits(hits))),
    [mode, names, hits],
  );
  // A new question is a new list: the cursor starts over. Keyed on the
  // question, not on the rows — a content search streams a fresh row array
  // per frame, and arrowing through results while it runs must hold.
  useEffect(() => setCursor(-1), [mode, query.needle]);

  const open = (row: ResultRow, preview: boolean) => {
    if (row.kind === "more") return;
    onOpen(row.path, row.kind === "hit" ? row.line : null, { preview });
    requestReveal(rootKey, row.path);
  };
  /** Move the cursor a hit on and show that hit — as a preview, so stepping through results reuses one tab. */
  const step = (dir: 1 | -1) => {
    const i = nextOpenable(rows, cursor, dir);
    setCursor(i);
    setScrollNonce((n) => n + 1);
    if (i >= 0) open(rows[i], true);
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      // Enter keeps the hit under the cursor; with none yet, it steps to the first.
      if (cursor >= 0 && rows[cursor]) open(rows[cursor], false);
      else step(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (text) setText("");
      else onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      step(e.key === "ArrowDown" ? 1 : -1);
    }
  };

  const toggle = (on: boolean, set: (v: boolean) => void, label: string, title: string) => (
    <button
      type="button"
      aria-pressed={on}
      title={title}
      onClick={() => set(!on)}
      className={cn("anim rounded-control border px-1 font-mono text-3xs leading-tight", on ? "border-accent/60 bg-accent-soft text-accent-ink" : "border-border text-text-dim hover:text-text")}
    >
      {label}
    </button>
  );

  const footer = mode === "names" ? (query.needle ? tr("workbench-files-search-file-files-index-stops-cap", { names: names.length, flag: (index?.truncated) ? "yes" : "no" }) : "") : error ?? searchStatus(summary, live, hits.length);

  return (
    <div className="mb-2 flex shrink-0 flex-col gap-1.5 rounded-control border border-border bg-surface-2 p-2" role="search" aria-label={tr("workbench-files-search-search-files")}>
      <div className="flex items-center gap-1.5">
        <TextInput
          ref={input}
          value={text}
          placeholder={mode === "names" ? tr("workbench-files-search-file-name-src-narrows") : tr("workbench-files-search-text-files-src-lock")}
          aria-label={mode === "names" ? tr("workbench-files-search-search-file-names") : tr("workbench-files-search-search-file-contents")}
          className="h-6 min-w-0 flex-1 text-2xs"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={onKey}
        />
        <Button size="sm" variant="ghost" aria-label={tr("workbench-files-search-close-search")} onClick={onClose}>
          <ICON.close size={12} aria-hidden />
        </Button>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <SegmentedControl label={tr("workbench-files-search-search")} size="sm" options={MODES} value={mode} onChange={setMode} />
        {mode === "contents" && (
          <>
            {toggle(regex, setRegex, ".*", tr("workbench-files-search-regular-expression"))}
            {toggle(caseSensitive, setCaseSensitive, "Aa", tr("workbench-files-search-match-case"))}
            {toggle(word, setWord, "ab", tr("workbench-files-search-whole-word"))}
          </>
        )}
        <span className="flex-1" />
        <span className={cn("text-2xs", error ? "text-danger" : "text-text-dim")} aria-live="polite">
          {footer}
        </span>
      </div>
      {rows.length > 0 && (
        <VirtualList
          items={rows}
          rowHeight={rowHeight}
          keyOf={(r) => r.key}
          className="max-h-64 rounded-control border border-border bg-surface"
          scrollToIndex={cursor}
          scrollNonce={scrollNonce}
          render={(row, i) => {
            const at = i === cursor;
            if (row.kind === "more") {
              return <div className="flex h-full items-center pl-6 text-2xs text-text-dim">{tr("workbench-files-search-more-file", { more: row.more })}</div>;
            }
            if (row.kind === "file") {
              return (
                <button type="button" tabIndex={-1} onClick={() => open(row, true)} className={cn("flex h-full w-full items-center gap-1.5 px-2 text-left text-2xs hover:bg-surface-2", at && "bg-accent-soft")}>
                  <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
                  <span className="shrink-0 font-mono text-text">{basename(row.path)}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-text-dim">{row.path}</span>
                  {row.count > 0 && <span className="tnum shrink-0 text-text-dim">{row.count}</span>}
                </button>
              );
            }
            const [before, match, after] = splitHit(row.text, row.column, regex ? 0 : query.needle.length);
            return (
              <button type="button" tabIndex={-1} onClick={() => open(row, true)} className={cn("flex h-full w-full items-center gap-2 pl-6 pr-2 text-left text-2xs hover:bg-surface-2", at && "bg-accent-soft")}>
                <span className="tnum w-8 shrink-0 text-right text-text-dim">{row.line}</span>
                <span className="min-w-0 flex-1 truncate font-mono">
                  <span className="text-text-dim">{before}</span>
                  <span className="rounded-sm bg-warn/30 text-text">{match}</span>
                  <span className="text-text-dim">{after}</span>
                </span>
              </button>
            );
          }}
        />
      )}
    </div>
  );
}
