/** Types for `serversModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface ServedOwnerRow {
  kind: "workstream" | "artifact";
  folder?: string;
  name?: string;
  [k: string]: unknown;
}
export interface ServedRow {
  id: string;
  url: string;
  page: string;
  owner: ServedOwnerRow;
  port: number;
}
export interface PortRow {
  port: number;
  process: string;
}
export interface BrowserMenuItem {
  id: string;
  label: string;
  hint: string;
  icon: string;
  disabled: boolean;
  separatorBefore?: boolean;
  danger?: boolean;
  /** The keymap command that means the same thing — `run_project` on ⌘⇧R's door. */
  command?: string;
}
export interface BrowserMenuFacts {
  checkout: boolean;
  servers: readonly ServedRow[];
  ports: readonly PortRow[];
  tabs: readonly { key: string; label: string; headless?: boolean }[];
  annotate: { key: string } | null;
  busy: boolean;
  /** ⌘⇧R's target id (`runDoor`), or null off a checkout. */
  door: string | null;
}
export declare function servedWords(server: { owner: ServedOwnerRow }): string;
export declare function serverLabel(server: { owner: ServedOwnerRow; port: number }): string;
export declare function openLabel(server: { port: number }): string;
export declare function stopLabel(server: { port: number }, servers: readonly unknown[]): string;
export declare function browserMenu(facts: BrowserMenuFacts): { main: { url: string | null; hint: string }; items: BrowserMenuItem[] };
export declare function serverAt<T extends { url: string }>(servers: readonly T[], url: string): T | null;
export declare function annotatable(session: { home: { scope: string; id: string } | null; url: string } | null, servers: readonly { url: string; owner: { kind: string } }[]): boolean;
/** Where a tab's annotations go: the checkout's conversation, or the conversation on screen. */
export type AnnotationHome = { kind: "checkout"; wid: string } | { kind: "screen" };
export declare function annotationHome(session: { home: { scope: string; id: string } | null } | null | undefined): AnnotationHome;
export declare function stoppedWords(server: ServedRow): string;
export declare function servingWords(server: ServedRow): string;
