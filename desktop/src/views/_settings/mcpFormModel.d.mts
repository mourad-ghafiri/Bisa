/** Types for `mcpFormModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { McpServerView, McpServerConfig } from "../../types";

export interface Pair {
  k: string;
  v: string;
}
export type McpKind = "stdio" | "http" | "sse";
export interface McpDraft {
  id: string;
  name: string;
  description: string;
  tags: string[];
  kind: McpKind;
  command: string;
  args: string;
  env: Pair[];
  cwd: string;
  url: string;
  headers: Pair[];
}
export type McpFormErrors = Partial<Record<"id" | "name" | "command" | "cwd" | "env" | "url" | "headers" | "form", string>>;

export declare const MASK: string;
export declare const RESERVED: string;
export declare const KINDS: readonly McpKind[];
export declare function kindWords(kind: McpKind | string): { label: string; hint: string };
export declare function blank(): McpDraft;
export declare function fromDef(m: McpServerView): McpDraft;
export declare function pairsToMap(pairs: readonly Pair[]): Record<string, string>;
export declare function toTransport(d: McpDraft): McpServerConfig;
export declare function isMasked(v: string): boolean;
export declare function isHttpUrl(url: string): boolean;
export declare function isHeaderName(name: string): boolean;
export declare function validate(d: McpDraft, editing: boolean): McpFormErrors;
/** The field each of the node's refusals is about, by the refusal's message id. */
export declare const FIELD_OF_REFUSAL: Readonly<Record<string, keyof McpFormErrors>>;
/** Whether a test's answer is still about the draft on screen. */
export declare function probeStillAbout(dialled: number, now: number): boolean;
export declare function fieldForRefusal(refusal: string | null | undefined): keyof McpFormErrors;
export declare function secretCount(t: McpServerConfig): string;
export declare function transportLine(t: McpServerConfig): string;
