/**
 * The Files tab's search box (ide/03): one field, two modes.
 * **Names** ranks the root's path index as you type — instant, the palette's
 * scorer. **Contents** streams the node's ripgrep (`GET /ide/search`) with a
 * hit per line as it lands, debounced, the previous run aborted. A hit opens
 * the document at its line and shows the path in the tree below. The facts —
 * what the box parses to, how hits fold, what the footer says — are
 * `fileSearchModel.mjs`'s. What was asked — the text, the mode, the three
 * switches, the replacement — is the root's view, kept across closing the
 * box, leaving the IDE and a restart; the hits are the disk's, and are read
 * again. **Replace** (ide/12) is a row under the box in Contents mode: a
 * preview draws every line that would change under its file, and an apply —
 * all, or one file — carries the preview's hashes back, so a file that moved
 * on in between is skipped and said so, never clobbered. Those facts are
 * `fileReplaceModel.mjs`'s; a preview made for another question is not drawn.
 */

import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../api";
import { usePathIndex } from "../../shell/pathIndexStore";
import { useViewState } from "../../shell/viewMemoryStore";
import { flagValue, textValue, wordOf } from "../../shell/viewValuesModel.mjs";
import type { FileScope, IdeReplacePreview, SearchHit, SearchSummary } from "../../types";
import { Button, ICON, SegmentedControl, TextInput, VirtualList, failureText, cn, requestReveal, useTokenPx } from "../../ui";
import { groupHits, nameResults, nextOpenable, parseQuery, resultRows, searchStatus, splitHit } from "./fileSearchModel.mjs";
import type { ResultRow } from "./fileSearchModel.mjs";
import { captureHint, previewRows, previewStale, previewWords, questionKey, replaceBody, replaceOutcomeWords, replacePlan } from "./fileReplaceModel.mjs";
import type { PreviewRow, ReplaceOutcomeWords } from "./fileReplaceModel.mjs";
import { idePlace } from "./idePlacesModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

type Mode = "names" | "contents";
/** What the list draws: the search's rows, or a preview's. */
type Row = ResultRow | PreviewRow;
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
  // Replace (ide/12): the row's switch and the replacement are the root's
  // view too; the preview is the disk's answer to one question, kept only
  // while the box still asks it, and never across a restart.
  const [replaceOn, setReplaceOn] = useViewState(place, "search.replace", false, flagValue);
  const [replacement, setReplacement] = useViewState(place, "search.replacement", "", textValue);
  const [preview, setPreview] = useState<{ key: string; files: IdeReplacePreview[] } | null>(null);
  const [replacing, setReplacing] = useState<"preview" | "apply" | null>(null);
  const [outcome, setOutcome] = useState<ReplaceOutcomeWords | null>(null);
  const [replaceError, setReplaceError] = useState<string | null>(null);
  /** Bumped after an apply: the search reads the disk again, as it is now. */
  const [rerun, setRerun] = useState(0);
  const replaceInput = useRef<HTMLInputElement>(null);
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
          setError(failureText("workbench", "files-search-failed", e));
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
  }, [mode, query.needle, query.include.join(","), query.exclude.join(","), regex, caseSensitive, word, scope, id, rerun]); // eslint-disable-line react-hooks/exhaustive-deps

  // The preview answers one question. The box moved on — the needle, a
  // glob, a switch, the replacement — and it is not drawn; Replace all
  // waits for a fresh one rather than write what nobody is looking at.
  const flags = { regex, caseSensitive, word };
  const key = questionKey(query, flags, replacement);
  const shown = mode === "contents" && replaceOn && preview && !previewStale(preview, key) ? preview : null;
  const runPreview = () => {
    if (!query.needle || replacing !== null) return;
    setReplacing("preview");
    setOutcome(null);
    setReplaceError(null);
    const asked = key;
    api
      .ideReplace(scope, id, replaceBody(query, flags, replacement))
      .then((r) => {
        if (!r.applied) setPreview({ key: asked, files: r.files });
      })
      .catch((e: unknown) => setReplaceError(failureText("workbench", "files-replace-failed", e)))
      .finally(() => setReplacing(null));
  };
  /** Write what the preview shows — every file, or the one named — each by the hash its preview carried. */
  const runApply = (only: string | null) => {
    if (!shown || replacing !== null) return;
    const plan = replacePlan(shown.files, only);
    if (!plan) return;
    setReplacing("apply");
    setReplaceError(null);
    api
      .ideReplace(scope, id, replaceBody(query, flags, replacement, plan))
      .then((r) => {
        if (!r.applied) return;
        setOutcome(replaceOutcomeWords(r.files));
        // One file written leaves the rest of the preview standing; all
        // written leaves nothing to show. Either way the search reads the
        // disk again; an open document hears of its file through the watcher.
        setPreview((p) => (p && only !== null ? { ...p, files: p.files.filter((f) => f.path !== only) } : null));
        setRerun((n) => n + 1);
      })
      .catch((e: unknown) => setReplaceError(failureText("workbench", "files-replace-failed", e)))
      .finally(() => setReplacing(null));
  };

  const rows: Row[] = useMemo(
    () => (shown ? previewRows(shown.files) : mode === "names" ? names.map((path) => ({ kind: "file" as const, key: `f:${path}`, path, count: 0 })) : resultRows(groupHits(hits))),
    [shown, mode, names, hits],
  );
  // A new question is a new list: the cursor starts over. Keyed on the
  // question, not on the rows — a content search streams a fresh row array
  // per frame, and arrowing through results while it runs must hold. A
  // preview arriving or leaving is a new list too.
  useEffect(() => setCursor(-1), [mode, query.needle, shown]);

  const open = (row: Row, preview: boolean) => {
    if (row.kind === "more") return;
    onOpen(row.path, row.kind === "hit" || row.kind === "change" ? row.line : null, { preview });
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
  const onReplaceKey = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      // Enter previews; with a fresh preview on screen, ⌘Enter (Ctrl+Enter) writes it all.
      if ((e.metaKey || e.ctrlKey) && shown) runApply(null);
      else runPreview();
    } else if (e.key === "Escape") {
      e.preventDefault();
      // Escape clears the replacement, then closes the row and hands the caret back to the box.
      if (replacement) setReplacement("");
      else {
        setReplaceOn(false);
        input.current?.focus();
      }
    }
  };
  /** The footer while Replace is on: the error, the work under way, the preview's count, or the last apply's line — the search's status when none. */
  const replaceFooter = (): { text: string; tone: "dim" | "ok" | "warn" | "danger" } | null => {
    if (mode !== "contents" || !replaceOn) return null;
    if (replaceError) return { text: replaceError, tone: "danger" };
    if (replacing === "preview") return { text: tr("workbench-files-replace-previewing"), tone: "dim" };
    if (replacing === "apply") return { text: tr("workbench-files-replace-replacing"), tone: "dim" };
    if (shown) return { text: previewWords(shown.files), tone: "dim" };
    if (outcome) return { text: outcome.text, tone: outcome.tone };
    return null;
  };
  const replaceLine = replaceFooter();
  /** Why Replace all is off, while it is. */
  const replaceAllOff = replacing !== null ? tr("workbench-files-replace-working") : !shown ? tr("workbench-files-replace-needs-a-preview") : shown.files.length === 0 ? tr("workbench-files-replace-nothing-would-change") : undefined;

  const toggle = (on: boolean, set: (v: boolean) => void, label: string, title: string) => (
    <button
      type="button"
      aria-pressed={on}
      title={title}
      onClick={() => set(!on)}
      // Pressed is where the search stands, not a summons: the neutral `selected` fill.
      className={cn("anim inline-flex h-6 min-w-6 items-center justify-center rounded-control border px-1 font-mono text-2xs leading-none", on ? "border-text/35 bg-selected text-text" : "border-border text-text-dim hover:bg-surface hover:text-text")}
    >
      {label}
    </button>
  );

  const footer = replaceLine ? replaceLine.text : mode === "names" ? (query.needle ? tr("workbench-files-search-file-files-index-stops-cap", { names: names.length, flag: (index?.truncated) ? "yes" : "no" }) : "") : error ?? searchStatus(summary, live, hits.length);
  const footerTone = replaceLine ? replaceLine.tone : error ? "danger" : "dim";

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
      {mode === "contents" && replaceOn && (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-1.5">
            <TextInput
              ref={replaceInput}
              value={replacement}
              placeholder={tr("workbench-files-replace-replace-with")}
              aria-label={tr("workbench-files-replace-replacement")}
              className="h-6 min-w-0 flex-1 font-mono text-2xs"
              spellCheck={false}
              onChange={(e) => setReplacement(e.target.value)}
              onKeyDown={onReplaceKey}
            />
            <Button
              size="sm"
              className="h-6 px-1.5 text-2xs"
              disabled={!query.needle || replacing !== null}
              disabledReason={replacing !== null ? tr("workbench-files-replace-working") : !query.needle ? tr("workbench-files-replace-needs-a-question") : undefined}
              onClick={runPreview}
            >{tr("workbench-files-replace-preview")}</Button>
            {/* The one primary act of the box: it writes what the preview shows, and nothing before a preview. */}
            <Button size="sm" variant="primary" className="h-6 px-1.5 text-2xs" disabled={replaceAllOff !== undefined} disabledReason={replaceAllOff} onClick={() => runApply(null)}>
              {tr("workbench-files-replace-all")}
            </Button>
          </div>
          <span className="text-2xs text-text-dim">{captureHint(regex)}</span>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-1.5">
        <SegmentedControl label={tr("workbench-files-search-search")} size="sm" options={MODES} value={mode} onChange={setMode} />
        {mode === "contents" && (
          <>
            {toggle(regex, setRegex, ".*", tr("workbench-files-search-regular-expression"))}
            {toggle(caseSensitive, setCaseSensitive, "Aa", tr("workbench-files-search-match-case"))}
            {toggle(word, setWord, "ab", tr("workbench-files-search-whole-word"))}
            <Button
              size="sm"
              variant="ghost"
              className={cn("h-6 px-1.5 text-2xs", replaceOn && "bg-selected text-text")}
              aria-pressed={replaceOn}
              title={tr("workbench-files-replace-toggle-title")}
              onClick={() => {
                setReplaceOn(!replaceOn);
                // The row mounts on the next paint; the caret goes to it then.
                if (!replaceOn) window.requestAnimationFrame(() => replaceInput.current?.focus());
              }}
            >{tr("workbench-files-replace-replace")}</Button>
          </>
        )}
        <span className="flex-1" />
        <span className={cn("text-2xs", footerTone === "danger" ? "text-danger" : footerTone === "warn" ? "text-warn" : footerTone === "ok" ? "text-ok" : "text-text-dim")} aria-live="polite">
          {footer}
        </span>
      </div>
      {rows.length > 0 && (
        <VirtualList
          items={rows}
          rowHeight={rowHeight}
          keyOf={(r) => r.key}
          className="max-h-64 rounded-control bg-surface"
          scrollToIndex={cursor}
          scrollNonce={scrollNonce}
          render={(row, i) => {
            const at = i === cursor;
            if (row.kind === "more") {
              return <div className="flex h-full items-center pl-6 text-2xs text-text-dim">{tr("workbench-files-search-more-file", { more: row.more })}</div>;
            }
            if (row.kind === "file") {
              const openRow = (
                <button type="button" tabIndex={-1} onClick={() => open(row, true)} className={cn("flex h-full min-w-0 flex-1 items-center gap-1.5 px-2 text-left text-2xs hover:bg-surface-2", at && "bg-selected")}>
                  <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
                  <span className="shrink-0 font-mono text-text">{basename(row.path)}</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-text-dim">{row.path}</span>
                  {row.count > 0 && <span className="tnum shrink-0 text-text-dim">{row.count}</span>}
                </button>
              );
              // A preview's file row carries its own Replace — one file written by its hash; a nested button is not HTML, so the two sit side by side.
              if (!("baseHash" in row)) return openRow;
              return (
                <div className="flex h-full items-center">
                  {openRow}
                  <Button size="sm" variant="ghost" className="mr-1 h-5 shrink-0 px-1.5 text-2xs" aria-label={tr("workbench-files-replace-in-file", { path: row.path })} disabled={replacing !== null} onClick={() => runApply(row.path)}>
                    {tr("workbench-files-replace-replace")}
                  </Button>
                </div>
              );
            }
            if (row.kind === "change") {
              // The line as it is and as it would be, in the diff's own colours.
              return (
                <button type="button" tabIndex={-1} onClick={() => open(row, true)} className={cn("flex h-full w-full items-center gap-2 pl-6 pr-2 text-left text-2xs hover:bg-surface-2", at && "bg-selected")}>
                  <span className="tnum w-8 shrink-0 text-right text-text-dim">{row.line}</span>
                  <span className="min-w-0 flex-1 truncate font-mono">
                    <span className="rounded-sm bg-danger-soft px-0.5 text-text-dim line-through">{row.before}</span>{" "}
                    <span className="rounded-sm bg-ok-soft px-0.5 text-text">{row.after}</span>
                  </span>
                </button>
              );
            }
            const [before, match, after] = splitHit(row.text, row.column, regex ? 0 : query.needle.length);
            return (
              <button type="button" tabIndex={-1} onClick={() => open(row, true)} className={cn("flex h-full w-full items-center gap-2 pl-6 pr-2 text-left text-2xs hover:bg-surface-2", at && "bg-selected")}>
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
