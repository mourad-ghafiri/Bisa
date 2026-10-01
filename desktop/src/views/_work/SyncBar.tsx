/**
 * The Git tab's sync bar: where the checkout's branch stands against its
 * upstream, **Push** (the one verb that publishes — through the project's
 * Publish gate, with the same banner the workstream panel shows) and a `⋮`
 * holding the rest (`syncMenu`): *Refresh* first, **Pull** in the three
 * modes — the one-click default is the `git.pull` setting, each confirmed —
 * and **Fetch** (safe) last; the bar binds each item by its id, never its
 * place. An operation half-done is the Resolve card's above — the
 * bar's line points at it and holds no verb of its own.
 *
 * What it says comes from `syncModel.mjs`: a pull that cannot fast-forward
 * offers the other modes, a conflict lists the files (each a link that opens
 * the conflict document below) and points at the card. Nothing here guesses
 * from a status code.
 */
import { useState, type ReactNode } from "react";
import { useEngineEvents } from "../../bus";
import type { GitStatusInfo, PullMode } from "../../types";
import { Button, ConfirmDialog, ICON, Menu, Tooltip, WorkingDot, type MenuItem } from "../../ui";
import { SafetyNote } from "./SafetyNote";
import { VERB } from "./gitWords.mjs";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import * as ops from "./gitOps";
import { patchSession, useGitSession } from "./gitPanelStore";
import { PublishBanner, type Publish } from "./PublishOutcomeBanner";
import { publishFailure } from "./publishOutcome.mjs";
import { PushWithLeaseDialog } from "./PushWithLeaseDialog";
import { PULL_LABEL, PULL_MEANING, pullBannerWords, pullChoice, syncControls, syncLine, syncMenu, syncState } from "./syncModel.mjs";
import type { PullBanner } from "./syncModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function SyncBar({
  pid,
  wid,
  goal,
  status,
  defaultBranch,
  busy: outerBusy,
  onChanged,
  onRefresh,
  onOpenAbout,
  onSelectPath,
}: {
  pid: string;
  wid: string;
  goal: string | null;
  status: GitStatusInfo;
  /** The project's default branch — never force-pushed. */
  defaultBranch: string | null;
  /** The panel below is busy with its own operation. */
  busy?: boolean;
  /** The tree or the record changed: the caller reloads what it shows. */
  onChanged: () => void;
  /** The caller's own reload — the Refresh button. */
  onRefresh: () => void;
  /** About — Checkout for the remote, Settings for the project's publishing policy. */
  onOpenAbout?: (view: "checkout" | "settings") => void;
  /** Select a file in the changes list — a conflicted one opens the conflict document. */
  onSelectPath?: (path: string) => void;
}) {
  const { resolved } = useResolvedSettings(pid);
  const defaultMode = pullChoice(resolved?.find((r) => r.key === "git.pull")?.value);
  const state = syncState(status);
  const controls = syncControls(state);
  // The bar's work and its two banners live in the checkout's session
  // (`gitPanelStore`), so a pull that stopped on a conflict is still reported
  // after a tab switch; the two confirmations are this mount's alone.
  const scope = rootKey("workstream", wid);
  const session = useGitSession(scope);
  const busy = session.busy === "fetch" || session.busy === "pull" || session.busy === "push" || session.busy === "push_lease" ? session.busy : null;
  const pullBanner = session.pullBanner;
  const publish = session.publish;
  const [confirmPull, setConfirmPull] = useState<PullMode | null>(null);
  const [forcePush, setForcePush] = useState(false);
  const anyBusy = session.busy !== null || outerBusy === true;

  const fetch = () => void ops.fetch(scope, wid).then((ran) => ran && onChanged());
  const pull = (mode: PullMode) => {
    setConfirmPull(null);
    void ops.pull(scope, wid, mode).then((ran) => ran && onChanged());
  };
  const push = () => void ops.push(scope, wid).then((ran) => ran && onChanged());
  const setPullBanner = (b: null) => patchSession(scope, { pullBanner: b });
  const setPublish = (p: Publish) => patchSession(scope, { publish: p });

  // A push approved at the gate that then did not go out (ide/08) is said
  // here too — the same banner the lifecycle shows, so the two never disagree.
  useEngineEvents((e) => {
    const failed = publishFailure(e.payload, wid);
    if (!failed) return;
    setPublish(failed);
    onChanged();
  });

  // The `⋮`: the model says the items and when each is off; this binds them.
  // The force push is the Branches view's lease push, asked the same way.
  const menu: MenuItem[] = syncMenu(state, defaultMode, defaultBranch).map((item) => ({
    label: item.disabled && item.reason ? `${item.label} — ${item.reason}` : item.label,
    icon: item.icon ? (ICON as Record<string, typeof ICON.file>)[item.icon] : undefined,
    disabled: item.disabled || anyBusy,
    separatorBefore: item.separatorBefore,
    danger: item.danger,
    onSelect: () => {
      if (item.id === "fetch") fetch();
      else if (item.id === "refresh") onRefresh();
      else if (item.id === "force_push") setForcePush(true);
      else if (item.id.startsWith("pull:")) setConfirmPull(item.id.slice(5) as PullMode);
    },
  }));

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {/* The sync bar's own dot is for its own work — fetch / pull / push /
            abort. A commit or a suggest shows its dot beside those buttons, not
            up here. */}
        {busy !== null && <WorkingDot title={`${busy}…`} />}
        <span className="min-w-0 truncate text-2xs text-text-dim" title={status.upstream ?? undefined}>
          {status.branch && <span className="font-mono text-text">{status.branch}</span>}
          {status.branch && " · "}
          {syncLine(state)}
        </span>
        <span className="flex-1" />
        {controls.setOrigin && onOpenAbout && (
          <Button size="sm" variant="ghost" onClick={() => onOpenAbout("checkout")}>
            <ICON.settings size={12} aria-hidden />{t("work-sync-bar-set-origin")}</Button>
        )}
        {controls.push && (
          <Tooltip label={state.kind === "no_upstream" ? t("work-sync-bar-publish-branch-origin-through-project-s") : t("work-sync-bar-push-through-project-s-publishing-policy")}>
            <span className="inline-flex">
              <Button size="sm" variant="primary" disabled={anyBusy} onClick={push}>
                <ICON.send size={12} aria-hidden />
                {busy === "push" ? t("work-sync-bar-pushing") : state.kind === "no_upstream" ? t("work-sync-bar-push-branch") : VERB.push}
              </Button>
            </span>
          </Tooltip>
        )}
        {state.kind !== "not_git" && (
          <Menu
            label={t("work-sync-bar-fetch-pull-refresh-force-push")}
            items={menu}
            trigger={
              <span className="anim flex h-7 w-7 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text" aria-label={t("work-sync-bar-fetch-pull-refresh-force-push")}>
                <ICON.more size={14} aria-hidden />
              </span>
            }
          />
        )}
      </div>

      {pullBanner && (
        <PullOutcomeBanner banner={pullBanner} onDismiss={() => setPullBanner(null)} onPull={(mode) => setConfirmPull(mode)} onSelectPath={onSelectPath} />
      )}
      <PublishBanner publish={publish} goal={goal} onDismiss={() => setPublish({ kind: "none" })} onOpenAbout={onOpenAbout} />

      <ConfirmDialog
        open={confirmPull !== null}
        onClose={() => setConfirmPull(null)}
        onConfirm={() => confirmPull && pull(confirmPull)}
        title={confirmPull ? t("work-sync-bar-from", { confirmPull: PULL_LABEL[confirmPull], upstream: status.upstream ?? "origin" }) : ""}
        body={
          <div className="flex flex-col gap-2">
            <p>{confirmPull ? PULL_MEANING[confirmPull] : ""}</p>
            <SafetyNote kind="tree" />
          </div>
        }
        confirmLabel={confirmPull ? PULL_LABEL[confirmPull] : VERB.pull}
      />
      <PushWithLeaseDialog open={forcePush} onClose={() => setForcePush(false)} wid={wid} branch={status.branch ?? null} />
    </div>
  );
}

/** One banner per pull outcome — moved, current, refused, conflicted. */
function PullOutcomeBanner({
  banner,
  onDismiss,
  onPull,
  onSelectPath,
}: {
  banner: PullBanner;
  onDismiss: () => void;
  onPull: (mode: PullMode) => void;
  onSelectPath?: (path: string) => void;
}) {
  const frame = (tone: string, children: ReactNode) => (
    <div className={`rounded-control border px-3 py-2 text-2xs ${tone}`}>
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">{children}</div>
        <button type="button" onClick={onDismiss} aria-label={t("work-publish-outcome-banner-dismiss")} className="anim shrink-0 rounded px-1 text-text-dim hover:text-text">
          <ICON.close size={12} aria-hidden />
        </button>
      </div>
    </div>
  );
  // The sentences are the model's (`pullBannerWords`); the buttons are the bar's.
  const words = pullBannerWords(banner);
  switch (banner.kind) {
    case "moved":
      return frame("border-transparent bg-ok-soft text-ok", <p className="font-semibold">{words.title}</p>);
    case "current":
      return frame("border-border bg-surface-2 text-text", <p>{words.title}</p>);
    case "not_fast_forward":
      return frame(
        "border-border bg-surface-2 text-text",
        <>
          <p className="font-semibold">{words.title}</p>
          <p className="mt-0.5 text-text-dim">{words.body}</p>
          <div className="mt-1.5 flex flex-wrap gap-2">
            <Button size="sm" variant="primary" onClick={() => onPull("rebase")}>
              {PULL_LABEL.rebase}
            </Button>
            <Button size="sm" onClick={() => onPull("merge")}>
              {PULL_LABEL.merge}
            </Button>
          </div>
        </>,
      );
    case "conflict":
      return frame(
        "border-warn/40 bg-warn-soft text-text",
        <>
          <p className="font-semibold">{words.title}</p>
          <p className="mt-0.5 text-text-dim">{words.body}</p>
          <div className="mt-1.5 flex flex-wrap gap-2">
            {banner.paths.length > 0 && (
              <Button size="sm" variant="primary" onClick={() => onSelectPath?.(banner.paths[0]!)}>{t("work-sync-bar-resolve-first")}<ICON.forward size={11} aria-hidden />
              </Button>
            )}
          </div>
        </>,
      );
    case "in_progress":
      return frame(
        "border-border bg-surface-2 text-text",
        <>
          <p className="font-semibold">{words.title}</p>
          <p className="mt-0.5 text-text-dim">{words.body}</p>
        </>,
      );
    case "error":
      return frame("border-transparent bg-danger-soft text-danger", <p>{words.title}</p>);
  }
}
