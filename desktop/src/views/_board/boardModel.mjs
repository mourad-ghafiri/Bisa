/**
 * The Board's facts (ide/16): five columns, where each workstream's card
 * sits, what its due date reads as, the rows a screen paints, and the
 * optimistic move the screen makes while the node answers.
 *
 * **A column is not the state.** A card sits where a person put it; an
 * unplaced card follows the workstream's lifecycle by one rule
 * (`columnForState`), mirrored from `core/board.rs`; a closed workstream is
 * always Archived. Nothing here changes a workstream — the Board is a view.
 *
 * Plain `.mjs` with a `.d.mts` beside it so `node --test` holds every rule.
 */

import { cardTitle } from "../_work/workstreamCardModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The five columns, in the order the Board shows them. */
export const COLUMNS = Object.freeze(["backlog", "todo", "doing", "done", "archived"]);

export const COLUMN_LABEL = Object.freeze({
  backlog: t("board-board-backlog"),
  todo: t("board-board-todo"),
  doing: t("board-board-doing"),
  done: t("board-board-done"),
  archived: t("board-board-center-archived"),
});

/** One line under each column's name — what the column is for. */
export const COLUMN_HINT = Object.freeze({
  backlog: t("board-board-opened-nothing-touched-yet"),
  todo: t("board-board-next-up"),
  doing: t("board-board-changes-commits-pull-request"),
  done: t("board-board-merged"),
  archived: t("board-board-closed-put-away"),
});

/**
 * Where an unplaced card sits: the lifecycle read as a column — the same
 * rule as `BoardColumn::for_state` in the core.
 * @param {{state: string} | null | undefined} state
 */
export function columnForState(state) {
  switch (state?.state) {
    case "dirty":
    case "committed":
    case "pushed":
    case "pr_open":
      return "doing";
    case "merged":
      return "done";
    case "closed":
      return "archived";
    default:
      return "backlog";
  }
}

/**
 * The column a card is shown in: the person's choice when there is one, the
 * lifecycle's otherwise — and Archived for a closed workstream whatever was
 * chosen.
 * @param {{state: {state: string}, board?: {column?: string | null} | null}} w
 */
export function columnShown(w) {
  if (w.state?.state === "closed") return "archived";
  const chosen = w.board?.column;
  return typeof chosen === "string" && COLUMNS.includes(chosen) ? chosen : columnForState(w.state);
}

/** Today's calendar day on this machine, `YYYY-MM-DD`. */
export function todayKey(now = Date.now()) {
  const d = new Date(now);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
}

/** Whole days from `today` to `due`, both `YYYY-MM-DD`; negative is past. */
export function daysUntil(due, today) {
  const at = (s) => {
    const [y, m, d] = s.split("-").map(Number);
    return Date.UTC(y, m - 1, d);
  };
  return Math.round((at(due) - at(today)) / 86_400_000);
}

/**
 * What a due date reads as: overdue (danger), due soon (warn, within
 * `soonDays`), or simply due (quiet) — and the words the chip shows.
 * @param {string | null | undefined} due `YYYY-MM-DD`
 * @param {string} today `YYYY-MM-DD`
 * @param {number} [soonDays]
 * @returns {{kind: "none" | "overdue" | "today" | "soon" | "due", tone: "quiet" | "warn" | "danger", label: string, days: number | null}}
 */
export function dueTone(due, today, soonDays = 3) {
  if (!due) return { kind: "none", tone: "quiet", label: "", days: null };
  const days = daysUntil(due, today);
  // A date nobody can read — a record another build wrote — is no date: a chip
  // that said *due NaN days* would be worse than none.
  if (!Number.isFinite(days)) return { kind: "none", tone: "quiet", label: "", days: null };
  if (days < 0) return { kind: "overdue", tone: "danger", label: days === -1 ? t("board-board-overdue-day") : t("board-board-overdue-days", { days: -days }), days };
  if (days === 0) return { kind: "today", tone: "warn", label: t("board-board-due-today"), days };
  if (days <= soonDays) return { kind: "soon", tone: "warn", label: days === 1 ? t("board-board-due-tomorrow") : t("board-board-due-days", { days }), days };
  return { kind: "due", tone: "quiet", label: t("board-board-due-on", { due }), days };
}

/**
 * A card's search text: title, branch, project, note.
 * @param {{title: string, branch: string | null, project: {name: string}, workstream: {note?: string | null}}} row
 * @param {string} needle
 */
export function matches(row, needle) {
  const n = needle.trim().toLowerCase();
  if (n === "") return true;
  return [row.title, row.branch ?? "", row.project.name, row.workstream.note ?? ""].some((s) => s.toLowerCase().includes(n));
}

/**
 * The rows a screen paints, per column: placed cards by rank, then unplaced
 * ones by age — oldest first, so a column reads top to bottom as it filled.
 * @param {{
 *   refs: readonly {workstream: any, project_name: string | null, exists: boolean}[],
 *   statuses: Readonly<Record<string, any>>,
 *   needle?: string,
 *   projects?: ReadonlySet<string> | null,  the rail's selection — a group's projects, one project, or null for all
 *   archived?: boolean,
 * }} input
 * @returns {{columns: Record<string, any[]>, total: number, hidden: number}}
 */
export function boardRows({ refs, statuses, needle = "", projects = null, archived = false }) {
  const columns = Object.fromEntries(COLUMNS.map((c) => [c, []]));
  let total = 0;
  let hidden = 0;
  for (const ref of refs) {
    const w = ref.workstream;
    const status = statuses[w.id] ?? null;
    const column = columnShown(w);
    const row = {
      id: w.id,
      workstream: w,
      ref,
      status,
      project: { id: w.project, name: ref.project_name ?? "" },
      column,
      placed: typeof w.board?.column === "string",
      rank: typeof w.board?.rank === "number" ? w.board.rank : null,
      due: w.board?.due ?? null,
      title: cardTitle(w, status),
      branch: status?.branch ?? (w.kind?.kind === "worktree" ? w.kind.branch : null),
      primary: w.kind?.kind === "primary",
    };
    total += 1;
    if ((projects && !projects.has(w.project)) || !matches(row, needle) || (!archived && column === "archived")) {
      hidden += 1;
      continue;
    }
    columns[column].push(row);
  }
  for (const c of COLUMNS) columns[c].sort(byPlace);
  return { columns, total, hidden };
}

/**
 * Every status by its workstream's id — the shape `boardRows` reads the
 * rail's one list of statuses as.
 * @param {readonly {workstream: string}[] | null | undefined} statuses
 */
export function statusIndex(statuses) {
  return Object.fromEntries((statuses ?? []).map((s) => [s.workstream, s]));
}

/**
 * When a card last moved, unix seconds: the newest activity of a session
 * standing in it, else when the workstream was opened, else now.
 * @param {readonly {workstream?: string | null, last_activity?: number | null}[] | null | undefined} sessions
 * @param {{id: string, workstream: {created_at?: number | null}}} row
 * @param {number} now unix milliseconds
 */
export function lastActivity(sessions, row, now) {
  let latest = 0;
  for (const s of sessions ?? []) if (s.workstream === row.id) latest = Math.max(latest, s.last_activity ?? 0);
  return latest || row.workstream.created_at || Math.floor(now / 1000);
}

/** Placed cards by rank first, then the rest oldest first. */
function byPlace(a, b) {
  if (a.rank !== null && b.rank !== null) return a.rank - b.rank;
  if (a.rank !== null) return -1;
  if (b.rank !== null) return 1;
  return (a.workstream.created_at ?? 0) - (b.workstream.created_at ?? 0);
}

/**
 * A Doing column against its limit: zero is no limit; over it the header
 * warns — a warning, never a refusal — and `title` says so on hover.
 * @param {number} count
 * @param {number} limit
 * @returns {{over: boolean, label: string, title: string | null}}
 */
export function wipState(count, limit) {
  if (!limit || limit <= 0) return { over: false, label: String(count), title: null };
  const over = count > limit;
  return { over, label: `${count} / ${limit}`, title: over ? t("board-board-over-doing-limit") : null };
}

/**
 * What the bar says the scope and the filters leave: *3 of 12 workstreams*.
 * @param {number} shown @param {number} total
 */
export function countWords(shown, total) {
  return t("board-board-count", { shown, total });
}

/**
 * What the Board's *Close* asks before it runs: what closing does here — the
 * record to Archived, the checkout left on disk — then what stands in the
 * workstream and ends with it, when anything does (`terminationConsent`).
 * @param {string | null | undefined} consent
 */
export function closeWords(consent) {
  return [t("board-board-close-record-moves-archived"), consent].filter((s) => typeof s === "string" && s !== "").join(" ");
}

/** A card's project, by name; a project the workspace could not name is still a door. @param {{name?: string | null}} project */
export function projectWords(project) {
  return project?.name ? project.name : t("board-board-card-project");
}

/**
 * Whether a card may be moved to a column: not to the one it is in, and a
 * closed workstream nowhere but Archived — the node refuses the same.
 * @param {{column: string, workstream: {state?: {state?: string}}}} row @param {string} column
 */
export function canMoveTo(row, column) {
  if (column === row.column) return false;
  return !(row.workstream?.state?.state === "closed" && column !== "archived");
}

/**
 * The columns after a card moved, before the node answers: the card leaves
 * its column and lands at `index` in `column`. A move that changes nothing
 * returns the same object, so a render is skipped.
 * @param {Record<string, any[]>} columns
 * @param {string} id
 * @param {string} column
 * @param {number} index
 */
export function optimisticMove(columns, id, column, index) {
  let from = null;
  let row = null;
  for (const c of COLUMNS) {
    const i = columns[c].findIndex((r) => r.id === id);
    if (i !== -1) {
      from = c;
      row = columns[c][i];
      break;
    }
  }
  if (!row) return columns;
  const source = columns[from].filter((r) => r.id !== id);
  const target = from === column ? source : [...columns[column]];
  const at = Math.max(0, Math.min(index, target.length));
  if (from === column && columns[from][at]?.id === id) return columns;
  target.splice(at, 0, { ...row, column });
  return { ...columns, [from]: from === column ? target : source, [column]: target };
}

/** The largest index the wire takes: `PlaceWorkstreamBody.index` is a `u32`. */
export const MAX_INDEX = 4_294_967_295;

/**
 * The index the node is asked for, for a card set down at `index` among the
 * cards the Board **shows** in `column`. The node counts among the column's
 * *placed* cards, whatever the Board is narrowed to — a card of a project the
 * rail's selection left out, one the search hides — so the slot is said by
 * its neighbour: before the next shown card that is placed, else after the
 * last shown one that is, else last. A card in the column by its lifecycle
 * alone has no rank and no say, on the node as here.
 * @param {{
 *   refs: readonly {workstream: {id: string, board?: {column?: string | null, rank?: number | null} | null}}[],  every workstream, shown or not
 *   shown: Record<string, readonly {id: string}[]>,  the columns as painted
 *   id: string,
 *   column: string,
 *   index: number,  where among the shown cards, the card itself left out
 * }} drop
 */
export function placeIndex({ refs, shown, id, column, index }) {
  const placed = refs
    .map((r) => r.workstream)
    .filter((w) => w.id !== id && w.board?.column === column && typeof w.board?.rank === "number")
    .sort((a, b) => a.board.rank - b.board.rank)
    .map((w) => w.id);
  const visible = (shown[column] ?? []).filter((r) => r.id !== id);
  const n = Number.isFinite(index) ? Math.trunc(index) : visible.length;
  const at = Math.max(0, Math.min(n, visible.length));
  const next = visible.slice(at).find((r) => placed.includes(r.id));
  if (next) return placed.indexOf(next.id);
  for (let i = at - 1; i >= 0; i -= 1) if (placed.includes(visible[i].id)) return placed.indexOf(visible[i].id) + 1;
  return placed.length;
}

/**
 * The body of `PUT /workstreams/{wid}/board/place`: the column and a whole
 * index the wire takes — never below zero, never past a `u32`, which a body
 * is refused for.
 * @param {string} column @param {number} index
 */
export function placeBody(column, index) {
  const n = Number.isFinite(index) ? Math.trunc(index) : 0;
  return { column, index: Math.max(0, Math.min(n, MAX_INDEX)) };
}

/** Which of the counts a header shows read as attention: overdue cards and cards waiting on you. */
export function headerCounts(rows, today, soonDays) {
  let overdue = 0;
  for (const r of rows) if (dueTone(r.due, today, soonDays).kind === "overdue") overdue += 1;
  return { count: rows.length, overdue };
}
