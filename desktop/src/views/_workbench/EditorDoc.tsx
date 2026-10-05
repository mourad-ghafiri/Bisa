/**
 * A file, open for editing (ide/03).
 *
 * Loads through `GET /ide/file` (the editor's cap, and the hash a save must
 * carry), mounts the one `CodeEditor`, and saves by compare-and-swap: a `409`
 * becomes a conflict banner with the current text one click away in a diff,
 * and the person's buffer is untouched until they choose. Autosave is
 * delay-then-CAS from the settings; `⌘S` saves now. A `file_changed` frame
 * for this path reloads a clean buffer silently and raises the same banner on
 * a dirty one. The modes a file offers are `fileDocModel`'s (ide/03
 * §Rendered documents): Markdown, Mermaid and a page have *Rendered ·
 * Split · Source* — glyphs alone on the control, the eye, the two columns,
 * the brackets, the word as the tooltip — a csv and an svg *Rendered ·
 * Source*; the rendered view is of the buffer as typed — the grid, the
 * figure, the sandboxed page — and the mode is remembered per document,
 * across a restart, beside its place (`useDocMode`). A page in a workstream can be
 * **annotated** for an agent from its rendered or split view (`PageAnnotator`,
 * the wand on the bar); everything else is the editor. A file that turns
 * out not to be text says so and reveals.
 *
 * **One backend, three sources.** The bytes come and go through
 * `docBackend.backendFor(source)`: a file under the root is the node's, a
 * loose file from anywhere on this machine is the shell's (ide/03 §Loose
 * files), an untitled document has none. The backend's two flags gate what
 * is a root file's alone — the watcher, the language server, blame, Files —
 * and what any file on disk has: a read on mount, ⌘S in place, autosave, a
 * conflict merged the same way. A document that is not the root's can move:
 * *Save to…* puts it into this project or elsewhere on this machine.
 */

import { errorFields, log } from "../../log";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { WorkbenchScope } from "../../routeModel.mjs";
import type { BlameLine } from "../../types";
import { gutterFor } from "../_work/blameModel.mjs";
import { lspRestarted, useLspDocument } from "./lspClient";
import { applyTextEdits, sameServer, serverChange, serverChip } from "./lspModel.mjs";
import type { LspServerStatus } from "../../types";
import {
  Button,
  Chip,
  CodeEditor,
  DiffEditor,
  EmptyState,
  KindView,
  Markdown,
  MermaidView,
  SegmentedControl,
  Spinner,
  Tooltip,
  failureText,
  absolute,
  cn,
  ICON,
  revealLabel,
  endsSelection,
  useKeptScroll,
  useToast,
  useWatchLease,
  ErrorNote,
  keyLabel,
} from "../../ui";
import { useSessionDraft } from "../_work/gitPanelStore";
import { PageAnnotator } from "./PageAnnotator";
import { useCoalesced } from "./useCoalesced";
import { useAgentEditSettled } from "./agentEditsStore";
import { useRememberedPick } from "../_studio/conversationPickStore";
import { settledTone, settledWords } from "./agentEditsModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { rootKey, tabId } from "./workbenchModel.mjs";
import { useAsync } from "../_work/useAsync";
import { useArtifactLibraries } from "../../shell/artifactSettings";
import { basenameOf, breadcrumbsOf, autosaveDue, autosaveFrom, changedOnDisk, conflicted, edited, formatOnSave, isDirty, isLoose, keepMine, loadFailed, loadFailure, loaded, refusalWords, reformatted, remounted, saveFailed, saveFailure, savePathProblem, saveStarted, saved, sizeNote, sourceName, takeTheirs, untitledBuffer } from "./editorModel.mjs";
import { createLatest } from "../../shell/latestModel.mjs";
import type { Buffer, DocSource } from "./editorModel.mjs";
import { backendFor } from "./docBackend";
import { looseFile, pickSavePath, revealPath, writeLooseFile } from "../../api";
import type { WorkbenchTab } from "./workbenchModel.mjs";
import { LinkRoots, copyText, scrollToFragment } from "../../ui";
import { fragmentOf, resolveDocLink } from "./docLink.mjs";
import { takeKeyboard } from "./docFocus";
import { artifactKindOf, binaryWords, defaultMode, docKindOf, docModes, modeGlyph, modeLabel } from "./fileDocModel.mjs";
import type { DocMode } from "./fileDocModel.mjs";
import type { LensLayout } from "./reviewLensModel.mjs";
import { mimeOfName } from "../../ui/artifact/artifactModel.mjs";
import type { EditorSelection } from "../../ui";
import { bufferOf, useDocBuffer } from "./docBuffersStore";
import { editorViewOf, keepEditorView, keepPageScroll, keepScroll, pageScrollOf, scrollOf, useDocMode } from "./docViewStore";
import { registerEditor, releaseActiveEditor, setActiveEditor, takeLine } from "./editorRegistry";
import { DOC_FIND, REVIEW_COMMAND, contexts, currentKeymap } from "../../shell/shortcuts";
import { chordFor } from "../../shell/keymapModel.mjs";
import { commandForEvent } from "../../shell/keymapModel.mjs";
import { isMac } from "../../ui/KeyHint";
import { wandWords } from "../../shell/browserChromeModel.mjs";
import { FindBar, Menu, PromptDialog, emptyFind, replaceAll, replaceOne, stepIndex, useDomFind } from "../../ui";
import type { Find } from "../../ui";
import { forgetEditorStatus, publishEditorStatus, setStatusEditor } from "./editorStatusStore";
import { SelectionAgentBar } from "./SelectionAgentBar";
import { usePendingReviewPaths } from "./reviewStore";
import {
  DEFAULT_LENS_LAYOUT,
  UNDO_DIRTY_HINT,
  fileActionWords,
  fileSettleTarget,
  lensLayoutKey,
  lensLayoutWords,
  pendingReviewWords,
  toggleLensLayout,
  hasNextHunk,
  hasPreviousHunk,
  hunkActionWords,
  hunkPositionWords,
  hunkSettleTarget,
  lensApplies,
  lensStorageKey,
  nextHunkIndex,
  previousHunkIndex,
  undoEnabled,
} from "./reviewLensModel.mjs";
import type { DiffAnnotation, DiffAnnotationAction } from "../../ui";
import type { FileReviewView, Hunk, SettleTarget } from "../../types";
import { t as tr } from "../../i18n/l10n.mjs";

export function EditorDoc({
  scope,
  id,
  source,
  project,
  registryKey,
  onOpenFile,
  onRevealInFiles,
  onReveal,
  onNamed,
  className,
}: {
  scope: WorkbenchScope;
  id: string;
  /** A file at a path, or an untitled document (⌘N) with no file behind it until it is saved. */
  source: DocSource;
  /** The project whose settings layer applies, when there is one. */
  project: string | null;
  /** How the workbench addresses this editor for the dirty dot and the close guard. */
  registryKey: string;
  onOpenFile?: (path: string) => void;
  /** A breadcrumb was clicked: show that folder or file in Files. */
  onRevealInFiles?: (path: string) => void;
  /** Show the file in the OS file manager — the way out for a file that is not text. */
  onReveal?: (path: string) => void;
  /** The document was saved somewhere new — into the root, or elsewhere on this machine: the workbench makes its tab that one. */
  onNamed?: (tab: WorkbenchTab) => void;
  className?: string;
}) {
  const toast = useToast();
  // The backend is keyed on what the source *is* — its tab id — never on the
  // object's identity: a parent that hands a fresh `{kind, path}` per render
  // must not cost a re-read, a new `save`, or a re-registration (ide/03).
  const sourceId = tabId(source);
  // Keyed on `sourceId`, never the `source` object, for the reason above.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const backend = useMemo(() => backendFor(source, scope, id), [sourceId, scope, id]);
  /** Under the root: the node's file, watched and followed by a language server. */
  const isFile = backend.inRoot;
  /** On this machine at all — under the root or loose — read on mount and saved in place. */
  const onDisk = backend.onDisk;
  const path = sourceName(source);
  // The buffer is the tab's, not this component's (`docBuffersStore.ts`): a
  // pane draws one tab at a time, and a document that left the screen keeps
  // its unsaved text — and the fact that it has some — until its tab closes.
  const [buffer, setBuffer] = useDocBuffer(registryKey, () => (onDisk ? remounted(bufferOf(registryKey), path) : untitledBuffer(source)));
  /**
   * The name an untitled document is being asked for. `resolve` hands the
   * save path what was typed, or `null` for Cancel; `problem` is the node's
   * refusal of the last answer, shown under the field until it changes.
   */
  const [naming, setNaming] = useState<{ initial: string; problem: string | null; busy: boolean; resolve: (value: string | null) => void } | null>(null);
  const [reviewing, setReviewing] = useState(false);
  // What this file is and the views it offers (ide/03 §Rendered documents).
  const docKind = docKindOf(path);
  const modes = docModes(docKind);
  const previewable = modes.length > 0;
  // The mode is the document's, kept beside its place: a tab switch keeps it, and a restart.
  const [mode, setMode] = useDocMode<DocMode>(registryKey, modes, defaultMode(docKind));
  const libraries = useArtifactLibraries();
  // A page in a workstream can be annotated for an agent from its rendered view (ide/03 §Annotate).
  const annotatable = docKind === "html" && scope === "workstream";
  const [annotating, setAnnotating] = useState(false);
  const [annotations, setAnnotations] = useState(0);
  // Find over the rendering (ide/03 §Rendered documents): the bar's query,
  // the match it stands on, and — from whichever renderer holds the text —
  // how many there are. Replacing lands in the buffer through `edited`, so
  // the rendering follows and the tab dirties as for any edit.
  const [find, setFind] = useState<Find | null>(null);
  const [replacing, setReplacing] = useState(false);
  const [findIndex, setFindIndex] = useState(-1);
  const [pageFound, setPageFound] = useState<{ index: number; count: number } | null>(null);
  const [sheetFound, setSheetFound] = useState<number | null>(null);
  const docRoot = useRef<HTMLDivElement>(null);
  // Every rendering's scrollport keeps its place across the unmount a tab
  // switch is (`ui/useKeptScroll`, `docViewStore.ts`). The identity carries
  // the mode — another mode is other scrollports — and whether the document
  // is drawn at all: while it loads there is no root to listen on.
  const drawn = buffer.status !== "loading" && buffer.status !== "error";
  useKeptScroll(
    docRoot,
    { read: (name) => scrollOf(registryKey, name), write: (name, place) => keepScroll(registryKey, name, place) },
    `${registryKey}:${mode}:${drawn ? "drawn" : "waiting"}`,
  );
  const trigger = useRef<((action: string) => void) | null>(null);
  useEffect(() => {
    if (mode === "source") setAnnotating(false);
  }, [mode]);
  const revealWord = useMemo(() => revealLabel(navigator.userAgent), []);
  const bufferRef = useRef(buffer);
  bufferRef.current = buffer;
  const reveal = useRef<((line: number) => void) | null>(null);
  const selection = useRef<(() => { start: number; end: number; text: string } | null) | null>(null);
  // The live selection with its pixel anchor, for the in-editor agent toolbar.
  // Only a workstream file can hand a selection to an agent (it is
  // where the agent runs); elsewhere the toolbar stays hidden.
  const [sel, setSel] = useState<EditorSelection | null>(null);
  /**
   * A selection whose toolbar was dismissed. The editor re-emits the selection
   * on every scroll, so without this the bar came back the moment the view
   * moved; cleared when the selection goes, so re-selecting the same lines
   * offers it again.
   */
  const dismissed = useRef<string | null>(null);
  /** The editor's own box, so the toolbar is placed inside it and not the pane's. */
  const editorBox = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ width: 0, height: 0 });
  // `null` shows the toolbar's Ask/Edit buttons; a keyboard command
  // (`ask_agent`/`edit_agent`) opens the input straight into a mode.
  const [agentMode, setAgentMode] = useState<null | "ask" | "edit">(null);
  const selRange = sel ? `${sel.start}-${sel.end}` : null;
  // The editor's rectangle, followed so a resized pane still places the bar
  // inside it.
  useEffect(() => {
    const el = editorBox.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const read = () => setBox({ width: el.clientWidth, height: el.clientHeight });
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, [mode]);
  useEffect(() => {
    // A fresh selection returns the toolbar to its buttons.
    setAgentMode(null);
  }, [selRange]);
  useEffect(() => {
    if (scope !== "workstream") return;
    const open = (e: Event) => {
      const wanted = (e as CustomEvent<{ mode: "ask" | "edit" }>).detail?.mode;
      // Only the editor with a live selection answers the chord.
      if (wanted && selection.current?.()) setAgentMode(wanted);
    };
    window.addEventListener("bisa:ide-agent", open);
    return () => window.removeEventListener("bisa:ide-agent", open);
  }, [scope]);
  // Blame is a checkout question: a goal's or an agent's folder has no
  // repository of its own to ask — and a document not yet on disk has no lines to ask about.
  const canBlame = scope === "workstream" && isFile;
  const [blameOn, setBlameOn] = useState(false);
  const [blame, setBlame] = useState<BlameLine[] | null>(null);
  const [blameNote, setBlameNote] = useState<string | null>(null);
  // Language intelligence (ide/10): which server, if any, follows this document.
  const [lspLanguage, setLspLanguage] = useState<string | null>(null);
  const lspStatus = useAsync(
    (s) => (lspLanguage ? api.lspStatus(scope, id, s) : Promise.resolve(null)),
    [scope, id, lspLanguage],
  );
  const server: LspServerStatus | null = lspStatus.data?.servers.find((r) => r.language === lspLanguage) ?? null;
  const chip = serverChip(lspLanguage, server);
  useLspDocument(scope, id, isFile ? path : null, buffer.text, setLspLanguage);
  // The chip says the server's state as it is now: the row is read again
  // when the server of this root and language starts, fails or stops.
  const reloadLspStatus = lspStatus.reload;
  useEngineEvents((e) => {
    if (sameServer(serverChange(e.payload), { scope, id, language: lspLanguage })) reloadLspStatus();
  });

  const settings = useAsync((s) => api.settingsResolved(project, s), [project]);
  const autosave = useMemo(() => autosaveFrom(settings.data?.settings), [settings.data]);

  // The reads of this document, in order (`latestModel`): the newest asked
  // for is the one that lands — a slow read of the file as it was never
  // replaces a later one — and none lands once the tab is gone, where it
  // would leave a buffer nobody forgets. One tracker per tab: `registryKey`
  // is its key, not something the memo reads.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const reads = useMemo(() => createLatest(), [registryKey]);
  useEffect(() => {
    reads.open();
    return () => reads.close();
  }, [reads]);
  const load = useCallback(async () => {
    // Nothing on disk to read: an untitled document is its buffer alone.
    if (!onDisk) return;
    const ticket = reads.begin();
    try {
      const file = await backend.read();
      if (!reads.lands(ticket)) return;
      setBuffer((b) => (b.status === "loading" ? loaded(b, file) : changedOnDisk(b, file)));
    } catch (e) {
      if (!reads.lands(ticket)) return;
      // What the refusal means — too large by the limit the node said, gone, failed — is the model's.
      const failure = loadFailure(e);
      log.info("editor", "a document could not be read", { path, kind: failure.kind, ...errorFields(e) });
      setBuffer((b) => loadFailed(b, failure));
    }
  }, [backend, onDisk, setBuffer, reads, path]);

  useEffect(() => {
    if (!onDisk) return;
    // Unsaved work a tab kept while off screen stays as typed; the read then
    // compares it with the disk (`changedOnDisk`) instead of replacing it.
    setBuffer((kept) => remounted(kept, path));
    void load();
  }, [path, load, onDisk, setBuffer]);

  // The watcher: hold the lease while this document is open.
  useWatchLease(scope, id);

  // A rename is the workbench's: it re-points the tab, and this document
  // remounts at the new path. Reloading the old path here would only 404.
  // A burst — a save's temp file and rename, an agent rewriting the file
  // several times — is one read.
  const reloadSoon = useCoalesced(() => void load());
  useEngineEvents((e) => {
    const p = e.payload;
    // The watcher is a root's; a loose file learns of a change at its next save.
    if (!isFile || p.type !== "file_changed" || p.scope !== scope || p.id !== id) return;
    if (p.kind === "renamed") return;
    if (p.kind === "rescan" || p.path === path) reloadSoon();
  });

  // The agent this document was handed to came to rest: read the file again
  // — watcher frame or not — and say how it ended (ide/03 §Annotate). The
  // hand-off went into the conversation this checkout is on, and remembered
  // it, so that is the one whose turn is followed.
  const ws = useWorkspace();
  const [conversation] = useRememberedPick(rootKey(scope, id));
  useAgentEditSettled("conversation", conversation ?? "", path, ({ agentId, state }) => {
    void load();
    const agentName = ws.agents.find((a) => a.id === agentId)?.name ?? agentId;
    toast[settledTone(state)](settledWords(agentName, path, state));
  });

  // The review lens (ide/03, ide/09): whether this root file has a pending
  // review in the conversation the checkout is on. A workstream's own file
  // only — a goal's or an agent's folder has no ledger, like blame above.
  const reviewApplies = scope === "workstream" && isFile;
  const review = useAsync(
    (s) => (reviewApplies && conversation ? api.conversationChangeFile(conversation, path, s) : Promise.resolve({ file: null })),
    [reviewApplies, conversation, path],
  );
  const reloadReviewSoon = useCoalesced(() => void review.reload());
  useEngineEvents((e) => {
    const p = e.payload;
    if ((p.type === "changes_moved" || p.type === "changes_settled") && conversation && p.conversation === conversation) reloadReviewSoon();
  });
  const fileReview: FileReviewView | null = review.data?.file ?? null;
  // Remembered per document in the session, like the doc mode — defaults to
  // on whenever there is something to review, until the person hides it.
  const [lensOn, setLensOn] = useSessionDraft<boolean>(lensStorageKey(rootKey(scope, id), path), !!fileReview);
  // Inline by default — removed and added lines in one column, the unchanged
  // regions folded — so a long file reads as its changes; side by side on ask.
  const [lensLayout, setLensLayout] = useSessionDraft<LensLayout>(lensLayoutKey(rootKey(scope, id), path), DEFAULT_LENS_LAYOUT);
  const [hunkIndex, setHunkIndex] = useState(0);
  useEffect(() => {
    setHunkIndex((i) => Math.min(i, Math.max(0, (fileReview?.hunks.length ?? 1) - 1)));
  }, [fileReview]);
  const showLens = reviewApplies && mode === "source" && lensOn && lensApplies(fileReview) && buffer.status !== "conflict";
  const pendingReviewPaths = usePendingReviewPaths(scope === "workstream" ? id : "");

  const settleReview = async (verdict: "keep" | "undo", target: SettleTarget) => {
    if (!conversation) return;
    try {
      await api.settleChanges(conversation, { verdict, target });
      review.reload();
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        toast.info(tr("workbench-editor-doc-file-changed-since-refetching-review"));
        review.reload();
      } else {
        toast.error(failureText("workbench", "editor-doc-failed", e));
      }
    }
  };
  const keepHunk = (h: Hunk) => fileReview && void settleReview("keep", hunkSettleTarget(path, h, fileReview.disk_hash));
  const undoHunk = (h: Hunk) => fileReview && void settleReview("undo", hunkSettleTarget(path, h, fileReview.disk_hash));
  const keepFile = () => void settleReview("keep", fileSettleTarget(path));
  const undoFile = () => void settleReview("undo", fileSettleTarget(path));
  const otherPendingFiles = [...pendingReviewPaths].filter((p) => p !== path).length;
  const nextFileToReview = () => {
    const next = [...pendingReviewPaths].find((p) => p !== path);
    if (next) onOpenFile?.(next);
    else toast.info(tr("workbench-editor-doc-nothing-else-review"));
  };
  /** A rendered or split view of a file whose diff waits in Source: said, with the way there. */
  const reviewWaitsInSource = reviewApplies && !!fileReview && !fileReview.opaque && mode !== "source" && buffer.status !== "conflict";

  // The bar's own chords (ide/15): the focused document's review lens answers.
  useEffect(() => {
    if (!showLens) return;
    const onCommand = (e: Event) => {
      const root = docRoot.current;
      if (!root || !root.contains(document.activeElement)) return;
      const cmd = (e as CustomEvent<{ command: string }>).detail?.command;
      const hunks = fileReview?.hunks ?? [];
      switch (cmd) {
        case "review_next_change":
          setHunkIndex((i) => nextHunkIndex(hunks, i));
          return;
        case "review_previous_change":
          setHunkIndex((i) => previousHunkIndex(hunks, i));
          return;
        case "review_keep_change": {
          const h = hunks[hunkIndex];
          if (h) keepHunk(h);
          else keepFile();
          return;
        }
        case "review_undo_change": {
          const h = hunks[hunkIndex];
          if (h && undoEnabled(isDirty(buffer))) undoHunk(h);
          else if (!h) undoFile();
          return;
        }
        default:
          return;
      }
    };
    window.addEventListener(REVIEW_COMMAND, onCommand);
    return () => window.removeEventListener(REVIEW_COMMAND, onCommand);
    // The four verbs (`keepHunk`, `keepFile`, `undoHunk`, `undoFile`) are plain
    // functions remade every render; the listener is re-hung on the facts they read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showLens, fileReview, hunkIndex, buffer]);

  const lensAnnotations: DiffAnnotation[] = useMemo(() => {
    if (!showLens || !fileReview || fileReview.opaque) return [];
    return fileReview.hunks.map((h, i) => {
      const dirty = isDirty(buffer);
      const actions: DiffAnnotationAction[] = [
        { label: hunkActionWords().keep, tone: "accent", onClick: () => keepHunk(h) },
        { label: hunkActionWords().undo, disabled: !undoEnabled(dirty), hint: !undoEnabled(dirty) ? UNDO_DIRTY_HINT : undefined, onClick: () => undoHunk(h) },
      ];
      return { line: h.disk.start + Math.max(1, h.disk.len), text: tr("workbench-editor-doc-change", { i: i + 1, hunks: fileReview.hunks.length }), actions };
    });
    // `keepHunk` and `undoHunk` are plain functions remade every render; the
    // cards are rebuilt on the facts they read.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showLens, fileReview, buffer]);

  // Blame follows the saved file: every save (a new hash) refreshes it, and
  // a dirty buffer keeps the last answer — line numbers may drift while you
  // type, which is the price of not re-blaming on every keystroke.
  const savedHash = buffer.status === "clean" || buffer.status === "read_only" ? buffer.savedHash : undefined;
  useEffect(() => {
    if (!blameOn || !canBlame) {
      setBlame(null);
      setBlameNote(null);
      return;
    }
    if (savedHash === undefined) return;
    let alive = true;
    api
      .gitBlame(id, path)
      .then((r) => {
        if (!alive) return;
        setBlame(r.lines);
        setBlameNote(null);
      })
      .catch((e) => {
        if (!alive) return;
        setBlame(null);
        setBlameNote(failureText("workbench", "editor-doc-failed", e));
      });
    return () => {
      alive = false;
    };
  }, [blameOn, canBlame, id, path, savedHash]);
  const gutter = useMemo(() => (blame ? gutterFor(blame, absolute) : null), [blame]);

  /**
   * Ask for the path an untitled document is saved under. Resolves with the
   * answer, or `null` for Cancel; `problem` is the node's word on the last
   * answer, so a taken name is refused under the field rather than in a toast.
   */
  const askPath = useCallback(
    (initial: string, problem: string | null) =>
      new Promise<string | null>((resolve) => setNaming({ initial, problem, busy: false, resolve })),
    [],
  );

  /**
   * Save into this root: ask for a path relative to it, write with no hash —
   * a create the node refuses when the name is taken — and hand the file's
   * tab to the workbench, which makes this tab that one. Asks again while
   * the node refuses; `false` when the person cancels, so a close guard
   * keeps the tab. The door an untitled document's ⌘S takes.
   */
  const saveInto = useCallback(async (): Promise<boolean> => {
    if (bufferRef.current.status === "saving") return false;
    let initial = "";
    let problem: string | null = null;
    for (;;) {
      const target = await askPath(initial, problem);
      if (target === null) {
        setNaming(null);
        return false;
      }
      setBuffer((cur) => saveStarted(cur));
      try {
        await api.ideWriteFile(scope, id, target, bufferRef.current.text);
        setNaming(null);
        toast.ok(tr("workbench-editor-doc-saved-2", { target }));
        onNamed?.({ kind: "file", path: target });
        return true;
      } catch (e) {
        const msg = failureText("workbench", "editor-doc-failed", e);
        setBuffer((cur) => saveFailed(cur, msg));
        initial = target;
        problem = msg;
      }
    }
  }, [askPath, scope, id, toast, onNamed, setBuffer]);

  /**
   * Save elsewhere on this machine: the OS's save dialog, then the shell
   * writes (ide/03 §Loose files). A path the dialog offers that already holds
   * a file was confirmed there, so its hash is read first and the write
   * carries it — never a clobber, and a binary or oversized file is refused
   * by name. The tab becomes the loose file's.
   */
  const saveElsewhere = useCallback(async (): Promise<boolean> => {
    if (bufferRef.current.status === "saving") return false;
    const suggested = isLoose(source) ? source.path : `${path}.txt`;
    const chosen = await pickSavePath(suggested, tr("workbench-editor-doc-save-4", { basenameOf: basenameOf(path) }));
    if (!chosen) return false;
    setBuffer((cur) => saveStarted(cur));
    try {
      let baseHash: string | null = null;
      try {
        const existing = await looseFile(chosen);
        if (existing.binary || !existing.hash) throw new Error(tr("workbench-editor-doc-not-text-file-replace", { chosen }));
        baseHash = existing.hash;
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404)) throw e;
      }
      const written = await writeLooseFile(chosen, bufferRef.current.text, baseHash);
      toast.ok(tr("workbench-editor-doc-saved-3", { written: written.path }));
      onNamed?.({ kind: "loose", path: written.path });
      return true;
    } catch (e) {
      const msg = failureText("workbench", "editor-doc-failed", e);
      setBuffer((cur) => saveFailed(cur, msg));
      toast.error(tr("workbench-editor-doc-could-not-save", { chosen, msg }));
      return false;
    }
  }, [source, path, toast, onNamed, setBuffer]);

  const save = useCallback(async (): Promise<boolean> => {
    if (!onDisk) return saveInto();
    const b = bufferRef.current;
    if (!isDirty(b) || b.readOnly || b.status === "saving") return !isDirty(b);
    let text = b.text;
    setBuffer(saveStarted(b));
    // Format on save (`editor.format_on_save`) through the language server,
    // when one follows this document. A formatter that fails leaves the text
    // as typed — a save must never depend on a server answering.
    const options = formatOnSave(settings.data?.settings, lspLanguage);
    if (options) {
      try {
        const r = await api.lspRequest(scope, id, path, "textDocument/formatting", { textDocument: { uri: path }, options });
        if (Array.isArray(r.result) && r.result.length > 0) {
          const formatted = applyTextEdits(text, r.result as { range: unknown; newText: string }[]);
          // The editor shows what the save writes, unless the person typed meanwhile (`reformatted`).
          setBuffer((cur) => reformatted(cur, b.text, formatted));
          text = formatted;
        }
      } catch (e) {
        // No formatter, or it said no: the text saves as typed — said, so a broken formatter is not silent on every save.
        log.debug("editor", "the formatter did not run; the text saves as typed", errorFields(e));
      }
    }
    try {
      const res = await backend.write(text, b.savedHash);
      setBuffer((cur) => saved(cur, text, res.hash));
      return true;
    } catch (e) {
      // What a save that threw means is the model's (`saveFailure`): the file moved on disk, or it failed.
      const why = saveFailure(e);
      if (why.kind === "conflict") {
        log.info("editor", "a save met a file that changed on disk", { path });
        setBuffer((cur) => conflicted(cur, why.text, why.hash));
        return false;
      }
      log.warn("editor", "a document could not be saved", { path, ...errorFields(e) });
      setBuffer((cur) => saveFailed(cur, why.message));
      toast.error(tr("workbench-editor-doc-could-not-save-2", { path, message: why.message }));
      return false;
    }
  }, [scope, id, path, toast, settings.data, lspLanguage, onDisk, backend, saveInto, setBuffer]);

  // One registration per key, with a handle that reads the latest `save` and
  // path through refs — the way the Files tree registers its explorer. The
  // registry notifies every subscriber on a registration, the workbench among
  // them, so a registration that followed a closure's identity was a loop.
  const saveRef = useRef(save);
  saveRef.current = save;
  const pathRef = useRef(isFile ? path : undefined);
  pathRef.current = isFile ? path : undefined;
  useEffect(
    () =>
      registerEditor(registryKey, {
        save: () => saveRef.current(),
        get path() {
          return pathRef.current;
        },
        revealLine: (line) => {
          if (!reveal.current) return false;
          reveal.current(line);
          return true;
        },
        selection: () => selection.current?.() ?? null,
        trigger: (action) => trigger.current?.(action),
      }),
    [registryKey],
  );
  // A mount claims the active slot; a focus reclaims it (the split beside
  // this one may have taken it); an unmount releases only its own claim, so
  // closing one pane never blanks the other's `:line`, attach or save.
  useEffect(() => {
    setActiveEditor(registryKey);
    setStatusEditor(registryKey);
    return () => {
      releaseActiveEditor(registryKey);
      forgetEditorStatus(registryKey);
    };
  }, [registryKey]);

  // Autosave: delay-then-CAS, from the first unsaved keystroke. The 250 ms
  // due-check is armed only while the buffer is dirty and torn down the moment
  // it saves clean, so a file left open but untouched runs no timer at all
  // — the autosave itself is never gated on window visibility.
  // Never for an untitled document: its save asks for a name, and a dialog
  // that opens itself mid-sentence is worse than an unsaved buffer.
  const dirty = isDirty(buffer);
  useEffect(() => {
    if (autosave.mode !== "after_delay" || !dirty || !onDisk) return;
    const t = window.setInterval(() => {
      if (autosaveDue(bufferRef.current, autosave.mode, autosave.delay, Date.now())) void save();
    }, 250);
    return () => window.clearInterval(t);
  }, [autosave, save, dirty, onDisk]);

  // on_focus_change saves when the window blurs — every dirty editor, since
  // every one of them is being left. ⌘S is the focused editor's alone: it is
  // caught on this document's own element, below, not on the window.
  useEffect(() => {
    if (!onDisk) return;
    const onBlur = () => {
      if (autosave.mode === "on_focus_change") void save();
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [save, autosave.mode, onDisk]);

  const onChange = useCallback((text: string) => setBuffer((b) => edited(b, text, Date.now())), [setBuffer]);

  // A relative image in a rendered Markdown file — `![](./diagram.png)` —
  // is kept by the sanitizer as `data-doc-src` (ide/17). Each is read
  // through the node's byte route into a blob URL, resolved against this
  // document like a link; one that climbs out of the root stays blank.
  const renderedBox = useRef<HTMLDivElement>(null);
  const renderedText = mode !== "source" && docKind === "markdown" ? buffer.text : null;
  // The rendering's own find: markdown and a diagram are DOM the app can
  // walk; a sheet searches its rows; a page searches inside its frame; an
  // svg is a picture — its bar points at the source.
  const domFindable = mode !== "source" && (docKind === "markdown" || docKind === "diagram");
  // The walk is redone when the rendering is: another text — a replace that
  // kept the length included — or another mode, since Rendered and Split
  // draw the preview in different boxes and the old text nodes are gone.
  const domFound = useDomFind(renderedBox, domFindable ? find : null, findIndex, domFindable ? `${mode}\u0000${buffer.text}` : "");
  const findCount = docKind === "html" ? (pageFound?.count ?? null) : docKind === "sheet_text" ? sheetFound : docKind === "svg" ? null : domFound.count;
  const findShown = find !== null && mode !== "source";
  useEffect(() => {
    setFindIndex((i) => (findCount && findCount > 0 ? (i >= 0 && i < findCount ? i : 0) : -1));
  }, [findCount, find?.query, find?.regex, find?.caseSensitive]);
  // The app's chord reaches the document that holds the focus: in the source
  // the editor's widget, in a rendering the bar.
  useEffect(() => {
    const onFind = (e: Event) => {
      const root = docRoot.current;
      if (!root || !root.contains(document.activeElement)) return;
      const wantReplace = !!(e as CustomEvent<{ replace: boolean }>).detail?.replace;
      if (mode === "source") {
        trigger.current?.(wantReplace ? "editor.action.startFindReplaceAction" : "actions.find");
        return;
      }
      if (docKind === "svg") {
        // A picture has no text to find in: the source is where the query lands.
        setMode("source");
        window.setTimeout(() => trigger.current?.(wantReplace ? "editor.action.startFindReplaceAction" : "actions.find"), 50);
        return;
      }
      setReplacing(wantReplace && !buffer.readOnly);
      setFind((f) => f ?? emptyFind());
    };
    window.addEventListener(DOC_FIND, onFind);
    return () => window.removeEventListener(DOC_FIND, onFind);
  }, [mode, docKind, buffer.readOnly, setMode]);
  /** The focusable box the rendering is drawn in — the Rendered wrapper, or the Split pane's half. */
  const renderedShown = () => docRoot.current?.querySelector<HTMLElement>("[data-rendered-doc][tabindex]") ?? null;
  const closeFind = () => {
    setFind(null);
    setReplacing(false);
    setFindIndex(-1);
    setPageFound(null);
    setSheetFound(null);
    // The bar hands the keyboard back to the rendering it searched, so the
    // next find chord — and Space, and the arrows — still reach the document.
    if (mode !== "source") takeKeyboard(renderedShown());
  };
  // A rendering takes the keyboard when it is shown — the document opened on
  // it, or switched to it — never on a re-render, and never from a field, a
  // shell or the Files tree (`docFocus.takeKeyboard`); so the find chord
  // finds in it, and Space and the arrows scroll it.
  const modeShown = useRef<DocMode | null>(null);
  useEffect(() => {
    const before = modeShown.current;
    modeShown.current = mode;
    if (mode === "source" || before === mode) return;
    takeKeyboard(renderedShown());
  }, [mode]);
  const replaceInBuffer = (all: boolean) => {
    if (!find || buffer.readOnly) return;
    const next = all ? replaceAll(buffer.text, find).text : replaceOne(buffer.text, find, findIndex).text;
    if (next !== buffer.text) setBuffer((b) => edited(b, next, Date.now()));
  };
  const findBar = findShown && find ? (
    <FindBar
      find={find}
      onChange={setFind}
      index={findIndex}
      count={findCount}
      onStep={(dir) => setFindIndex((i) => stepIndex(i, findCount ?? 0, dir))}
      onClose={closeFind}
      replace={replacing ? { onReplace: () => replaceInBuffer(false), onReplaceAll: () => replaceInBuffer(true), disabled: buffer.readOnly } : undefined}
      label={tr("workbench-editor-doc-find-rendering")}
    />
  ) : null;
  useEffect(() => {
    const box = renderedBox.current;
    if (!box || renderedText === null) return;
    const ac = new AbortController();
    const urls: string[] = [];
    for (const img of Array.from(box.querySelectorAll<HTMLImageElement>("img[data-doc-src]"))) {
      const target = resolveDocLink(path, img.dataset.docSrc ?? "");
      if (!target) continue;
      api
        .ideRaw(scope, id, target, ac.signal)
        .then((bytes) => {
          if (ac.signal.aborted) return;
          const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mimeOfName(target) }));
          urls.push(url);
          img.src = url;
        })
        .catch((e: unknown) => {
          // A missing picture stays the broken icon it would be anywhere.
          if (ac.signal.aborted) return;
          log.debug("editor", "a picture in the document could not be read; it stays the broken icon", { scope, id, path: target, ...errorFields(e) });
        });
    }
    return () => {
      ac.abort();
      for (const u of urls) URL.revokeObjectURL(u);
    };
  }, [renderedText, path, scope, id]);

  if (buffer.status === "loading") {
    return (
      <div className={cn("flex items-center gap-2 p-4 text-2xs text-text-dim", className)}>
        <Spinner />{tr("workbench-editor-doc-opening", { path })}</div>
    );
  }
  if (buffer.status === "error") {
    // The node's own sentence, then — for a file over the size the editor
    // opens — its size against the limit the node said, and the way out.
    const more = refusalWords(buffer.refusal);
    return (
      <div className={cn("flex flex-col items-start gap-2 p-4 text-xs", className)}>
        <ErrorNote error={buffer.error ?? tr("workbench-editor-doc-file-could-not-opened")} retry={buffer.refusal?.kind === "too_large" ? undefined : () => void load()} />
        {more && <p className="text-2xs text-text-dim">{more}</p>}
        {more && onReveal && (
          <Button size="sm" variant="ghost" onClick={() => onReveal(path)}>
            <ICON.reveal size={12} aria-hidden />
            {revealWord}
          </Button>
        )}
      </div>
    );
  }
  if (buffer.status === "binary") {
    // A file the kind table gave the editor that is not text: a `.bin`, a
    // `.txt` of bytes. Nothing to draw; the way out is the file manager.
    return (
      <div className={cn("flex h-full min-h-0 flex-col", className)}>
        <EmptyState
          icon={ICON.file}
          title={path.split("/").pop() ?? path}
          hint={binaryWords(buffer.size)}
          action={
            onReveal ? (
              <Button size="sm" variant="ghost" onClick={() => onReveal(path)}>
                <ICON.reveal size={12} aria-hidden />
                {revealWord}
              </Button>
            ) : null
          }
        />
      </div>
    );
  }

  const modeControl = previewable ? (
    <SegmentedControl
      label={tr("workbench-editor-doc-view")}
      iconOnly
      value={mode}
      onChange={(v) => setMode(v as DocMode)}
      options={modes.map((m) => ({ id: m, label: modeLabel(docKind, m), icon: ICON[modeGlyph(m)] }))}
    />
  ) : null;

  // The wand: annotate the page for an agent — pressed while the pointer picks
  // elements, with how many annotations wait in the tray. Only where an agent
  // can run and the page is on screen. Pressed is a mode, not a summons: the
  // neutral `selected` fill; the count of annotations waiting keeps the accent,
  // the colour the page's own badges wear.
  const annotateControl =
    annotatable && mode !== "source" ? (
      <Tooltip label={wandWords({ enabled: true, why: null }, annotating)}>
        <span className="inline-flex">
          <Button size="icon" className={cn("relative", annotating && "bg-selected text-text")} variant="ghost" aria-pressed={annotating} aria-label={tr("workbench-editor-doc-annotate-page-agent")} onClick={() => setAnnotating((v) => !v)}>
            <ICON.annotate size={13} aria-hidden />
            {annotations > 0 && (
              <span className="absolute -right-1 -top-1 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-accent px-1 text-3xs font-semibold text-accent-contrast" aria-label={tr("workbench-editor-doc-annotations", { annotations })}>
                {annotations}
              </span>
            )}
          </Button>
        </span>
      </Tooltip>
    ) : null;

  // The bar's right-hand cluster, one element for both bars so the view
  // control keeps its place in every mode: the find bar, the mode glyphs,
  // the wand — after the one spacer, before Blame and Save where those are.
  const viewControls = (
    <>
      {findBar}
      {modeControl}
      {annotateControl}
    </>
  );

  /**
   * The live view of the buffer: Markdown (with its diagrams), a Mermaid
   * file, a page in its sandbox — annotatable in a workstream — or, through
   * the one renderer per kind, a csv as a grid and an svg as a figure, each
   * from the text as typed.
   */
  const preview =
    docKind === "html" ? (
      <PageAnnotator
        wid={id}
        pid={project}
        path={path}
        html={buffer.text}
        title={path.split("/").pop() ?? path}
        libraries={libraries}
        enabled={annotatable}
        inspecting={annotating}
        onInspecting={setAnnotating}
        onCount={setAnnotations}
        find={findShown ? find : null}
        findIndex={findIndex}
        onFound={(index, count) => setPageFound({ index, count })}
        place={pageScrollOf(registryKey)}
        onPlace={(next) => keepPageScroll(registryKey, next)}
      />
    ) : docKind === "diagram" ? (
      // The rendering's find walks this box, as it walks a rendered Markdown.
      <div ref={renderedBox} data-rendered-doc className="h-full min-h-0">
        <MermaidView
          source={buffer.text}
          startLine={1}
          themeSetting={settings.data?.settings.find((r) => r.key === "diagrams.theme")?.value}
          exportScale={settings.data?.settings.find((r) => r.key === "diagrams.export.scale")?.value}
          onGoToLine={(line) => reveal.current?.(line)}
          exportName={path.replace(/^.*\//, "").replace(/\.[^.]+$/, "")}
          className="h-full"
        />
      </div>
    ) : docKind === "markdown" ? (
      <div
        ref={renderedBox}
        data-rendered-doc
        data-scroll-keep="markdown"
        tabIndex={-1}
        className="h-full overflow-auto p-3 outline-none"
        onClick={(e) => {
          // A URL or a bare path went to the link handler inside `Markdown`.
          // What reaches here is a heading of this document — a table of
          // contents' `#build-and-upload`, scrolled to here and never a hash
          // the window would follow — or a relative link, resolved against
          // this document; one that climbs out of the root is left inert.
          const anchor = (e.target as HTMLElement).closest("a");
          const href = anchor?.getAttribute("href");
          if (!href) return;
          e.preventDefault();
          // The end of a selection, not a click on the link (`ui/selectionModel.mjs`).
          if (endsSelection(window.getSelection(), e.currentTarget)) return;
          const fragment = fragmentOf(href);
          if (fragment !== null) {
            scrollToFragment(e.currentTarget, fragment);
            return;
          }
          const target = resolveDocLink(path, href);
          if (target) onOpenFile?.(target);
        }}
      >
        <LinkRoots roots={[{ scope, id }]}>
          <Markdown text={buffer.text} relativeLinks onGoToLine={(line) => reveal.current?.(line)} />
        </LinkRoots>
      </div>
    ) : (
      <TextKindPreview kind={docKind} path={path} text={buffer.text} libraries={libraries} find={findShown ? find : null} findIndex={findIndex} onFound={setSheetFound} />
    );

  if (mode === "rendered") {
    return (
      <div
        ref={docRoot}
        data-document
        className={cn("flex h-full min-h-0 flex-col", className)}
        onFocusCapture={() => {
          setActiveEditor(registryKey);
          setStatusEditor(registryKey);
        }}
      >
        {/* The same right-hand cluster as the source bar, after the one
            spacer: the view control keeps its place whichever mode the
            document is in, with nothing on the left as well as with crumbs. */}
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-3 py-1">
          <StatusChip buffer={buffer} />
          <span className="flex-1" />
          {viewControls}
        </div>
        {buffer.conflict && (
          <ChangedOnDiskBanner
            page
            onReview={() => {
              setMode("source");
              setReviewing(true);
            }}
            onKeepMine={() => setBuffer((b) => keepMine(b))}
            onTakeTheirs={() => setBuffer((b) => takeTheirs(b))}
          />
        )}
        <div className="min-h-0 flex-1" data-rendered-doc tabIndex={-1}>
          {preview}
        </div>
      </div>
    );
  }

  const conflict = buffer.conflict;
  return (
    <div
      ref={docRoot}
      data-document
      className={cn("flex h-full min-h-0 flex-col", className)}
      // Focus anywhere in this document — the editor, the bar — makes it the
      // active one; ⌘S in it saves it and nothing beside it.
      onFocusCapture={() => {
        setActiveEditor(registryKey);
        setStatusEditor(registryKey);
      }}
      onKeyDownCapture={(e) => {
        // The keymap's `save` — ⌘S by default, a rebinding followed — for this document and nothing beside it.
        if (commandForEvent(currentKeymap(), e, contexts(e.target), isMac) === "save") {
          e.preventDefault();
          void save();
        }
      }}
    >
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-3 py-1 text-2xs">
        {/* The path as crumbs: a folder reveals itself in Files, the file copies its path. */}
        <nav aria-label={tr("workbench-editor-doc-path")} className="flex min-w-0 items-center gap-0.5 truncate font-mono text-text-dim">
          {breadcrumbsOf(path).map((crumb, i) => (
            <span key={crumb.path} className="flex min-w-0 items-center gap-0.5">
              {i > 0 && <ICON.collapsed size={10} aria-hidden className="shrink-0 text-text-dim" />}
              {isFile ? (
                <button
                  type="button"
                  title={crumb.dir ? tr("workbench-editor-doc-reveal-files", { crumb: crumb.path }) : tr("workbench-editor-doc-copy-path")}
                  onClick={() => (crumb.dir ? onRevealInFiles?.(crumb.path) : void copyText(path))}
                  className={cn("anim min-w-0 truncate rounded px-0.5 hover:bg-surface-2 hover:text-text", !crumb.dir && "text-text")}
                >
                  {crumb.label}
                </button>
              ) : onDisk ? (
                // A loose file: the whole absolute path, the file revealing itself in the file manager.
                <button
                  type="button"
                  title={crumb.dir ? tr("workbench-editor-doc-machine-under-no-project") : `${revealWord} · ${path}`}
                  disabled={crumb.dir}
                  onClick={() => void revealPath(path).catch((e: unknown) => toast.error(failureText("workbench", "editor-doc-failed", e)))}
                  className={cn("anim min-w-0 truncate rounded px-0.5", crumb.dir ? "cursor-default" : "text-text hover:bg-surface-2")}
                >
                  {crumb.label}
                </button>
              ) : (
                <span title={tr("workbench-editor-doc-not-saved-yet-s-asks-where")} className="min-w-0 truncate px-0.5 italic text-text">
                  {crumb.label}
                </span>
              )}
            </span>
          ))}
        </nav>
        <StatusChip buffer={buffer} />
        {/* Why it cannot be typed into, or is drawn plain: the node's word on the file, in the model's words. */}
        {sizeNote(buffer) && <span className="text-text-dim">{sizeNote(buffer)}</span>}
        {blameNote && <span className="text-warn">{blameNote}</span>}
        {lspLanguage && chip && (
          <span className={cn("inline-flex items-center gap-1", chip.tone === "ok" ? "text-ok" : chip.tone === "danger" ? "text-danger" : "text-text-dim")} title={chip.title}>
            <span aria-hidden className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-current" />
            {chip.words}
            {chip.restart && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void api.lspRestart(scope, id, lspLanguage).then(
                    () => {
                      // The verdict is forgotten: the documents it left on no server are opened again.
                      lspRestarted(scope, id, lspLanguage);
                      lspStatus.reload();
                    },
                    (e: unknown) => toast.error(failureText("workbench", "editor-doc-failed", e)),
                  )
                }
              >
                {tr("workbench-device-doc-restart")}
              </Button>
            )}
          </span>
        )}
        <span className="flex-1" />
        {viewControls}
        {reviewApplies && fileReview && (
          <Tooltip label={lensOn ? fileActionWords().dropLens : tr("workbench-editor-doc-show-review-lens-over-file-s")}>
            <Button size="sm" variant="ghost" className={cn(lensOn && "bg-selected text-text")} aria-pressed={lensOn} onClick={() => setLensOn((v) => !v)}>
              <ICON.inspect size={12} aria-hidden />{tr("workbench-editor-doc-review")}</Button>
          </Tooltip>
        )}
        {canBlame && (
          <Button
            size="sm"
            variant="ghost"
            className={cn(blameOn && "bg-selected text-text")}
            aria-pressed={blameOn}
            title={tr("workbench-editor-doc-who-last-touched-each-line-beside")}
            onClick={() => setBlameOn((v) => !v)}
          >{tr("workbench-editor-doc-blame")}</Button>
        )}
        {!buffer.readOnly && onDisk && (
          <Button size="sm" disabled={!isDirty(buffer) || buffer.status === "saving"} onClick={() => void save()}>{tr("workbench-editor-doc-save")}</Button>
        )}
        {!buffer.readOnly && !isFile && (
          // A document that can move: an untitled one has nowhere yet, a loose
          // one may join the root or go elsewhere on this machine.
          <Menu
            label={tr("workbench-editor-doc-save-3")}
            trigger={
              <span className="anim inline-flex items-center gap-1 rounded-control px-2 py-1 text-xs text-text-dim hover:bg-surface-2 hover:text-text">{tr("workbench-editor-doc-save-2")}<ICON.down size={11} aria-hidden />
              </span>
            }
            items={[
              { label: tr("workbench-editor-doc-into-project"), icon: ICON.project, onSelect: () => void saveInto() },
              { label: tr("workbench-editor-doc-elsewhere-machine"), icon: ICON.folder, onSelect: () => void saveElsewhere() },
            ]}
          />
        )}
      </div>
      {naming && (
        <PromptDialog
          open
          onClose={() => naming.resolve(null)}
          onSubmit={(value) => {
            setNaming({ ...naming, busy: true });
            naming.resolve(value);
          }}
          title={tr("workbench-editor-doc-save-into-project", { basenameOf: basenameOf(path) })}
          description={tr("workbench-editor-doc-file-written-into-root-tab-becomes")}
          label={tr("workbench-editor-doc-path")}
          hint={tr("workbench-editor-doc-relative-root-folders-made-way")}
          placeholder="src/new-file.ts" // for the machine
          initial={naming.initial}
          mono
          submitLabel={tr("workbench-editor-doc-save")}
          busy={naming.busy}
          validate={(value) => savePathProblem(value) ?? (naming.problem && value === naming.initial ? naming.problem : null)}
        />
      )}
      {conflict && !reviewing && <ChangedOnDiskBanner onReview={() => setReviewing(true)} onKeepMine={() => setBuffer((b) => keepMine(b))} onTakeTheirs={() => setBuffer((b) => takeTheirs(b))} />}
      {showLens && fileReview && fileReview.opaque && (
        <ReviewLensOpaqueBanner onKeepFile={keepFile} onUndoFile={undoFile} onDrop={() => setLensOn(false)} />
      )}
      {reviewWaitsInSource && fileReview && (
        <ReviewPendingBanner
          n={fileReview.hunks.length}
          onShow={() => {
            setLensOn(true);
            setMode("source");
          }}
        />
      )}
      {showLens && fileReview && !fileReview.opaque ? (
        <div className="flex min-h-0 flex-1 flex-col">
          <ReviewLensBar
            hunks={fileReview.hunks}
            index={hunkIndex}
            layout={lensLayout}
            left={otherPendingFiles}
            onPrevious={() => setHunkIndex((i) => previousHunkIndex(fileReview.hunks, i))}
            onNext={() => setHunkIndex((i) => nextHunkIndex(fileReview.hunks, i))}
            onKeepFile={keepFile}
            onUndoFile={undoFile}
            onNextFile={nextFileToReview}
            onToggleLayout={() => setLensLayout((l) => toggleLensLayout(l))}
            onDrop={() => setLensOn(false)}
          />
          <DiffEditor
            original={fileReview.base_text}
            modified={buffer.text}
            path={path}
            readOnly={buffer.readOnly}
            inline={lensLayout === "inline"}
            foldUnchanged={lensLayout === "inline"}
            onModifiedChange={onChange}
            annotations={lensAnnotations}
            className="min-h-0 flex-1"
          />
        </div>
      ) : conflict && reviewing ? (
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex shrink-0 items-center gap-2 border-b border-hairline px-3 py-1 text-2xs text-text-dim">
            <span>{tr("workbench-editor-doc-theirs-yours")}</span>
            <span className="flex-1" />
            <Button
              size="sm"
              variant="primary"
              onClick={() => {
                setBuffer((b) => keepMine(b));
                setReviewing(false);
              }}
            >{tr("workbench-editor-doc-keep-mine-merged")}</Button>
            <Button
              size="sm"
              onClick={() => {
                setBuffer((b) => takeTheirs(b));
                setReviewing(false);
              }}
            >{tr("workbench-editor-doc-take-theirs")}</Button>
            <Button size="sm" variant="ghost" onClick={() => setReviewing(false)}>{tr("workbench-editor-doc-back")}</Button>
          </div>
          <DiffEditor
            original={conflict.theirs}
            modified={buffer.text}
            path={path}
            onModifiedChange={onChange}
            className="min-h-0 flex-1"
          />
        </div>
      ) : (
        <div ref={editorBox} className={cn("relative flex min-h-0 flex-1", mode === "split" && "divide-x divide-border")}>
          <CodeEditor
            value={buffer.text}
            path={path}
            readOnly={buffer.readOnly}
            plain={buffer.plain}
            onChange={onChange}
            onSelectionChange={(next) => {
              setSel(next);
              if (next === null) dismissed.current = null;
            }}
            onCursorChange={(caret) => publishEditorStatus(registryKey, { path, ...caret })}
            gutter={gutter}
            handle={(h) => {
              reveal.current = h.revealLine;
              selection.current = h.selection;
              trigger.current = h.trigger;
              // A line asked for before the editor was up — a search hit, a
              // definition — is shown now that it can be.
              const waiting = takeLine(registryKey);
              if (waiting !== null) h.revealLine(waiting);
            }}
            // A pane draws one tab at a time: the editor's view — scroll,
            // cursor, selection, folds — is the tab's, kept across the unmount.
            initialViewState={editorViewOf(registryKey)}
            onViewState={(state) => keepEditorView(registryKey, state)}
            className={cn("min-h-0", mode === "split" ? "w-1/2" : "flex-1")}
          />
          {sel && selRange !== dismissed.current && scope === "workstream" && !buffer.readOnly && (
            <SelectionAgentBar
              key={selRange ?? ""}
              wid={id}
              pid={project}
              path={path}
              sel={sel}
              // In *Split* the editor is half the pane; the bar belongs in the
              // editor's half, not over the preview.
              box={{ width: mode === "split" ? box.width / 2 : box.width, height: box.height }}
              mode={agentMode}
              onMode={setAgentMode}
              onClose={() => {
                setAgentMode(null);
                dismissed.current = selRange;
              }}
            />
          )}
          {mode === "split" && (
            <div className="min-h-0 w-1/2" data-rendered-doc tabIndex={-1}>
              {preview}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * The rendered view of a text the editor holds — a csv as a grid, an svg as
 * a figure, a page in its sandbox — through the one renderer per kind, from
 * the buffer as typed: the bytes are the text's, the blob URL the figure's.
 */
function TextKindPreview({
  kind,
  path,
  text,
  libraries,
  find = null,
  findIndex = -1,
  onFound,
}: {
  kind: ReturnType<typeof docKindOf>;
  path: string;
  text: string;
  libraries: boolean;
  find?: Find | null;
  findIndex?: number;
  onFound?: (count: number) => void;
}) {
  const name = path.split("/").pop() ?? path;
  const mime = mimeOfName(name);
  const bytes = useMemo(() => new TextEncoder().encode(text), [text]);
  const [url, setUrl] = useState("");
  useEffect(() => {
    if (kind !== "svg") return;
    const u = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
    setUrl(u);
    return () => URL.revokeObjectURL(u);
  }, [bytes, mime, kind]);
  if (kind === "svg" && !url) return null;
  return <KindView kind={artifactKindOf(kind)} name={name} title={name} mime={mime} size={bytes.byteLength} bytes={{ bytes, url }} text={text} libraries={libraries} find={find} findIndex={findIndex} onFound={onFound} />;
}

/**
 * The file changed on disk under a dirty buffer: the three ways out, in
 * every mode — a rendered page too, where *Take theirs* is *Reload the
 * page* and *Review* opens the source with the difference. It asks for a
 * decision now, so it wears the summons as `ReviewPendingBanner` does —
 * not warn, which is caution that blocks nothing.
 */
function ChangedOnDiskBanner({ page = false, onReview, onKeepMine, onTakeTheirs }: { page?: boolean; onReview: () => void; onKeepMine: () => void; onTakeTheirs: () => void }) {
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-accent/40 bg-accent-soft px-3 py-1.5 text-2xs">
      <ICON.warn size={12} aria-hidden className="shrink-0 text-accent-ink" />
      <span className="font-semibold text-text">{tr("workbench-editor-doc-changed-disk-since-read")}</span>
      <span className="text-text">{page ? tr("workbench-editor-doc-page-shows-text-reload-see-file") : tr("workbench-editor-doc-text-untouched-review-difference-keep-yours")}</span>
      <span className="flex-1" />
      {page ? (
        <Button size="sm" variant="primary" onClick={onTakeTheirs}>{tr("workbench-editor-doc-reload-page")}</Button>
      ) : (
        <Button size="sm" variant="primary" onClick={onReview}>{tr("workbench-editor-doc-review")}</Button>
      )}
      <Button size="sm" onClick={onKeepMine}>{tr("workbench-editor-doc-keep-mine")}</Button>
      {page ? (
        <Button size="sm" onClick={onReview}>{tr("workbench-editor-doc-review")}</Button>
      ) : (
        <Button size="sm" onClick={onTakeTheirs}>{tr("workbench-editor-doc-take-theirs")}</Button>
      )}
    </div>
  );
}

/** A review chord as the tooltip says it — *Next change (⌥→)*. */
function withChord(label: string, command: string): string {
  const chord = chordFor(currentKeymap(), command);
  return chord ? `${label} (${keyLabel(chord)})` : label;
}

/**
 * The review lens's own bar (ide/09, ide/20): previous/next change with the
 * chords in their tooltips, the layout toggle — inline, or side by side —
 * *Keep file*, *Undo file*, *Next file* with how many wait, and the glyph
 * that hides the lens; the choices are remembered per document
 * (`lensStorageKey`, `lensLayoutKey`).
 */
function ReviewLensBar({
  hunks,
  index,
  layout,
  left,
  onPrevious,
  onNext,
  onKeepFile,
  onUndoFile,
  onNextFile,
  onToggleLayout,
  onDrop,
}: {
  hunks: readonly Hunk[];
  index: number;
  layout: LensLayout;
  /** How many other files still wait for a word. */
  left: number;
  onPrevious: () => void;
  onNext: () => void;
  onKeepFile: () => void;
  onUndoFile: () => void;
  onNextFile: () => void;
  onToggleLayout: () => void;
  onDrop: () => void;
}) {
  const words = fileActionWords(left);
  const other = lensLayoutWords(toggleLensLayout(layout));
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-hairline bg-surface-2/40 px-3 py-1 text-2xs text-text-dim" data-review-lens-bar>
      <Tooltip label={withChord(tr("workbench-editor-doc-previous-change"), "review_previous_change")}>
        <Button size="icon" variant="ghost" aria-label={tr("workbench-editor-doc-previous-change")} disabled={!hasPreviousHunk(hunks, index)} onClick={onPrevious}>
          <ICON.collapsed size={12} aria-hidden className="rotate-180" />
        </Button>
      </Tooltip>
      <span className="tnum font-medium text-text">{hunkPositionWords(hunks, index)}</span>
      <Tooltip label={withChord(tr("workbench-editor-doc-next-change"), "review_next_change")}>
        <Button size="icon" variant="ghost" aria-label={tr("workbench-editor-doc-next-change")} disabled={!hasNextHunk(hunks, index)} onClick={onNext}>
          <ICON.collapsed size={12} aria-hidden />
        </Button>
      </Tooltip>
      <span aria-hidden className="text-text-dim">·</span>
      <span>{withChord(tr("workbench-editor-doc-keep"), "review_keep_change")}</span>
      <span aria-hidden className="text-text-dim">·</span>
      <span>{withChord(tr("workbench-editor-doc-undo"), "review_undo_change")}</span>
      <span className="flex-1" />
      <Tooltip label={tr("workbench-editor-doc-show-diff", { other: other.toLowerCase() })}>
        <Button size="icon" variant="ghost" aria-label={tr("workbench-editor-doc-show-diff", { other: other.toLowerCase() })} onClick={onToggleLayout}>
          <ICON.splitRight size={12} aria-hidden className={cn(layout === "side-by-side" && "text-text")} />
        </Button>
      </Tooltip>
      <Button size="sm" variant="primary" onClick={onKeepFile} className="text-2xs">
        <ICON.check size={12} aria-hidden />
        {words.keepFile}
      </Button>
      <Button size="sm" variant="danger" onClick={onUndoFile} className="text-2xs">
        <ICON.undo size={12} aria-hidden />
        {words.undoFile}
      </Button>
      <Button size="sm" variant="ghost" onClick={onNextFile} className="text-2xs">
        {words.nextFile}
        <ICON.collapsed size={12} aria-hidden />
      </Button>
      <Tooltip label={words.dropLens}>
        <Button size="icon" variant="ghost" aria-label={words.dropLens} onClick={onDrop}>
          <ICON.hidden size={12} aria-hidden />
        </Button>
      </Tooltip>
    </div>
  );
}

/** A rendered or split view of a file whose diff waits in Source: the count, and the way there. It asks the person to keep or undo, so it wears the accent. */
function ReviewPendingBanner({ n, onShow }: { n: number; onShow: () => void }) {
  const words = pendingReviewWords(n);
  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-accent/40 bg-accent-soft px-3 py-1.5 text-2xs" data-review-pending-banner>
      <ICON.inspect size={12} aria-hidden className="shrink-0 text-accent-ink" />
      <span className="text-text">{words.text}</span>
      <span className="flex-1" />
      <Button size="sm" variant="primary" onClick={onShow} className="text-2xs">
        {words.show}
      </Button>
    </div>
  );
}

/** An opaque (binary) file's review: no hunks, keep or undo the whole thing. */
function ReviewLensOpaqueBanner({ onKeepFile, onUndoFile, onDrop }: { onKeepFile: () => void; onUndoFile: () => void; onDrop: () => void }) {
  const words = fileActionWords();
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline bg-surface-2/60 px-3 py-1.5 text-2xs">
      <span className="text-text-dim">{tr("workbench-editor-doc-file-binary-keep-undo-whole")}</span>
      <span className="flex-1" />
      <Button size="sm" onClick={onKeepFile}>
        {words.keepFile}
      </Button>
      <Button size="sm" onClick={onUndoFile}>
        {words.undoFile}
      </Button>
      <Button size="sm" variant="ghost" onClick={onDrop}>
        {words.dropLens}
      </Button>
    </div>
  );
}

function StatusChip({ buffer }: { buffer: Buffer }) {
  switch (buffer.status) {
    case "dirty":
      return <Chip>{tr("workbench-editor-doc-unsaved")}</Chip>;
    case "saving":
      return <Chip tone="quiet">{tr("workbench-editor-doc-saving")}</Chip>;
    case "conflict":
      return <Chip tone="warn">{tr("workbench-editor-doc-changed-disk")}</Chip>;
    case "clean":
      return <Chip tone="quiet">{tr("workbench-editor-doc-saved")}</Chip>;
    default:
      return null;
  }
}
