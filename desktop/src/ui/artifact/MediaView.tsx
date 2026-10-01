/**
 * A video or a recording (ide/12), from the blob URL. What WebKit decodes
 * plays here — MP4 with H.264 and AAC, MP3, WAV; a codec it cannot play is
 * said so, with the door to the default app beside the words.
 */

import { useState, type ReactNode } from "react";
import { cn } from "../cn";
import { t } from "../../i18n/l10n.mjs";

export function MediaView({
  url,
  kind,
  mime,
  className,
  openWith,
}: {
  url: string;
  kind: "video" | "audio";
  mime: string;
  className?: string;
  /** The verb that opens it elsewhere when it cannot play here. */
  openWith?: ReactNode;
}) {
  const [failed, setFailed] = useState(false);
  if (failed) {
    return (
      <div className={cn("flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-2xs text-text-dim", className)}>
        <p>{t("ui-media-view-this-kind-cannot-play", { kind, mime: mime || t("ui-media-view-unknown-type") })}</p>
        {openWith}
      </div>
    );
  }
  if (kind === "audio") {
    return (
      <div className={cn("flex h-full items-center justify-center p-4", className)}>
        <audio controls src={url} onError={() => setFailed(true)} className="w-full max-w-md" />
      </div>
    );
  }
  return (
    <div className={cn("flex h-full items-center justify-center bg-black", className)}>
      <video controls src={url} onError={() => setFailed(true)} className="max-h-full max-w-full" />
    </div>
  );
}
