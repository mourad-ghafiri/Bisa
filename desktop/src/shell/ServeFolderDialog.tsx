/**
 * *From folder…* (ide/18): choose what the node serves — **Root**, the
 * checkout whole, or one folder of it — from the checkout's own tree.
 *
 * The field on top finds a folder by a few letters of its path and keeps the
 * caret: Up and Down walk the rows from it, Enter serves the one the cursor
 * is on, and Left and Right stay the text's. In the tree they open and close
 * a folder, as in Files. A double-click serves. What is offered, in what
 * order and in which words is `serveFolderModel.mjs`'s; the listing is the
 * explorer's own state machine (`ui/fileTreeModel.mjs`), one read per folder
 * opened, so an ignored `dist/` is there to be served. This file is effects
 * and paint.
 *
 * It opens on the folder last served in this checkout, revealed — Root the
 * first time — so ⌘⇧R then Enter serves again what was served before.
 */

import { useEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../api";
import { Button, Chip, Dialog, ICON, TextInput, TreeList, cn, failureReason } from "../ui";
import { ROOT, initialState, loadKey, pendingLoads, reduce } from "../ui/fileTreeModel.mjs";
import {
  ancestorsOf,
  choiceWords,
  filterProblem,
  filterRows,
  folderRows,
  foldersOf,
  idOfFolder,
  indexFolders,
  listedFolders,
  serverFor,
  stepCursor,
  submitWords,
  type FolderRow,
} from "../views/_workbench/serveFolderModel.mjs";
import type { ServedFolder } from "../types";
import { usePathIndex } from "./pathIndexStore";
import { t } from "../i18n/l10n.mjs";

const ROW_HEIGHT = 26;

export function ServeFolderDialog({
  open,
  onClose,
  onServe,
  wid,
  rootLabel,
  servers,
  initialFolder,
}: {
  open: boolean;
  onClose: () => void;
  /** The folder chosen — the empty path for the root. */
  onServe: (folder: string) => void;
  wid: string;
  /** The project's name: what Root is called beside its word. */
  rootLabel: string;
  /** The servers up on this checkout, so a folder already served says so. */
  servers: readonly ServedFolder[];
  /** The folder the cursor starts on, revealed. */
  initialFolder: string;
}) {
  const [state, dispatch] = useReducer(reduce, undefined, initialState);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState<string | null>(null);
  const [scrollTo, setScrollTo] = useState<{ id: string | null; nonce: number } | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const inflight = useRef(new Map<string, AbortController>());
  const index = usePathIndex(open ? "workstream" : null, open ? wid : null);

  // Opened afresh: the listing is read again — folders come and go with a
  // build — and the cursor stands on the folder served last, shown.
  useEffect(() => {
    if (!open) return;
    for (const ac of inflight.current.values()) ac.abort();
    inflight.current.clear();
    dispatch({ type: "reset" });
    for (const path of ancestorsOf(initialFolder)) dispatch({ type: "expand", path });
    setQuery("");
    setCursor(idOfFolder(initialFolder));
    setScrollTo((prev) => ({ id: idOfFolder(initialFolder), nonce: (prev?.nonce ?? 0) + 1 }));
  }, [open, wid, initialFolder]);

  const pending = open ? pendingLoads(state) : [];
  const pendingKey = loadKey(pending);
  useEffect(() => {
    for (const path of pending) {
      if (inflight.current.has(path)) continue;
      const ac = new AbortController();
      inflight.current.set(path, ac);
      dispatch({ type: "loading", path });
      // Depth 1, always: a folder is listed when somebody opens it (`FileTreeView`'s rule).
      api
        .tree("workstream", wid, path || undefined, 1, ac.signal)
        .then((tree) => dispatch({ type: "loaded", path, entries: tree.entries, truncated: tree.truncated, root: tree.root }))
        .catch((e: unknown) => {
          if (!ac.signal.aborted) dispatch({ type: "failed", path, error: failureReason("serve", "a folder could not be listed", e) }); // for the log
        })
        .finally(() => {
          if (inflight.current.get(path) === ac) inflight.current.delete(path);
        });
    }
    // `pending` is a fresh array every render; `pendingKey` is its text (`FileTree`'s rule).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingKey, wid, open]);

  const paths = index?.paths ?? EMPTY;
  const pages = useMemo(() => indexFolders(paths), [paths]);
  const known = useMemo(() => [...foldersOf(paths), ...listedFolders(state)], [paths, state]);
  const filtering = query.trim() !== "";
  const rows = useMemo(() => (filtering ? filterRows(query, known, rootLabel) : folderRows(state, rootLabel)), [filtering, query, known, state, rootLabel]);

  // A filter's rows are new rows: the cursor goes to the best of them.
  useEffect(() => {
    if (filtering) setCursor(rows[0]?.id ?? null);
    // On a typed change only: `rows` also changes as folders load, and
    // following it would pull the cursor off the row the person moved to.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const chosen = rows.find((r) => r.id === cursor && r.folder !== null) ?? null;
  const folder = chosen?.folder ?? null;
  const server = folder === null ? null : serverFor(servers, folder);
  const rootListing = state.dirs[ROOT];

  const submit = (row: FolderRow | null = chosen) => {
    if (!row || row.folder === null) return;
    onServe(row.folder);
  };
  const move = (step: 1 | -1) => {
    const next = stepCursor(rows, cursor, step);
    if (next === null) return;
    setCursor(next);
    setScrollTo((prev) => ({ id: next, nonce: (prev?.nonce ?? 0) + 1 }));
  };
  const onFieldKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      move(e.key === "ArrowDown" ? 1 : -1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      submit();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("shell-serve-folder-dialog-from-folder")}
      description={t("shell-serve-folder-dialog-choose-what-serve-port-machine-open")}
      initialFocus={field}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("shell-serve-folder-dialog-cancel")}</Button>
          <Button variant="primary" disabled={folder === null} onClick={() => submit()}>
            {submitWords(server)}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-2">
        <TextInput
          ref={field}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onFieldKey}
          placeholder={t("shell-serve-folder-dialog-find-folder-docs-dist-site-public")}
          aria-label={t("shell-serve-folder-dialog-find-folder-checkout")}
          spellCheck={false}
          autoComplete="off"
        />
        <TreeList<FolderRow>
          rows={rows}
          rowHeight={ROW_HEIGHT}
          cursor={cursor}
          onCursor={(id) => setCursor(id)}
          onAction={(action) => {
            if (action.kind === "expand") dispatch({ type: "expand", path: action.id });
            else if (action.kind === "collapse") dispatch({ type: "collapse", path: action.id });
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
          label={t("shell-serve-folder-dialog-folders-checkout")}
          busy={rootListing?.status === "loading"}
          scrollTo={scrollTo}
          guides={!filtering}
          className="h-72 rounded-control border border-border bg-surface-1"
          fill
          empty={<p className="px-3 py-6 text-center text-2xs text-text-dim">{filterProblem(query)}</p>}
          render={(row, rowState) => (
            <FolderLine
              row={row}
              indent={rowState.indent}
              cursor={rowState.cursor}
              hasPage={row.folder !== null && pages.has(row.folder)}
              port={row.folder === null ? null : (serverFor(servers, row.folder)?.port ?? null)}
              onToggle={() => dispatch({ type: "toggle", path: row.id })}
              onPick={() => setCursor(row.id)}
              onServe={() => submit(row)}
            />
          )}
        />
        <p className="min-h-8 text-2xs leading-relaxed text-text-dim" aria-live="polite">
          {folder === null ? (rootListing?.status === "error" ? t("shell-serve-folder-dialog-checkout-could-listed-root-can-still", { error: rootListing.error }) : t("shell-serve-folder-dialog-choose-root-folder-under")) : choiceWords(folder, server)}
        </p>
      </div>
    </Dialog>
  );
}

const EMPTY: readonly string[] = [];

/** One row: the chevron, the glyph, the name, and what helps a person choose. */
function FolderLine({
  row,
  indent,
  cursor,
  hasPage,
  port,
  onToggle,
  onPick,
  onServe,
}: {
  row: FolderRow;
  /** The row's left inset for its depth — a margin, so the wash hugs the row (`TreeRowState.indent`). */
  indent: number;
  cursor: boolean;
  hasPage: boolean;
  port: number | null;
  onToggle: () => void;
  onPick: () => void;
  onServe: () => void;
}) {
  if (row.kind === "loading" || row.kind === "error") {
    return (
      <div className={cn("flex h-full items-center text-2xs", row.kind === "error" ? "text-danger" : "text-text-dim")} style={{ marginLeft: indent + 18 }}>
        {row.kind === "error" ? t("shell-serve-folder-dialog-could-not-be-listed", { error: row.error ?? t("shell-serve-folder-dialog-no-reason-given") }) : t("shell-serve-folder-dialog-listing")}
      </div>
    );
  }
  const root = row.kind === "root";
  const Glyph = root ? ICON.project : row.expanded ? ICON.folderOpen : ICON.folder;
  const Chevron = row.expanded ? ICON.expanded : ICON.collapsed;
  return (
    <div
      onClick={onPick}
      onDoubleClick={onServe}
      className={cn("anim flex h-full cursor-default items-center gap-1.5 rounded-control px-1 text-xs", cursor ? "bg-selected text-text" : "hover:bg-surface-2", row.ignored && !cursor && "text-text-dim")}
      style={{ marginLeft: indent }}
    >
      {row.expandable ? (
        <button
          type="button"
          tabIndex={-1}
          aria-label={row.expanded ? t("shell-serve-folder-dialog-close", { row: row.label }) : t("shell-serve-folder-dialog-open", { row: row.label })}
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
          className="anim flex size-4 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-selected hover:text-text"
        >
          <Chevron size={12} aria-hidden />
        </button>
      ) : (
        <span className="size-4 shrink-0" aria-hidden />
      )}
      <Glyph size={13} className={cn("shrink-0", root ? "text-text" : "text-text-dim")} aria-hidden />
      <span className={cn("min-w-0 truncate", (root || row.kind === "typed") && "font-medium")}>{root ? t("shell-serve-folder-dialog-root") : row.kind === "typed" ? t("shell-serve-folder-dialog-serve-as-typed", { label: row.label }) : row.label}</span>
      {root && <span className="min-w-0 truncate text-2xs text-text-dim">{t("shell-serve-folder-dialog-whole-checkout", { row: row.label })}</span>}
      <span className="flex-1" />
      {hasPage && <Chip tone="quiet">index.html</Chip>}{/* for the machine */}
      {port !== null && <Chip tone="neutral">{t("shell-serve-folder-dialog-serving-port", { port })}</Chip>}
    </div>
  );
}
