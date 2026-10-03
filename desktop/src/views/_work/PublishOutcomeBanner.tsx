/**
 * What a push, a pull request or a merge did — one banner per outcome, never
 * collapsed into "ok". Shared by the workstream panel
 * and the Git tab's sync bar, so the two never disagree about a refusal.
 *
 * The gate case says plainly that nothing was pushed and points at the inbox,
 * because the person who has to approve it may not be the person who clicked.
 * A refusal is read from the node's `code` (`publishOutcome.mjs`), never from
 * the status, and the ones a person can act on lead to the control.
 */
import type { ReactNode } from "react";
import { ApiError } from "../../api";
import { navigate } from "../../router";
import { Button, CopyText, ExternalLink, ICON, failureText } from "../../ui";
import { refusalOf } from "./publishOutcome.mjs";
import type { RefusalKind } from "./publishOutcome.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** What the last publishing attempt actually did. Never collapsed into "ok". */
export type Publish =
  | { kind: "none" }
  | { kind: "pushed" }
  | { kind: "pr"; number: number; url: string }
  /** 202: the gate is open and the branch is still here; `gate` is its id when the answer named one, else null — never a blank. */
  | { kind: "gate"; gate: string | null; what: "push" | "pr" }
  /** A refusal, by the node's `code` — `manual` is one of them, never the guess. */
  | { kind: RefusalKind; detail: string }
  /** Approved at the gate, and it did not go out (ide/08): `what` the act as the gate asked it, `reason` the node's words; the act is offered again under its step. */
  | { kind: "failed"; what: string; reason: string };

/** The banner for a failed request: what the node named, or its sentence. */
export function refused(e: unknown): Publish {
  const message = failureText("work", "publish-outcome-banner-failed", e);
  return e instanceof ApiError ? refusalOf({ status: e.status, code: e.code, message }) : { kind: "error", detail: message };
}


/**
 * One banner per publishing outcome. The gate case says plainly that nothing
 * was pushed and points at the inbox, because the person who has to approve
 * it may not be the person who clicked.
 */
export function PublishBanner({
  publish,
  goal,
  onDismiss,
  onOpenAbout,
}: {
  publish: Publish;
  /** The goal whose inbox holds the gate; a goal-less workstream has none to point at. */
  goal: string | null | undefined;
  onDismiss: () => void;
  /** Open About's Settings view, where the project's publishing policy is set. */
  onOpenAbout?: (view: "checkout" | "settings") => void;
}) {
  if (publish.kind === "none") return null;

  const changePolicy = onOpenAbout && (
    <Button size="sm" onClick={() => onOpenAbout("settings")}>
      <ICON.settings size={12} aria-hidden />{t("work-publish-outcome-banner-change-policy")}</Button>
  );

  const frame = (tone: string, children: ReactNode) => (
    <div className={`rounded-control border px-3 py-2 text-2xs ${tone}`}>
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">{children}</div>
        <button
          type="button"
          onClick={onDismiss}
          aria-label={t("work-publish-outcome-banner-dismiss")}
          className="anim shrink-0 rounded px-1 text-text-dim hover:text-text"
        >
          <ICON.close size={12} aria-hidden />
        </button>
      </div>
    </div>
  );

  switch (publish.kind) {
    case "gate":
      // The one banner that keeps the accent: a decision now waits on someone,
      // and its door — *Decide in Inbox* — is the act the banner asks for.
      return frame(
        "border-accent/40 bg-accent-soft text-accent-ink",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-waiting-approval-nothing-has-left-machine")}</p>
          <p className="mt-0.5 opacity-90">{t("work-publish-outcome-banner-queued-behind-decision", { what: publish.what === "pr" ? t("work-publish-outcome-banner-pull-request") : t("work-publish-outcome-banner-push") })}</p>
          <div className="mt-1.5 flex flex-wrap items-center gap-2">
            {goal && (
              <Button
                size="sm"
                variant="primary"
                onClick={() => navigate({ name: "inbox" }, { item: goal })}
              >{t("work-publish-outcome-banner-decide-inbox")}</Button>
            )}
            {publish.gate !== null && <CopyText value={publish.gate} label={t("work-publish-outcome-banner-gate", { tail: publish.gate.slice(-6) })} />}
          </div>
        </>,
      );
    case "pushed":
      return frame(
        "border-transparent bg-ok-soft text-ok",
        <p className="font-semibold">{t("work-publish-outcome-banner-pushed-origin")}</p>,
      );
    case "pr":
      return frame(
        "border-transparent bg-ok-soft text-ok",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-pull-request-open", { number: publish.number })}</p>
          <ExternalLink href={publish.url} className="mt-0.5 inline-block underline underline-offset-2">
            {publish.url}
          </ExternalLink>
        </>,
      );
    case "manual":
      return frame(
        "border-border bg-surface-2/70 text-text",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-project-does-not-publish-from-here")}</p>
          <p className="mt-0.5 text-text-dim">{rich("work-publish-outcome-banner-manual-policy-blurb")}</p>
          <p className="mt-1 text-text-dim">{publish.detail}</p>
          {changePolicy && <div className="mt-1.5">{changePolicy}</div>}
        </>,
      );
    case "no_goal":
      return frame(
        "border-border bg-surface-2/70 text-text",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-nobody-ask")}</p>
          <p className="mt-0.5 text-text-dim">{rich("work-publish-outcome-banner-gated-no-goal-blurb")}</p>
          <p className="mt-1 text-text-dim">{publish.detail}</p>
          {changePolicy && <div className="mt-1.5">{changePolicy}</div>}
        </>,
      );
    case "not_ready":
      return frame(
        "border-border bg-surface-2/70 text-text",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-nothing-publish-yet")}</p>
          <p className="mt-0.5 text-text-dim">{t("work-publish-outcome-banner-no-commits-beyond-base-blurb")}</p>
          <p className="mt-1 text-text-dim">{publish.detail}</p>
        </>,
      );
    case "state":
      return frame(
        "border-border bg-surface-2/70 text-text",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-state-does-not-allow")}</p>
          <p className="mt-0.5 text-text-dim">{publish.detail}</p>
        </>,
      );
    case "failed":
      // The gate was approved and the act then failed in the node — nobody
      // was waiting on the call, so the fact arrives on the bus and is said
      // here, under the step that still offers the act.
      return frame(
        "border-transparent bg-danger-soft text-danger",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-approved-did-not-go-out")}</p>
          {publish.what && <p className="mt-0.5 opacity-90">{t("work-publish-outcome-banner-could-not", { what: publish.what })}</p>}
          {publish.reason && <p className="mt-0.5 opacity-90">{publish.reason}</p>}
          <p className="mt-1 opacity-90">{t("work-publish-outcome-banner-act-offered-again")}</p>
        </>,
      );
    case "declined":
      return frame(
        "border-transparent bg-danger-soft text-danger",
        <>
          <p className="font-semibold">{t("work-publish-outcome-banner-publish-gate-declined-nothing-left-machine")}</p>
          <p className="mt-0.5 opacity-90">{publish.detail}</p>
        </>,
      );
    case "error":
      return frame(
        "border-transparent bg-danger-soft text-danger",
        <p>{publish.detail}</p>,
      );
  }
}
