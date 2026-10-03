/**
 * The Details panel's Documents card: the files a person gave the goal as
 * context — a brief, a spec, a screenshot — kept under the goal's
 * `documents/` folder and read by whoever works on it.
 *
 * Adding one here goes the way capture does: uploaded on pick
 * (`useUploads`), then named to the goal in one request. The folder itself
 * is on the Files panel, where a document opens like any file the goal
 * holds; a row here is the way there.
 */

import { useEffect, useRef } from "react";
import { api } from "../../api";
import type { GoalDocumentRow } from "../../types";
import { Button, ICON, PendingFiles, SectionHeader, Tooltip, useToast, useUploads } from "../../ui";
import { fileIcon } from "../../ui/icons";
import { formatBytes } from "../_studio/attachmentModel.mjs";
import { attempt, useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

export function DocumentsCard({ goal, readOnly = false, onOpenFiles }: { goal: string; /** Listed, never added to: the goal's run is going. */ readOnly?: boolean; onOpenFiles: () => void }) {
  const toast = useToast();
  const { data, reload } = useAsync((s) => api.goalDocuments(goal, s), [goal]);
  const rows: GoalDocumentRow[] = data?.documents ?? [];
  const uploads = useUploads();
  const input = useRef<HTMLInputElement>(null);

  // Every upload that answered is handed to the goal at once, in one
  // request, and the strip empties: a chip that lingered would read as a
  // document not yet given.
  const { uploaded, uploading, pending, clear } = uploads;
  useEffect(() => {
    if (uploading || uploaded.length === 0) return;
    if (pending.some((p) => p.state === "failed")) return;
    let cancelled = false;
    void attempt(
      async () => {
        await api.addGoalDocuments(goal, uploaded);
      },
      toast.error,
      () => {
        if (cancelled) return;
        toast.ok(uploaded.length === 1 ? t("work-documents-card-document-added") : t("work-documents-card-documents-added", { uploaded: uploaded.length }));
        clear();
        reload();
      },
    );
    return () => {
      cancelled = true;
    };
  }, [goal, uploaded, uploading, pending, clear, reload, toast]);

  return (
    <section>
      <SectionHeader flush
        title={t("work-documents-card-documents")}
        count={rows.length}
        action={
          readOnly ? undefined : (
            <Button variant="ghost" size="sm" onClick={() => input.current?.click()}>
              <ICON.attach size={11} aria-hidden />{t("work-documents-card-add")}</Button>
          )
        }
        alwaysAction={!readOnly}
      />
      <input
        ref={input}
        type="file"
        multiple
        hidden
        onChange={(e) => {
          uploads.take(e.target.files);
          e.target.value = "";
        }}
      />
      {rows.length === 0 && pending.length === 0 ? (
        <p className="text-2xs text-text-dim">{t("work-documents-card-none-brief-spec-screenshot-given-here")}</p>
      ) : (
        <ul className="flex flex-col gap-0.5">
          {rows.map((d) => {
            const Glyph = fileIcon(d.name);
            return (
              <li key={`${d.file.sha256}:${d.name}`}>
                <Tooltip label={d.present ? t("work-documents-card-open-files-panel", { d: d.path }) : t("work-documents-card-bytes-not-node-yet-they-arrive")}>
                  <button
                    type="button"
                    onClick={onOpenFiles}
                    className="anim flex w-full items-center gap-2 rounded-control px-1.5 py-1 text-left text-2xs hover:bg-surface-2"
                  >
                    <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />
                    <span className="min-w-0 flex-1 truncate">{d.name}</span>
                    <span className="tnum shrink-0 text-text-dim">{d.present ? formatBytes(d.file.size) : t("work-documents-card-not-here-yet")}</span>
                  </button>
                </Tooltip>
              </li>
            );
          })}
        </ul>
      )}
      <PendingFiles files={pending} onRemove={uploads.remove} className="mt-1" />
    </section>
  );
}
