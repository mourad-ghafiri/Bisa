/**
 * The spine of the Workstreams panel's lifecycle (ide/07, ide/08): the seven
 * steps a branch takes to its base — always seven, so the spine never changes
 * length under a person — which one is in hand, and, under each step, the
 * surface that belongs to it. Under the step in hand, first, the one action
 * that is legal: `current` is the act's own step by the model's word
 * (`cta.step`), so *Merge* sits under *Merge* and never under *Checks* because
 * the checks happened to be running. Nothing else on the panel is a primary
 * button. A step merely in flight — checks running, the code host still
 * computing — wears a ring that pulses, not the accent dot: the dot means one
 * thing, here is the act.
 *
 * The steps and their words are `prLifecycleModel.lifecycle`'s; this file
 * draws them. The action is the caller's (`action`), because *Merge* is a
 * control of its own with a strategy and a confirmation, and *Commit* is a
 * link to Git › Changes rather than an act here. What sits under the action
 * (`children`) is the outcome banner, so a success or a refusal reads beside
 * the step it belongs to and nowhere else. `render(step)` is the step's own
 * surface — the pull request card under *Open pull request*, the review stage
 * under *Review* — drawn whatever the step's status, so reading down the
 * spine is reading the work in order: the review above the merge, though it
 * is optional and never holds it.
 *
 * The current step's note is never cut: a blocked reason (*a draft — mark it
 * ready on the code host*) is the sentence a person needs whole, so it wraps
 * under the label as a `ReasonLine`; the other steps' notes stay one line.
 * The connector between marks is drawn between every two steps, whatever
 * is open under them, so the rail is a stable thing to read.
 */

import type { ReactNode } from "react";
import { ICON, cn } from "../../ui";
import type { LifecycleStep } from "./prLifecycleModel.mjs";
import { ReasonLine } from "./ReasonLine";
import { t } from "../../i18n/l10n.mjs";

function Mark({ status }: { status: LifecycleStep["status"] }) {
  const base = "flex h-4 w-4 shrink-0 items-center justify-center rounded-full border text-3xs";
  switch (status) {
    case "done":
      return (
        <span aria-hidden className={cn(base, "border-ok/60 bg-ok/15 text-ok")}>
          <ICON.check size={10} />
        </span>
      );
    case "current":
      return (
        <span aria-hidden className={cn(base, "border-accent bg-accent-soft")}>
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
        </span>
      );
    case "waiting":
      return (
        <span aria-hidden className={cn(base, "border-accent/60")}>
          <span className="h-1.5 w-1.5 rounded-full border border-accent opacity-70 motion-safe:animate-pulse" />
        </span>
      );
    case "blocked":
      return (
        <span aria-hidden className={cn(base, "border-danger/60 bg-danger-soft text-danger")}>
          <ICON.warn size={10} />
        </span>
      );
    default:
      return <span aria-hidden className={cn(base, "border-border")} />;
  }
}

export function LifecycleStepper({
  steps,
  current,
  note,
  action,
  render,
  children,
  footer,
}: {
  steps: readonly LifecycleStep[];
  current: string | null;
  /** A sentence instead of steps — a copy, a closed workstream. */
  note: string | null;
  /** The one legal act, drawn under the step in hand — the act's own step, by the model's word. */
  action?: ReactNode;
  /** A step's own surface, drawn under its row whatever its status. */
  render?: (step: LifecycleStep) => ReactNode;
  /** The outcome banner, under the action. */
  children?: ReactNode;
  /** Under the spine: when the code host was last read, and the door to read it now. */
  footer?: ReactNode;
}) {
  if (steps.length === 0) {
    return note ? <ReasonLine>{note}</ReasonLine> : null;
  }
  return (
    <section aria-label={t("work-lifecycle-stepper-where-branch")} className="flex flex-col gap-1">
      <ol className="flex flex-col">
        {steps.map((s, i) => {
          const now = s.id === current;
          const surface = render?.(s) ?? null;
          const cautioned = s.cautions.length > 0 && s.status !== "blocked";
          const under = (now && (action || children)) || surface;
          const last = i === steps.length - 1;
          // The note: one line beside a step that is not the one in hand;
          // whole, under the label, for the one that is — that is the
          // sentence a person needs.
          const noteTone = s.status === "blocked" ? "danger" : cautioned ? "warn" : "dim";
          return (
            <li key={s.id} className="flex">
              <span className="flex flex-col items-center">
                <span className="flex h-6 items-center">
                  <Mark status={s.status} />
                </span>
                {!last && <span aria-hidden className={cn("w-px flex-1", s.status === "done" ? "bg-ok/40" : "bg-border")} />}
              </span>
              <div className={cn("flex min-w-0 flex-1 flex-col pl-2", !last && "pb-1")}>
                <div className={cn("flex min-h-6 items-center gap-2 text-2xs", s.status === "todo" && "text-text-dim")} aria-current={now ? "step" : undefined}>
                  <span className={cn("shrink-0", now && "font-medium text-text", s.status === "blocked" && "text-danger")}>{s.label}</span>
                  {s.note && !now && (
                    <span className={cn("min-w-0 truncate", noteTone === "danger" ? "text-danger" : noteTone === "warn" ? "text-warn" : "text-text-dim")} title={s.note}>
                      {t("work-lifecycle-stepper-dot-note", { note: s.note })}
                    </span>
                  )}
                </div>
                {s.note && now && <ReasonLine tone={noteTone}>{s.note}</ReasonLine>}
                {under && (
                  <div className="mt-1 mb-1 flex flex-col gap-2">
                    {now && action}
                    {now && children}
                    {surface}
                  </div>
                )}
              </div>
            </li>
          );
        })}
      </ol>
      {note && <ReasonLine>{note}</ReasonLine>}
      {current === null && children && <div>{children}</div>}
      {footer}
    </section>
  );
}
