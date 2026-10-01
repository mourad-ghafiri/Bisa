/**
 * How a rail row is treated, as facts (ide/07 §Why switching is free): the
 * wash it sits on, the pill at its left edge, the ink its name is set in,
 * whether it is muted — decided here from what the row *is* (its kind,
 * whether it is the current root, what it wants of the person, whether it
 * is put away or missing) so `RailRow` maps each word to classes once and a
 * row component never names a colour. Plain `.mjs`: `node --test` reads the
 * rule with no DOM.
 *
 * The rules: the current root wears the current wash, the accent pill and
 * the current ink whatever else it wants — where you are outranks what is
 * asking; otherwise a row waiting on you wears the accent pill, a failed
 * one the danger pill; a project's name is set strong, a workstream's plain,
 * a shell's or a session's dim unless it wants attention; a row put away or
 * without its folder is muted.
 */

/** A row's pill: what wants the person, beyond where they are. */
export const ATTENTIONS = Object.freeze(["none", "accent", "danger"]);

/**
 * @param {{kind: "group" | "goal" | "project" | "workstream" | "terminal" | "agent", current?: boolean, attention?: "none" | "accent" | "danger", archived?: boolean, exists?: boolean}} row
 * @returns {{wash: "current" | "rest", pill: "none" | "accent" | "danger", ink: "current" | "strong" | "plain" | "dim", muted: boolean}}
 */
export function rowTreatment(row) {
  const current = !!row.current;
  const attention = ATTENTIONS.includes(row.attention) ? row.attention : "none";
  const pill = current ? "accent" : attention;
  const ink = current ? "current" : inkOf(row.kind, attention);
  return {
    wash: current ? "current" : "rest",
    pill,
    ink,
    muted: !!row.archived || row.exists === false,
  };
}

function inkOf(kind, attention) {
  if (kind === "project") return "strong";
  if (kind === "workstream") return "plain";
  if (kind === "terminal" || kind === "agent") return attention === "none" ? "dim" : "plain";
  return "dim";
}

/** The pill's offset from the row's left edge, in pixels — `RailRow`'s `left-0.5`. */
export const PILL_LEFT_PX = 2;

/**
 * How far the row's rounded corner reaches in at the pill's left edge, in
 * pixels — where the arc of radius `radiusPx` crosses `x = PILL_LEFT_PX`:
 * `r − √(r² − (r − x)²)`. Nothing when the radius is within the offset.
 * @param {number} radiusPx the family's `--radius-control`
 */
export function pillClearance(radiusPx) {
  const r = Number(radiusPx);
  if (!(r > PILL_LEFT_PX)) return 0;
  const dx = r - PILL_LEFT_PX;
  return r - Math.sqrt(r * r - dx * dx);
}

/**
 * The pill's inset from the row's top and bottom — half the control radius,
 * what `RailRow` draws as `calc(var(--radius-control) / 2)`. The test holds
 * it above `pillClearance` for every family's radius, so a pill never pokes
 * past the corner.
 * @param {number} radiusPx
 */
export function pillInset(radiusPx) {
  return Number(radiusPx) / 2;
}

/** The space a heading wears above it: none on the list's first row, a section's worth after. @param {boolean} first */
export function headingSpacing(first) {
  return first ? "none" : "section";
}

/** The avatar a row draws: a project's card at 22px, a heading's photo at 16px. @param {"project" | "group" | "goal"} kind */
export function avatarSize(kind) {
  return kind === "project" ? 22 : 16;
}
