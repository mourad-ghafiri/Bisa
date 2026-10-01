/**
 * The team relation, in the one place two screens can agree on it.
 *
 * This file was called `TeamDetail.tsx` and was the team detail *pane*,
 * rendered inside the Agents screen. Teams are their own screen now, the pane
 * moved to `views/Teams.tsx`, and the name outlived the component: nothing
 * here renders anything, so it is a `.ts` module named for what it holds —
 * what assignment actually does, and how a team is put on a goal.
 * `views/Teams.tsx` and `views/GoalDetail.tsx` both read it, which is the
 * whole reason it is a module and not a section of either screen.
 *
 * One member of a team is in no stored list. `GET /teams` and `GET /teams/{id}`
 * both answer with the roster **plus** the core agent, because the core agent
 * belongs to every team and is stored in none of them: a list you can edit is
 * a list you can empty. Membership is added at read time and stripped on every
 * write, so there is nothing to remove and no roster to remove it from — which
 * is why the difference between "has a member" and "has been staffed" needs
 * its own predicate rather than a length check.
 */

import { api } from "../../api";
import type { Assignee } from "../../types";
import { assigneeWire } from "./types";
import { t } from "../../i18n/l10n.mjs";

// A member is an `Assignee` — the same type a goal and a project carry, so a
// roster and an assignee list are the same bytes on the wire. What of a
// roster is stored, and who put it there, is `rosterModel.mjs`'s to say.

/**
 * The two team calls the shared client does not carry as one method: it types
 * `GET /teams/{id}` as the team alone (the route also returns the goals),
 * and has no goal-assignment method at all. Kept here rather than
 * duplicating the base-URL and error handling that `api.ts` owns.
 */
export const teamApi = {
  /** The team plus what it is carrying. */
  detail: (id: string, s?: AbortSignal) => api.team(id, s),
  /**
   * Put a team on a goal, or take every team off it with `null`.
   *
   * Assignees are a list now, so this preserves whatever else carries the
   * goal — agents and people named directly stay put. `current` is the
   * goal's existing assignees; the caller has them in hand.
   */
  assign: (goal: string, team: string | null, current: Assignee[] = [], s?: AbortSignal) => {
    const kept = current.filter((a) => !("team" in a)).map(assigneeWire);
    return api.setAssignees(goal, team ? [...kept, `team:${team}`] : kept, s);
  },
};

/** What assignment actually does — true since the relation landed. */
export const TEAM_EFFECT = t("work-team-relation-work-goes-team-s-agents-people");
