/**
 * The right-hand pane of the designer: the selected step's forms, or — with
 * nothing selected — the workflow itself: name, description, tags, inputs,
 * and the problems list.
 *
 * Every edit goes back up as a whole new definition (`workflowGraph.mjs`'s
 * functions), so the history and the autosave see one value.
 */

import { useId, useMemo, useState } from "react";
import type { InputDef, ListenerView, NewWorkflowBody, Problem, Step, Workflow } from "../../types";
import { Button, Chip, Field, ICON, Labelled, Switch, TAG_VOCABULARY, TagInput, TextArea, TextInput, cn, stepKindIcon } from "../../ui";

/** One array for the life of the module: a literal per render is a new identity to memoise on. */
const TAG_SUGGESTIONS: string[] = [...TAG_VOCABULARY];
import { familyInk } from "./familyInk";
import { AgentStepForm } from "./forms/AgentStepForm";
import { ApprovalStepForm } from "./forms/ApprovalStepForm";
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
import { ProblemsList } from "./ProblemsList";
import { TEMPLATE_HINT, kindLabel } from "./stepKinds.mjs";
import { removeStep, renameInput, renameStep, replaceStep, upstreamOf, type Definition } from "./workflowGraph.mjs";
import { layout, withPositions } from "./workflowLayout.mjs";
import { t } from "../../i18n/l10n.mjs";

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

export function Inspector<D extends NewWorkflowBody | Workflow>({
  value,
  selected,
  problems,
  unreadable,
  onChange,
  onSelect,
  readOnly,
  editable,
  header,
  host = null,
  listeners = [],
}: {
  value: D;
  selected: string | null;
  problems: readonly Problem[];
  /** Why the node could not judge the current design, when it could not. */
  unreadable?: string | null;
  onChange: (next: D) => void;
  onSelect: (id: string | null) => void;
  readOnly?: boolean;
  /** In an amendment: the steps that may still change. Null means every step. */
  editable?: Set<string> | null;
  /** Something the screen wants above the workflow's own fields. */
  header?: React.ReactNode;
  /** Who listens with the design's starts once stored — a library workflow or a goal: a hook's call is theirs. */
  host?: StartHost | null;
  /** The host's listeners while it listens: a hook's public call and its secret's state. */
  listeners?: readonly ListenerView[];
}) {
  const step = selected ? value.steps.find((s) => s.id === selected) : undefined;
  // One array per value, not per render: `TagInput` memoises on it.
  const tags = useMemo(() => [...(value.tags ?? [])], [value.tags]);
  // *Flow and failure* stays as the person left it from one step to the next.
  const [flowOpen, setFlowOpen] = useState(false);
  const flowId = useId();
  const tagsId = useId();

  if (step) {
    const frozen = readOnly || (editable !== null && editable !== undefined && !editable.has(step.id));
    const mine = problems.filter((p) => p.step === step.id);
    const Icon = stepKindIcon(step.kind);
    return (
      <div className="@container flex flex-col gap-4 px-4 py-3">
        <div className="flex items-center gap-2">
          <Icon size={14} aria-hidden className={familyInk(step.kind)} />
          <h3 className="min-w-0 flex-1 truncate text-sm font-semibold">{step.name || step.id}</h3>
          <Chip tone="quiet">{kindLabel(step.kind)}</Chip>
          <Button size="sm" variant="ghost" onClick={() => onSelect(null)} aria-label={t("workflow-inspector-back-workflow")}>
            <ICON.close size={12} aria-hidden />
          </Button>
        </div>
        {frozen && !readOnly && (
          <p className="rounded-control border border-border bg-surface-2 px-2 py-1.5 text-2xs text-text-dim">{t("workflow-inspector-step-has-started")}</p>
        )}
        {mine.length > 0 && <ProblemsList problems={mine} />}
        {/* Keyed by the step: a draft, a picked word or a secret shown under one step never carries over to the next one picked.
            What the step is first — its id and name, then its kind's own form — and how it joins, fails and repeats folded under them. */}
        <StepCommonForm
          key={step.id}
          part="identity"
          step={step}
          steps={value.steps}
          disabled={frozen}
          onChange={(next) => onChange(replaceStep(value, step.id, next))}
          onRename={(to) => {
            onChange(renameStep(value, step.id, to));
            onSelect(to);
          }}
        />
        <KindForm key={step.id} step={step} wf={value} disabled={frozen} host={host} listeners={listeners} onChange={(next) => onChange(replaceStep(value, step.id, next))} />
        <div className="border-t border-hairline pt-3">
          <button
            type="button"
            aria-expanded={flowOpen}
            aria-controls={flowId}
            onClick={() => setFlowOpen((o) => !o)}
            className="anim -mx-1 flex w-full items-center gap-1.5 rounded-control px-1 py-0.5 text-left text-sm font-semibold text-text hover:bg-surface-2"
          >
            <ICON.collapsed size={12} aria-hidden className={cn("anim shrink-0 text-text-dim", flowOpen && "rotate-90")} />
            {t("workflow-inspector-flow-and-failure")}
          </button>
          {flowOpen && (
            <div id={flowId} className="mt-3 flex flex-col gap-3">
              {/* A flow drawn here writes every derived position down first, as one drawn on the canvas does: the picture never reflows under the person. */}
              <ThenField key={`then:${step.id}`} wf={value} step={step} disabled={frozen} onChange={(next) => onChange(withPositions(next as D, layout(value).positions))} />
              <StepCommonForm
                key={step.id}
                part="flow"
                step={step}
                steps={value.steps}
                inputs={value.inputs ?? []}
                disabled={frozen}
                onChange={(next) => onChange(replaceStep(value, step.id, next))}
                onRename={(to) => {
                  onChange(renameStep(value, step.id, to));
                  onSelect(to);
                }}
              />
            </div>
          )}
        </div>
        {!frozen && (
          <div>
            <Button
              size="sm"
              variant="danger"
              onClick={() => {
                onChange(removeStep(value, step.id));
                onSelect(null);
              }}
            >
              <ICON.delete size={12} aria-hidden />{t("workflow-inspector-remove-step")}</Button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="@container flex flex-col gap-4 px-4 py-3">
      {header}
      <Field label={t("workflow-inspector-name")}>
        <TextInput value={value.name} disabled={readOnly} onChange={(e) => onChange({ ...value, name: e.target.value })} />
      </Field>
      <Field label={t("workflow-inspector-description")} hint={t("workflow-inspector-one-sentence-what")}>
        <TextArea rows={2} value={value.description ?? ""} disabled={readOnly} onChange={(e) => onChange({ ...value, description: e.target.value })} />
      </Field>
      <Labelled label={t("workflow-inspector-tags")} hint={t("workflow-inspector-how-files-library")}>
        <TagInput id={tagsId} value={tags} disabled={readOnly} onChange={(tags) => onChange({ ...value, tags })} suggestions={TAG_SUGGESTIONS} />
      </Labelled>
      <Switch
        checked={Boolean(value.decision_making)}
        disabled={readOnly}
        onChange={(decision_making) => onChange({ ...value, decision_making })}
        label={t("workflow-inspector-let-decision-making-agent-decide-runs-workflow")}
      />
      <Labelled label={t("workflow-inspector-inputs")} hint={t("workflow-inspector-what-run-started-steps-read-them", { TEMPLATE_HINT })}>
        <InputDefsEditor
          inputs={value.inputs ?? []}
          disabled={readOnly}
          onChange={(inputs) => onChange({ ...value, inputs })}
          onRename={(from, to) => onChange(renameInput(value, from, to))}
        />
      </Labelled>
      <div className="mt-2">
        <h3 className="mb-1.5 text-2xs font-semibold text-text-dim">{t("workflow-goal-workflow-tab-problems")}</h3>
        <ProblemsList problems={problems} unreadable={unreadable} onSelect={onSelect} />
      </div>
    </div>
  );
}
