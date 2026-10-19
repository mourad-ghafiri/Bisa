/**
 * The right-hand pane over a workflow, drawn by both hosts — the designer's
 * Properties pane and the goal's Workflow tab column — from one model
 * (`inspectorModel.mjs`): the workflow itself when nothing is picked (name,
 * description, tags, the Decision-Making switch, inputs, problems), a
 * **step** (identity, its kind's form, the folded *Flow and failure*, its
 * own problems, Remove), a **flow** (its ends, its word, its branch to
 * rename, Remove), or a word on **several** picked. Which view, in which
 * order, with which read-only reason and which run facts is the model's;
 * this file only draws.
 *
 * Every edit goes back up as a whole new definition (`workflowGraph.mjs`'s
 * functions) through one door, `commit`: the derived positions are written
 * down first (`withPositions`), as the canvas does, so an edit made here
 * never reflows the picture; and a typed edit carries a token (`step:<id>`,
 * `workflow`, `inputs`) the history coalesces on, so a word is one undo.
 *
 * **Stable while typing.** The step's children sit under one keyed fragment
 * (`key={step.id}`), so another step picked is another page — a draft, a
 * picked word, a shown secret never carries over — and the same step
 * re-rendered keeps every form mounted: nothing above the field being typed
 * in moves, and the step's problems sit under its forms, not between the
 * title and the first field. A rename keeps the page: the key moves with
 * the id and the host moves what it kept of the scroll (`onRenamed`).
 */

import { Fragment, useMemo } from "react";
import type { Answer, InputDef, ListenerView, NewWorkflowBody, Problem, Step, Workflow, WorkflowRun } from "../../types";
import { Button, Chip, Field, ICON, Labelled, Note, RelativeTime, SectionHeader, StepStateChip, Switch, TAG_VOCABULARY, TagInput, TextArea, TextInput, cn, stepKindIcon, useCollapsed } from "../../ui";

/** One array for the life of the module: a literal per render is a new identity to memoise on. */
const TAG_SUGGESTIONS: string[] = [...TAG_VOCABULARY];
import { familyInk } from "./familyInk";
import { AgentStepForm } from "./forms/AgentStepForm";
import { ApprovalStepForm } from "./forms/ApprovalStepForm";
import { BranchName } from "./forms/BranchName";
import { CheckStepForm } from "./forms/CheckStepForm";
import { ConnectorStepForm } from "./forms/ConnectorStepForm";
import { DecideStepForm } from "./forms/DecideStepForm";
import { EmitStepForm } from "./forms/EmitStepForm";
import { EndStepForm } from "./forms/EndStepForm";
import { ForEachStepForm } from "./forms/ForEachStepForm";
import { HumanStepForm } from "./forms/HumanStepForm";
import { IfStepForm } from "./forms/IfStepForm";
import { InputDefsEditor } from "./forms/InputDefsEditor";
import { JudgeStepForm } from "./forms/JudgeStepForm";
import { NotifyStepForm } from "./forms/NotifyStepForm";
import { ParallelStepForm } from "./forms/ParallelStepForm";
import { SpawnStepForm } from "./forms/SpawnStepForm";
import { StartStepForm, type StartHost } from "./forms/StartStepForm";
import { StepCommonForm } from "./forms/StepCommonForm";
import { SwitchStepForm } from "./forms/SwitchStepForm";
import { ThenField } from "./forms/ThenField";
import { WaitStepForm } from "./forms/WaitStepForm";
import { WhileStepForm } from "./forms/WhileStepForm";
import { inspectorView, type ReadOnly, type ReadOnlyWhy, type StepView } from "./inspectorModel.mjs";
import { ProblemsList } from "./ProblemsList";
import { edgeIdOf, flowRefAfterRelabel, selectionAfterDelete, stepSelection, type FlowSelection, type Selection } from "./selectionModel.mjs";
import { StepActions } from "./StepActions";
import { StepRecordBody } from "./StepRecordBody";
import { TEMPLATE_HINT, branchesOf } from "./stepKinds.mjs";
import { removeFlow, removeStep, renameInput, renameStep, replaceStep, upstreamOf, type Definition } from "./workflowGraph.mjs";
import { layout, withPositions } from "./workflowLayout.mjs";
import { HOLDER_LABEL } from "../_goals/goalStripModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The fold's name in the collapsed store: *Flow and failure* stays as the person left it, from step to step and across visits. */
const FLOW_FOLD = "workflow:inspector:flow";
/** The output fold's, under *This run*. */
const OUTPUT_FOLD = "workflow:inspector:output";

const noop = () => {};

function KindForm({
  step,
  wf,
  onChange,
  disabled,
  host,
  listeners,
}: {
  step: Step;
  wf: Definition;
  onChange: (next: Step) => void;
  disabled?: boolean;
  host?: StartHost | null;
  listeners?: readonly ListenerView[];
}) {
  const inputs: InputDef[] = wf.inputs ?? [];
  const upstream = upstreamOf(wf, step.id);
  switch (step.kind) {
    case "start":
      return <StartStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} host={host} listeners={listeners} />;
    case "emit":
      return <EmitStepForm step={step} onChange={onChange} disabled={disabled} />;
    case "parallel":
      return <ParallelStepForm step={step} />;
    case "agent":
      return <AgentStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "human":
      return <HumanStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "approval":
      return <ApprovalStepForm step={step} onChange={onChange} disabled={disabled} />;
    case "check":
      return <CheckStepForm step={step} upstream={upstream} onChange={onChange} disabled={disabled} />;
    case "decide":
      return <DecideStepForm step={step} upstream={upstream} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "if":
      return <IfStepForm step={step} upstream={upstream} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "switch":
      return <SwitchStepForm step={step} onChange={onChange} disabled={disabled} />;
    case "judge":
      return <JudgeStepForm step={step} onChange={onChange} disabled={disabled} />;
    case "for_each":
      return <ForEachStepForm step={step} onChange={onChange} disabled={disabled} />;
    case "while":
      return <WhileStepForm step={step} upstream={upstream} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "connector":
      return <ConnectorStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "wait":
      return <WaitStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "notify":
      return <NotifyStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "spawn":
      return <SpawnStepForm step={step} inputs={inputs} onChange={onChange} disabled={disabled} />;
    case "end":
      return <EndStepForm step={step} onChange={onChange} disabled={disabled} />;
    default:
      // A kind from a newer node: shown by name, kept as it is, never edited blind.
      return <p className="text-2xs text-text-dim">{t("workflow-inspector-unknown-kind", { kind: (step as { kind: string }).kind })}</p>;
  }
}

/** The one note a read-only pane wears, with its door when the host has one. */
function ReadOnlyNote({ readOnly, onEdit, onOpenDesigner }: { readOnly: ReadOnly; onEdit?: () => void; onOpenDesigner?: () => void }) {
  const open = readOnly.door?.act === "edit" ? onEdit : readOnly.door?.act === "designer" ? onOpenDesigner : undefined;
  return <Note door={readOnly.door && open ? { label: readOnly.door.label, onClick: open } : null}>{readOnly.reason}</Note>;
}

/** The ghost door back to the workflow's own view, at the head of a step's, a flow's or a many's page. */
function BackToWorkflow({ onSelect }: { onSelect: (selection: Selection) => void }) {
  return (
    <Button size="sm" variant="ghost" className="-ml-2 self-start" aria-label={t("workflow-inspector-back-workflow")} onClick={() => onSelect(null)}>
      <ICON.back size={12} aria-hidden />
      {t("workflow-inspector-back")}
    </Button>
  );
}

export function Inspector<D extends NewWorkflowBody | Workflow>({
  value,
  selected,
  problems,
  unreadable,
  onChange,
  onSelect,
  readOnly,
  editable,
  host = null,
  listeners = [],
  why = null,
  run = null,
  onEdit,
  onOpenDesigner,
  onRenamed,
  onChanged,
  onOpenItem,
  onDecide,
}: {
  value: D;
  /** What is picked — the one value the canvas marks too (`selectionModel.mjs`). */
  selected: Selection;
  problems: readonly Problem[];
  /** Why the node could not judge the current design, when it could not. */
  unreadable?: string | null;
  /** A new definition; `token` names what was typed into, for the history to coalesce a burst on. */
  onChange: (next: D, token?: string | null) => void;
  onSelect: (selection: Selection) => void;
  readOnly?: boolean;
  /** In an amendment: the steps that may still change. Null means every step. */
  editable?: Set<string> | null;
  /** Who listens with the design's starts once stored — a library workflow or a goal: a hook's call is theirs. */
  host?: StartHost | null;
  /** The host's listeners while it listens: a hook's public call and its secret's state. */
  listeners?: readonly ListenerView[];
  /** Why nothing may be edited — the facts the note at the top is decided from (`inspectorModel.readOnlyReason`). */
  why?: ReadOnlyWhy | null;
  /** The run shown, for a step's facts under *This run*. */
  run?: WorkflowRun | null;
  /** The read-only note's door to the goal's editor. */
  onEdit?: () => void;
  /** The read-only note's door to the library's designer. */
  onOpenDesigner?: () => void;
  /** A step renamed here: the host moves what it kept under the old id. */
  onRenamed?: (from: string, to: string) => void;
  /** A step's act landed — an answer, a release: the host reads the run again. */
  onChanged?: () => void;
  onOpenItem?: (item: string) => void;
  onDecide?: () => void;
}) {
  const view = useMemo(
    () => inspectorView({ selection: selected, workflow: value, problems, unreadable, edit: { readOnly: !!readOnly, editable: editable ?? null }, why, run }),
    [selected, value, problems, unreadable, readOnly, editable, why, run],
  );
  // One array per value, not per render: `TagInput` memoises on it.
  const tags = useMemo(() => [...(value.tags ?? [])], [value.tags]);
  const laid = useMemo(() => layout(value as Definition), [value]);
  // The one door every edit leaves by: the derived positions written down first, so a form's edit never reflows the picture.
  const commit = (next: D, token: string | null = null) => onChange(withPositions(next, laid.positions), token);
  const [flowFolded, toggleFlow] = useCollapsed(FLOW_FOLD, true);

  if (view.kind === "step") {
    const { step, frozen } = view;
    const Icon = stepKindIcon(step.kind);
    const token = `step:${step.id}`;
    const patch = (next: Step) => commit(replaceStep(value, step.id, next), token);
    const rename = (to: string) => {
      commit(renameStep(value, step.id, to));
      onSelect(stepSelection(to));
      onRenamed?.(step.id, to);
    };
    const section = (name: StepView["sections"][number]) => {
      switch (name) {
        case "run":
          return <RunSection key="run" view={view} run={run} onChanged={onChanged} onOpenItem={onOpenItem} onDecide={onDecide} />;
        case "identity":
          return (
            <section key="identity" className="flex flex-col gap-2">
              <SectionHeader flush title={t("workflow-inspector-section-identity")} />
              <StepCommonForm part="identity" step={step} steps={value.steps} disabled={frozen} onChange={patch} onRename={rename} />
            </section>
          );
        case "kind":
          return (
            <section key="kind" className="flex flex-col gap-2">
              <SectionHeader flush title={view.subtitle} />
              <KindForm step={step} wf={value} disabled={frozen} host={host} listeners={listeners} onChange={patch} />
            </section>
          );
        case "flow":
          return (
            <section key="flow" className="flex flex-col gap-2">
              <SectionHeader flush title={t("workflow-inspector-flow-and-failure")} open={!flowFolded} onToggle={toggleFlow} />
              {!flowFolded && (
                <div className="flex flex-col gap-3">
                  <ThenField wf={value} step={step} disabled={frozen} onChange={(next) => commit(next as D)} />
                  <StepCommonForm part="flow" step={step} steps={value.steps} inputs={value.inputs ?? []} disabled={frozen} onChange={patch} onRename={rename} />
                </div>
              )}
            </section>
          );
        case "problems":
          return (
            <section key="problems" className="flex flex-col gap-2">
              <SectionHeader flush title={t("workflow-inspector-problems")} count={view.problems.count} />
              <ProblemsList problems={view.problems.list} />
            </section>
          );
        case "danger":
          return (
            <div key="danger" className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-hairline pt-3">
              <Button
                size="sm"
                variant="danger"
                onClick={() => {
                  commit(removeStep(value, step.id));
                  onSelect(selectionAfterDelete(selected, [step.id], []));
                }}
              >
                <ICON.delete size={12} aria-hidden />{t("workflow-inspector-remove-step")}</Button>
              <span className="text-2xs text-text-dim">{t("workflow-inspector-remove-hint")}</span>
            </div>
          );
        default:
          return null;
      }
    };
    return (
      <div className="@container flex flex-col gap-5 px-4 py-3">
        {/* One key for the whole page: another step is another page, the same step keeps every form mounted while it is typed in. */}
        <Fragment key={step.id}>
          <BackToWorkflow onSelect={onSelect} />
          <div className="-mt-3 flex items-center gap-2">
            <Icon size={14} aria-hidden className={cn("shrink-0", familyInk(step.kind))} />
            <h3 className="min-w-0 flex-1 truncate text-sm font-semibold">{view.title}</h3>
            {view.problems.count > 0 && (
              <Chip tone="danger" icon={ICON.warn}>{t("workflow-goal-workflow-tab-problems-count", { n: view.problems.count })}</Chip>
            )}
            <Chip tone="quiet">{view.subtitle}</Chip>
          </div>
          {view.readOnly && <ReadOnlyNote readOnly={view.readOnly} onEdit={onEdit} onOpenDesigner={onOpenDesigner} />}
          {view.stepStarted && <Note>{t("workflow-inspector-step-has-started")}</Note>}
          {view.sections.map(section)}
        </Fragment>
      </div>
    );
  }

  if (view.kind === "flow") {
    const ref = selected as FlowSelection;
    const { flow, frozen } = view;
    const from = flow.from;
    return (
      <div className="@container flex flex-col gap-5 px-4 py-3">
        <Fragment key={edgeIdOf(ref)}>
          <BackToWorkflow onSelect={onSelect} />
          <div className="-mt-3 flex items-center gap-2">
            <ICON.forward size={14} aria-hidden className="shrink-0 text-text-dim" />
            <h3 className="min-w-0 flex-1 truncate text-sm font-semibold">{view.title}</h3>
            <Chip tone="quiet">{flow.word}</Chip>
          </div>
          {view.readOnly && <ReadOnlyNote readOnly={view.readOnly} onEdit={onEdit} onOpenDesigner={onOpenDesigner} />}
          <section className="flex flex-col gap-3">
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-1.5 text-xs">
              <dt className="text-2xs text-text-dim">{t("workflow-inspector-flow-from")}</dt>
              <dd className="min-w-0">
                <StepDoor step={flow.from} id={ref.from} onSelect={onSelect} />
              </dd>
              <dt className="text-2xs text-text-dim">{t("workflow-inspector-flow-to")}</dt>
              <dd className="min-w-0">
                <StepDoor step={flow.to} id={ref.to} onSelect={onSelect} />
              </dd>
            </dl>
            {flow.loop && <p className="text-2xs text-text-dim">{t("workflow-inspector-flow-loop")}</p>}
            {!flow.exists && <Note>{t("workflow-inspector-flow-gone")}</Note>}
            {flow.relabel && from && ref.branch !== null && (
              <Labelled label={t("workflow-branch-name-branch")}>
                <BranchName
                  step={from as Extract<Step, { kind: "decide" | "switch" | "judge" }>}
                  branch={ref.branch}
                  disabled={frozen}
                  className="w-40 font-mono"
                  onChange={(next) => {
                    // The flow follows its branch to the new name, so this page stays the flow's.
                    const was = ref.branch as string;
                    const now = branchesOf(next).find((b) => !branchesOf(from).includes(b)) ?? was;
                    commit(replaceStep(value, from.id, next));
                    onSelect(flowRefAfterRelabel(ref, from.id, was, now));
                  }}
                />
              </Labelled>
            )}
          </section>
          {view.remove && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-hairline pt-3">
              <Button
                size="sm"
                variant="danger"
                onClick={() => {
                  commit(removeFlow(value, ref));
                  onSelect(selectionAfterDelete(ref, [], [edgeIdOf(ref)]));
                }}
              >
                <ICON.delete size={12} aria-hidden />{t("workflow-inspector-remove-flow")}</Button>
              <span className="text-2xs text-text-dim">{t("workflow-inspector-remove-hint")}</span>
            </div>
          )}
        </Fragment>
      </div>
    );
  }

  if (view.kind === "many") {
    return (
      <div className="@container flex flex-col gap-5 px-4 py-3">
        <BackToWorkflow onSelect={onSelect} />
        <div className="-mt-3 flex items-center gap-2">
          <ICON.list size={14} aria-hidden className="shrink-0 text-text-dim" />
          <h3 className="min-w-0 flex-1 truncate text-sm font-semibold">{view.title}</h3>
        </div>
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{view.subtitle}</p>
        {view.many.steps.length > 0 && (
          <ul className="flex flex-col gap-1">
            {view.many.steps.map((s) => (
              <li key={s.id}>
                <StepDoor step={s} id={s.id} onSelect={onSelect} />
              </li>
            ))}
          </ul>
        )}
      </div>
    );
  }

  return (
    <div className="@container flex flex-col gap-5 px-4 py-3">
      <div className="flex items-center gap-2">
        <ICON.workflow size={14} aria-hidden className="shrink-0 text-text-dim" />
        <h3 className="shrink-0 text-sm font-semibold">{view.title}</h3>
        <span className="min-w-0 truncate text-2xs text-text-dim">{view.subtitle}</span>
      </div>
      {view.readOnly && <ReadOnlyNote readOnly={view.readOnly} onEdit={onEdit} onOpenDesigner={onOpenDesigner} />}
      <section className="flex flex-col gap-2">
        <SectionHeader flush title={t("workflow-inspector-section-identity")} />
        <div className="flex flex-col gap-3">
          <Field label={t("workflow-inspector-name")}>
            <TextInput value={value.name} disabled={readOnly} onChange={(e) => commit({ ...value, name: e.target.value }, "workflow")} />
          </Field>
          <Field label={t("workflow-inspector-description")} hint={t("workflow-inspector-one-sentence-what")}>
            <TextArea rows={2} value={value.description ?? ""} disabled={readOnly} onChange={(e) => commit({ ...value, description: e.target.value }, "workflow")} />
          </Field>
          <Labelled label={t("workflow-inspector-tags")} hint={t("workflow-inspector-how-files-library")}>
            <TagInput value={tags} disabled={readOnly} onChange={(next) => commit({ ...value, tags: next })} suggestions={TAG_SUGGESTIONS} />
          </Labelled>
        </div>
      </section>
      <section className="flex flex-col gap-2">
        <SectionHeader flush title={t("workflow-inspector-section-runs")} />
        <Switch
          checked={Boolean(value.decision_making)}
          disabled={readOnly}
          onChange={(decision_making) => commit({ ...value, decision_making })}
          label={t("workflow-inspector-let-decision-making-agent-decide-runs-workflow")}
        />
      </section>
      <section className="flex flex-col gap-2">
        {/* The header is the group's caption; the hint reads under the editor, as a field's does. */}
        <SectionHeader flush title={t("workflow-inspector-inputs")} />
        <InputDefsEditor
          inputs={value.inputs ?? []}
          disabled={readOnly}
          onChange={(inputs) => commit({ ...value, inputs }, "inputs")}
          onRename={(from, to) => commit(renameInput(value, from, to))}
        />
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{t("workflow-inspector-what-run-started-steps-read-them", { TEMPLATE_HINT })}</p>
      </section>
      <section className="flex flex-col gap-2">
        <SectionHeader flush title={t("workflow-inspector-problems")} count={view.problems.count} />
        <ProblemsList problems={view.problems.list} unreadable={view.problems.unreadable} onSelect={(id) => onSelect(stepSelection(id))} />
      </section>
    </div>
  );
}

/** A step named in the flow's or the many's page, as a door to its own page — or its bare id when the workflow no longer has it. */
function StepDoor({ step, id, onSelect }: { step: Step | null; id: string; onSelect: (selection: Selection) => void }) {
  if (!step) return <code className="font-mono text-2xs text-text-dim">{id}</code>;
  const Icon = stepKindIcon(step.kind);
  return (
    <Button size="sm" variant="ghost" className="-ml-2 max-w-full" onClick={() => onSelect(stepSelection(step.id))}>
      <Icon size={12} aria-hidden className={familyInk(step.kind)} />
      <span className="truncate">{step.name || step.id}</span>
    </Button>
  );
}

/** *This run*: the step's state, who holds it and since when, what it said and produced, and what the person can do. */
function RunSection({
  view,
  run,
  onChanged,
  onOpenItem,
  onDecide,
}: {
  view: StepView;
  run: WorkflowRun | null;
  onChanged?: () => void;
  onOpenItem?: (item: string) => void;
  onDecide?: () => void;
}) {
  const facts = view.run;
  if (!facts || !run) return null;
  const row = facts.row;
  return (
    <section className="flex flex-col gap-2">
      <SectionHeader flush title={t("workflow-inspector-section-run")} trailing={row ? <StepStateChip state={row.state as Parameters<typeof StepStateChip>[0]["state"]} /> : undefined} />
      {row && (row.holder || row.startedAt !== null) && (
        <p className="flex flex-wrap items-center gap-x-2 text-2xs text-text-dim">
          {row.holder && <span className="first-letter:uppercase">{HOLDER_LABEL[row.holder as keyof typeof HOLDER_LABEL]}</span>}
          {row.startedAt !== null && (
            <span className="tnum">
              <RelativeTime at={row.startedAt} />
              {row.duration !== null && ` · ${row.duration}`}
            </span>
          )}
        </p>
      )}
      <StepRecordBody error={facts.record.error} answer={facts.record.answer as Answer | null | undefined} output={facts.record.output} fold={OUTPUT_FOLD} />
      <StepActions run={run.id} step={view.step} record={facts.record} onChanged={onChanged ?? noop} onOpenItem={onOpenItem} onDecide={onDecide} />
    </section>
  );
}
