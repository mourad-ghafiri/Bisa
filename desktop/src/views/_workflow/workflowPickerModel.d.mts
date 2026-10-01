/** Types for `workflowPickerModel.mjs`. */
import type { Workflow, WorkflowRow } from "../../types";

export declare function templateValue(slug: string): string;
export declare function templateSlugOf(value: string | null | undefined): string | null;
export declare function optionLabel(row: Pick<WorkflowRow, "workflow" | "problems">): string;
export declare function unlistedLabel(workflow: Pick<Workflow, "name" | "archived" | "origin">): string;
export declare function installedRow(installed: { workflows?: readonly { slug: string; id: string }[] } | null | undefined, rows: readonly WorkflowRow[] | null | undefined, slug: string): WorkflowRow | null;
/** What an install answered for `slug`: the workflow it made, or `null` when the template was already installed. */
export declare function installedHit(installed: { workflows?: readonly { slug: string; id: string }[] } | null | undefined, slug: string): { slug: string; id: string } | null;
/** What *Use template* says once the install answered. */
export declare function installedWords(slug: string, installed: { workflows?: readonly { slug: string; id: string }[]; agents?: readonly string[]; skills?: readonly string[] } | null | undefined): string;
