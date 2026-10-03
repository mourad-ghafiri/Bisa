/** Types for `changedFilesModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { ChangesView, FileChangeView } from "../../types";

export { fileChipTone, fileChipWords, fileVerbs } from "./turnChangesModel.mjs";
export declare function changedFileRows(view: ChangesView | null | undefined): FileChangeView[];
export declare function changedByAgents(view: ChangesView | null | undefined): string[];
export declare function changedFilesHeaderWords(view: ChangesView | null | undefined, names?: (agent: string) => string): string;
export declare function splitPathForRow(path: string): { dir: string; base: string };
export declare function changeKindMark(kind: string): "+" | "−" | "~";
export declare function changeKindTone(kind: string): "ok" | "warn";
export declare function barWords(): { keepAll: string; undoAll: string; review: string; attach: string };
export declare function undoAllConfirmWords(view: ChangesView | null | undefined): { title: string; body: string; confirm: string };
export declare function settledAllWords(verdict: "keep" | "undo", files: number): string | null;
