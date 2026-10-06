/**
 * A file drawn as it is (ide/03 §Rendered documents): a PDF page by page, a
 * picture, a recording with controls, a spreadsheet as a grid, a Word
 * document as prose, a deck as an outline — the kinds `fileDocModel` says
 * have no editor behind them. Chosen by the workbench from the path alone,
 * so a PDF is never asked of the editor's text route and never sniffed.
 *
 * One fetch of the bytes (`useIdeFileBytes`, the bearer header, a blob URL
 * revoked on leave), read again when the watcher says the file moved; the
 * body is the one renderer per kind the artifact viewer draws
 * (`KindView`). The toolbar is the editor's grammar: the path as crumbs, the
 * kind's word and glyph, the size, the fact the renderer learns, and
 * *Reveal in Finder*. A file above the node's cap says so and reveals.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useEngineEvents } from "../../bus";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import type { WorkbenchScope } from "../../routeModel.mjs";
import { Button, Chip, EmptyState, ErrorNote, FindBar, ICON, KindView, Spinner, cn, copyText, emptyFind, kindWords, revealLabel, stepIndex, textOf, useDomFind, useIdeFileBytes, useKeptScroll, bytesWords } from "../../ui";
import type { Find } from "../../ui";
import { DOC_FIND } from "../../shell/shortcuts";
import { useArtifactLibraries } from "../../shell/artifactSettings";
import { keepScroll, scrollOf } from "./docViewStore";
import { breadcrumbsOf } from "./editorModel.mjs";
import { editorKey } from "./editorRegistry";
import { takeKeyboard } from "./docFocus";
import { artifactKindOf, docKindOf, tooLargeWords } from "./fileDocModel.mjs";
import { rootKey, tabId } from "./workbenchModel.mjs";
import { mimeOfName } from "../../ui/artifact/artifactModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function RenderedFileDoc({
  scope,
  id,
  path,
  onRevealInFiles,
  onReveal,
  className,
}: {
  /** The workbench's root: a document here is a tab of it, and a run has no workbench. */
  scope: WorkbenchScope;
  id: string;
  path: string;
  /** A breadcrumb was clicked: show that folder in Files. */
  onRevealInFiles?: (path: string) => void;
  /** Show the file in the OS file manager. */
  onReveal?: (path: string) => void;
  className?: string;
}) {
  const kind = artifactKindOf(docKindOf(path));
  const name = path.split("/").pop() ?? path;
  const mime = mimeOfName(name);
  // The watcher: the file changed under this document — read it again.
  const [version, setVersion] = useState(0);
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type !== "file_changed" || p.scope !== scope || p.id !== id) return;
    if (p.kind === "rescan" || p.path === path) setVersion((v) => v + 1);
  });
  // What changed on disk while the node was away was said by no frame.
  useReloadOnReconnect(() => setVersion((v) => v + 1));
  const bytes = useIdeFileBytes(scope, id, path, mime, version);
  const libraries = useArtifactLibraries();
  const [facts, setFacts] = useState<string | null>(null);
  // Find over a rendering of bytes: a sheet searches its rows, a document its
  // prose; a pdf, a picture or a recording has no text to find in. Nothing
  // here is replaced — the bytes are not a buffer.
  const findable = kind === "sheet" || kind === "document";
  const docRoot = useRef<HTMLDivElement>(null);
  const bodyBox = useRef<HTMLDivElement>(null);
  // A pane draws one tab at a time: where the person was in the rendering —
  // a sheet's rows and columns, a PDF's page — is the tab's, kept across the
  // unmount (`ui/useKeptScroll`, `docViewStore.ts`).
  const viewKey = editorKey(rootKey(scope, id), tabId({ kind: "file", path }));
  useKeptScroll(docRoot, { read: (name) => scrollOf(viewKey, name), write: (name, place) => keepScroll(viewKey, name, place) }, viewKey);
  const [find, setFind] = useState<Find | null>(null);
  const [findIndex, setFindIndex] = useState(-1);
  const [sheetFound, setSheetFound] = useState<number | null>(null);
  const domFound = useDomFind(bodyBox, kind === "document" ? find : null, findIndex, version);
  const findCount = kind === "sheet" ? sheetFound : domFound.count;
  useEffect(() => {
    setFindIndex((i) => (findCount && findCount > 0 ? (i >= 0 && i < findCount ? i : 0) : -1));
  }, [findCount, find?.query, find?.regex, find?.caseSensitive]);
  useEffect(() => {
    if (!findable) return;
    const onFind = (e: Event) => {
      const root = docRoot.current;
      if (!root || !root.contains(document.activeElement)) return;
      if ((e as CustomEvent<{ replace: boolean }>).detail?.replace) return;
      setFind((f) => f ?? emptyFind());
    };
    window.addEventListener(DOC_FIND, onFind);
    return () => window.removeEventListener(DOC_FIND, onFind);
  }, [findable]);
  // A rendering with text to find in takes the keyboard when it is shown —
  // never from a field, a shell or the Files tree (`docFocus.takeKeyboard`).
  useEffect(() => {
    if (findable) takeKeyboard(bodyBox.current);
  }, [findable]);
  const words = kindWords(kind);
  const Glyph = (ICON as Record<string, typeof ICON.file>)[words.glyph] ?? ICON.file;
  const reveal = useMemo(() => revealLabel(navigator.userAgent), []);
  const text = useMemo(() => (bytes.state === "ready" && (kind === "html" || kind === "svg") ? textOf(bytes.bytes) : null), [bytes, kind]);

  const revealVerb = onReveal ? (
    <Button size="sm" variant="ghost" onClick={() => onReveal(path)}>
      <ICON.reveal size={12} aria-hidden />
      {reveal}
    </Button>
  ) : null;

  let body;
  if (bytes.state === "loading") {
    body = (
      <div className="p-3">
        <Spinner label={t("workbench-rendered-file-doc-opening", { name })} />
      </div>
    );
  } else if (bytes.state === "too_large") {
    body = <EmptyState icon={Glyph} title={t("workbench-rendered-file-doc-too-large-render-here")} hint={tooLargeWords(bytes.size, bytes.limit)} action={revealVerb} />;
  } else if (bytes.state === "failed") {
    body = (
      <div className="p-3">
        <ErrorNote error={bytes.error} />
      </div>
    );
  } else {
    body = (
      <KindView
        kind={kind}
        name={name}
        title={name}
        mime={mime}
        size={bytes.size}
        bytes={{ bytes: bytes.bytes, url: bytes.url }}
        text={text}
        libraries={libraries}
        onFacts={setFacts}
        openWith={revealVerb}
        find={find}
        findIndex={findIndex}
        onFound={setSheetFound}
      />
    );
  }

  return (
    <div ref={docRoot} data-document className={cn("flex h-full min-h-0 flex-col", className)}>
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-hairline px-3 py-1 text-2xs">
        <nav aria-label={t("workbench-editor-doc-path")} className="flex min-w-0 items-center gap-0.5 truncate font-mono text-text-dim">
          {breadcrumbsOf(path).map((crumb, i) => (
            <span key={crumb.path} className="flex min-w-0 items-center gap-0.5">
              {i > 0 && <ICON.collapsed size={10} aria-hidden className="shrink-0 text-text-dim/60" />}
              <button
                type="button"
                title={crumb.dir ? t("workbench-editor-doc-reveal-files", { crumb: crumb.path }) : t("workbench-editor-doc-copy-path")}
                onClick={() => (crumb.dir ? onRevealInFiles?.(crumb.path) : void copyText(path))}
                className={cn("anim min-w-0 truncate rounded px-0.5 hover:bg-surface-2 hover:text-text", !crumb.dir && "text-text")}
              >
                {crumb.label}
              </button>
            </span>
          ))}
        </nav>
        <Chip tone="quiet" icon={Glyph}>
          {words.label}
        </Chip>
        {bytes.state === "ready" && <span className="tnum text-text-dim">{bytesWords(bytes.size)}</span>}
        {facts && <span className="tnum text-text-dim">{facts}</span>}
        <span className="flex-1" />
        {find && findable && (
          <FindBar
            find={find}
            onChange={setFind}
            index={findIndex}
            count={findCount}
            onStep={(dir) => setFindIndex((i) => stepIndex(i, findCount ?? 0, dir))}
            onClose={() => {
              setFind(null);
              setFindIndex(-1);
              setSheetFound(null);
              // The found occurrence stays selected; then the bar hands the
              // keyboard back, so the next find chord reaches the document.
              domFound.select();
              takeKeyboard(bodyBox.current);
            }}
            label={t("workbench-editor-doc-find-rendering")}
          />
        )}
        {revealVerb}
      </div>
      <div ref={bodyBox} className="min-h-0 flex-1 outline-none" data-rendered-doc={findable ? "" : undefined} tabIndex={findable ? -1 : undefined}>
        {body}
      </div>
    </div>
  );
}
