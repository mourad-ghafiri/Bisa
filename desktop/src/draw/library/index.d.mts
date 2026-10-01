/** Types for the shape libraries. */
export interface LibraryShape {
  id: string;
  name: string;
  skeleton: () => Record<string, unknown>[];
}
export declare const LIBRARY_ITEMS: readonly LibraryShape[];
export declare function buildLibrary<E>(convert: (skeleton: Record<string, unknown>[]) => E[], created?: number): { id: string; status: "published"; created: number; name: string; elements: E[] }[];
