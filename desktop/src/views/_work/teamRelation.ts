/**
 * The team relation, in the one place two screens can agree on it.
 *
 * This file was called `TeamDetail.tsx` and was the team detail *pane*,
 * rendered inside the Agents screen. Teams are their own screen now, the pane
 * moved to `views/Teams.tsx`, and the name outlived the component: nothing
 * here renders anything, so it is a `.ts` module named for what it holds —
 * what assignment actually does, and the single read of one team.
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
import { t } from "../../i18n/l10n.mjs";

// A member is an `Assignee` — the same type a goal and a project carry, so a
// roster and an assignee list are the same bytes on the wire. What of a
// roster is stored, and who put it there, is `rosterModel.mjs`'s to say.

/**
 * The team call two screens share: `GET /teams/{id}` — the team, and the
 * goals it carries — asked when the list does not hold an id, so the node
 * says whether the team is gone or its record cannot be read.
 */
export const teamApi = {
  detail: (id: string, s?: AbortSignal) => api.team(id, s),
};

/** What assignment actually does — true since the relation landed. */
export const TEAM_EFFECT = t("work-team-relation-work-goes-team-s-agents-people");
