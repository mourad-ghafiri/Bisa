/**
 * The Workflows library (03-workflows §The library): every card — a workflow
 * of yours, a catalog template — is the one card with the graph's thumbnail,
 * drawn without the canvas; one filter bar narrows both views and lives in
 * the address. Run with `node --test desktop/src/scenarios/library.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("both cards paint the one library card with the graph's thumbnail, and the thumbnail loads no canvas", () => {
  const workflow = src("../views/_workflow/WorkflowCard.tsx");
  const template = src("../views/_workflow/TemplateCard.tsx");
  const card = src("../views/_workflow/LibraryCard.tsx");
  const thumb = src("../views/_workflow/WorkflowThumbnail.tsx");
  assert.ok(workflow.includes("<LibraryCard") && template.includes("<LibraryCard"), "one card for both views");
  assert.ok(card.includes("<WorkflowThumbnail"), "the card wears the thumbnail");
  assert.ok(workflow.includes("definition={w}"), "a workflow's thumbnail is its stored definition");
  assert.ok(template.includes("entry.workflow?.steps"), "a template's thumbnail is the definition its row carries");
  assert.ok(!thumb.includes("ui/flow") && !thumb.includes("FlowCanvas"), "the thumbnail is an SVG, never the canvas");
  assert.ok(thumb.includes('from "./thumbnailModel.mjs"') && thumb.includes("<svg"), "painted from the model");
  // The card's `⋮`: vertical, always there, its shape the model's — Open, the
  // run verbs, Delete… through the designer's own retire dialog — laid by the
  // shell beside the door, never inside the anchor.
  assert.ok(workflow.includes("<MoreMenu vertical") && workflow.includes("cardMenu(verbs)") && workflow.includes("cardStatus(row)") && workflow.includes("cardMeta(row)"), "the card reads the model");
  assert.ok(workflow.includes("<RetireDialog") && workflow.includes('wanted="delete"'), "Delete… is the designer's dialog");
  assert.ok(!workflow.includes("headChips") && !template.includes("headChips"), "the state is one line, not a row of chips");
  const door = card.indexOf("</a>");
  const menuSlot = card.indexOf("{menu && (");
  assert.ok(door > 0 && menuSlot > door, "the menu is rendered after the door closes — beside it, never inside");
  assert.ok(card.includes("status.live && ") && card.includes("motion-reduce:animate-none"), "a live dot pulses, and stays still under reduced motion");
  // A workflow that listens wears its mark at the end of the state line: On, or Paused — the model's words.
  assert.ok(workflow.includes("mark={onMark(row)}") && card.includes("data-on-mark={mark.tone}") && card.includes("{mark.words}"), "the On mark is the model's, drawn beside the state");
  // On and Off are the card's verbs and, in the designer, the header's switch — never both there.
  assert.ok(workflow.includes('<TurnOnDialog open={pending === "turn_on"}') && workflow.includes("api.turnOffWorkflow(wf.workflow.id)"), "the card turns it On through the dialog, Off at once");
  const designer = src("../views/WorkflowDesigner.tsx");
  assert.ok(designer.includes('item.verb !== "turn_on" && item.verb !== "turn_off"') && !designer.includes('"Run…"'), "the designer filters by the verb, never by its words");
  assert.ok(designer.includes("<ListeningSwitch row={data}"), "and wears the switch in its header");
});

test("the screen's filters live in the address and narrow both views; the domain sections are one grouping", () => {
  const screen = src("../views/Workflows.tsx");
  const gallery = src("../views/_workflow/TemplateGallery.tsx");
  for (const word of ["parseFilters(params", "serializeFilters(", "searchWorkflows(", "searchTemplates(", "statusSegments(view)", "<TagFilterBar", "narrowed(filters, tagFilter)"]) {
    assert.ok(screen.includes(word), `the screen reads the model: ${word}`);
  }
  assert.ok(screen.includes("groupByDomain(shownRows") && gallery.includes("groupByDomain(entries"), "one grouping, both views");
  assert.ok(!screen.includes("function byDomain") && !gallery.includes("function groupByDomain"), "the copies are gone");
  assert.ok(gallery.includes("entries: readonly CatalogEntry[]") && !gallery.includes("api.catalog("), "the gallery draws what the screen searched");
  assert.ok(screen.includes('api.catalog({ kind: "workflow" }') && screen.includes('api.workflows({ scope: "library", archived }'), "the screen reads both lists");
});
