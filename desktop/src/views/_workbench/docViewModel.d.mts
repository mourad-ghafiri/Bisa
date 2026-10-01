/** Types for `docViewModel.mjs`. */

import type { Place } from "../../ui/keptScrollModel.mjs";

export declare const EDITOR_VIEW: string;
export declare const PAGE_SCROLL: string;
export declare const DOC_MODE: string;
export declare function scrollName(name: string): string;
export declare function editorViewValue(raw: unknown): object | null;
export declare function placeValue(raw: unknown): Place | null;
