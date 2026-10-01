import type { WorkbenchScope } from "../routeModel.mjs";

/** A root a link resolves under opens in the IDE: the IDE's scopes. */
export type FileScope = WorkbenchScope;

export interface UrlSpan {
  kind: "url";
  start: number;
  end: number;
  raw: string;
  url: string;
}
export interface PathSpan {
  kind: "path";
  start: number;
  end: number;
  raw: string;
  path: string;
  line: number | null;
  col: number | null;
}
export type LinkSpan = UrlSpan | PathSpan;

/** A root the resolver may name a path under. */
export interface LinkRoot {
  scope: FileScope;
  id: string;
  /** The root's absolute path, when the checkout is on disk. */
  root: string | null;
  label: string;
  /** The root's path index — every file under it the node lists. */
  paths: readonly string[];
}

export interface DocResolution {
  kind: "doc" | "dir";
  scope: FileScope;
  id: string;
  path: string;
  line: number | null;
  col: number | null;
  root: string | null;
  label: string;
  /** Whether the index lists it; a file under the root the index skips (ignored) still opens. */
  indexed: boolean;
}
export type LinkResolution =
  | DocResolution
  | { kind: "choice"; candidates: DocResolution[] }
  | { kind: "outside"; absolute: string; line: number | null; col: number | null }
  | { kind: "unknown"; raw: string };

export declare function parseAddress(raw: string): { path: string; line: number | null; col: number | null };
export declare function findLinks(text: string): LinkSpan[];
export declare function linkifyHtml(html: string): string;
export declare function resolveLink(hit: { path: string; line: number | null; col: number | null }, roots: readonly LinkRoot[]): LinkResolution;
export declare function relativeUnder(absolute: string, root: string): string | null;
export declare function addressWords(path: string, line: number | null, col: number | null): string;
export declare function urlWords(url: string): { host: string; url: string; scheme: string | null };
