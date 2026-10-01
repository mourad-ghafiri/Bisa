/** Types for the templates. A skeleton is what the canvas lays out (`convertToExcalidrawElements`). */
export interface DrawTemplate {
  id: string;
  label: string;
  /** One line on when to reach for it. */
  blurb: string;
  skeleton: () => Record<string, unknown>[];
}
export declare const TEMPLATES: readonly DrawTemplate[];
export declare function templateOf(id: string | null | undefined): DrawTemplate;
