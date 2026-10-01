export function toLspPosition(p: { lineNumber: number; column: number }): { line: number; character: number };
export function toMonacoRange(r: { start: { line: number; character: number }; end: { line: number; character: number } }): {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
};
export function toMarkerSeverity(lsp: number | undefined): number;
export function toMarkers(diagnostics: unknown[] | undefined): Array<{
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
  message: string;
  severity: number;
  code?: string;
  source: string;
}>;
export function toMonacoSymbolKind(lsp: unknown): number;
/** A word for a symbol kind, in the catalog's words. */
export function symbolKindName(lsp: unknown): string;
/** A `workspace/symbol` answer as the palette's rows; a symbol outside the root is dropped. */
export function toWorkspaceSymbols(result: unknown): { name: string; kind: string; path: string; line: number }[];
export function toDocumentSymbols(result: unknown): unknown[];
export function toLocations(result: unknown): { path: string; line: number; column: number }[];
export function hoverMarkdown(contents: unknown): string;
export function applyTextEdits(text: string, edits: Array<{ range: unknown; newText: string }> | undefined): string;

/** A server's own lifecycle, as an engine frame says it. */
export interface ServerChange {
  scope: string;
  id: string;
  language: string;
  /** `restarted` is the person's act answered by the node; the other three are the server's own notifications. */
  event: "started" | "failed" | "stopped" | "restarted";
}
/** A lifecycle frame (`bisa/serverStarted` · `…Failed` · `…Stopped`) as the change it is; null for any other frame. */
export function serverChange(frame: { type?: string; scope?: unknown; id?: unknown; language?: unknown; method?: unknown } | null | undefined): ServerChange | null;
/** Whether a change is this root and language's server's. */
export function sameServer(change: { scope: string; id: string; language: string } | null | undefined, server: { scope: string; id: string; language: string | null | undefined }): boolean;
/** An open document as the language client holds it: whether the node's server follows it, an open is out, or none does. */
export interface FollowedDocument {
  scope: string;
  id: string;
  language: string | null | undefined;
  following: "yes" | "asking" | "no";
}
/** The open documents a server's change leaves to be opened again: all of its own when it stopped, the unfollowed ones when one is there again. */
export function documentsToReopen<D extends FollowedDocument>(change: ServerChange | null | undefined, documents: readonly D[] | null | undefined): D[];
/** The editor's chip for a document's server — null while no language or no status row is known. */
export function serverChip(
  language: string | null | undefined,
  server: { command: string; available: boolean; install_hint?: string | null; state: { state: "starting" | "running" | "stopped" } | { state: "failed"; reason: string } } | null | undefined,
): { tone: "ok" | "danger" | "dim"; words: string; title: string; restart: boolean } | null;
