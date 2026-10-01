import type { ConversationOrigin, ConversationView } from "../../types";

export interface OriginNames {
  goal?: (id: string) => string | null | undefined;
  workflow?: (id: string) => string | null | undefined;
  project?: (id: string) => string | null | undefined;
  workstream?: (id: string) => string | null | undefined;
  drawing?: (id: string) => string | null | undefined;
  note?: (id: string) => string | null | undefined;
}
export interface RowWords {
  title: string;
  about: string;
  agents: string;
  count: string;
  archived: boolean;
}
export declare const ORIGIN_KINDS: readonly string[];
export declare const UNTITLED: string;
export declare function titleOf(row: { title?: string | null; first_line?: string | null } | null | undefined): string;
export declare function originTakesId(kind: string): boolean;
export declare function originWords(origin: { kind: string; id?: string; project?: string } | ConversationOrigin | null | undefined, names?: OriginNames): string;
/** The glyph of each origin kind, a `ui/icons` name. */
export declare const ORIGIN_ICON: Readonly<Record<string, string>>;
export declare function originIcon(kind: string): string;
export declare function movedAt(row: { last_message_at?: number | null; created_at: number }): number;
export declare function sortConversations<R extends { id: string; archived: boolean; last_message_at?: number | null; created_at: number }>(rows: readonly R[]): R[];
export declare function projectConversations<R extends { project?: string | null; archived: boolean; id: string; last_message_at?: number | null; created_at: number; agents: readonly string[] }>(rows: readonly R[], project: string, archived?: boolean): R[];
export declare function runsHere(row: { origin?: { kind?: string; id?: string } | null } | null | undefined, workstream: string, project: string): boolean;
export declare function routeOf(row: { id: string; origin: { kind: string; id?: string; project?: string } }): { route: { name: string; id?: string; scope?: string }; search: Record<string, string> | null };
export declare function agentsWords(ids: readonly string[] | null | undefined, nameOf: (id: string) => string | null | undefined): string;
export declare function rowWords(row: ConversationView, words?: { names?: OriginNames; agentName?: (id: string) => string | null | undefined }): RowWords;
