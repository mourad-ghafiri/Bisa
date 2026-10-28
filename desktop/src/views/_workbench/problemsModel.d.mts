export type Severity = "error" | "warning" | "info" | "hint";

/** One server's word on one document, as the node relayed it. */
export interface DiagnosticEntry {
  language: string;
  /** Root-relative. */
  uri: string;
  diagnostics: readonly unknown[];
}

export interface ProblemCounts {
  errors: number;
  warnings: number;
  infos: number;
  hints: number;
  files: number;
}

export type ProblemRow =
  | { kind: "file"; key: string; path: string; errors: number; warnings: number; count: number }
  | { kind: "problem"; key: string; path: string; line: number; column: number; severity: Severity; message: string; source: string; code: string | null }
  | { kind: "more"; key: string; path: string; more: number };

export declare function severityOf(lsp: unknown): Severity;
export declare function severityWord(severity: Severity): string;
export declare function withDiagnostics<E extends DiagnosticEntry>(entries: readonly E[], frame: DiagnosticEntry): readonly E[];
export declare function withoutLanguage<E extends { language: string }>(entries: readonly E[], language: string): readonly E[];
export declare function problemCounts(entries: readonly DiagnosticEntry[]): ProblemCounts;
export declare function problemRows(entries: readonly DiagnosticEntry[], perFile?: number): ProblemRow[];
export declare function problemsBadge(counts: ProblemCounts): { tone: "danger"; title: string } | null;
export declare function problemsWords(counts: ProblemCounts): string;
