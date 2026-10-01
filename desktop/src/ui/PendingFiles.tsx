/**
 * The strip of files a surface is holding (`useUploads`): each row is its
 * own state — uploading, ready with its size, or failed with the reason — so
 * a request-level error never says "upload failed" about four files when one
 * of them did. A caller may put its own control on a ready row (the
 * composer's *file · artifact* toggle) through `trailing`.
 */

import type { ReactNode } from "react";
import { formatBytes } from "../views/_studio/attachmentModel.mjs";
import { cn } from "./cn";
import { ICON } from "./icons";
import type { PendingFile } from "./useUploads";
import { t } from "../i18n/l10n.mjs";

export function PendingFiles({
  files,
  onRemove,
  trailing,
  tone,
  className,
}: {
  files: PendingFile[];
  onRemove: (key: string) => void;
  /** A control on a ready row, after its size. */
  trailing?: (file: PendingFile) => ReactNode;
  /** A chip's look beyond its state — the composer marks an artifact. */
  tone?: (file: PendingFile) => "plain" | "accent";
  className?: string;
}) {
  if (files.length === 0) return null;
  return (
    <div className={cn("flex flex-wrap gap-1", className)}>
      {files.map((f) => {
        const accent = tone?.(f) === "accent";
        return (
          <span
            key={f.key}
            className={cn(
              "inline-flex max-w-72 items-center gap-1.5 rounded-full border px-2 py-0.5 text-2xs",
              f.state === "failed" ? "border-danger/40 bg-danger-soft text-danger" : accent ? "border-accent/40 bg-accent-soft text-text" : "border-border bg-surface-2 text-text-dim",
            )}
          >
            {accent ? <ICON.artifact size={11} aria-hidden className="shrink-0" /> : <ICON.attach size={11} aria-hidden className="shrink-0" />}
            <span className="min-w-0 truncate">{f.name}</span>
            <span className="tnum shrink-0">{f.state === "uploading" ? t("ui-pending-files-uploading") : f.state === "failed" ? (f.error ?? t("ui-pending-files-failed")) : formatBytes(f.size)}</span>
            {f.state === "done" && trailing?.(f)}
            <button type="button" aria-label={t("ui-pending-files-remove", { f: f.name })} onClick={() => onRemove(f.key)} className="anim shrink-0 rounded px-0.5 hover:text-danger">
              <ICON.close size={11} aria-hidden />
            </button>
          </span>
        );
      })}
    </div>
  );
}
