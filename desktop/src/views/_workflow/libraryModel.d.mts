import type { CatalogEntry, WorkflowRow } from "../../types";
import type { TagFilterState } from "../../ui/tagSearchModel.mjs";

export type LibraryView = "yours" | "templates";
export type YoursStatus = "all" | "runs" | "on" | "problems" | "used";
export type TemplateStatus = "all" | "installed" | "available";
export type LibraryStatus = YoursStatus | TemplateStatus;

export interface LibraryFilters {
  view: LibraryView;
  q?: string;
  status: LibraryStatus;
  archived: boolean;
}

export declare const VIEWS: readonly LibraryView[];
export declare const DEFAULT_VIEW: LibraryView;
export declare const STATUSES: Readonly<Record<LibraryView, readonly LibraryStatus[]>>;
export declare const STATUS_ALL: "all";
export declare const GENERAL_DOMAIN: string;
/** A section's heading: the tag as written, and the platform's word for the cards with none. */
export declare function domainLabel(domain: string): string;
export declare function viewOf(value: unknown): LibraryView;
export declare function statusOf(view: LibraryView, value: unknown): LibraryStatus;
export declare function parseFilters(params: { get?: (k: string) => string | null } | null | undefined, rememberedView: string): LibraryFilters;
export declare function serializeFilters(f: LibraryFilters): Record<string, string | null>;
export declare function narrowed(f: LibraryFilters, tagFilter: TagFilterState | null | undefined): boolean;
export declare function statusSegments(view: LibraryView): { id: LibraryStatus; label: string }[];
export declare function rowStatuses(row: WorkflowRow): LibraryStatus[];
export declare function templateStatuses(entry: CatalogEntry): LibraryStatus[];
export declare function stepWords(definition: { steps?: readonly { name?: string; id?: string; kind?: string }[] } | null | undefined): (string | undefined)[];
export declare function searchWorkflows(rows: readonly WorkflowRow[], tagFilter: TagFilterState, q: string | null | undefined, status: string): WorkflowRow[];
export declare function searchTemplates(entries: readonly CatalogEntry[], tagFilter: TagFilterState, q: string | null | undefined, status: string): CatalogEntry[];
export declare function groupByDomain<T>(items: readonly T[], tagsOf: (item: T) => readonly string[] | null | undefined): [string, T[]][];
/** Which of the library's two reads an engine fact moves: the rows, the catalog's templates. */
export declare function libraryReads(payload: { type?: string } | null | undefined): { library: boolean; catalog: boolean };
