/**
 * Choose a workflow: one from the library, or a catalog template — which
 * is installed when it is picked, so the value is always an installed
 * workflow's id (or null). With a `goal`, that goal's own designs are
 * offered too, in their own group: a design belongs to its goal and never
 * shows up as "this workspace".
 *
 * One `<select>`, up to three groups. A template is named by slug and
 * installed on change; the id that comes back is what the caller keeps.
 */

import { useState } from "react";
import { api } from "../../api";
import type { Workflow, WorkflowRow } from "../../types";
import { Select, useToast } from "../../ui";
import { useAsync } from "../_work/useAsync";
import { useEngineEvents } from "../../bus";
import { WORKFLOW_FACTS } from "./designerSession.mjs";
import { installedRow, optionLabel, templateSlugOf, templateValue, unlistedLabel } from "./workflowPickerModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

export function WorkflowPicker({
  value,
  onChange,
  allowNone = false,
  noneLabel = tr("workflow-workflow-picker-no-workflow"),
  disabled,
  ariaLabel = tr("workflow-workflow-picker-workflow"),
  goal,
}: {
  value: string | null;
  /** The installed workflow's id (or null), and the definition when known. */
  onChange: (id: string | null, workflow?: Workflow) => void;
  allowNone?: boolean;
  noneLabel?: string;
  disabled?: boolean;
  ariaLabel?: string;
  /** Offer this goal's own designs beside the library. */
  goal?: string;
}) {
  const toast = useToast();
  const installed = useAsync((s) => api.workflows({ scope: "library" }, s), []);
  const designs = useAsync(
    (s) => (goal ? api.workflows({ goal }, s) : Promise.resolve({ workflows: [] as WorkflowRow[] })),
    [goal],
  );
  const catalog = useAsync((s) => api.catalog({ kind: "workflow" }, s), []);
  const [installing, setInstalling] = useState(false);
  // A workflow saved, archived, deleted or installed elsewhere moves what may be
  // picked: an archived one is refused for a goal, a deleted one is gone.
  useEngineEvents((e) => {
    if (!WORKFLOW_FACTS.includes(e.payload.type)) return;
    installed.reload();
    designs.reload();
    catalog.reload();
  });

  const rows = installed.data?.workflows ?? [];
  const designRows = designs.data?.workflows ?? [];
  const templates = (catalog.data?.entries ?? []).filter((e) => !e.installed);
  // A value no group carries — archived, or another goal's design — is
  // fetched and shown as it is, rather than rendered as the first option.
  const listed = !value || [...rows, ...designRows].some((r) => r.workflow.id === value);
  const unlisted = useAsync(async (s) => (value && !listed && !installed.loading ? (await api.workflow(value, s)).workflow : null), [value, listed, installed.loading]);

  const pick = async (v: string) => {
    if (v === "") return onChange(null);
    const slug = templateSlugOf(v);
    if (slug !== null) {
      setInstalling(true);
      try {
        const { installed: done } = await api.installCatalogEntry("workflow", slug);
        const { workflows } = await api.workflows({ scope: "all" });
        const row = installedRow(done, workflows, slug);
        installed.reload();
        catalog.reload();
        if (row) onChange(row.workflow.id, row.workflow);
        else toast.error(tr("workflow-workflow-picker-installed-but-could-not-find-afterwards", { slug }));
      } catch (e) {
        toast.error(e instanceof Error ? e.message : String(e));
      } finally {
        setInstalling(false);
      }
      return;
    }
    onChange(v, [...rows, ...designRows].find((r) => r.workflow.id === v)?.workflow);
  };

  return (
    <Select value={value ?? ""} aria-label={ariaLabel} disabled={disabled || installing} onChange={(e) => void pick(e.target.value)}>
      <option value="">{installing ? tr("workflow-workflow-picker-installing") : installed.loading ? tr("workflow-workflow-picker-loading") : allowNone ? noneLabel : tr("workflow-workflow-picker-pick-workflow")}</option>
      {value && !listed && (
        <option value={value}>{unlisted.data && unlisted.data.id === value ? unlistedLabel(unlisted.data) : value}</option>
      )}
      {designRows.length > 0 && (
        <optgroup label={tr("workflow-workflow-picker-goal-s-designs")}>
          {designRows.map((r) => (
            <option key={r.workflow.id} value={r.workflow.id}>
              {optionLabel(r)}
            </option>
          ))}
        </optgroup>
      )}
      {rows.length > 0 && (
        <optgroup label={tr("workflow-workflow-picker-library")}>
          {rows.map((r) => (
            <option key={r.workflow.id} value={r.workflow.id}>
              {optionLabel(r)}
            </option>
          ))}
        </optgroup>
      )}
      {templates.length > 0 && (
        <optgroup label={tr("workflow-workflow-picker-catalog-templates-installed-when-picked")}>
          {templates.map((t) => (
            <option key={t.slug} value={templateValue(t.slug)}>
              {t.name}
            </option>
          ))}
        </optgroup>
      )}
    </Select>
  );
}
