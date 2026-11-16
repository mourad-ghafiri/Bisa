/**
 * What a `connector` step's form and a connector start's share, drawn once
 * so the two cannot drift: the connectors a picker offers — grouped by
 * whether this machine holds an account, since forty-five in one run is a
 * list nobody reads — and the rows of an operation's parameters: the name,
 * its kind, whether a person must fill it, what an empty field sends, the
 * template itself, and what the definition says of it. The rules are
 * `connectorStepModel.mjs`'s.
 */

import type { ConnectorParamDef, ConnectorRow } from "../../../types";
import { Button, Chip } from "../../../ui";
import { connectorGroups, isRequired, paramHint } from "./connectorStepModel.mjs";
import { TemplateField } from "./TemplateField";
import { t } from "../../../i18n/l10n.mjs";

/** The connectors installed here, as a `<select>`'s options: those with an account first, then the rest, each by name. */
export function ConnectorOptions({ rows }: { rows: readonly ConnectorRow[] }) {
  const { ready, rest } = connectorGroups(rows);
  const option = (c: ConnectorRow) => (
    <option key={c.id} value={c.id}>
      {c.name} · {c.id}
    </option>
  );
  // One group is no grouping: the options stand bare.
  if (ready.length === 0 || rest.length === 0) return <>{[...ready, ...rest].map(option)}</>;
  return (
    <>
      <optgroup label={t("workflow-connector-step-form-group-ready")}>{ready.map(option)}</optgroup>
      <optgroup label={t("workflow-connector-step-form-group-rest")}>{rest.map(option)}</optgroup>
    </>
  );
}

/**
 * One row per parameter the operation declares, then one per parameter the
 * call still sets and the operation no longer takes — shown so it can be
 * removed, never silently dropped. A value is committed on blur or Enter: a
 * template rewritten on every keystroke drops a space typed before a word.
 */
export function ParamRows({
  declared,
  stray,
  params,
  disabled,
  onParam,
}: {
  declared: readonly ConnectorParamDef[];
  stray: readonly string[];
  params: Record<string, string>;
  disabled?: boolean;
  onParam: (name: string, value: string) => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      {declared.map((p) => (
        <div key={p.name} className="flex flex-col gap-0.5">
          <div className="flex items-center gap-1.5">
            <code className="font-mono text-2xs">{p.name}</code>
            <Chip tone="quiet">{p.kind ?? "text"}</Chip>{/* for the machine: the parameter kind's wire word, drawn as p.kind is */}
            {/* What a person must fill: required, and with no default an empty field would send in its place. */}
            {isRequired(p) && <Chip tone="warn">{t("workflow-connector-step-form-required")}</Chip>}
            {paramHint(p) && <Chip tone="quiet">{t("workflow-connector-step-form-default", { value: paramHint(p) })}</Chip>}
            <span className="truncate text-2xs text-text-dim">{p.label}</span>
          </div>
          <TemplateField
            value={params[p.name] ?? ""}
            rows={p.kind === "json" ? 3 : 1}
            disabled={disabled}
            ariaLabel={t("workflow-connector-step-form-parameter-named", { name: p.name })}
            placeholder={paramHint(p) || undefined}
            onCommit={(v) => onParam(p.name, v)}
          />
          {p.kind === "file" && <span className="text-2xs text-text-dim">{t("workflow-connector-step-form-file-path-inside-checkout")}</span>}
          {p.kind === "path" && <span className="text-2xs text-text-dim">{t("workflow-connector-step-form-path-on-platform")}</span>}
          {p.doc && <span className="text-2xs text-text-dim">{p.doc}</span>}
        </div>
      ))}
      {stray.map((name) => (
        <div key={name} className="flex items-center gap-1.5">
          <code className="font-mono text-2xs text-danger">{name}</code>
          <span className="text-2xs text-text-dim">{t("workflow-connector-step-form-not-parameter-operation")}</span>
          <Button size="sm" variant="ghost" disabled={disabled} onClick={() => onParam(name, "")}>{t("workflow-connector-step-form-remove")}</Button>
        </div>
      ))}
    </div>
  );
}
