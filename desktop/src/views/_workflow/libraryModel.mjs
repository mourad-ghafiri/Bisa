/**
 * The Workflows library's rules, with no React in them (03-workflows §The
 * library): which view, which filters, what a search finds, how the cards
 * group. Two views — *Yours*, the library's workflows, and *Templates*, the
 * catalog's — one filter bar. The filters live in the address
 * (`?view=&q=&status=&archived=1`, the Goals screen's habit), so a link
 * carries a search and Back restores it; which view a person lives in is
 * also remembered per machine (`bisa.workflow.library.view`, the screen's).
 *
 * A search finds a word in the name, the description, the catalog slug, a
 * step's name or kind, or a tag; the tag facets narrow first and the words
 * narrow within what they left (`ui/tagSearchModel.mjs`, the rule every list
 * shares); a status narrows by what the row is — runs, On (listening for its
 * events), has problems, in use; installed, not yet installed. The cards
 * group by their first tag, as they always did, in one function both views
 * call.
 */

import { filterByTagsAndWords } from "../../ui/tagSearchModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export const VIEWS = Object.freeze(["yours", "templates"]);
export const DEFAULT_VIEW = "yours";

/** The statuses a view's rows are narrowed by, `all` first. */
export const STATUSES = Object.freeze({
  yours: Object.freeze(["all", "runs", "on", "problems", "used"]),
  templates: Object.freeze(["all", "installed", "available"]),
});
export const STATUS_ALL = "all";

const STATUS_LABEL = Object.freeze({
  all: t("screens-workflows-status-all"),
  runs: t("screens-workflows-status-runs"),
  on: t("screens-workflows-status-on"),
  problems: t("screens-workflows-status-problems"),
  used: t("screens-workflows-status-used"),
  installed: t("screens-workflows-status-installed"),
  available: t("screens-workflows-status-available"),
});

/** A stored or typed view made safe. @param {unknown} value */
export function viewOf(value) {
  return VIEWS.includes(value) ? value : DEFAULT_VIEW;
}

/** A status the view knows, else `all`. @param {"yours" | "templates"} view @param {unknown} value */
export function statusOf(view, value) {
  return STATUSES[view].includes(value) ? value : STATUS_ALL;
}

/**
 * `?view=&q=&status=&archived=1` — the filters, read off the address. A view
 * the address does not name is the remembered one the caller passes.
 * @param {{get?: (k: string) => string | null}} params
 * @param {string} rememberedView
 */
export function parseFilters(params, rememberedView) {
  const read = (k) => {
    const v = params?.get?.(k);
    return v ? v : undefined;
  };
  const view = viewOf(read("view") ?? rememberedView);
  return {
    view,
    q: read("q"),
    status: statusOf(view, read("status")),
    archived: read("archived") === "1",
  };
}

/** The address a set of filters writes: nothing for a default. @param {{view: string, q?: string, status?: string, archived?: boolean}} f */
export function serializeFilters(f) {
  return {
    view: f.view,
    q: f.q ? f.q : null,
    status: f.status && f.status !== STATUS_ALL ? f.status : null,
    archived: f.archived ? "1" : null,
  };
}

/** Whether anything narrows the list beyond the view — what *Clear filters* undoes. */
export function narrowed(f, tagFilter) {
  return Boolean(f.q) || (f.status ?? STATUS_ALL) !== STATUS_ALL || (tagFilter?.selected?.length ?? 0) > 0;
}

/** The status segments a view's bar draws, in the model's order. @param {"yours" | "templates"} view */
export function statusSegments(view) {
  return STATUSES[view].map((id) => ({ id, label: STATUS_LABEL[id] }));
}

/**
 * What a library row is: it runs (no problems, not archived), it is On
 * (listening for its events, paused included), it has problems, a goal or
 * another workflow uses it. `all` is every row.
 * @param {{workflow: {archived?: unknown}, problems: readonly unknown[], used_by: readonly unknown[], listening?: unknown}} row
 */
export function rowStatuses(row) {
  const out = [STATUS_ALL];
  if (row.problems.length === 0 && !row.workflow.archived) out.push("runs");
  if (row.listening) out.push("on");
  if (row.problems.length > 0) out.push("problems");
  if (row.used_by.length > 0) out.push("used");
  return out;
}

/** What a template row is: installed here, or not yet. @param {{installed: boolean}} entry */
export function templateStatuses(entry) {
  return [STATUS_ALL, entry.installed ? "installed" : "available"];
}

/**
 * The words a definition is searched by: every step's name and kind.
 * @param {{steps?: readonly {name?: string, id?: string, kind?: string}[]} | null | undefined} definition
 */
export function stepWords(definition) {
  return (definition?.steps ?? []).flatMap((s) => [s.name, s.id, s.kind]);
}

/**
 * The library rows that pass the filters, in the list's order.
 * @template {{workflow: {name: string, description?: string, tags?: readonly string[], origin: {origin: string, slug?: string}, steps?: readonly any[], archived?: unknown}, problems: readonly unknown[], used_by: readonly unknown[]}} R
 * @param {readonly R[]} rows
 * @param {{selected: readonly string[], match: "any" | "all"}} tagFilter
 * @param {string | null | undefined} q
 * @param {string} status
 * @returns {R[]}
 */
export function searchWorkflows(rows, tagFilter, q, status) {
  const byStatus = rows.filter((r) => rowStatuses(r).includes(statusOf("yours", status)));
  return filterByTagsAndWords(
    byStatus,
    tagFilter,
    q,
    (r) => r.workflow.tags ?? [],
    (r) => [r.workflow.name, r.workflow.description, r.workflow.origin.origin === "catalog" ? r.workflow.origin.slug : null, ...(r.workflow.tags ?? []), ...stepWords(r.workflow)],
  );
}

/**
 * The templates that pass the filters, in the catalog's order.
 * @template {{name: string, slug: string, description: string, tags: readonly string[], installed: boolean, workflow?: {steps?: readonly any[]} | null}} E
 * @param {readonly E[]} entries
 * @param {{selected: readonly string[], match: "any" | "all"}} tagFilter
 * @param {string | null | undefined} q
 * @param {string} status
 * @returns {E[]}
 */
export function searchTemplates(entries, tagFilter, q, status) {
  const byStatus = entries.filter((e) => templateStatuses(e).includes(statusOf("templates", status)));
  return filterByTagsAndWords(byStatus, tagFilter, q, (e) => e.tags, (e) => [e.name, e.slug, e.description, ...e.tags, ...stepWords(e.workflow)]);
}

/** The domain a row files under: its first tag, else *general*. */
export const GENERAL_DOMAIN = "general";

/** A section's heading: the tag as it was written — a tag is content — and the platform's own word for the cards with none. */
export function domainLabel(domain) {
  return domain === GENERAL_DOMAIN ? t("workflow-library-domain-general") : String(domain ?? "");
}

/**
 * The cards in sections by their first tag, the sections sorted by name — the
 * one grouping both views draw.
 * @template T
 * @param {readonly T[]} items
 * @param {(item: T) => readonly string[] | null | undefined} tagsOf
 * @returns {[string, T[]][]}
 */
export function groupByDomain(items, tagsOf) {
  const groups = new Map();
  for (const item of items) {
    const key = tagsOf(item)?.[0] ?? GENERAL_DOMAIN;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(item);
  }
  return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b));
}

/** A workflow's own news: made, saved, proposed, deleted, put away or taken back out — an install among them. */
const WORKFLOW_FACTS = new Set(["workflow_changed", "workflow_proposed", "workflow_deleted", "workflow_archived"]);
/** What moves a card without moving its workflow: a run of it started or ended, a goal closed, its listening turned. */
const ROW_FACTS = new Set(["run_started", "run_finished", "run_cancelled", "goal_closed", "listening_changed"]);

/**
 * Which of the library's two reads an engine fact moves. A workflow's own
 * news reads both again — an install moves a template's mark and the
 * library at once. A run starting or ending moves a card's state line
 * (*running n runs*), its holders and so its verbs; turned On or Off, its
 * mark: the rows alone. A step inside a run moves no card.
 * @param {{type?: string} | null | undefined} payload an engine event's payload
 * @returns {{library: boolean, catalog: boolean}}
 */
export function libraryReads(payload) {
  const type = payload?.type ?? "";
  if (WORKFLOW_FACTS.has(type)) return { library: true, catalog: true };
  return { library: ROW_FACTS.has(type), catalog: false };
}
