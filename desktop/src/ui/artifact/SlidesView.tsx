/**
 * A deck as its slides (ide/12): one card per slide with its title, its
 * text, its pictures and its notes, read off the `.pptx` zip by
 * `pptxModel.mjs` — an outline, honest about being one; the toolbar's
 * *Open with the default app* is the door to the deck as designed.
 */

import { useEffect, useState } from "react";
import { Spinner } from "../Card";
import { cn } from "../cn";
import { notesPath, notesText, slideImages, slidePaths, slideRelsPath, slideText, slideWords } from "./pptxModel.mjs";
import { t } from "../../i18n/l10n.mjs";

interface Slide {
  index: number;
  title: string;
  paragraphs: string[];
  images: string[];
  notes: string;
}

async function slidesOf(bytes: Uint8Array): Promise<{ slides: Slide[]; urls: string[] }> {
  const { default: JSZip } = await import("jszip");
  const zip = await JSZip.loadAsync(bytes);
  const names = Object.keys(zip.files);
  const urls: string[] = [];
  const slides: Slide[] = [];
  for (const [i, path] of slidePaths(names).entries()) {
    const xml = (await zip.file(path)?.async("string")) ?? "";
    const rels = (await zip.file(slideRelsPath(path))?.async("string")) ?? "";
    const { title, paragraphs } = slideText(xml);
    const images: string[] = [];
    for (const entry of slideImages(rels, path)) {
      const file = zip.file(entry);
      if (!file) continue;
      const blob = await file.async("blob");
      const url = URL.createObjectURL(blob);
      urls.push(url);
      images.push(url);
    }
    const notesEntry = notesPath(rels, path);
    const notes = notesEntry ? notesText((await zip.file(notesEntry)?.async("string")) ?? "") : "";
    slides.push({ index: i + 1, title, paragraphs, images, notes });
  }
  return { slides, urls };
}

export function SlidesView({ bytes, className, onFacts }: { bytes: Uint8Array; className?: string; onFacts?: (words: string) => void }) {
  const [slides, setSlides] = useState<Slide[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    let urls: string[] = [];
    setSlides(null);
    setError(null);
    slidesOf(bytes)
      .then((out) => {
        urls = out.urls;
        if (!live) return;
        setSlides(out.slides);
        onFacts?.(slideWords(out.slides.length));
      })
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
      for (const u of urls) URL.revokeObjectURL(u);
    };
  }, [bytes, onFacts]);
  if (error) return <p className={cn("p-3 text-2xs text-danger", className)}>{error}</p>;
  if (!slides) {
    return (
      <div className={cn("p-3", className)}>
        <Spinner label={t("ui-slides-view-reading-deck")} />
      </div>
    );
  }
  return (
    <div data-scroll-keep="slides" className={cn("h-full overflow-auto bg-surface-2 p-3", className)}>
      <p className="mb-2 text-2xs text-text-dim">{t("ui-slides-view-outline-deck-slides-their-words-pictures")}</p>
      <div className="flex flex-col gap-3">
        {slides.map((s) => (
          <section key={s.index} className="rounded-card border border-border bg-surface p-3">
            <div className="mb-1 flex items-baseline gap-2">
              <span className="tnum text-2xs text-text-dim">{s.index}</span>
              <h3 className="text-xs font-semibold text-text">{s.title || t("ui-slides-view-untitled-slide")}</h3>
            </div>
            {s.paragraphs.length > 0 && (
              <ul className="ml-4 list-disc text-2xs text-text">
                {s.paragraphs.map((p, i) => (
                  <li key={i}>{p}</li>
                ))}
              </ul>
            )}
            {s.images.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-2">
                {s.images.map((u) => (
                  <img key={u} src={u} alt="" className="max-h-40 rounded-control border border-border object-contain" />
                ))}
              </div>
            )}
            {s.notes && <p className="mt-2 border-t border-border pt-2 text-2xs italic text-text-dim">{s.notes}</p>}
          </section>
        ))}
      </div>
    </div>
  );
}
