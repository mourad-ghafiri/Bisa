/**
 * A rendered page that can be annotated for an agent (ide/03 §Annotate):
 * the sandboxed frame with the inspector aboard (`PageFrame`,
 * `usePageInspector`) and the tray of annotations under the page
 * (`AnnotationTray`). The note box is the page's own: the frame draws it
 * over the clicked element — the crumbs, *What should change here?*, Add or
 * Change — exactly as a browser tab's page does (ide/18), one
 * implementation for both.
 *
 * While the person inspects, the frame outlines the element under the
 * pointer and, on a click, opens the box on it and says the pick; the note
 * typed there comes back as `bisa:note` with the element it is about, and
 * adding makes annotation *n*: a numbered badge the frame draws and a line
 * in the tray. The draft is the checkout's session's, per document
 * (`annotationsKey`), so a tab switch keeps it; sending or attaching
 * empties it. The frame reloads on every change of the page's text — a
 * keystroke in *Split* — and the inspector starts blank, so the hook says
 * inspect and marks again on `load`; a box open at that moment is gone with
 * the document it sat in. Escape in the page with no box open turns the
 * wand off.
 *
 * Off a workstream (`enabled` false) this is the plain frame: nowhere for an
 * agent to run.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { PageFrame, useInspectorTheme, usePageInspector } from "../../ui";
import { useSessionDraft } from "../_work/gitPanelStore";
import { AnnotationTray } from "./AnnotationTray";
import { EMPTY_DRAFT, addAnnotation, annotationsKey, marksOf } from "./annotationModel.mjs";
import { filePage } from "./contextChips.mjs";
import type { AnnotationDraft } from "./annotationModel.mjs";
import type { Find } from "../../ui";
import { rootKey } from "./workbenchModel.mjs";

export function PageAnnotator({
  wid,
  pid,
  path,
  html,
  title,
  libraries,
  enabled,
  inspecting,
  onInspecting,
  onCount,
  find = null,
  findIndex = 0,
  onFound,
  place,
  onPlace,
}: {
  wid: string;
  pid: string | null;
  path: string;
  html: string;
  title: string;
  libraries: boolean;
  /** A workstream's page: it can be annotated. Off, the plain frame. */
  enabled: boolean;
  /** The pointer picks elements — the toolbar's wand is pressed. */
  inspecting: boolean;
  onInspecting: (on: boolean) => void;
  /** How many annotations the draft holds, for the toolbar's badge. */
  onCount?: (n: number) => void;
  /** The find bar's query over the page's text; `null` when the bar is closed. */
  find?: Find | null;
  findIndex?: number;
  onFound?: (index: number, count: number) => void;
  /** Where the page was scrolled to, to go back to after each load; and where it is now, to keep. */
  place?: { top: number; left: number } | null;
  onPlace?: (place: { top: number; left: number }) => void;
}) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const page = useMemo(() => filePage(path), [path]);
  const [draft, setDraft] = useSessionDraft<AnnotationDraft>(annotationsKey(rootKey("workstream", wid), page), EMPTY_DRAFT);
  const [lost, setLost] = useState<number[]>([]);
  const marks = useMemo(() => marksOf(draft), [draft]);

  useEffect(() => onCount?.(draft.annotations.length), [draft.annotations.length, onCount]);

  // The frame carries the IDE's script for every page — a find works in a
  // goal's page too; only a workstream's is annotated (`enabled`).
  const theme = useInspectorTheme();
  const { onLoad } = usePageInspector(frameRef, {
    enabled: true,
    theme,
    mode: enabled && inspecting ? "picking" : "off",
    marks,
    find,
    findIndex,
    onFound,
    // A page reloads at the top on every `srcdoc` change — a keystroke in
    // Split, a tab come back to: its place is kept outside it and said back.
    place: place ?? null,
    onPlace,
    onNote: (m) => setDraft((d) => addAnnotation(d, m, m.note)),
    onMarked: setLost,
    onEscape: () => onInspecting(false),
  });

  const done = () => {
    setDraft(EMPTY_DRAFT);
    setLost([]);
    onInspecting(false);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="relative min-h-0 flex-1">
        <PageFrame html={html} title={title} libraries={libraries} inspector frameRef={frameRef} onLoad={onLoad} />
      </div>
      {enabled && (draft.annotations.length > 0 || inspecting) && <AnnotationTray wid={wid} pid={pid} page={page} draft={draft} lost={lost} onDraft={setDraft} onDone={done} />}
    </div>
  );
}
