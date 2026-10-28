/**
 * A `connector` step: which connector installed here, which of its
 * operations, as which account, with what parameters.
 *
 * The pickers read the definition (`useConnectors`, `useConnectorDetail`) so
 * a step can only name an operation the connector has and a parameter it
 * takes; every parameter value is a template over the run, rendered as text
 * and typed by the operation's own parameter kind. An unset optional
 * parameter is absent from the wire, never an empty string — and so is a
 * connector or an operation not chosen yet: the empty option reads as `""`
 * here and is written as `null`, which the node lists as a problem where
 * `""` is an id it cannot parse. The account is
 * the connector's default when unnamed, one of this machine's by id, or an
 * input of kind `account` for that connector — the shape a template stays
 * portable in. A `writes` operation is flagged: an approval belongs before
 * it — or the person's own word that it runs **unattended**, the switch
 * shown beside the flag; the validator refuses silence (`ungated_write`).
 * That word is said of one write: picking another operation or another
 * connector drops it (`withOperation`, `withConnector`).
 * The spellings and the words are `connectorStepModel.mjs`'s.
 */

import type { ConnectorOperationDef, ConnectorParamDef, InputDef, Step } from "../../../types";
import { Button, Chip, Field, ICON, Labelled, Select, Switch } from "../../../ui";
import { TEMPLATE_HINT } from "../stepKinds.mjs";
import { useConnectorDetail, useConnectors } from "../useConnectors";
import { DEFAULT_ACCOUNT, UNATTENDED_HINT, UNATTENDED_LABEL, accountFromValue, accountValue, accountsNote, connectorPickerHint, definitionHint, paramHint, fixedValue, inputValue, strayAccount, strayConnector, strayOperation, strayParams, withConnector, withOperation, withParam, writeWords } from "./connectorStepModel.mjs";
import { JsonField } from "./JsonField";
import { TemplateField } from "./TemplateField";
import { t } from "../../../i18n/l10n.mjs";
import { rich } from "../../../i18n/rich";

type Connector = Extract<Step, { kind: "connector" }>;

export function ConnectorStepForm({ step, inputs, onChange, disabled }: { step: Connector; inputs: InputDef[]; onChange: (next: Step) => void; disabled?: boolean }) {
  const set = (patch: Partial<Connector>) => onChange({ ...step, ...patch });
  const connectors = useConnectors();
  const connector = step.connector ?? null;
  const chosenOperation = step.operation ?? null;
  const detail = useConnectorDetail(connector);
  const def = detail.data?.connector ?? null;
  const accounts = detail.data?.accounts ?? [];
  const operation: ConnectorOperationDef | null =
    chosenOperation === null ? null : (def?.operations.find((o) => o.id === chosenOperation) ?? null);
  const params = step.params ?? {};
  const accountInputs = inputs.filter((i) => i.kind === "account" && connector !== null && i.connector === connector);
  // What the definition names and the pickers do not offer, said in the picker — never *pick one* over a name still written.
  const notInstalled = strayConnector(connector, connectors.rows);
  const notItsOperation = strayOperation(chosenOperation, def?.operations ?? [], def ? def.operations : null);
  // An account the step names and the picker does not offer — gone from this machine, an input gone or moved to another connector — is said in the picker too.
  const notAnAccount = strayAccount(step.account, detail.data ? accounts : null, inputs, connector);

  const setParam = (name: string, value: string) => set({ params: withParam(params, name, value) });

  const declared: ConnectorParamDef[] = operation?.params ?? [];
  // A stray is judged against the definition, never against one not read yet.
  const stray = def ? strayParams(params, declared) : [];
  const unattended = step.unattended ?? false;

  return (
    <div className="flex flex-col gap-3">
      <div className="grid gap-3 @sm:grid-cols-2">
        <Field label={t("workflow-step-kinds-connector")} hint={connectorPickerHint({ count: connectors.rows.length, error: connectors.error })}>
          <Select
            className="font-mono"
            value={connector ?? ""}
            disabled={disabled}
            onChange={(e) => onChange(withConnector(step, e.target.value))}
          >
            <option value="">{t("workflow-connector-step-form-pick-connector")}</option>
            {connectors.rows.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} · {c.id}
              </option>
            ))}
            {connector !== null && notInstalled !== null && <option value={connector}>{notInstalled}</option>}
          </Select>
        </Field>
        <Field label={t("workflow-connector-step-form-operation")} hint={definitionHint(detail.error, operation ? operation.description : t("workflow-connector-step-form-one-connector-s-operations"))}>
          <Select
            className="font-mono"
            value={chosenOperation ?? ""}
            disabled={disabled || !def}
            onChange={(e) => onChange(withOperation(step, e.target.value))}
          >
            <option value="">{t("workflow-connector-step-form-pick-operation")}</option>
            {(def?.operations ?? []).map((o) => (
              <option key={o.id} value={o.id}>
                {t("workflow-connector-step-form-operation-option", { id: o.id, name: o.name, writes: o.writes ? "yes" : "no" })}
              </option>
            ))}
            {chosenOperation !== null && notItsOperation !== null && <option value={chosenOperation}>{notItsOperation}</option>}
          </Select>
        </Field>
      </div>
      {operation?.writes && (
        <div className="flex flex-col gap-2 rounded-control border border-warn/40 bg-warn-soft px-2 py-1.5">
          <p className="flex items-center gap-1.5 text-2xs text-warn">
            <ICON.warn size={12} aria-hidden />
            {writeWords(unattended)}
          </p>
          <Switch checked={unattended} onChange={(on) => set({ unattended: on })} label={UNATTENDED_LABEL} hint={UNATTENDED_HINT} disabled={disabled} />
        </div>
      )}
      <Field
        label={t("workflow-connector-step-form-account")}
        hint={accountsNote({ loading: detail.loading, error: detail.error, count: accounts.length, inputs: accountInputs.length }) ?? undefined}
      >
        <Select value={accountValue(step.account)} disabled={disabled || !def} onChange={(e) => set({ account: accountFromValue(e.target.value) as Connector["account"] })}>
          <option value={DEFAULT_ACCOUNT}>{t("workflow-connector-step-form-connector-s-default-account")}</option>
          {notAnAccount !== null && <option value={accountValue(step.account)}>{notAnAccount}</option>}
          {accounts.length > 0 && (
            <optgroup label={t("workflow-connector-step-form-account-machine")}>
              {accounts.map((a) => (
                <option key={a.id} value={fixedValue(a.id)}>
                  {t("workflow-connector-step-form-account-option", { label: a.label, default: a.default ? "yes" : "no" })}
                </option>
              ))}
            </optgroup>
          )}
          {accountInputs.length > 0 && (
            <optgroup label={t("workflow-connector-step-form-from-input-start")}>
              {accountInputs.map((i) => (
                <option key={i.name} value={inputValue(i.name)}>{t("workflow-connector-step-form-input", { i: i.name })}</option>
              ))}
            </optgroup>
          )}
        </Select>
      </Field>
      <Labelled label={t("workflow-connector-step-form-parameters")} hint={declared.length === 0 ? t("workflow-connector-step-form-operation-takes-no-parameters") : t("workflow-connector-step-form-one-template-per-parameter-required-one", { TEMPLATE_HINT })}>
        <div className="flex flex-col gap-2">
          {declared.map((p) => (
            <div key={p.name} className="flex flex-col gap-0.5">
              <div className="flex items-center gap-1.5">
                <code className="font-mono text-2xs">{p.name}</code>
                <Chip tone="quiet">{p.kind ?? "text"}</Chip>{/* for the machine: the parameter kind's wire word, drawn as p.kind is */}
                {p.required && <Chip tone="warn">{t("workflow-connector-step-form-required")}</Chip>}
                {paramHint(p) && <Chip tone="quiet">{t("workflow-connector-step-form-default", { value: paramHint(p) })}</Chip>}
                <span className="truncate text-2xs text-text-dim">{p.label}</span>
              </div>
              <TemplateField
                value={params[p.name] ?? ""}
                rows={p.kind === "json" ? 3 : 1}
                disabled={disabled}
                ariaLabel={t("workflow-connector-step-form-parameter-named", { name: p.name })}
                placeholder={paramHint(p) || undefined}
                onCommit={(v) => setParam(p.name, v)}
              />
              {p.kind === "file" && <span className="text-2xs text-text-dim">{t("workflow-connector-step-form-file-path-inside-checkout")}</span>}
              {p.doc && <span className="text-2xs text-text-dim">{p.doc}</span>}
            </div>
          ))}
          {stray.map((name) => (
            <div key={name} className="flex items-center gap-1.5">
              <code className="font-mono text-2xs text-danger">{name}</code>
              <span className="text-2xs text-text-dim">{t("workflow-connector-step-form-not-parameter-operation")}</span>
              <Button size="sm" variant="ghost" disabled={disabled} onClick={() => setParam(name, "")}>{t("workflow-connector-step-form-remove")}</Button>
            </div>
          ))}
        </div>
      </Labelled>
      {operation && (
        <p className="text-2xs text-text-dim">
          {operation.output?.select
            ? rich("workflow-connector-step-form-output-selected", { selected: <code className="font-mono">{operation.output.select}</code> }, { step: step.id })
            : t("workflow-connector-step-form-output-whole-answer", { step: step.id })}
        </p>
      )}
      <JsonField
        label={t("workflow-agent-step-form-output-schema")}
        hint={t("workflow-connector-step-form-json-schema-selected-answer-must-satisfy")}
        value={step.output_schema}
        rows={3}
        disabled={disabled}
        onCommit={(output_schema) => set({ output_schema })}
      />
    </div>
  );
}
