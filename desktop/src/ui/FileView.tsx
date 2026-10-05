/**
 * One file, read back from the node.
 *
 * Hoisted out of `FileTree.tsx`, where it was a private `FilePreview`, so the
 * workbench's document tab and the goal inspector's inline preview are one
 * component rather than two. The reason that matters is not tidiness: the
 * **three outcomes** below are a fact about `GET /file`, and a second renderer
 * of them is how one surface quietly stops saying *the first part only*.
 *
 * - `binary: true` arrives with no bytes at all. There is nothing to render and
 *   nothing was withheld.
 * - `truncated: true` means what is on screen is a *prefix*. A reader who is
 *   not told will conclude the file ends where the pane does.
 * - Everything else is the whole file.
 *
 * # Markdown renders as markdown
 *
 * A `.md` shows rendered by default, with a **Source** toggle — because a
 * repository's markdown is written to be read, and a wall of `##` and `|---|`
 * in a monospace block is the one thing an IDE-shaped surface should not do to
 * it. Relative links resolve against this file's own directory and open as
 * another document, which is how a repository's own docs are navigated.
 *
 * The caller owns the box: `className` sets the height, and the body is
 * `flex-1`, so the same component works as a 20rem preview under a tree and as
 * a full-height document.
 */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "../api";
import type { WorkbenchScope } from "../routeModel.mjs";
import type { FileContent } from "../types";
import { ErrorNote, Spinner } from "./Card";
import { sayFailure } from "./failure";
import { cn } from "./cn";
import { ICON } from "./icons";
import { LinkRoots } from "./linkContext";
import { Markdown } from "./Markdown";
import { SegmentedControl } from "./SegmentedControl";
import { Tooltip } from "./Tooltip";
import { formatSize } from "./fileTreeModel.mjs";
import { scrollToFragment } from "./docAnchors";
import { fragmentOf, isMarkdown, resolveDocLink } from "../views/_workbench/docLink.mjs";
import { binaryWords } from "../views/_workbench/fileDocModel.mjs";
import { t } from "../i18n/l10n.mjs";


export function FileView({
  scope,
  id,
  path,
  nonce = 0,
  onOpenFile,
  actions,
  className,
}: {
  scope: WorkbenchScope;
  id: string;
  path: string;
  /** Bumped when the tree refreshes, so an open file follows a live run too. */
  nonce?: number;
  /** Following a relative markdown link. Without it, such links are inert. */
  onOpenFile?: (path: string) => void;
  /** Extra header controls — the tree passes a close button; a tab passes none. */
  actions?: ReactNode;
  className?: string;
}) {
  const [content, setContent] = useState<FileContent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [asSource, setAsSource] = useState(false);

  const markdown = useMemo(() => isMarkdown(path), [path]);

  useEffect(() => {
    const ac = new AbortController();
    setError(null);
    setContent(null);
    api
      .file(scope, id, path, ac.signal)
      .then((c) => setContent(c))
      .catch((e) => {
        if (!ac.signal.aborted) setError(sayFailure("files", t("ui-file-view-could-not-read"), e));
      });
    return () => ac.abort();
  }, [scope, id, path, nonce]);

  // A different file is a different question about how to show it.
  useEffect(() => setAsSource(false), [path]);

  const rendered = markdown && !asSource && content?.text;

  return (
    <div className={cn("flex min-h-0 min-w-0 flex-col rounded-control border border-border", className)}>
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-hairline px-2">
        <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
        <Tooltip label={path}>
          <span className="min-w-0 flex-1 truncate font-mono text-2xs">{path}</span>
        </Tooltip>
        {content && (
          <span className="tnum shrink-0 text-2xs text-text-dim">{formatSize(content.size)}</span>
        )}
        {markdown && content?.text && (
          <SegmentedControl
            label={t("ui-file-view-how-show-file")}
            size="sm"
            value={asSource ? "source" : "rendered"}
            onChange={(v) => setAsSource(v === "source")}
            options={[
              { id: "rendered", label: t("ui-file-view-rendered") },
              { id: "source", label: t("ui-file-view-source") },
            ]}
          />
        )}
        {actions}
      </div>

      {error ? (
        <div className="px-2 py-1.5">
          <ErrorNote error={error} />
        </div>
      ) : !content ? (
        <div className="px-2 py-1.5">
          <Spinner label={t("ui-file-view-reading")} />
        </div>
      ) : content.binary ? (
        <p className="px-2 py-1.5 text-2xs text-text-dim">{t("ui-file-view-binary-open-elsewhere", { words: binaryWords(content.size) })}</p>
      ) : (
        <>
          {content.truncated && (
            <p className="shrink-0 border-b border-hairline bg-warn-soft px-2 py-1 text-2xs text-warn">
              {t("ui-file-view-first-part-only", { size: formatSize(content.size) })}
            </p>
          )}
          {rendered ? (
            <div
              className="min-h-0 flex-1 overflow-auto p-3"
              onClick={(e) => {
                // A URL or a bare path went to the link handler inside
                // `Markdown` and never reaches here. What does is a heading
                // of this document — `#build-and-upload`, scrolled to here,
                // never a hash the window would follow — or a relative link,
                // resolved against the file it sits in; one that climbs out
                // of the root, or points nowhere, is left inert.
                const anchor = (e.target as HTMLElement).closest("a");
                const href = anchor?.getAttribute("href");
                if (!href) return;
                e.preventDefault();
                const fragment = fragmentOf(href);
                if (fragment !== null) {
                  scrollToFragment(e.currentTarget, fragment);
                  return;
                }
                const target = resolveDocLink(path, href);
                if (!target || !onOpenFile) return;
                onOpenFile(target);
              }}
            >
              <LinkRoots roots={[{ scope, id }]}>
                <Markdown text={content.text ?? ""} relativeLinks />
              </LinkRoots>
            </div>
          ) : (
            <pre className="min-h-0 flex-1 overflow-auto p-2 font-mono text-2xs leading-relaxed">
              {content.text ?? ""}
            </pre>
          )}
        </>
      )}
    </div>
  );
}
