/**
 * An artifact under a message (ide/12): the kind's glyph, the title, the
 * size, and the first thing to see — a picture inline, a page live in its
 * sandbox at a bounded height, everything else a poster with *Open*. Live
 * frames are mounted only while on screen and at most `MAX_LIVE_FRAMES` at
 * once, so a long thread of dashboards does not run them all. A click on
 * the header opens the viewer; ⌘-click opens it expanded. Absent bytes show
 * the *Request* door, the card's outline dashed.
 */

import { useEffect, useRef, useState } from "react";
import type { ArtifactRef } from "../../types";
import { Button } from "../Button";
import { cn } from "../cn";
import { ICON } from "../icons";
import { bytesWords, inlinePreview, kindWords, MAX_LIVE_FRAMES } from "./artifactModel.mjs";
import { textOf, useArtifactBytes } from "./artifactBytes";
import { PageFrame } from "./PageFrame";
import { t } from "../../i18n/l10n.mjs";

let liveFrames = 0;

/** Whether this card may run its page live: on screen, and under the cap. */
function useLiveSlot(wanted: boolean, el: React.RefObject<HTMLElement | null>): boolean {
  const [visible, setVisible] = useState(false);
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const node = el.current;
    if (!wanted || !node) return;
    const io = new IntersectionObserver((entries) => setVisible(entries.some((e) => e.isIntersecting)), { rootMargin: "200px" });
    io.observe(node);
    return () => io.disconnect();
  }, [wanted, el]);
  useEffect(() => {
    if (!wanted) return;
    if (visible && !held && liveFrames < MAX_LIVE_FRAMES) {
      liveFrames += 1;
      setHeld(true);
    }
    if (!visible && held) {
      liveFrames -= 1;
      setHeld(false);
    }
  }, [visible, held, wanted]);
  useEffect(
    () => () => {
      if (held) liveFrames -= 1;
    },
    [held],
  );
  return held;
}

export function ArtifactCard({
  artifact,
  present,
  onOpen,
  onRequest,
  libraries = true,
}: {
  artifact: ArtifactRef;
  present: boolean;
  onOpen: (opts: { expand: boolean }) => void;
  onRequest?: () => Promise<void>;
  libraries?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const preview = inlinePreview(artifact.kind);
  const live = useLiveSlot(preview === "live" && present, box);
  const bytes = useArtifactBytes(artifact.sha256, artifact.mime, present && (preview === "image" || live));
  const words = kindWords(artifact.kind);
  const Glyph = (ICON as Record<string, typeof ICON.file>)[words.glyph] ?? ICON.file;
  const [asking, setAsking] = useState(false);

  const request = async () => {
    if (!onRequest) return;
    setAsking(true);
    try {
      await onRequest();
    } finally {
      setAsking(false);
    }
  };
  const open = (e: React.MouseEvent) => onOpen({ expand: e.metaKey || e.ctrlKey });

  let body: React.ReactNode = null;
  if (present && preview === "image" && bytes.state === "ready") {
    body = (
      <button type="button" onClick={open} aria-label={t("ui-artifact-card-open")} className="block w-full bg-surface-2">
        <img src={bytes.url} alt={artifact.title} className="mx-auto max-h-64 w-auto object-contain" />
      </button>
    );
  } else if (present && preview === "live" && live && bytes.state === "ready") {
    // A button, not a clickable box: the preview opens from the keyboard and
    // has a name; the frame underneath is inert and takes no pointer.
    body = (
      <button type="button" onClick={open} aria-label={t("ui-artifact-card-open")} className="relative block h-56 w-full text-left">
        <PageFrame html={textOf(bytes.bytes)} title={artifact.title} libraries={libraries} inert />
        <span className="absolute inset-0" aria-hidden />
      </button>
    );
  } else if (present && preview !== "poster" && (bytes.state === "loading" || (preview === "live" && !live))) {
    body = (
      <button type="button" onClick={open} className="flex h-24 w-full items-center justify-center gap-2 bg-surface-2 text-2xs text-text-dim" title={t("ui-artifact-card-open")}>
        <Glyph size={16} aria-hidden />
        {bytes.state === "loading" ? t("ui-card-loading") : t("ui-artifact-card-open-run", { words: words.label })}
      </button>
    );
  }

  return (
    <div
      ref={box}
      className={cn(
        "max-w-md overflow-hidden rounded-card border bg-surface",
        present ? "border-border" : "border-dashed border-border",
      )}
    >
      <div className="flex h-8 items-center gap-2 px-2">
        <Glyph size={13} aria-hidden className="shrink-0 text-text-dim" />
        <button type="button" onClick={open} className="anim min-w-0 flex-1 truncate text-left text-xs font-semibold text-text hover:underline" title={`${artifact.name} · ${bytesWords(artifact.size)}`}>
          {artifact.title}
        </button>
        <span className="shrink-0 text-2xs text-text-dim">
          {words.label} · {bytesWords(artifact.size)}
        </span>
        {present ? (
          <Button size="sm" variant="ghost" onClick={open}>{t("ui-artifact-card-open")}</Button>
        ) : (
          <Button size="sm" variant="ghost" disabled={asking || !onRequest} onClick={() => void request()}>
            {asking ? t("ui-artifact-card-asking") : t("ui-artifact-card-request")}
          </Button>
        )}
      </div>
      {body}
    </div>
  );
}
