/** Types for `designerMemoryModel.mjs`, plain JavaScript so `node --test` reads it. */

/** Where a canvas looks: the pan and the zoom. */
export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

/** What a canvas keeps of where it was left. */
export interface DesignerMemory {
  readonly selected: string | null;
  readonly viewport: Viewport | null;
}

export declare const NOTHING: DesignerMemory;
export declare function viewportOf(v: unknown): Viewport | null;
export declare function sameViewport(a: Viewport | null | undefined, b: Viewport | null | undefined): boolean;
/** A step's id as the memory gives it back, or none. */
export declare function keptStep(raw: unknown): string | null;
/** What the memory kept of one canvas, made safe to open on. */
export declare function parseMemory(raw: { selected?: unknown; viewport?: unknown } | null | undefined): DesignerMemory;
export declare function keptSelection(selected: string | null | undefined, steps: readonly { id: string }[] | null | undefined): string | null;
