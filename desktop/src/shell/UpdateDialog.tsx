/**
 * Update — whether a newer release of the platform exists, asked of the
 * node when the dialog opens (`GET /updates`: the latest release as GitHub
 * lists it, held by the node for an hour; *Check again* asks GitHub again)
 * and set against this desktop's own version (`__APP_VERSION__`, baked in
 * at build time). The words, the states and the doors are
 * `updateModel.mjs`'s; this draws them.
 *
 * When a newer release is out, the one primary is the release page on
 * GitHub — where the disk image and its checksum are — the accent spent
 * on the thing that asks for the person's attention; *What changed* opens
 * the changelog at that release's tag, and the release's own notes read
 * inline under *What's new*. Every door opens through `openExternal`,
 * never a link, so the window itself never navigates. Nothing here checks
 * on its own: a request leaves the machine only when the dialog is open.
 */

import { useState } from "react";
import { api, openExternal } from "../api";
import { Button, Dialog, ErrorNote, ICON, Markdown, PlatformMark, RelativeTime, SkeletonRows } from "../ui";
import { useAsync } from "../views/_work/useAsync";
import { PRODUCT } from "./aboutModel.mjs";
import { releaseLinks, releasesIndex, updateState, updateWords, versionLine } from "./updateModel.mjs";
import type { UpdateState } from "./updateModel.mjs";
import { t } from "../i18n/l10n.mjs";

export function UpdateDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  // Each *Check again* is a new ask with `refresh`, so the node asks GitHub
  // rather than answering from what it holds.
  const [again, setAgain] = useState(0);
  const check = useAsync((s) => (open ? api.updateCheck(again > 0, s) : Promise.resolve(null)), [open, again]);
  const app = __APP_VERSION__;
  const state: UpdateState = updateState({ app, check: open ? check.data : null, loading: check.loading || check.refreshing, error: check.error });
  const words = updateWords(state);
  const checkedAt = "checkedAt" in state ? state.checkedAt : null;
  const busy = check.loading || check.refreshing;
  const checkAgain = () => setAgain((n) => n + 1);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("shell-update-dialog-title")}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={checkAgain}>
            <ICON.refresh size={12} aria-hidden className={busy ? "motion-safe:animate-spin" : undefined} />
            {t("shell-update-check-again")}
          </Button>
          <Button onClick={onClose}>{t("shell-update-dialog-close")}</Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 text-2xs">
        {/* The mark beside the two versions, as About draws them. */}
        <div className="flex items-center gap-3">
          <PlatformMark size={40} className="shrink-0" title={t("shell-about-dialog-mark", { PRODUCT })} />
          <div className="flex min-w-0 flex-col">
            <span className="text-sm font-semibold text-text">{PRODUCT}</span>
            <span className="text-text-dim">{versionLine(state)}</span>
          </div>
        </div>

        {state.kind === "failed" ? (
          <ErrorNote error={words.line} retry={checkAgain} />
        ) : (
          <div className="flex flex-col gap-1">
            <p className="max-w-measure leading-relaxed text-text">{words.line}</p>
            {words.detail && <p className="max-w-measure leading-relaxed text-text-dim">{words.detail}</p>}
          </div>
        )}

        {state.kind === "checking" && <SkeletonRows rows={2} />}

        {state.kind === "available" && (
          <>
            <div className="flex flex-wrap items-center gap-2">
              {releaseLinks(state, __APP_REPOSITORY__).map((l) => (
                <Button key={l.id} size="sm" variant={l.id === "release" ? "primary" : "ghost"} title={l.url} onClick={() => void openExternal(l.url)}>
                  <ICON.open size={12} aria-hidden />
                  {l.label}
                </Button>
              ))}
            </div>
            <p className="text-text-dim">
              {t("shell-update-download-hint")}
              {state.publishedAt !== null && (
                <>
                  {" · "}
                  {t("shell-update-published")} <RelativeTime at={state.publishedAt} />
                </>
              )}
            </p>
            {state.notes && (
              <section aria-label={t("shell-update-whats-new", { version: state.version })} className="flex flex-col gap-2">
                <h3 className="text-xs font-semibold text-text">{t("shell-update-whats-new", { version: state.version })}</h3>
                <div className="max-h-72 overflow-y-auto rounded-control border border-hairline bg-surface-2/50 px-3 py-2">
                  <Markdown text={state.notes} />
                </div>
              </section>
            )}
          </>
        )}

        {state.kind === "none" && releasesIndex(__APP_REPOSITORY__) && (
          <div>
            <Button size="sm" variant="ghost" onClick={() => void openExternal(releasesIndex(__APP_REPOSITORY__) ?? "")}>
              <ICON.open size={12} aria-hidden />
              {t("shell-update-see-releases")}
            </Button>
          </div>
        )}

        {checkedAt !== null && (
          <p className="text-text-dim">
            {t("shell-update-checked")} <RelativeTime at={checkedAt} />
          </p>
        )}
      </div>
    </Dialog>
  );
}
