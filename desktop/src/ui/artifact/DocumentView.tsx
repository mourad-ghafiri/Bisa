/**
 * A Word document as prose (ide/12): mammoth turns the `.docx` into HTML,
 * DOMPurify strips anything that is not prose from it, and the result is
 * drawn in the same `prose-i` the app's Markdown wears. mammoth loads when
 * the first document opens and nowhere else; DOMPurify is the kit's already
 * (`ui/Markdown.tsx` sanitizes a rendered document's raw HTML with it), so
 * its import here resolves at once.
 */

import { useEffect, useMemo, useState } from "react";
import { ErrorNote, Spinner } from "../Card";
import { sayFailure } from "../failure";
import { cn } from "../cn";
import { t } from "../../i18n/l10n.mjs";

export function DocumentView({ bytes, className }: { bytes: Uint8Array; className?: string }) {
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setHtml(null);
    setError(null);
    Promise.all([import("mammoth"), import("dompurify")])
      .then(async ([mammoth, purify]) => {
        const out = await mammoth.convertToHtml({ arrayBuffer: bytes.slice().buffer });
        const clean = purify.default.sanitize(out.value, {
          USE_PROFILES: { html: true },
          FORBID_TAGS: ["style", "script", "iframe", "object", "embed", "form"],
          FORBID_ATTR: ["style", "onerror", "onload"],
        });
        if (live) setHtml(clean);
      })
      .catch((e: unknown) => live && setError(sayFailure("artifact", t("ui-document-view-could-not-show"), e)));
    return () => {
      live = false;
    };
  }, [bytes]);
  // One object per string: react-dom sets `innerHTML` again whenever the
  // object is new, which would replace the text nodes a find holds ranges over.
  const inner = useMemo(() => (html === null ? undefined : { __html: html }), [html]);
  if (error)
    return (
      <div className={cn("p-3", className)}>
        <ErrorNote error={error} />
      </div>
    );
  if (html === null) {
    return (
      <div className={cn("p-3", className)}>
        <Spinner label={t("ui-document-view-reading-document")} />
      </div>
    );
  }
  return (
    <div data-scroll-keep="document" className={cn("h-full overflow-auto p-4", className)}>
      {/* Sanitised above: a prose profile, no styles, no handlers, no frames. */}
      <div className="prose-i mx-auto max-w-3xl text-xs leading-relaxed" dangerouslySetInnerHTML={inner} />
    </div>
  );
}
