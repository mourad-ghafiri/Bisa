/** Types for `changesBulkModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { StageScopes } from "./gitFiles.mjs";

export type BulkVerb = "stage_tracked" | "stage_untracked" | "unstage_all" | "discard_all" | "delete_untracked";
export interface BulkMenuItem {
  id: BulkVerb;
  label: string;
  paths: string[];
  disabled: boolean;
  separatorBefore?: boolean;
  danger?: boolean;
}
/** The toolbar's `▾`: the other stage scopes, then apart the two throw-aways, each with its count and off at zero. */
export declare function bulkMenu(scopes: StageScopes): BulkMenuItem[];
