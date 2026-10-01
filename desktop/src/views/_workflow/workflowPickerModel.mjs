/**
 * The workflow picker's words and its one rule: what each option says —
 * a workflow by its name, with how many problems stand in its way; a value
 * no group carries by what it is, archived or another goal's design — and
 * which row a template became once it was installed. The picker
 * (`WorkflowPicker.tsx`) reads the lists and paints.
 */

import { t } from "../../i18n/l10n.mjs";

/** How an option names a catalog template: installed when it is picked, so never an id. */
const TEMPLATE = "template:";

/** The option's value for a template, by slug. */
export function templateValue(slug) {
  return `${TEMPLATE}${slug}`;
}

/** The slug of the template an option's value names; `null` for an installed workflow's id, or for nothing. */
export function templateSlugOf(value) {
  return typeof value === "string" && value.startsWith(TEMPLATE) && value.length > TEMPLATE.length ? value.slice(TEMPLATE.length) : null;
}

/** An option of a group: the workflow's name, and its problems counted when it has any — it can be picked, and cannot start. */
export function optionLabel(row) {
  const name = row?.workflow?.name ?? "";
  const n = (row?.problems ?? []).length;
  return n > 0 ? t("workflow-workflow-picker-option-problems", { name, n }) : name;
}

/** A value no group carries, said as what it is rather than drawn as the first option. */
export function unlistedLabel(workflow) {
  const name = workflow?.name ?? "";
  if (workflow?.archived) return t("workflow-workflow-picker-option-archived", { name });
  if (workflow?.origin?.origin === "goal") return t("workflow-workflow-picker-option-another-goals-design", { name });
  return name;
}

/** What an install answered for `slug`: the workflow it made, or `null` when the template was already installed. */
export function installedHit(installed, slug) {
  return (installed?.workflows ?? []).find((w) => w.slug === slug) ?? null;
}

/**
 * What *Use template* says once the install answered: what it installed and
 * what came with it — the agents its steps name, their skills — or that the
 * copy which already existed is being opened.
 * @param {string} slug
 * @param {{workflows?: {slug: string}[], agents?: string[], skills?: string[]} | null | undefined} installed
 */
export function installedWords(slug, installed) {
  if (installedHit(installed, slug) === null) return t("workflow-template-gallery-already-installed-opening", { slug });
  const brought = [...(installed?.agents ?? []), ...(installed?.skills ?? [])];
  return brought.length > 0 ? t("workflow-template-gallery-installed-2", { slug, brought: brought.join(", ") }) : t("workflow-template-gallery-installed-3", { slug });
}

/**
 * The row a template became: the workflow the install answered for that
 * slug, else the library's workflow born of it — `null` when neither is
 * there, which the picker says rather than keeping a value that names
 * nothing.
 * @param {{workflows?: {slug: string, id: string}[]} | null | undefined} installed what the install answered
 * @param {readonly {workflow: {id: string, origin?: {origin?: string, slug?: string}}}[] | null | undefined} rows every workflow, read after the install
 * @param {string} slug
 */
export function installedRow(installed, rows, slug) {
  const all = rows ?? [];
  const hit = installedHit(installed, slug);
  return (hit ? all.find((r) => r.workflow.id === hit.id) : undefined) ?? all.find((r) => r.workflow.origin?.origin === "catalog" && r.workflow.origin.slug === slug) ?? null;
}
