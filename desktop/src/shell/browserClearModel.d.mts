/** Types for `browserClearModel.mjs`. */

export interface ClearBox {
  left: number;
  top: number;
  width: number;
  height: number;
  radius: number;
}

export declare const MAX_CLEARS: number;
export declare function clearOf(box: { left: number; top: number; width: number; height: number }, radius: number): ClearBox | null;
export declare function meets(a: { left: number; top: number; width: number; height: number }, b: { left: number; top: number; width: number; height: number }): boolean;
export declare function clearsOver(
  entries: ReadonlyArray<{ id: string; clear: ClearBox | null }>,
  slots: ReadonlyArray<{ visible: boolean; layer: string | null; rect: { left: number; top: number; width: number; height: number } | null }>,
): ClearBox[];
export declare function sameClears(a: readonly ClearBox[], b: readonly ClearBox[]): boolean;
export declare function sameClear(a: ClearBox | null | undefined, b: ClearBox | null | undefined): boolean;
