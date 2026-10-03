/**
 * The Workflows library's rules: the filters in the address, a status per
 * view, a search that finds a step's name and a slug, tags and words that
 * compose, one grouping by domain. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/libraryModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  DEFAULT_VIEW,
  GENERAL_DOMAIN,
  STATUSES,
  STATUS_ALL,
  VIEWS,
  groupByDomain,
  libraryReads,
  narrowed,
  parseFilters,
  rowStatuses,
  searchTemplates,
  searchWorkflows,
  serializeFilters,
  statusOf,
  statusSegments,
  stepWords,
  templateStatuses,
  viewOf,
} from "./libraryModel.mjs";
import { NO_TAG_FILTER } from "../../ui/tagSearchModel.mjs";

const params = (o) => ({ get: (k) => (k in o ? o[k] : null) });
const row = (name, over = {}) => ({
  workflow: { id: name, name, description: "", tags: [], origin: { origin: "workspace" }, steps: [], ...over.workflow },
  problems: over.problems ?? [],
  used_by: over.used_by ?? [],
  listening: over.listening ?? null,
});
const entry = (slug, over = {}) => ({ kind: "workflow", slug, name: slug, description: "", tags: [], requires: [], installed: false, ...over });

test("two views, Yours first; a stray view falls to the remembered one, then to Yours; each view has its statuses, `all` first", () => {
  assert.deepEqual([...VIEWS], ["yours", "templates"]);
  assert.equal(DEFAULT_VIEW, "yours");
  assert.equal(viewOf("templates"), "templates");
  assert.equal(viewOf("nope"), "yours");
  assert.deepEqual([...STATUSES.yours], ["all", "runs", "on", "problems", "used"]);
  assert.deepEqual([...STATUSES.templates], ["all", "installed", "available"]);
  assert.equal(statusOf("yours", "problems"), "problems");
  assert.equal(statusOf("yours", "installed"), STATUS_ALL, "a template's status is not a library row's");
  assert.equal(statusOf("templates", "available"), "available");
  assert.deepEqual(
    statusSegments("templates").map((s) => [s.id, s.label]),
    [
      ["all", "All"],
      ["installed", "Installed"],
      ["available", "Not yet installed"],
    ],
  );
  assert.deepEqual(statusSegments("yours").map((s) => s.label), ["All", "Ready to run", "On", "Has problems", "In use"]);
});

test("the filters round-trip the address: a default writes nothing, the remembered view stands in when the address names none", () => {
  const f = parseFilters(params({ view: "templates", q: "review", status: "available", archived: "1" }), "yours");
  assert.deepEqual(f, { view: "templates", q: "review", status: "available", archived: true });
  assert.deepEqual(serializeFilters(f), { view: "templates", q: "review", status: "available", archived: "1" });
  const bare = parseFilters(params({}), "templates");
  assert.deepEqual(bare, { view: "templates", q: undefined, status: "all", archived: false }, "the remembered view");
  assert.deepEqual(serializeFilters(bare), { view: "templates", q: null, status: null, archived: null }, "defaults leave the address");
  assert.equal(parseFilters(params({ view: "yours", status: "installed" }), "yours").status, "all", "a status of the other view is nobody's");
  assert.equal(parseFilters(null, "nope").view, "yours");
  assert.equal(narrowed(bare, NO_TAG_FILTER), false);
  assert.equal(narrowed({ ...bare, q: "x" }, NO_TAG_FILTER), true);
  assert.equal(narrowed({ ...bare, status: "installed" }, NO_TAG_FILTER), true);
  assert.equal(narrowed(bare, { selected: ["code"], match: "any" }), true);
});

test("a library row is what it is: runs, has problems, in use — and an archived one never runs", () => {
  assert.deepEqual(rowStatuses(row("clean")), ["all", "runs"]);
  assert.deepEqual(rowStatuses(row("broken", { problems: [{ kind: "no_start" }] })), ["all", "problems"]);
  assert.deepEqual(rowStatuses(row("busy", { used_by: [{ kind: "goal", id: "g" }] })), ["all", "runs", "used"]);
  assert.deepEqual(rowStatuses(row("away", { workflow: { archived: { at: 1 } } })), ["all"]);
  assert.deepEqual(rowStatuses(row("listening", { listening: { since: 1 } })), ["all", "runs", "on"], "On: listening for its events");
  assert.deepEqual(rowStatuses(row("paused", { listening: { since: 1, paused: { reason: { reason: "budget_spent" }, at: 2 } } })), ["all", "runs", "on"], "a paused one is still On");
  const rows = [row("Nightly", { listening: { since: 1 } }), row("Draft")];
  assert.deepEqual(searchWorkflows(rows, NO_TAG_FILTER, "", "on").map((r) => r.workflow.name), ["Nightly"]);
  assert.deepEqual(templateStatuses(entry("t")), ["all", "available"]);
  assert.deepEqual(templateStatuses(entry("t", { installed: true })), ["all", "installed"]);
});

test("a search finds a word in the name, the description, the slug, a step's name or kind, or a tag; the status and the tags narrow first", () => {
  const rows = [
    row("Bug fix", { workflow: { origin: { origin: "catalog", slug: "bug-fix" }, tags: ["engineering", "code"], steps: [{ id: "reproduce", name: "Reproduce the defect", kind: "agent" }] } }),
    row("Weekly review", { workflow: { description: "What moved this week", tags: ["management"], steps: [{ id: "gather", name: "Gather", kind: "connector" }] }, used_by: [{ kind: "workflow", id: "w2" }] }),
    row("Draft", { problems: [{ kind: "no_start" }] }),
  ];
  const names = (out) => out.map((r) => r.workflow.name);
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "", "all")), ["Bug fix", "Weekly review", "Draft"], "nothing narrows: the list's order");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "defect", "all")), ["Bug fix"], "a step's name");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "connector", "all")), ["Weekly review"], "a step's kind");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "bug-fix", "all")), ["Bug fix"], "the catalog slug");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "moved", "all")), ["Weekly review"], "the description");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "MANAGEMENT", "all")), ["Weekly review"], "a tag, whatever the case");
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "", "problems")), ["Draft"]);
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "", "used")), ["Weekly review"]);
  assert.deepEqual(names(searchWorkflows(rows, NO_TAG_FILTER, "", "runs")), ["Bug fix", "Weekly review"]);
  assert.deepEqual(names(searchWorkflows(rows, { selected: ["code"], match: "any" }, "review", "all")), [], "the tags narrow, then the words: nothing left");
  assert.deepEqual(names(searchWorkflows(rows, { selected: ["code"], match: "any" }, "fix", "runs")), ["Bug fix"]);
  assert.deepEqual(stepWords({ steps: [{ id: "a", name: "A", kind: "end" }] }), ["A", "a", "end"]);
  assert.deepEqual(stepWords(null), []);
});

test("a template is searched the same way, its definition's steps included, and narrowed by whether it is here", () => {
  const entries = [
    entry("bug-fix", { name: "Bug fix", tags: ["engineering"], workflow: { steps: [{ id: "reproduce", name: "Reproduce the defect", kind: "agent" }] } }),
    entry("weekly-review", { name: "Weekly review", description: "the week", tags: ["management"], installed: true }),
  ];
  const slugs = (out) => out.map((e) => e.slug);
  assert.deepEqual(slugs(searchTemplates(entries, NO_TAG_FILTER, "reproduce", "all")), ["bug-fix"], "a step of an uninstalled template");
  assert.deepEqual(slugs(searchTemplates(entries, NO_TAG_FILTER, "", "installed")), ["weekly-review"]);
  assert.deepEqual(slugs(searchTemplates(entries, NO_TAG_FILTER, "", "available")), ["bug-fix"]);
  assert.deepEqual(slugs(searchTemplates(entries, { selected: ["management"], match: "any" }, "", "all")), ["weekly-review"]);
  assert.deepEqual(slugs(searchTemplates(entries, NO_TAG_FILTER, "weekly", "available")), [], "installed, so not available");
});

test("the cards group by their first tag, the sections by name, a card with no tag under general", () => {
  const groups = groupByDomain(
    [
      { n: "b", tags: ["product", "web"] },
      { n: "a", tags: ["engineering"] },
      { n: "c", tags: [] },
      { n: "d", tags: ["engineering", "code"] },
    ],
    (i) => i.tags,
  );
  assert.deepEqual(
    groups.map(([domain, items]) => [domain, items.map((i) => i.n)]),
    [
      ["engineering", ["a", "d"]],
      [GENERAL_DOMAIN, ["c"]],
      ["product", ["b"]],
    ],
  );
  assert.deepEqual(groupByDomain([], () => []), []);
});

test("which of the library's two reads a fact on the bus moves: a workflow's own news both, a run or its listening the rows alone", () => {
  for (const type of ["workflow_changed", "workflow_proposed", "workflow_deleted", "workflow_archived"]) {
    assert.deepEqual(libraryReads({ type }), { library: true, catalog: true }, `${type}: an install moves a template's mark and the library both`);
  }
  for (const type of ["run_started", "run_finished", "run_cancelled", "goal_closed", "listening_changed"]) {
    assert.deepEqual(libraryReads({ type }), { library: true, catalog: false }, `${type}: a card's state line, its mark and its verbs`);
  }
  for (const type of ["step_changed", "session_state", "gate_opened", "boundary_fired"]) {
    assert.deepEqual(libraryReads({ type }), { library: false, catalog: false }, `${type}: no card says it`);
  }
  assert.deepEqual(libraryReads(null), { library: false, catalog: false });
});

test("a section is headed by its tag as written, and the cards with none by the platform's own word", async () => {
  const { domainLabel, GENERAL_DOMAIN, groupByDomain } = await import("./libraryModel.mjs");
  assert.equal(domainLabel(GENERAL_DOMAIN), "general");
  assert.equal(domainLabel("Engineering"), "Engineering", "a tag is content, never translated");
  const groups = groupByDomain([{ tags: ["ops"] }, { tags: [] }, { tags: null }], (item) => item.tags);
  assert.deepEqual(groups.map(([domain, list]) => [domainLabel(domain), list.length]), [["general", 2], ["ops", 1]]);
  const { readFileSync } = await import("node:fs");
  for (const screen of ["../Workflows.tsx", "./TemplateGallery.tsx"]) {
    const text = readFileSync(new URL(screen, import.meta.url), "utf8");
    assert.ok(text.includes("{domainLabel(domain)}") && !text.includes(">{domain}<"), `${screen} draws the heading the model says`);
  }
});
