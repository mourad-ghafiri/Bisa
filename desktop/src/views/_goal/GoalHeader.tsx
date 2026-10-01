/**
 * The goal's header: what it is and where it
 * stands — title, statement, who holds the ball, the step count and the
 * workflow's name — one primary action, the Browser door (ide/18: the
 * embedded browser beside the goal, its tabs and whether an agent is
 * browsing) and the details toggle. The step chips stay on the Goals list;
 * here the Progress tab below is the run, so the header does not draw it
 * twice. Everything else is behind the menu or in the Details pane; the
 * header ranks, it does not list.
 *
 * A goal whose design begins on events listens while it is open: the line
 * under the title says so — *Listening — every Monday 09:00 · next Mon
 * 09:00*, or *Paused: a run it started failed* — with its one verb beside
 * it, *Stop listening* or *Listen again* (`runControl.listenVerb`). Its
 * listeners are read while it listens, again whenever it starts a run.
 */
import { useState, type ReactNode } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import { Button, Chip, ICON, Menu, PageHeader, WorkingDot, cn, useToast, type MenuItem } from "../../ui";
import type { GoalView, ListenerView } from "../../types";
import { HolderBadge } from "../_goals/HolderBadge";
import { attempt, useAsync } from "../_work/useAsync";
import { HeldSignals } from "../_workflow/HeldSignals";
import { mintedBy } from "../_workflow/hookSecretsModel.mjs";
import { showHookSecrets } from "../_workflow/hookSecretsStore";
import { goalListening } from "../_workflow/listeningModel.mjs";
import { headerWords } from "./goalPageModel.mjs";
import { progressCount } from "./progressModel.mjs";
import { listenVerb } from "./runControl.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The goal's listening, in words, and its verb; nothing for a goal that does not listen. */
function ListeningLine({ view, onChanged }: { view: GoalView; onChanged?: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const { goal } = view;
  const listening = view.listening ?? goal.listening ?? null;
  const since = listening?.since ?? null;
  const heard = useAsync<ListenerView[]>((s) => (since === null ? Promise.resolve([]) : api.goalListeners(goal.id, s)), [goal.id, since, goal.run ?? null]);
  const line = goalListening(listening, heard.data ?? []);
  const verb = listenVerb({ ...goal, listening });
  if (!line) return null;
  const act = async () => {
    if (!verb) return;
    setBusy(true);
    if (verb.id === "stop") {
      await attempt(() => api.stopListeningGoal(goal.id), toast.error, () => {
        toast.ok(t("goal-goal-header-stopped-listening"));
        onChanged?.();
      });
    } else {
      await attempt(() => api.listenGoal(goal.id, { inputs: listening?.inputs ?? {} }), toast.error, (armed) => {
        toast.ok(t("goal-goal-header-listening-again"));
        // A public hook's secret minted by listening again is shown once, by the dialog at the app's root.
        showHookSecrets(mintedBy(armed));
        onChanged?.();
      });
    }
    setBusy(false);
  };
  return (
    <span className="flex min-w-0 items-center gap-2" data-goal-listening={line.paused ? "paused" : "on"}>
      <span className={cn("inline-flex min-w-0 items-center gap-1 text-xs", line.tone === "warn" ? "text-warn" : "text-accent-ink")}>
        <ICON.signal size={12} aria-hidden className="shrink-0" />
        <span className="truncate">{line.words}</span>
      </span>
      {verb && (
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void act()}>
          {verb.id === "stop" ? <ICON.close size={12} aria-hidden /> : <ICON.restart size={12} aria-hidden />}
          {verb.label}
        </Button>
      )}
      {/* What its events hold for a person to read, while it listens. */}
      <HeldSignals host={{ goal: goal.id }} listening onChanged={heard.reload} />
    </span>
  );
}

export function GoalHeader({
  view,
  working,
  primary,
  door,
  detailsOpen,
  onToggleDetails,
  menu,
  onChanged,
}: {
  view: GoalView;
  working: boolean;
  /** The one primary action, when there is one: start, or adopt and start. */
  primary?: ReactNode;
  /** The Browser door, from the page that knows the goal's id. */
  door?: ReactNode;
  detailsOpen: boolean;
  onToggleDetails: () => void;
  menu: MenuItem[];
  /** After the listening line's verb: the page refetches. */
  onChanged?: () => void;
}) {
  const { goal, strip, holder } = view;
  const count = progressCount(strip);
  // The goal by its words — the title, else the sentence captured — never by
  // its id; a long sentence folds to two lines, whole in the tooltip.
  const words = headerWords(goal);
  return (
    <div className="shrink-0 border-b border-border">
      <PageHeader
        className="items-start px-4 pt-3 pb-1"
        back={{ label: t("goal-goal-header-back-to-goals"), onClick: () => navigate({ name: "goals" }) }}
        title={<span className="line-clamp-2 whitespace-normal" title={words.title}>{words.title}</span>}
        subtitle={words.subtitle ? <span className="line-clamp-2">{words.subtitle}</span> : undefined}
        meta={
          <>
            <HolderBadge holder={holder} strip={strip} />
            {goal.archived && (
              <Chip tone="quiet" icon={ICON.archive} title={t("goal-goal-header-put-away-hidden-from-goals-list")}>{t("goal-goal-header-archived")}</Chip>
            )}
            {working && <WorkingDot title={holder === "design" ? t("goal-designing-card-workflow-agent-designing") : t("goal-goal-header-agent-writing")} />}
          </>
        }
        actions={
          <>
            {primary}
            {door}
            <Button
              size="icon"
              aria-label={detailsOpen ? t("goal-goal-header-hide-details") : t("goal-goal-header-show-details")}
              aria-pressed={detailsOpen}
              className={detailsOpen ? "border-accent/50 text-accent-ink" : ""}
              onClick={onToggleDetails}
            >
              <ICON.info size={13} aria-hidden />
            </Button>
            <Menu
              label={t("goal-goal-header-actions", { title: goal.title ?? t("goal-goal-header-goal") })}
              trigger={
                <span className="anim inline-flex h-7 w-7 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text">
                  <ICON.more size={14} aria-hidden />
                </span>
              }
              items={menu}
            />
          </>
        }
      />
      <div className="flex min-w-0 flex-wrap items-center gap-3 px-4 pb-3">
        <span className="tnum text-xs text-text-dim">{count.label}</span>
        {strip.workflow_name && <span className="truncate text-xs text-text-dim">· {strip.workflow_name}</span>}
        <ListeningLine view={view} onChanged={onChanged} />
      </div>
    </div>
  );
}
