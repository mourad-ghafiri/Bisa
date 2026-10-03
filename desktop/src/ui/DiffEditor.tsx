/**
 * Monaco's diff editor, wrapped once, with **view zones** for annotations —
 * the two capabilities that decided the editor: a review note
 * lives in the diff as a card beside the line it is about, not as text
 * pasted into a terminal.
 */

import { useEffect, useRef, useState } from "react";
import { ErrorNote } from "./Card";
import { failureReason } from "./failure";
import { cn } from "./cn";
import { loadMonaco, type Monaco } from "./monaco";
import { useEditorTypography } from "./useEditorTypography";
import { t } from "../i18n/l10n.mjs";

export interface DiffAnnotationAction {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  /** A reason shown as the button's title while `disabled`. */
  hint?: string;
  tone?: "default" | "accent" | "danger";
}

export interface DiffAnnotation {
  /** 1-based line in the modified side. */
  line: number;
  /** The widget's words — *Change 2 of 5*. */
  text: string;
  /**
   * Generic action buttons beside the card's body — a hunk's Keep/Undo zone,
   * say. Nothing here knows what an action *means*; the caller names the
   * label and the effect.
   */
  actions?: readonly DiffAnnotationAction[];
}

export interface DiffEditorProps {
  original: string;
  modified: string;
  language?: string;
  path?: string;
  readOnly?: boolean;
  /** One column, removed and added lines interleaved, instead of two side by side. */
  inline?: boolean;
  /** Fold the regions the two sides share, so a long file reads as its changes. */
  foldUnchanged?: boolean;
  annotations?: readonly DiffAnnotation[];
  onModifiedChange?: (value: string) => void;
  className?: string;
}

const ACTION_TONE = {
  default: "border-border text-text hover:bg-surface-2",
  accent: "border-accent/40 bg-accent text-accent-contrast hover:opacity-90",
  danger: "border-danger/40 text-danger hover:bg-danger-soft",
} as const;

/**
 * One change's widget under its last line — the words and the actions on
 * one row, the shape a hovered change wears in Copilot and Cursor: compact,
 * so a file with five changes still reads as the file.
 */
function card(a: DiffAnnotation): HTMLElement {
  const el = document.createElement("div");
  el.className = "mx-2 my-1 flex h-7 items-center gap-2 rounded-control border border-border bg-surface px-2 text-2xs text-text shadow-sm";
  el.setAttribute("role", "toolbar");
  el.setAttribute("aria-label", a.text);
  const body = document.createElement("span");
  body.className = "min-w-0 flex-1 truncate font-medium text-text-dim";
  body.textContent = a.text;
  el.append(body);
  if (a.actions && a.actions.length > 0) {
    const row = document.createElement("div");
    row.className = "flex shrink-0 items-center gap-1";
    for (const action of a.actions) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = action.label;
      btn.disabled = !!action.disabled;
      if (action.hint) btn.title = action.hint;
      btn.className = `anim rounded-control border px-2 py-0.5 text-2xs font-medium disabled:opacity-40 ${ACTION_TONE[action.tone ?? "default"]}`;
      btn.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        action.onClick();
      });
      row.append(btn);
    }
    el.append(row);
  }
  return el;
}

/** The zone's height: one compact row, with or without its buttons. */
function zoneHeight(): number {
  return 36;
}

export function DiffEditor({
  original,
  modified,
  language,
  path,
  readOnly = false,
  inline = false,
  foldUnchanged = false,
  annotations = [],
  onModifiedChange,
  className,
}: DiffEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<Monaco.editor.IStandaloneDiffEditor | null>(null);
  const zones = useRef<string[]>([]);
  const changeRef = useRef(onModifiedChange);
  changeRef.current = onModifiedChange;
  const typography = useEditorTypography();
  const typographyRef = useRef(typography);
  typographyRef.current = typography;
  // Flips true once the diff editor exists, so the effects that update it re-run
  // after the on-demand Monaco load resolves.
  const [ready, setReady] = useState(false);
  /** Why the editor could not mount — Monaco failed to load, or refused the model. */
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    if (!host.current) return;
    // Monaco is loaded on demand; a mount torn down before it
    // arrives disposes whatever it created.
    let disposed = false;
    let cleanup = () => {};
    const mount = loadMonaco().then((m) => {
      if (disposed || !host.current) return;
      // Both sides live under schemes of their own: the file's own URI is
      // the document editor's, and a diff opened beside it must not claim it.
      const uri = path ? m.Uri.file(path) : undefined;
      const taken = (scheme: string) => (uri && !m.editor.getModel(uri.with({ scheme })) ? uri.with({ scheme }) : undefined);
      const originalModel = m.editor.createModel(original, language, taken("original"));
      const modifiedModel = m.editor.createModel(modified, language, taken("modified"));
      const ed = m.editor.createDiffEditor(host.current, {
        automaticLayout: true,
        readOnly,
        renderSideBySide: !inline,
        hideUnchangedRegions: { enabled: foldUnchanged },
        originalEditable: false,
        fontFamily: typographyRef.current.fontFamily,
        fontSize: typographyRef.current.fontSize,
        lineHeight: typographyRef.current.lineHeight,
        scrollBeyondLastLine: false,
        minimap: { enabled: false },
      });
      ed.setModel({ original: originalModel, modified: modifiedModel });
      editor.current = ed;
      const sub = modifiedModel.onDidChangeContent(() => changeRef.current?.(modifiedModel.getValue()));
      setReady(true);
      cleanup = () => {
        sub.dispose();
        ed.dispose();
        originalModel.dispose();
        modifiedModel.dispose();
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
    // The text is the mount's: `original` and `modified` are pushed into the
    // models by the effect below, and following them here would rebuild the
    // editor on every keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, language, readOnly, inline, foldUnchanged]);

  // The same typography as the code editor, live: a diff and the file it is
  // about read in one face at one size.
  useEffect(() => {
    editor.current?.updateOptions({
      fontFamily: typography.fontFamily,
      fontSize: typography.fontSize,
      lineHeight: typography.lineHeight,
    });
  }, [typography, ready]);

  useEffect(() => {
    const ed = editor.current;
    if (!ed) return;
    const om = ed.getOriginalEditor().getModel();
    const mm = ed.getModifiedEditor().getModel();
    if (om && om.getValue() !== original) om.setValue(original);
    if (mm && mm.getValue() !== modified) mm.setValue(modified);
  }, [original, modified, ready]);

  // Annotations are view zones on the modified side: a card that takes
  // vertical space under its line, so nothing overlaps the code.
  useEffect(() => {
    const ed = editor.current;
    if (!ed) return;
    const target = ed.getModifiedEditor();
    target.changeViewZones((accessor) => {
      for (const id of zones.current) accessor.removeZone(id);
      zones.current = annotations.map((a) =>
        accessor.addZone({
          afterLineNumber: a.line,
          heightInPx: zoneHeight(),
          domNode: card(a),
        }),
      );
    });
  }, [annotations, ready]);

  if (failed) return <ErrorNote error={t("ui-code-editor-editor-could-open", { failed })} />;
  return <div ref={host} className={cn("h-full min-h-40 w-full", className)} />;
}
