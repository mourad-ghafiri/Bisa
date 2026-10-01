/**
 * An image or a vector figure (ide/12): an `<img>` from the blob URL — an
 * image element never runs a script, which is why an SVG is drawn this way
 * and not as a page. A click swaps between fit-to-width and its own size.
 */

import { useState } from "react";
import { cn } from "../cn";
import { t } from "../../i18n/l10n.mjs";

export function ImageView({ url, alt, className, fit = true }: { url: string; alt: string; className?: string; fit?: boolean }) {
  const [zoomed, setZoomed] = useState(false);
  return (
    <div data-scroll-keep="image" className={cn("h-full w-full overflow-auto", className)}>
      <img
        src={url}
        alt={alt}
        onClick={() => fit && setZoomed((z) => !z)}
        title={fit ? (zoomed ? t("ui-image-view-fit-pane") : t("ui-image-view-actual-size")) : undefined}
        className={cn(
          "block",
          fit ? (zoomed ? "max-w-none cursor-zoom-out" : "mx-auto max-h-full max-w-full cursor-zoom-in object-contain") : "max-h-64 w-auto object-contain",
        )}
      />
    </div>
  );
}
