/**
 * A PDF, page by page (ide/12), through pdf.js — loaded when the first PDF
 * opens and nowhere else. Each page is a canvas drawn when it scrolls into
 * view, so a long report costs what is looked at; a page counter and a zoom
 * sit above.
 */

import { useEffect, useRef, useState } from "react";
import { Button } from "../Button";
import { ErrorNote, Spinner } from "../Card";
import { sayFailure } from "../failure";
import { cn } from "../cn";
import { t } from "../../i18n/l10n.mjs";

type PdfDocument = {
  numPages: number;
  getPage: (n: number) => Promise<PdfPage>;
  destroy: () => Promise<void>;
};
type PdfPage = {
  getViewport: (opts: { scale: number }) => { width: number; height: number };
  render: (opts: { canvasContext: CanvasRenderingContext2D; viewport: { width: number; height: number } }) => { promise: Promise<void> };
};

let loader: Promise<typeof import("pdfjs-dist")> | null = null;

/** pdf.js once, its worker as a Vite asset like Monaco's. */
function loadPdfjs() {
  if (!loader) {
    loader = Promise.all([import("pdfjs-dist"), import("pdfjs-dist/build/pdf.worker.min.mjs?url")]).then(([lib, worker]) => {
      lib.GlobalWorkerOptions.workerSrc = worker.default;
      return lib;
    });
  }
  return loader;
}

const SCALES = [0.75, 1, 1.25, 1.5, 2];

export function PdfView({ bytes, className, onFacts }: { bytes: Uint8Array; className?: string; onFacts?: (words: string) => void }) {
  const [doc, setDoc] = useState<PdfDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scale, setScale] = useState(1);
  const [page, setPage] = useState(1);

  useEffect(() => {
    let live = true;
    let opened: PdfDocument | null = null;
    setDoc(null);
    setError(null);
    loadPdfjs()
      .then((lib) => lib.getDocument({ data: bytes.slice() }).promise)
      .then((d) => {
        if (!live) return void d.destroy();
        opened = d as unknown as PdfDocument;
        setDoc(opened);
        onFacts?.(`${d.numPages} ${d.numPages === 1 ? "page" : "pages"}`);
      })
      .catch((e: unknown) => live && setError(sayFailure("artifact", t("ui-pdf-view-could-not-show"), e)));
    return () => {
      live = false;
      void opened?.destroy();
    };
  }, [bytes, onFacts]);

  if (error)
    return (
      <div className={cn("p-3", className)}>
        <ErrorNote error={error} />
      </div>
    );
  if (!doc) {
    return (
      <div className={cn("p-3", className)}>
        <Spinner label={t("ui-pdf-view-opening-pdf")} />
      </div>
    );
  }
  return (
    <div className={cn("flex h-full flex-col", className)}>
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-hairline px-2 text-2xs text-text-dim">
        <span className="tnum">{t("ui-pdf-view-page", { page, numPages: doc.numPages })}</span>
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={() => setScale((s) => SCALES[Math.max(0, SCALES.indexOf(s) - 1)] ?? s)} disabled={scale === SCALES[0]}>
          −
        </Button>
        <span className="tnum w-10 text-center">{Math.round(scale * 100)}%</span>
        <Button size="sm" variant="ghost" onClick={() => setScale((s) => SCALES[Math.min(SCALES.length - 1, SCALES.indexOf(s) + 1)] ?? s)} disabled={scale === SCALES[SCALES.length - 1]}>
          +
        </Button>
      </div>
      <div data-scroll-keep="pdf" className="min-h-0 flex-1 overflow-auto bg-surface-2 p-3">
        {Array.from({ length: doc.numPages }, (_, i) => (
          <PdfPageView key={i} doc={doc} number={i + 1} scale={scale} onVisible={() => setPage(i + 1)} />
        ))}
      </div>
    </div>
  );
}

function PdfPageView({ doc, number, scale, onVisible }: { doc: PdfDocument; number: number; scale: number; onVisible: () => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            setVisible(true);
            if (e.intersectionRatio > 0.5) onVisible();
          }
        }
      },
      { threshold: [0, 0.5] },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [onVisible]);
  useEffect(() => {
    if (!visible) return;
    let live = true;
    void doc.getPage(number).then(async (p) => {
      const el = canvas.current;
      if (!el || !live) return;
      const viewport = p.getViewport({ scale: scale * window.devicePixelRatio });
      el.width = viewport.width;
      el.height = viewport.height;
      el.style.width = `${viewport.width / window.devicePixelRatio}px`;
      el.style.height = `${viewport.height / window.devicePixelRatio}px`;
      const ctx = el.getContext("2d");
      if (!ctx) return;
      await p.render({ canvasContext: ctx, viewport }).promise;
    });
    return () => {
      live = false;
    };
  }, [doc, number, scale, visible]);
  return (
    <canvas
      ref={canvas}
      aria-label={t("ui-pdf-view-page-2", { number })}
      className="mx-auto mb-3 block bg-white shadow-sm"
      style={visible ? undefined : { width: `${612 * scale}px`, height: `${792 * scale}px` }}
    />
  );
}
