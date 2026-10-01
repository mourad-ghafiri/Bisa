/** Types for `changesFilterModel.mjs`. */

import type { GitFileRow } from "../../types";
import type { ChangesFilter } from "../_workbench/rightPanelModel.mjs";

export declare function admits(row: GitFileRow | null | undefined, filter: ChangesFilter): boolean;
export declare function filterGitFiles(files: readonly GitFileRow[] | null | undefined, filter: ChangesFilter): GitFileRow[];
export declare function filterCounts(files: readonly GitFileRow[] | null | undefined): Record<ChangesFilter, number>;
export declare function filterWords(filter: ChangesFilter, count: number): string;
export declare function emptyWords(filter: ChangesFilter): string;
