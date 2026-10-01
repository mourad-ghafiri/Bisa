/**
 * Goals: one flat, live list of run strips.
 *
 * A goal is its workflow, so each card *is* the run — every step as a chip
 * with its state (folded to one line when the workflow is long), the current
 * step named, the progress counted, who and what it carries, and the move
 * yours to make inline when it is yours. There are no sections and no lifecycle buckets, on
 * purpose: filters narrow (who holds the ball, which workflow, tags, text)
 * and the list orders itself by last activity, so what moved is at the top
 * and nothing is filed under a stage name. The facts live in
 * `_goals/goalStripModel.mjs` with tests; this file is paint.
 *
 * The filters of the bar live in the address; the tags picked and where the
 * list was scrolled are the screen's memory (`shell/viewMemoryStore`), so
 * the list comes back as it was left — after a goal was opened, and after a
 * restart.
 */

import { useMemo, useRef } from "react";
import { useSearchParams, setSearch } from "../router";
import type { GoalRow } from "../types";
import {
  Button,
  EmptyState,
  ICON,
  NO_TAG_FILTER,
  SkeletonRows,
  TagFilterBar,
  parseTagFilter,
  passesTagFilter,
  type TagFilterState,
} from "../ui";
import { useEffect, useState } from "react";
import { NEW_GOAL, onDoor } from "../shell/shortcuts";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { NewGoalDialog } from "./_work/NewGoalDialog";
import { GoalCard } from "./_goals/GoalCard";
import { GoalsFilters } from "./_goals/GoalsFilters";
import {
  matchesFilters,
  parseFilters,
  serializeFilters,
  sortByActivity,
  workflowFacets,
  type Filters,
} from "./_goals/goalStripModel.mjs";
import { t } from "../i18n/l10n.mjs";

const tagsOf = (row: GoalRow): string[] => row.tags ?? [];

/** Where the screen keeps its memory. */
const PLACE = placeOf({ name: "goals" });

export default function Goals() {
  const ws = useWorkspace();
  const params = useSearchParams();
  const filters = useMemo(() => parseFilters(params), [params]);
  const [tagFilter, setTagFilter] = useViewState<TagFilterState>(PLACE, "tags", NO_TAG_FILTER, parseTagFilter);
  const [creating, setCreating] = useState(false);
  useEffect(() => onDoor(NEW_GOAL, () => setCreating(true)), []);
  // The list is drawn once the workspace is read and has a goal: what is on
  // screen changes then, and the scroll is put back then.
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, `${PLACE}#${ws.ready && ws.goals.length > 0 ? "list" : "reading"}`, PLACE);

  const setFilters = (next: Filters) => {
    const s = serializeFilters(next);
    setSearch({ holder: s.holder ?? null, workflow: s.workflow ?? null, q: s.q ?? null, archived: s.archived ?? null });
  };

  // The goals put away join the list only when the switch says so.
  const rows = useMemo(() => (filters.archived ? [...ws.goals, ...ws.archivedGoals] : ws.goals), [ws.goals, ws.archivedGoals, filters.archived]);
  const facets = useMemo(() => workflowFacets(rows), [rows]);
  const shown = useMemo(
    () =>
      sortByActivity(
        rows.filter((r) => matchesFilters(r, filters) && passesTagFilter(tagsOf(r), tagFilter)),
      ),
    [rows, filters, tagFilter],
  );

  const dialog = (
    <NewGoalDialog open={creating} onClose={() => setCreating(false)} onCreated={ws.refresh} />
  );

  if (!ws.ready) {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        <SkeletonRows rows={10} className="max-w-3xl" />
      </div>
    );
  }

  if (ws.goals.length === 0) {
    return (
      <div className="min-h-0 flex-1 overflow-y-auto p-6">
        <EmptyState
          icon={ICON.goal}
          title={t("screens-goals-nothing-flight-yet")}
          hint={t("screens-goals-say-what-want-make-real-workflow")}
          action={
            <Button variant="primary" onClick={() => setCreating(true)}>{t("screens-goals-new-goal")}</Button>
          }
        />
        {dialog}
      </div>
    );
  }

  return (
    <div ref={root} className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-6 py-2">
        <GoalsFilters filters={filters} facets={facets} onChange={setFilters} />
        <span className="tnum text-2xs text-text-dim">{t("screens-goals-words", { shown: shown.length, rows: rows.length })}</span>
        <Button size="sm" variant="primary" className="ml-auto" onClick={() => setCreating(true)}>{t("screens-goals-new-goal")}</Button>
      </div>

      <div data-scroll-keep="list" className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        <div className="min-w-0">
          <TagFilterBar
            items={rows}
            tagsOf={tagsOf}
            value={tagFilter}
            onChange={setTagFilter}
            className="mb-3"
          />
          {shown.length === 0 ? (
            <EmptyState
              icon={ICON.filter}
              title={filters.holder === "you" ? t("screens-goals-nothing-waits") : t("screens-agents-nothing-matches")}
              hint={t("screens-goals-no-goal-matches-filters-have")}
              action={
                <Button
                  variant="ghost"
                  onClick={() => {
                    setFilters({});
                    setTagFilter(NO_TAG_FILTER);
                  }}
                >{t("screens-agents-clear-filters")}</Button>
              }
            />
          ) : (
            <div className="flex flex-col">
              {shown.map((r) => (
                <GoalCard
                  key={r.id}
                  row={r}
                  working={(ws.working[r.id]?.length ?? 0) > 0}
                  projects={ws.projects}
                  inbox={ws.inbox}
                  onChanged={ws.refresh}
                />
              ))}
            </div>
          )}
        </div>
      </div>
      {dialog}
    </div>
  );
}
