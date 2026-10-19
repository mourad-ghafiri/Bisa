/**
 * The workflow panel — the designer's Properties pane and the goal's
 * Workflow tab column — stable and clear (03-workflows §The designer).
 * Source guards on what the components must keep doing, and pure journeys
 * over the models they draw from. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/designerPanel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { COALESCE_MS, canUndo, create, push, undo } from "../views/_workflow/history.mjs";
import { flowRef, keptSelection, selectionAfterDelete, stepSelection } from "../views/_workflow/selectionModel.mjs";
import { addStep, removeFlow, removeStep, replaceStep } from "../views/_workflow/workflowGraph.mjs";
import { inspectorView } from "../views/_workflow/inspectorModel.mjs";

const src = (name) => readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
const between = (text, from, to) => text.slice(text.indexOf(from), text.indexOf(to, text.indexOf(from)));

test("the panel draws one model's view, in its order, through the kit's own pieces, and every edit writes the positions down first", () => {
  const inspector = src("views/_workflow/Inspector.tsx");
  assert.ok(inspector.includes("inspectorView({ selection: selected, workflow: value, problems, unreadable, edit: { readOnly: !!readOnly, editable: editable ?? null }, why, run })"), "the view is the model's, from the facts the host hands in");
  assert.ok(inspector.includes("{view.sections.map(section)}"), "a step's sections stack in the model's order");
  for (const name of ['case "run":', 'case "identity":', 'case "kind":', 'case "flow":', 'case "problems":', 'case "danger":']) assert.ok(inspector.includes(name), `${name} is drawn`);
  assert.ok(inspector.includes('const FLOW_FOLD = "workflow:inspector:flow";') && inspector.includes("useCollapsed(FLOW_FOLD, true)"), "Flow and failure is a kit fold, closed at first, kept as the person left it");
  assert.ok(inspector.includes("const commit = (next: D, token: string | null = null) => onChange(withPositions(next, laid.positions), token);"), "one door for every edit: the derived positions written down, the token for the history");
  assert.ok(!/onChange\(replaceStep|onChange\(renameStep|onChange\(removeStep|onChange\(\{ \.\.\.value/.test(inspector), "no edit leaves by another door");
  assert.ok(inspector.includes("<SectionHeader flush") && inspector.includes("<Note") && inspector.includes("<ReadOnlyNote") && !inspector.includes("rounded-control border border-border bg-surface-2 px-2"), "sections and notes are the kit's, not hand-built boxes");
  assert.ok(inspector.includes('<Chip tone="danger" icon={ICON.warn}>') && inspector.includes("view.problems.count > 0"), "a step with problems wears their count beside its name");
  assert.ok(inspector.includes("onRenamed?.(step.id, to)") && inspector.includes("onSelect(stepSelection(to))"), "a rename keeps the page and tells the host");
  assert.ok(inspector.includes('if (view.kind === "flow")') && inspector.includes("removeFlow(value, ref)") && inspector.includes("flowRefAfterRelabel(ref, from.id, was, now)"), "a flow has a page: its ends, its branch to rename, Remove");
  assert.ok(inspector.includes('if (view.kind === "many")') && inspector.includes("<StepDoor"), "several picked is a word and a door to each");
  assert.ok(inspector.includes("<RunSection") && inspector.includes('fold={OUTPUT_FOLD}') && inspector.includes("<StepActions run={run.id} step={view.step}"), "a step in a run reads its record under This run, the output folded");
  assert.ok(!inspector.includes("header?:") && !inspector.includes("{header}"), "the unused header slot is gone");
});

test("the canvas reports every pick through xyflow's own listener, marks selected edges, and leaves the keymap's chord alone", () => {
  const canvas = src("ui/FlowCanvas.tsx");
  assert.ok(canvas.includes("onSelectionChange={handleSelection}") && !canvas.includes("onNodeClick") && !canvas.includes("onPaneClick"), "one source for the selection: the listener, never a node click alone");
  assert.ok(canvas.includes("if (!ready.current) return;") && canvas.includes("if (samePick(pick, pickRef.current)) return;"), "the mount's call and the echo of the props are no pick");
  assert.ok(canvas.includes("selected: e.selected,"), "an edge carries its selection to the mirror");
  assert.ok(canvas.includes('mod && !e.shiftKey && e.key.toLowerCase() === "d"'), "Mod+Shift+D is a keymap chord, not a duplicate");
  assert.ok(canvas.includes('onSelect?.({ nodes: [], edges: [] })'), "Escape clears through the same door");
  const designer = src("views/_workflow/Designer.tsx");
  assert.ok(designer.includes("onSelect(selectionOf(pick))") && designer.includes("selectionAfterDelete(selected, gone.nodes, gone.edges)") && designer.includes("removeFlow(next, flowRefOf(e))"), "the designer turns a pick into the shared value, and a delete leaves what remains picked");
  assert.ok(designer.includes("marked.nodes.includes(n.id)") && designer.includes("marked.edges.includes(e.id)"), "nodes and edges are marked from the one value");
  assert.ok(designer.includes("yourMove(n.step, run.steps[n.id])") && !designer.includes("YOUR_ACTS"), "whose move a step is comes from runView");
  assert.ok(src("views/_workflow/StepActions.tsx").includes("const mine = yours(step, record);"), "and StepActions wears the accent only then");
});

test("the hosts share the selection, coalesce a typed burst, land undo where the person was, and say why nothing may be edited", () => {
  const screen = src("views/WorkflowDesigner.tsx");
  assert.ok(screen.includes("useState<Selection>(() => stepSelection(memory.selected))") && screen.includes("rememberSelectedStep(id, storedSelection(selected))"), "the designer seeds from its memory and keeps a step as the bare id it always wrote");
  assert.ok(screen.includes("edit(s, next, Date.now(), { selection: selected, token })") && screen.includes("landedSelection(next) as Selection"), "every edit is stamped; undo and redo land the selection");
  assert.ok(between(screen, "const change = (next: NewWorkflowBody, token: string | null = null) => {", "};").includes("toast.error(readOnlyReason(true, { runningIn: running })?.reason"), "a frozen designer says why instead of dropping the edit");
  assert.ok(screen.includes("data-scroll-keep={propertiesScrollName(selected)}") && screen.includes("onRenamed={(from, to) => moveKeptScroll(place, propertiesScrollName(stepSelection(from)), propertiesScrollName(stepSelection(to)))}"), "each page keeps its scroll, and a rename moves it");
  assert.ok(screen.includes("<ResizeHandle side=\"left\" size={fit.right ?? panelWidth}"), "the handle drags from the width drawn now");
  assert.ok(screen.includes("why={{ runningIn: running }}"), "the designer hands its one reason in");
  const tab = src("views/_workflow/GoalWorkflowTab.tsx");
  assert.ok(tab.includes('role="complementary"') && tab.includes('aria-label={t("workflow-goal-workflow-tab-inspector-label")}') && tab.includes("data-scroll-keep={`workflow:inspector:${stepOf(selected) ?? \"\"}`}"), "the goal's column is a named landmark that keeps each step's scroll");
  assert.ok(tab.includes('const INSPECTOR_KEY = "bisa.goal.workflow.inspector.width";') && tab.includes("useStoredSize(INSPECTOR_KEY, INSPECTOR_WIDTH, { min: INSPECTOR_MIN, max: INSPECTOR_MAX })") && tab.includes("<ResizeHandle side=\"left\" size={fit.right ?? inspectorWidth}"), "and is resizable, its width kept");
  assert.ok(!tab.includes("<ProblemsList") && !tab.includes("JSON.stringify(record.output") && !tab.includes("<StepActions"), "no second Problems block, no raw output strip, no actions above the step's own page: the Inspector draws them");
  assert.ok(tab.includes("push(h, next, { selection: selected, at: Date.now(), token })") && tab.includes("select(next.landed as Selection)"), "the draft's history is stamped and undo lands");
  assert.ok(tab.includes("why={why}") && tab.includes("runFinished: !!activeRun && !runLive") && tab.includes("onOpenDesigner={workflow && workflow.origin.origin !== \"goal\" ? () => navigate({ name: \"workflow\", id: workflow.id }) : undefined}"), "every read-only state has its reason, a library workflow its door to the designer");
  assert.ok(tab.includes("const kept = storedSelection(sel);") && tab.includes("setTransient(kept === null ? sel : null);"), "a step is the memory's; a flow or a box is the tab's for the moment");
});

test("every blur-commit field lands its draft on the way out, lists are keyed by place, groups are Labelled, icon buttons are named", () => {
  const forms = (name) => src(`views/_workflow/forms/${name}`);
  for (const [file, count] of [["JsonField.tsx", 1], ["ConnectorStepForm.tsx", 1], ["StepCommonForm.tsx", 1], ["BoundaryEventsEditor.tsx", 1], ["BranchName.tsx", 1], ["ExactFieldsEditor.tsx", 1], ["InputDefsEditor.tsx", 3], ["AgentStepForm.tsx", 1]]) {
    assert.equal((forms(file).match(/useCommitOnUnmount\(/g) ?? []).length, count, `${file} commits on unmount`);
  }
  assert.ok(src("ui/Field.tsx").includes("useCommitOnUnmount(draft !== String(value) ? draft : null"), "the kit's NumberInput too");
  assert.ok(forms("StepCommonForm.tsx").includes("const sent = useRef<string | null>(null);") && forms("StepCommonForm.tsx").includes("useCommitOnUnmount(problem === null && id !== step.id ? id : null, rename);"), "a rename is sent once, never twice");
  for (const [file, key] of [["DecideStepForm.tsx", "<div key={i} className=\"rounded-control"], ["SwitchStepForm.tsx", "<div key={i} className=\"flex flex-wrap"], ["JudgeStepForm.tsx", "<div key={i} className=\"flex flex-col"], ["BoundaryEventsEditor.tsx", "<BoundaryRow key={i}"], ["ExactFieldsEditor.tsx", "<div key={i} className=\"flex items-start"]]) {
    assert.ok(forms(file).includes(key), `${file}: rows keyed by place, so a renamed row keeps its focus`);
  }
  for (const [file, id] of [["HumanStepForm.tsx", "workflow-human-step-form-options"], ["BoundaryEventsEditor.tsx", "workflow-boundary-events-editor-boundary-events"], ["DecideStepForm.tsx", "workflow-decide-step-form-rules"], ["JudgeStepForm.tsx", "workflow-human-step-form-options"], ["MessageFilterFields.tsx", "workflow-message-filter-fields-from"], ["ScheduleFields.tsx", "workflow-wait-step-form-seconds"], ["ScheduleFields.tsx", "workflow-wait-step-form-cron"], ["SwitchStepForm.tsx", "workflow-switch-step-form-cases"], ["WaitStepForm.tsx", "workflow-wait-step-form-seconds"], ["WaitStepForm.tsx", "workflow-wait-step-form-cron"], ["SignalFilterFields.tsx", "workflow-wait-step-form-fields"]]) {
    const text = forms(file);
    assert.ok(text.includes(`<Labelled label={t("${id}")`) && !text.includes(`<Field label={t("${id}")`), `${file}: a group of controls is Labelled, never a <label>`);
  }
  const human = forms("HumanStepForm.tsx");
  assert.ok(human.includes('aria-label={t("workflow-human-step-form-recommend-option")} aria-pressed={o.recommended}') && human.includes('aria-label={t("workflow-human-step-form-remove-option")}'), "the two icon buttons are named, the toggle pressed");
  assert.ok(forms("ConditionEditor.tsx").includes('t("workflow-condition-editor-nth", { n: i + 1 })'), "a group's member is a word, not a machine's");
  assert.ok(forms("JsonField.tsx").includes('error={problem ? t("workflow-json-field-not-json", { problem }) : undefined}'), "a JSON refusal is the field's error, under the control");
  assert.ok(!/grid gap-3 @xs:grid-cols-2/.test(forms("AgentStepForm.tsx") + forms("StartStepForm.tsx") + forms("WaitStepForm.tsx") + forms("ScheduleFields.tsx") + forms("ConnectorStepForm.tsx")), "field pairs go two columns from @sm, as the common form's do");
  assert.ok(src("ui/Labelled.tsx").includes('role="alert"'), "Labelled carries an error of its own");
});

test("journeys: three keystrokes then a drag are two undos; a deleted step comes back selected; an edge-only delete keeps the step", () => {
  const wf = { name: "W", steps: [{ id: "start", name: "Start", kind: "start", on: { event: "manual" }, then: [{ to: "draft" }] }, { id: "draft", name: "Draft", kind: "agent", instructions: "", then: [] }] };
  const draft = stepSelection("draft");
  let h = create(wf);
  const typed = (body, text, at) => push(h, replaceStep(body, "draft", { ...body.steps[1], name: text }), { selection: draft, at, token: "step:draft" });
  h = typed(h.present, "D", 1000);
  h = typed(h.present, "Dr", 1200);
  h = typed(h.present, "Dra", 1400);
  const moved = { ...h.present, steps: h.present.steps.map((s) => (s.id === "draft" ? { ...s, position: { x: 100, y: 200 } } : s)) };
  h = push(h, moved, { selection: draft, at: 1400 + COALESCE_MS - 1 });
  assert.equal(h.past.length, 2, "the word is one entry, the drag another, within the window");
  h = undo(h, draft);
  assert.deepEqual([h.present.steps[1].name, h.present.steps[1].position, h.landed], ["Dra", undefined, draft]);
  h = undo(h, draft);
  assert.deepEqual([h.present.steps[1].name, h.landed], ["Draft", draft]);

  // Delete the step, undo: it is back, and selected again.
  h = create(wf);
  h = push(h, removeStep(h.present, "draft"), { selection: draft, at: 5000 });
  let selected = selectionAfterDelete(draft, ["draft"], []);
  assert.equal(selected, null);
  h = undo(h, selected);
  selected = keptSelection(h.landed, h.present);
  assert.deepEqual([h.present.steps.length, selected], [2, draft]);
  assert.equal(canUndo(h), false);

  // Delete only the flow into the step: the step stays picked, the flow's page is gone.
  const flow = flowRef("start", "draft");
  const many = { kind: "many", steps: ["draft"], flows: [flow] };
  const cut = removeFlow(wf, flow);
  assert.deepEqual(cut.steps[0].then, []);
  assert.deepEqual(selectionAfterDelete(many, [], ["start->draft"]), draft);
  assert.equal(keptSelection(flow, cut), null);
  // The panel over the cut workflow: the step's own page, no Remove while frozen, the flow's page says it is gone.
  assert.equal(inspectorView({ selection: draft, workflow: cut }).kind, "step");
  assert.equal(inspectorView({ selection: flow, workflow: cut }).flow.exists, false);
  // An added step is the selection the next entry is stamped with, and undoing the add lands on what was picked before it.
  let g = create(wf);
  const { wf: added, id } = addStep(g.present, "human");
  g = push(g, added, { selection: null, at: 1 });
  assert.equal(keptSelection(stepSelection(id), g.present)?.id, id);
  g = undo(g, stepSelection(id));
  assert.equal(keptSelection(g.landed, g.present), null, "the add undone, the new step's pick goes with it");
});
