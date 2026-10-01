export type Cell = string | number | boolean | Date | null | undefined;
export interface Sheet {
  name: string;
  rows: Cell[][];
}
export declare function detectDelimiter(text: string): string;
export declare function parseDelimited(text: string, delimiter?: string): string[][];
export declare function sheetFromText(text: string, name?: string): Sheet;
export declare function squareRows<T extends Cell>(rows: T[][]): (T | string)[][];
export declare function cellText(v: Cell): string;
export declare function columnWidths(rows: readonly Cell[][], opts?: { min?: number; max?: number; sample?: number }): number[];
export declare function sheetFacts(sheets: readonly Sheet[]): string;
export declare function columnLetter(index: number): string;

export declare function findCells(rows: readonly unknown[][], find: { query: string; regex: boolean; caseSensitive: boolean }): { row: number; col: number }[];
