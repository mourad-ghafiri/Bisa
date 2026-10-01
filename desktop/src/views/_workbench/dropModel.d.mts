/** Types for `dropModel.mjs`. */

import type { Route } from "../../router";

export declare function isFileDrop(types: Iterable<string> | ArrayLike<string> | null | undefined): boolean;
export declare function pathsForDrop(names: readonly string[], paths: readonly string[]): string[];
export declare function looseTargetRoot(route: Route | null | undefined, roots: readonly string[]): string | null;
