/**
 * A Word document as prose (ide/12): mammoth turns the `.docx` into HTML,
 * DOMPurify strips anything that is not prose from it, and the result is
 * drawn in the same `prose-i` the app's Markdown wears. Both libraries load
 * when the first document opens and nowhere else.
 */

import { useEffect, useState } from "react";
import { Spinner } from "../Card";
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
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [bytes]);
  if (error) return <p className={cn("p-3 text-2xs text-danger", className)}>{error}</p>;
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
      <div className="prose-i mx-auto max-w-3xl text-xs leading-relaxed" dangerouslySetInnerHTML={{ __html: html }} />
    </div>
  );
}
