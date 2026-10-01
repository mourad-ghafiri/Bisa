/**
 * LSP ↔ Monaco, the pure half (ide/10): ranges, positions, severities,
 * symbol kinds, and applying text edits. Plain JavaScript so `node --test`
 * runs it; the client that talks to the node is the impure half.
 *
 * LSP is 0-based in lines and characters; Monaco is 1-based in both. Monaco's
 * `SymbolKind` and `MarkerSeverity` enumerations are not LSP's, so both are
 * mapped explicitly rather than cast.
 */

import { t } from "../../i18n/l10n.mjs";

/** @param {{lineNumber: number, column: number}} p */
export function toLspPosition(p) {
  return { line: Math.max(0, p.lineNumber - 1), character: Math.max(0, p.column - 1) };
}

/** @param {{start: {line: number, character: number}, end: {line: number, character: number}}} r */
export function toMonacoRange(r) {
  return {
    startLineNumber: r.start.line + 1,
    startColumn: r.start.character + 1,
    endLineNumber: r.end.line + 1,
    endColumn: r.end.character + 1,
  };
}

/** Monaco `MarkerSeverity`: Hint 1, Info 2, Warning 4, Error 8. LSP: Error 1 … Hint 4. */
export function toMarkerSeverity(lsp) {
  switch (lsp) {
    case 1:
      return 8;
    case 2:
      return 4;
    case 3:
      return 2;
    case 4:
      return 1;
    default:
      return 8;
  }
}

/**
 * Diagnostics → Monaco markers.
 * @param {Array<{range: any, message: string, severity?: number, code?: any, source?: string}>} diagnostics
 */
export function toMarkers(diagnostics) {
  return (diagnostics ?? []).map((d) => ({
    ...toMonacoRange(d.range),
    message: d.message,
    severity: toMarkerSeverity(d.severity),
    code: d.code == null ? undefined : String(typeof d.code === "object" ? d.code.value : d.code),
    source: d.source ?? "lsp",
  }));
}

/** Monaco's `SymbolKind` is LSP's minus one (LSP File = 1, Monaco File = 0). */
export function toMonacoSymbolKind(lsp) {
  const n = Number(lsp);
  return Number.isFinite(n) && n >= 1 && n <= 26 ? n - 1 : 12; // Variable
}

/** LSP's `SymbolKind`, 1-based, as the keys the catalog's one message selects a word on. */
const KIND_KEYS = ["file", "module", "namespace", "package", "class", "method", "property", "field", "constructor", "enum", "interface", "function", "variable", "constant", "string", "number", "boolean", "array", "object", "key", "null", "enum-member", "struct", "event", "operator", "type-parameter"];

/** A word for a symbol kind, for the palette — the catalog's, in this window's language; *symbol* for a kind LSP does not name. */
export function symbolKindName(lsp) {
  return t("workbench-lsp-symbol-kind", { kind: KIND_KEYS[Number(lsp) - 1] ?? "symbol" });
}

/**
 * `DocumentSymbol[]` (hierarchical) or `SymbolInformation[]` (flat) → Monaco
 * `DocumentSymbol[]`.
 */
export function toDocumentSymbols(result) {
  if (!Array.isArray(result)) return [];
  return result.map((s) => {
    const range = s.range ?? s.location?.range;
    const r = range ? toMonacoRange(range) : { startLineNumber: 1, startColumn: 1, endLineNumber: 1, endColumn: 1 };
    return {
      name: s.name,
      detail: s.detail ?? s.containerName ?? "",
      kind: toMonacoSymbolKind(s.kind),
      tags: [],
      range: r,
      selectionRange: s.selectionRange ? toMonacoRange(s.selectionRange) : r,
      children: s.children ? toDocumentSymbols(s.children) : [],
    };
  });
}

/**
 * A definition answer — `Location`, `Location[]` or `LocationLink[]` — as a
 * list of `{path, line}`; entries the node blanked (outside the root) are dropped.
 */
export function toLocations(result) {
  const list = result == null ? [] : Array.isArray(result) ? result : [result];
  const out = [];
  for (const l of list) {
    const uri = l.uri ?? l.targetUri;
    const range = l.range ?? l.targetSelectionRange ?? l.targetRange;
    if (typeof uri !== "string" || !range) continue;
    out.push({ path: uri, line: range.start.line + 1, column: range.start.character + 1 });
  }
  return out;
}

/**
 * A `workspace/symbol` answer as the palette's rows: each symbol's name, a
 * word for its kind, the root-relative path and the 1-based line it is on.
 * A symbol the node blanked the address of — it lives outside the root — is
 * dropped: there is nothing here to open it by. Anything that is no list is
 * no rows.
 * @param {unknown} result
 * @returns {{name: string, kind: string, path: string, line: number}[]}
 */
export function toWorkspaceSymbols(result) {
  if (!Array.isArray(result)) return [];
  const out = [];
  for (const s of result) {
    const uri = s?.location?.uri;
    if (typeof uri !== "string" || !uri) continue;
    out.push({ name: String(s.name ?? ""), kind: symbolKindName(s.kind), path: uri, line: (s.location?.range?.start?.line ?? 0) + 1 });
  }
  return out;
}

/** Hover contents in any of LSP's four shapes → one markdown string. */
export function hoverMarkdown(contents) {
  if (contents == null) return "";
  if (typeof contents === "string") return contents;
  if (Array.isArray(contents)) return contents.map(hoverMarkdown).filter(Boolean).join("\n\n");
  if (typeof contents === "object") {
    if ("kind" in contents) return String(contents.value ?? "");
    if ("language" in contents) return "```" + contents.language + "\n" + contents.value + "\n```";
    if ("value" in contents) return String(contents.value);
  }
  return "";
}

/**
 * Apply LSP `TextEdit[]` to a text. Edits are applied from the end so earlier
 * offsets stay valid; overlapping edits are refused (the text is returned as
 * it was) rather than guessed at.
 * @param {string} text
 * @param {Array<{range: any, newText: string}>} edits
 */
export function applyTextEdits(text, edits) {
  if (!edits || edits.length === 0) return text;
  const lines = text.split("\n");
  const offsets = [0];
  for (const l of lines) offsets.push(offsets[offsets.length - 1] + l.length + 1);
  const at = (p) => {
    const line = Math.min(p.line, lines.length - 1);
    const base = offsets[Math.max(0, line)] ?? text.length;
    const ch = Math.min(p.character, (lines[line] ?? "").length);
    return Math.min(text.length, base + ch);
  };
  const spans = edits
    .map((e) => ({ start: at(e.range.start), end: at(e.range.end), newText: e.newText ?? "" }))
    .sort((a, b) => b.start - a.start || b.end - a.end);
  for (let i = 1; i < spans.length; i++) {
    if (spans[i].end > spans[i - 1].start) return text; // overlap: refuse
  }
  let out = text;
  for (const s of spans) out = out.slice(0, s.start) + s.newText + out.slice(s.end);
  return out;
}

/** What a server says of its own lifecycle, by the notification it is said in — `bisa_engine::lsp`. */
const LIFECYCLE = Object.freeze({ "bisa/serverStarted": "started", "bisa/serverFailed": "failed", "bisa/serverStopped": "stopped" });

/**
 * A server's own lifecycle as an engine frame says it: which root and
 * language, and what happened — it started, it failed (a start that did not
 * take, or the crash loop's verdict), it stopped (a person's *Restart*, a
 * crash, an `lsp.*` setting that changed, the idle stop). `null` for any
 * other frame: a diagnostic, another server notification, another event.
 * @param {{type?: string, scope?: unknown, id?: unknown, language?: unknown, method?: unknown} | null | undefined} frame an engine frame's payload
 * @returns {{scope: string, id: string, language: string, event: "started" | "failed" | "stopped"} | null}
 */
export function serverChange(frame) {
  if (!frame || frame.type !== "lsp" || typeof frame.method !== "string") return null;
  const event = Object.hasOwn(LIFECYCLE, frame.method) ? LIFECYCLE[frame.method] : undefined;
  const { scope, id, language } = frame;
  if (!event || typeof scope !== "string" || typeof id !== "string" || typeof language !== "string" || !scope || !id || !language) return null;
  return { scope, id, language, event };
}

/**
 * Whether a change is this server's: one server a root and language.
 * @param {{scope: string, id: string, language: string} | null | undefined} change
 * @param {{scope: string, id: string, language: string | null | undefined}} server
 */
export function sameServer(change, server) {
  return !!change && !!server.language && change.scope === server.scope && change.id === server.id && change.language === server.language;
}

/**
 * The open documents a change of their server leaves to be opened again
 * (ide/10 — *the node owns the process; the editor's buffer is the text*).
 * A server holds the documents it was told of, and the next server knows
 * none of them: a document that waited for its next edit to be opened again
 * had no diagnostics, no hover and no outline until somebody typed.
 *
 * - **stopped** — every document of that root and language is opened on the
 *   next server, whatever it was doing: an open still out may have landed on
 *   the server that just went, so the newest asked for is the one that stands;
 * - **started**, or **restarted** (the person asked, and the node answered) —
 *   a server is there again: the documents on none (`following: "no"` — their
 *   last open found no server, or did not answer) are opened on it; one that
 *   is followed, or whose open is out, is left alone — a second `didOpen`
 *   is nothing a server wants;
 * - **failed** — nothing: there is no server to open anything on.
 *
 * A document whose language the node never said (`language` null — no
 * server applied when it was opened) is on nobody's list: nothing here can
 * tell which server's it would be.
 * @template {{scope: string, id: string, language: string | null | undefined, following: "yes" | "asking" | "no"}} D
 * @param {{scope: string, id: string, language: string, event: "started" | "failed" | "stopped" | "restarted"} | null | undefined} change
 * @param {readonly D[] | null | undefined} documents
 * @returns {D[]}
 */
export function documentsToReopen(change, documents) {
  if (!change || change.event === "failed") return [];
  const theirs = (documents ?? []).filter((d) => sameServer(change, d));
  return change.event === "stopped" ? theirs : theirs.filter((d) => d.following === "no");
}

/**
 * The editor's chip for the server that follows a document: its words, its
 * tone, the hover's line, and whether *Restart* is offered — `null` while
 * no language or no status row is known. The state is said in the catalog's
 * words, never the wire's.
 * @param {string | null | undefined} language
 * @param {{command: string, available: boolean, install_hint?: string | null, state: {state: "starting" | "running" | "stopped" | "failed", reason?: string}} | null | undefined} server the language's row of `GET /ide/lsp/{scope}/{id}/status`
 * @returns {{tone: "ok" | "danger" | "dim", words: string, title: string, restart: boolean} | null}
 */
export function serverChip(language, server) {
  if (!language || !server) return null;
  const state = server.state.state;
  const failed = state === "failed";
  const stateWords = t("workbench-lsp-server-state", { state });
  return {
    tone: state === "running" ? "ok" : failed ? "danger" : "dim",
    words: server.available ? t("workbench-editor-doc-server", { lspLanguage: language, state: stateWords }) : t("workbench-editor-doc-no-server-installed", { lspLanguage: language }),
    title: failed
      ? `${server.command}: ${server.state.reason ?? ""}`
      : !server.available
        ? t("workbench-editor-doc-not-path-path", { command: server.command, install_hint: server.install_hint ?? "", flag: server.install_hint ? "yes" : "no" })
        : `${server.command} · ${stateWords}`,
    restart: failed,
  };
}
