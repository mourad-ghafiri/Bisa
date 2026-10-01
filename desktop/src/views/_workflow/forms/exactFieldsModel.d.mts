/** Types for `exactFieldsModel.mjs`. */

/** The wire's shape: a path to the value it must equal — or, in a payload, the template it carries. */
export type ExactFields = Record<string, string>;

export interface FieldRow {
  path: string;
  value: string;
}
export type Renamed = { ok: true; fields: ExactFields } | { ok: false; reason: string };

/** The longest path a field may have. */
export declare const MAX_PATH: number;
export declare function fieldRows(fields: ExactFields | null | undefined): FieldRow[];
export declare function freshPath(fields: ExactFields | null | undefined): string;
export declare function addField(fields: ExactFields | null | undefined): ExactFields;
export declare function removeField<F extends ExactFields | null | undefined>(fields: F, path: string): F | ExactFields;
export declare function setFieldValue<F extends ExactFields | null | undefined>(fields: F, path: string, value: string): F | ExactFields;
export declare function renameField(fields: ExactFields | null | undefined, from: string, to: string): Renamed;
