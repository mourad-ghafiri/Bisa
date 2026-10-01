/**
 * One renderer per kind (ide/12): the body an artifact's viewer and a
 * rendered document in the IDE (ide/03) both draw — a page in its sandbox, a
 * figure or a picture from a blob URL, a recording with controls, a PDF
 * page by page, a sheet as a grid, a Word document as prose, a deck as an
 * outline, text in the read-only editor. The bytes come from whoever holds
 * them; nothing here fetches.
 */

import type { ReactNode } from "react";
import { t } from "../../i18n/l10n.mjs";
import type { ArtifactKind } from "../../types";
import { ICON } from "../icons";
import { bytesWords, kindWords } from "./artifactModel.mjs";
import { DocumentView } from "./DocumentView";
import { ImageView } from "./ImageView";
import { MediaView } from "./MediaView";
import { PageFrame } from "./PageFrame";
import { PdfView } from "./PdfView";
import { SheetView } from "./SheetView";
import type { Find } from "../find/findModel.mjs";
import { SlidesView } from "./SlidesView";
import { TextView } from "./TextView";

export interface KindBytes {
  bytes: Uint8Array;
  /** A blob URL of the same bytes, for an image or a recording. */
  url: string;
}

export function KindView({
  kind,
  name,
  title,
  mime,
  size,
  bytes,
  text,
  libraries = true,
  onFacts,
  openWith,
  find = null,
  findIndex = -1,
  onFound,
}: {
  kind: ArtifactKind;
  /** The file's name — a sheet's format is told from it. */
  name: string;
  title: string;
  mime: string;
  size: number;
  bytes: KindBytes;
  /** The bytes as text, for a text kind and a page; null when the caller did not decode them. */
  text: string | null;
  /** Whether a page may load libraries from the CDNs (`artifacts.html.libraries`). */
  libraries?: boolean;
  /** A fact the renderer learns — *12 pages*, *120 rows × 6 columns*. */
  onFacts?: (words: string) => void;
  /** The host's *Open with the default app*, for a recording WebKit cannot play. */
  openWith?: ReactNode;
  /** A find over the rendering, for the kinds that search their own model — the sheet. */
  find?: Find | null;
  findIndex?: number;
  onFound?: (count: number) => void;
}) {
  switch (kind) {
    case "html":
      return <PageFrame html={text ?? ""} title={title} libraries={libraries} />;
    case "svg":
    case "image":
      return <ImageView url={bytes.url} alt={title} />;
    case "video":
    case "audio":
      return <MediaView url={bytes.url} kind={kind} mime={mime} openWith={openWith} />;
    case "pdf":
      return <PdfView bytes={bytes.bytes} onFacts={onFacts} />;
    case "sheet":
      return <SheetView bytes={bytes.bytes} name={name} onFacts={onFacts} find={find} current={findIndex} onFound={onFound} />;
    case "document":
      return <DocumentView bytes={bytes.bytes} />;
    case "slides":
      return <SlidesView bytes={bytes.bytes} onFacts={onFacts} />;
    case "markdown":
    case "diagram":
    case "code":
    case "data":
    case "text":
      return <TextView artifact={{ sha256: "", name, mime, size, title, kind }} text={text ?? ""} />;
    default: {
      const words = kindWords(kind);
      const Glyph = (ICON as Record<string, typeof ICON.file>)[words.glyph] ?? ICON.file;
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-2xs text-text-dim">
          <Glyph size={24} aria-hidden />
          <p>
            {t("ui-kind-view-nothing-to-draw-here", { name, size: bytesWords(size) })}
          </p>
          {openWith}
        </div>
      );
    }
  }
}
