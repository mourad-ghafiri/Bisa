/**
 * The one place a Monaco editor is mounted.
 *
 * Controlled by `value` from the outside and reporting every change through
 * `onChange`; the model is created for the mount and disposed with it — the
 * leak the reference implementation hit and fixed. Read-only above the
 * editable size is the caller's decision (`readOnly`); a large file also
 * turns tokenisation off here, because Monaco degrades badly past ~10 MiB
 * and pretending otherwise is how an IDE earns a reputation for hanging.
 */

import { useEffect, useRef, useState } from "react";
import { ErrorNote } from "./Card";
import { failureReason } from "./failure";
import { cn } from "./cn";
import { loadMonaco, loadedMonaco, type Monaco } from "./monaco";
import { useEditorTypography } from "./useEditorTypography";
import { t } from "../i18n/l10n.mjs";

export interface CodeEditorProps {
  value: string;
  /** A Monaco language id, or a path to infer it from. */
  language?: string;
  path?: string;
  readOnly?: boolean;
  /** Skip tokenisation and the minimap — the large-file mode. */
  plain?: boolean;
  onChange?: (value: string) => void;
  /** Called on mount with the few things a view may ask of the editor. */
  handle?: (api: {
    revealLine: (line: number) => void;
    focus: () => void;
    /** Run one of Monaco's own actions by id — `actions.find`, `editor.action.startFindReplaceAction`. */
    trigger: (action: string) => void;
    /** The current selection, 1-based inclusive lines, or null when empty. */
    selection: () => { start: number; end: number; text: string } | null;
  }) => void;
  /**
   * The selection, with a pixel anchor (relative to the editor's box) at its
   * end, so a view can float a toolbar beside it; `null` when empty.
   * Re-fires on selection and on scroll, so the anchor tracks the text.
   */
  onSelectionChange?: (sel: EditorSelection | null) => void;
  /** The caret moved: its line and column, and the model's language — what a footer shows. */
  onCursorChange?: (pos: EditorCaret) => void;
  /**
   * Text drawn beside each line number — blame, in practice — with an
   * optional hover. `null` puts the plain numbers back. Rendered through
   * Monaco's own `lineNumbers` renderer rather than a widget per line, so a
   * 20k-line file costs nothing it did not already.
   */
  gutter?: GutterEntry[] | null;
  /**
   * Where the person was in this text — what `onViewState` handed over the
   * last time this document was on screen. Read once, at mount; opaque.
   */
  initialViewState?: unknown;
  /**
   * The editor's view — scroll, cursor, selection, folds — for a caller
   * that keeps a document's place (ide/03 §Tabs): handed over as the editor
   * unmounts (a pane draws one tab at a time), and a beat after the person
   * last moved in it, because a window that closes unmounts nothing.
   */
  onViewState?: (state: unknown) => void;
  className?: string;
}

/** How long the editor waits after the last move of the cursor or the scroll before it hands its view over. */
const VIEW_BEAT_MS = 400;

export interface GutterEntry {
  line: number;
  text: string;
  hover?: string;
}

/** A selection with a pixel anchor for a floating toolbar. */
export interface EditorCaret {
  /** 1-based. */
  line: number;
  column: number;
  language: string | null;
}

export interface EditorSelection {
  /** 1-based inclusive line numbers. */
  start: number;
  end: number;
  text: string;
  /**
   * Where the selection begins and ends, in the editor box's own pixels, for
   * a toolbar that floats beside it. Either is `null` when that end is
   * scrolled out of sight — a caller must not read that as the origin.
   */
  anchors: {
    start: { top: number; left: number } | null;
    end: { top: number; bottom: number; left: number } | null;
  };
}

type MonacoModule = typeof import("monaco-editor");

function languageFor(m: MonacoModule, language: string | undefined, path: string | undefined): string | undefined {
  if (language) return language;
  if (!path) return undefined;
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  const found = m.languages.getLanguages().find((l) => l.extensions?.some((e) => e.toLowerCase() === `.${ext}`));
  return found?.id;
}

export function CodeEditor({
  value,
  language,
  path,
  readOnly = false,
  plain = false,
  onChange,
  handle,
  onSelectionChange,
  onCursorChange,
  gutter = null,
  initialViewState = null,
  onViewState,
  className,
}: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSelRef = useRef(onSelectionChange);
  onSelRef.current = onSelectionChange;
  const onCaretRef = useRef(onCursorChange);
  onCaretRef.current = onCursorChange;
  // Read at mount and written at unmount only: neither is a reason to remount.
  const initialViewRef = useRef(initialViewState);
  initialViewRef.current = initialViewState;
  const onViewStateRef = useRef(onViewState);
  onViewStateRef.current = onViewState;
  const typography = useEditorTypography();
  const typographyRef = useRef(typography);
  typographyRef.current = typography;
  // Flips true once the editor exists, so the effects that decorate it re-run
  // after the on-demand Monaco load resolves (they no-op before it does).
  const [ready, setReady] = useState(false);
  /** Why the editor could not mount — Monaco failed to load, or refused the model. */
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    if (!host.current) return;
    // Monaco is loaded on demand; the mount awaits it, and a mount
    // torn down before it arrives disposes whatever it created.
    let disposed = false;
    let cleanup = () => {};
    const mount = loadMonaco().then((m) => {
      if (disposed || !host.current) return;
      // One model per URI: a second view of the same file — a conflict
      // beside the document, a split showing it twice — gets a model of
      // its own with no address rather than a throw from Monaco.
      const uri = path ? m.Uri.file(path) : undefined;
      const model = m.editor.createModel(
        value,
        plain ? "plaintext" : languageFor(m, language, path),
        uri && m.editor.getModel(uri) ? undefined : uri,
      );
      const ed = m.editor.create(host.current, {
        model,
        readOnly,
        automaticLayout: true,
        minimap: { enabled: !plain && typographyRef.current.minimap },
        fontFamily: typographyRef.current.fontFamily,
        fontSize: typographyRef.current.fontSize,
        lineHeight: typographyRef.current.lineHeight,
        scrollBeyondLastLine: false,
        renderWhitespace: "selection",
        largeFileOptimizations: true,
        wordWrap: typographyRef.current.wordWrap,
      });
      editor.current = ed;
      // Back where the person was: scroll, cursor, selection and folds —
      // Monaco's own record of them, opaque here. After the model, which the
      // editor was created with; a state made for another text restores what
      // it can and harms nothing.
      if (initialViewRef.current) ed.restoreViewState(initialViewRef.current as Monaco.editor.ICodeEditorViewState);
      handle?.({
        revealLine: (line) => {
          ed.revealLineInCenter(line);
          ed.setPosition({ lineNumber: line, column: 1 });
          ed.focus();
        },
        focus: () => ed.focus(),
        trigger: (action) => {
          ed.focus();
          ed.trigger("bisa", action, null);
        },
        selection: () => {
          const sel = ed.getSelection();
          const mdl = ed.getModel();
          if (!sel || !mdl || sel.isEmpty()) return null;
          return {
            start: sel.startLineNumber,
            end: sel.endColumn === 1 && sel.endLineNumber > sel.startLineNumber ? sel.endLineNumber - 1 : sel.endLineNumber,
            text: mdl.getValueInRange(sel),
          };
        },
      });
      const emitSelection = () => {
        const cb = onSelRef.current;
        if (!cb) return;
        const sel = ed.getSelection();
        const mdl = ed.getModel();
        if (!sel || !mdl || sel.isEmpty()) {
          cb(null);
          return;
        }
        const from = ed.getScrolledVisiblePosition(sel.getStartPosition());
        const at = ed.getScrolledVisiblePosition(sel.getEndPosition());
        const end = sel.endColumn === 1 && sel.endLineNumber > sel.startLineNumber ? sel.endLineNumber - 1 : sel.endLineNumber;
        cb({
          start: sel.startLineNumber,
          end,
          text: mdl.getValueInRange(sel),
          anchors: {
            start: from ? { top: from.top, left: from.left } : null,
            end: at ? { top: at.top, bottom: at.top + at.height, left: at.left } : null,
          },
        });
      };
      const emitCaret = () => {
        const cb = onCaretRef.current;
        const pos = ed.getPosition();
        if (!cb || !pos) return;
        cb({ line: pos.lineNumber, column: pos.column, language: model.getLanguageId() || null });
      };
      emitCaret();
      // The view is handed over a beat after the person last moved: a
      // closing window unmounts nothing, so the unmount alone would lose
      // the place of the document on screen.
      let viewBeat = 0;
      const keepView = () => {
        if (!onViewStateRef.current) return;
        window.clearTimeout(viewBeat);
        viewBeat = window.setTimeout(() => onViewStateRef.current?.(ed.saveViewState()), VIEW_BEAT_MS);
      };
      const subs = [
        model.onDidChangeContent(() => {
          onChangeRef.current?.(model.getValue());
        }),
        ed.onDidChangeCursorPosition(emitCaret),
        // The selection drives the in-editor agent toolbar; a scroll
        // moves its anchor, so both re-emit.
        ed.onDidChangeCursorSelection(emitSelection),
        ed.onDidScrollChange(emitSelection),
        ed.onDidChangeCursorSelection(keepView),
        ed.onDidScrollChange(keepView),
      ];
      setReady(true);
      cleanup = () => {
        window.clearTimeout(viewBeat);
        // The place is handed over before the editor goes: a caller keeps it
        // for the next mount of this document.
        onViewStateRef.current?.(ed.saveViewState());
        for (const s of subs) s.dispose();
        ed.dispose();
        model.dispose();
        editor.current = null;
      };
    });
    void mount.catch((e: unknown) => {
      if (!disposed) setFailed(failureReason("editor", "the editor could not open", e)); // for the log
    });
    return () => {
      disposed = true;
      setReady(false);
      cleanup();
    };
    // The mount owns its model; a new path or language is a new mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, language, plain]);

  // Read-only is an option on the mounted editor: flipping it keeps the
  // model, the undo history and the scroll.
  useEffect(() => {
    editor.current?.updateOptions({ readOnly });
  }, [readOnly, ready]);

  // The typography is a setting (`editor.font_*`, `editor.minimap`,
  // `editor.word_wrap`, `appearance.font_mono`): a change reaches a mounted
  // editor without a remount, so the buffer, the cursor and the scroll
  // position stay.
  useEffect(() => {
    editor.current?.updateOptions({
      fontFamily: typography.fontFamily,
      fontSize: typography.fontSize,
      lineHeight: typography.lineHeight,
      minimap: { enabled: !plain && typography.minimap },
      wordWrap: typography.wordWrap,
    });
  }, [typography, plain, ready]);

  // A value pushed from outside (a reload after `file_changed`, a merge)
  // replaces the text without disturbing an equal buffer.
  useEffect(() => {
    const model = editor.current?.getModel();
    if (model && model.getValue() !== value) model.setValue(value);
  }, [value, ready]);

  // The gutter: line-number text and a hover per line, both from one map.
  const decorations = useRef<Monaco.editor.IEditorDecorationsCollection | null>(null);
  useEffect(() => {
    const ed = editor.current;
    const m = loadedMonaco();
    if (!ed || !m) return;
    decorations.current?.clear();
    if (!gutter || gutter.length === 0) {
      ed.updateOptions({ lineNumbers: "on", lineNumbersMinChars: 5 });
      return;
    }
    const byLine = new Map(gutter.map((g) => [g.line, g]));
    const widest = gutter.reduce((w, g) => Math.max(w, g.text.length), 0);
    ed.updateOptions({
      lineNumbers: (n) => {
        const g = byLine.get(n);
        return g ? `${g.text.padEnd(widest)}  ${n}` : `${"".padEnd(widest)}  ${n}`;
      },
      lineNumbersMinChars: widest + 7,
    });
    decorations.current = ed.createDecorationsCollection(
      gutter
        .filter((g) => g.hover)
        .map((g) => ({
          range: new m.Range(g.line, 1, g.line, 1),
          options: {
            isWholeLine: true,
            hoverMessage: { value: g.hover ?? "" },
            glyphMarginHoverMessage: { value: g.hover ?? "" },
          },
        })),
    );
  }, [gutter, ready]);

  if (failed) return <ErrorNote error={t("ui-code-editor-editor-could-open", { failed })} />;
  return <div ref={host} className={cn("h-full min-h-40 w-full", className)} />;
}
