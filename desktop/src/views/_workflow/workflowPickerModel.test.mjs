/**
 * The workflow picker's words and its one rule. Run with
 * `node --test --import ./src/i18n/preload.mjs src/views/_workflow/workflowPickerModel.test.mjs`
 * from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { installedHit, installedRow, installedWords, optionLabel, templateSlugOf, templateValue, unlistedLabel } from "./workflowPickerModel.mjs";

const row = (id, over = {}, problems = []) => ({ workflow: { id, name: `Workflow ${id}`, archived: null, origin: { origin: "workspace" }, ...over }, problems });

test("an option is the workflow's name, with its problems counted when it has any", () => {
  assert.equal(optionLabel(row("a")), "Workflow a");
  assert.equal(optionLabel(row("a", {}, [{ kind: "unfilled" }])), "Workflow a (1 problem)");
  assert.equal(optionLabel(row("a", {}, [{ kind: "unfilled" }, { kind: "unreachable" }])), "Workflow a (2 problems)");
  assert.equal(optionLabel(null), "");
});

test("a value no group carries is said as what it is: archived, or another goal's design", () => {
  assert.equal(unlistedLabel(row("a", { archived: { at: 1 } }).workflow), "Workflow a (archived)");
  assert.equal(unlistedLabel(row("a", { origin: { origin: "goal", goal: "01G" } }).workflow), "Workflow a (another goal's design)");
  assert.equal(unlistedLabel(row("a", { archived: { at: 1 }, origin: { origin: "goal", goal: "01G" } }).workflow), "Workflow a (archived)", "put away comes first");
  assert.equal(unlistedLabel(row("a").workflow), "Workflow a");
});

test("a template is named by its slug until it is installed, and an id is never taken for one", () => {
  assert.equal(templateValue("bug-fix"), "template:bug-fix");
  assert.equal(templateSlugOf(templateValue("bug-fix")), "bug-fix");
  for (const none of ["01ARZ3NDEKTSV4RRFFQ69G5FAV", "", "template:", null, undefined]) assert.equal(templateSlugOf(none), null);
});

test("the row a template became is the one the install answered, else the library's workflow born of that slug, else none", () => {
  const rows = [row("01A"), row("01B", { origin: { origin: "catalog", slug: "bug-fix" } }), row("01C", { origin: { origin: "catalog", slug: "weekly-review" } })];
  assert.equal(installedRow({ workflows: [{ slug: "weekly-review", id: "01C" }] }, rows, "weekly-review").workflow.id, "01C");
  // Already installed: the install answers nothing new, and the library has it.
  assert.equal(installedRow({ workflows: [] }, rows, "bug-fix").workflow.id, "01B");
  // The install names an id the list does not hold yet: the slug still finds it.
  assert.equal(installedRow({ workflows: [{ slug: "bug-fix", id: "01GONE" }] }, rows, "bug-fix").workflow.id, "01B");
  assert.equal(installedRow({ workflows: [] }, rows, "incident-response"), null, "said, never kept as a value that names nothing");
  assert.equal(installedRow(null, null, "bug-fix"), null);
});

test("the picker spells none of it", () => {
  const picker = readFileSync(new URL("./WorkflowPicker.tsx", import.meta.url), "utf8");
  for (const call of ["optionLabel(r)", "unlistedLabel(unlisted.data)", "installedRow(done, workflows, slug)", "templateSlugOf(v)", "templateValue(t.slug)"]) assert.ok(picker.includes(call), call);
  assert.ok(!picker.includes("problems)") && !picker.includes("(archived)") && !picker.includes('"template:"') && !picker.includes('= "workflow"'));
});

test("Use template says what it installed and what came with it, or that the copy which was there is being opened", () => {
  const made = { workflows: [{ slug: "bug-fix", id: "01B" }], agents: ["developer", "reviewer"], skills: ["code-review"] };
  assert.deepEqual(installedHit(made, "bug-fix"), { slug: "bug-fix", id: "01B" });
  assert.equal(installedHit(made, "weekly-review"), null);
  assert.equal(installedWords("bug-fix", made), "Installed bug-fix, and with it: developer, reviewer, code-review.");
  assert.equal(installedWords("bug-fix", { workflows: made.workflows, agents: [], skills: [] }), "Installed bug-fix.");
  assert.equal(installedWords("bug-fix", { workflows: [], agents: [], skills: [] }), "bug-fix was already installed — opening it.");
  assert.equal(installedWords("bug-fix", null), "bug-fix was already installed — opening it.");
  const gallery = readFileSync(new URL("./TemplateGallery.tsx", import.meta.url), "utf8");
  assert.ok(gallery.includes("toast.ok(installedWords(slug, installed))") && gallery.includes("installedRow(installed, workflows, slug)"), "the gallery and the picker find the copy by one rule");
  assert.ok(!gallery.includes('origin.origin === "catalog"'));
});
