/**
 * A project filter's fields — which projects, what changes (a commit, a push
 * or a fetch, a pull, a branch made or deleted, a pull request's change, a
 * merge, a change of files, a checkout's move), which branches, which files
 * — shared by a start that begins when a project changes and a wait that
 * holds for one. A project is named, never a path on a disk: fixed, or read
 * from an input of kind project; one or several, any of which is heard; none
 * chosen yet is a problem the validator names. The branches and the globs
 * are templates, any of them matching.
 */

import type { InputDef } from "../../../types";
import { useWorkspace } from "../../../shell/useWorkspaceData";
import { ICON, Labelled, Select } from "../../../ui";
import { PROJECT_CHANGES } from "../stepKinds.mjs";
import type { OneOrMany, ProjectChangeEx, ProjectFilterEx, ProjectRef } from "./eventFilters";
import { WordChecks, WordsInput } from "./ManyFields";
import { addWord, hasWord, removeWord } from "./manyModel.mjs";
import { asList } from "./startForm.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** The placeholder an input of kind project reads as. */
const inputWord = (name: string) => `{inputs.${name}}`;

/**
 * Projects, one or several: each a chip — its name, or `{inputs.x}` for an
 * input of kind project — and one select to add the next from the workspace
 * or from the inputs. A project gone from the workspace keeps its chip, by
 * id, so what the design names is never silently dropped.
 */
export function ProjectRefsInput({
  value,
  inputs,
  onChange,
  disabled,
}: {
  value: OneOrMany<ProjectRef> | null | undefined;
  inputs: readonly InputDef[];
  onChange: (next: OneOrMany<ProjectRef> | null) => void;
  disabled?: boolean;
}) {
  const ws = useWorkspace();
  const refs = asList(value);
  const nameOf = (ref: ProjectRef) => (typeof ref === "object" ? inputWord(ref.input) : (ws.projects.find((p) => p.project.id === ref)?.project.name ?? ref));
  const projects = ws.projects.filter((p) => !hasWord(value, p.project.id));
  const candidates = inputs.filter((i) => i.kind === "project" && !hasWord(value, { input: i.name }));
  return (
    <div className="flex flex-col gap-1.5">
      {refs.length > 0 && (
        <div className="flex flex-wrap items-center gap-1">
          {refs.map((ref) => {
            const word = nameOf(ref);
            return (
              <button
                key={typeof ref === "object" ? inputWord(ref.input) : ref}
                type="button"
                title={disabled ? undefined : t("workflow-many-fields-remove", { word })}
                aria-label={disabled ? word : t("workflow-many-fields-remove", { word })}
                disabled={disabled}
                onClick={() => onChange(removeWord(value, ref))}
                className="anim inline-flex h-5 max-w-full items-center gap-1 rounded-full bg-surface-2 pr-1.5 pl-2 text-2xs text-text hover:bg-selected disabled:opacity-60"
              >
                <span className="truncate">{word}</span>
                {!disabled && <ICON.close size={10} aria-hidden className="shrink-0 text-text-dim" />}
              </button>
            );
          })}
        </div>
      )}
      {(projects.length > 0 || candidates.length > 0) && (
        <Select
          value=""
          disabled={disabled}
          aria-label={t("workflow-project-filter-fields-add-project")}
          onChange={(e) => {
            const v = e.target.value;
            if (!v) return;
            onChange(addWord(value, v.startsWith("input:") ? { input: v.slice("input:".length) } : v));
          }}
        >
          <option value="">{refs.length ? t("workflow-many-fields-add") : t("workflow-project-filter-fields-add-project")}</option>
          {projects.map((p) => (
            <option key={p.project.id} value={p.project.id}>
              {p.project.name}
            </option>
          ))}
          {candidates.length > 0 && (
            <optgroup label={t("workflow-notify-step-form-read-from-input-start")}>
              {candidates.map((i) => (
                <option key={i.name} value={`input:${i.name}`}>{t("workflow-connector-step-form-input", { i: i.name })}</option>
              ))}
            </optgroup>
          )}
        </Select>
      )}
    </div>
  );
}

const CHANGES = PROJECT_CHANGES.map((c) => ({ word: c.change as ProjectChangeEx, label: c.label }));

export function ProjectFilterFields<F extends object>({
  value,
  inputs,
  onChange,
  disabled,
}: {
  value: F;
  inputs: readonly InputDef[];
  onChange: (next: F) => void;
  disabled?: boolean;
}) {
  const filter = value as ProjectFilterEx;
  const set = (patch: Partial<ProjectFilterEx>) => onChange({ ...value, ...patch });
  const changes = asList(filter.change ?? "commit");
  return (
    <div className="flex flex-col gap-3">
      <Labelled label={t("workflow-project-filter-fields-projects")} hint={t("workflow-project-filter-fields-projects-hint")}>
        <ProjectRefsInput value={filter.project} inputs={inputs} disabled={disabled} onChange={(project) => set({ project })} />
      </Labelled>
      <Labelled label={t("workflow-project-filter-fields-change")} hint={t("workflow-project-filter-fields-change-hint")}>
        <WordChecks options={CHANGES} value={filter.change} fallback="commit" columns={3} disabled={disabled} onChange={(change) => set({ change })} />
      </Labelled>
      <Labelled label={t("workflow-project-filter-fields-branch")} hint={t("workflow-project-filter-fields-branch-hint")}>
        <WordsInput mono value={filter.branch} disabled={disabled} /* for the machine */ placeholder="main" aria-label={t("workflow-project-filter-fields-branch")} onChange={(branch) => set({ branch })} />
      </Labelled>
      {changes.includes("files") && (
        <Labelled label={t("workflow-project-filter-fields-files")} hint={t("workflow-project-filter-fields-files-hint")}>
          <WordsInput mono value={filter.glob} disabled={disabled} /* for the machine */ placeholder="*.csv" aria-label={t("workflow-project-filter-fields-files")} onChange={(glob) => set({ glob })} />
        </Labelled>
      )}
      {(changes.includes("pull_request") || changes.includes("merge")) && <p className="text-2xs text-text-dim">{t("workflow-project-filter-fields-pull-requests-note")}</p>}
    </div>
  );
}
