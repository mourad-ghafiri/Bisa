/**
 * Where the Workflow Agent's proposal is shown.
 *
 * A proposed workflow arrives as a `NeedsAction` whose subject starts
 * `adopt:` and which carries the `proposal` (every step, the inputs). It used
 * to render only in the goal's *Your move* band, while the goal opened on a
 * card that pointed elsewhere — so the plan was never the first thing a person
 * saw. Now the proposal is the goal's landing: this module is the one rule for
 * which action is that proposal, so the Progress tab can show it and the band
 * can leave it out — shown once, not twice.
 */

/** The subject an adoption's approval gate wears: `adopt:<workflow>`. */
export const ADOPT_SUBJECT = "adopt:";

/** Whether a gate's or an ask's subject names an adoption — the one place the prefix is read. */
export function namesAdoption(subject) {
  return typeof subject === "string" && subject.startsWith(ADOPT_SUBJECT);
}

/** Whether an action is the adopt-a-proposal card (a plan waiting to be reviewed). */
export function isAdoptProposal(action) {
  return !!action && namesAdoption(action.subject) && !!action.proposal;
}

/** The one adopt proposal among the goal's pending actions, or null. */
export function adoptAction(actions) {
  return (actions ?? []).find(isAdoptProposal) ?? null;
}

/** The actions the *Your move* band shows — everything but the proposal, which the Progress tab hosts. */
export function bandActions(actions) {
  return (actions ?? []).filter((a) => !isAdoptProposal(a));
}
