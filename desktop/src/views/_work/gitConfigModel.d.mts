export type ConfigScope = "global" | "local";

export interface KindLike {
  type: string;
  options?: string[];
}

export interface SchemaKeyLike {
  key: string;
  kind: KindLike;
  label: string;
  hint: string;
  scopes: readonly string[];
}

export interface EntryLike {
  key: string;
  local?: string | null;
  global?: string | null;
  effective?: string | null;
}

export interface ConfigRow {
  key: string;
  label: string;
  hint: string;
  kind: KindLike;
  value: string | null;
  inherited: string | null;
  effective: string | null;
  editable: boolean;
}

export interface ConfigWrite {
  set: Record<string, string>;
  unset: string[];
}

export interface Ident {
  name: string;
  email: string;
}

export declare function configRows(schema: readonly SchemaKeyLike[], entries: readonly EntryLike[], scope: ConfigScope): ConfigRow[];
export declare function diffWrites(rows: readonly ConfigRow[], edits: Readonly<Record<string, string>>): ConfigWrite;
export declare function isEmptyWrite(write: ConfigWrite): boolean;
export declare function identityPairProblem(rows: readonly Pick<ConfigRow, "key" | "value" | "inherited" | "editable">[], write: ConfigWrite): string | null;
export declare function validateValue(kind: KindLike, key: string, value: string | null | undefined): string | null;
export declare function validateEdits(rows: readonly ConfigRow[], edits: Readonly<Record<string, string>>): Record<string, string>;
export declare function globalIdentityOf(entries: readonly EntryLike[]): Ident | null;
export declare function formatIdent(ident: Ident): string;
