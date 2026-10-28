/**
 * The impure half of language intelligence (ide/10): Monaco's providers,
 * registered once and answering through the node's proxy; a hook that keeps
 * one document synchronised with its server — and opens it again on the
 * next one when that server stopped; diagnostics from the engine stream onto
 * the model's markers.
 *
 * The webview never learns an absolute path: every `uri` here is
 * root-relative, and the node rewrites both ways.
 */

import { errorFields, log } from "../../log";
import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { FileScope } from "../../types";
import { loadMonaco, loadedMonaco, type Monaco } from "../../ui/monaco";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { boolOf } from "../../shell/settingsModel.mjs";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import {
  documentsToReopen,
  type FollowedDocument,
  hoverMarkdown,
  openAnswered,
  openFailed,
  sameServer,
  serverChange,
  type ServerChange,
  toCompletionItems,
  toDocumentSymbols,
  toLocations,
  toLspPosition,
  toMarkers,
  toMonacoRange,
} from "./lspModel.mjs";

type MonacoModule = typeof import("monaco-editor");

/** Which root a Monaco model belongs to, by its URI. */
interface Owner {
  scope: FileScope;
  id: string;
  path: string;
  language: string;
}
const owners = new Map<string, Owner>();

/** Set by the workbench: how to open a file at a line when a definition is elsewhere. */
let openAt: ((path: string, line: number) => void) | null = null;
export function setLspOpener(f: ((path: string, line: number) => void) | null): void {
  openAt = f;
}

function ownerOf(model: Monaco.editor.ITextModel): Owner | null {
  return owners.get(model.uri.toString()) ?? null;
}

/** One document of one root, as the pending changes are keyed. */
function ownerKey(o: { scope: string; id: string; path: string }): string {
  return `${o.scope}:${o.id}:${o.path}`;
}

/**
 * `lsp.completion`, as the settings last said it: the provider is Monaco's,
 * registered once for every model, so the switch is read here and not in
 * a hook of its own. On until the settings say otherwise.
 */
let completionOn = true;

/**
 * The characters a server is asked on, beyond the word being typed: a
 * member, a path, a scope, a tag, a string's start, an attribute. The
 * servers' own lists stay with the node (an `initialize` answer the editor
 * never sees); this one covers the languages the catalog knows.
 */
const COMPLETION_TRIGGERS = [".", ":", ">", "/", "\\", "\"", "'", "@", "<", "#", "$"];

/**
 * The edits not yet sent, one per open document: the 300 ms debounce lives
 * here rather than in each hook, so a request that needs the server to hold
 * the text as it stands can send what is pending first (`flushChange`).
 */
const pendingChanges = new Map<string, { timer: number; send: () => Promise<unknown> | void }>();

/** Send `send` once the debounce passes, or when a request asks for it sooner; a newer edit replaces an older one unsent. */
function scheduleChange(key: string, send: () => Promise<unknown> | void, delayMs: number): void {
  const prev = pendingChanges.get(key);
  if (prev) window.clearTimeout(prev.timer);
  const timer = window.setTimeout(() => {
    pendingChanges.delete(key);
    void send();
  }, delayMs);
  pendingChanges.set(key, { timer, send });
}

/** Forget a pending edit without sending it: the document closed, or an open carries the buffer whole. */
function cancelChange(key: string): void {
  const prev = pendingChanges.get(key);
  if (!prev) return;
  window.clearTimeout(prev.timer);
  pendingChanges.delete(key);
}

/** Send a pending edit now, so the server answers for the text as it stands; nothing pending is nothing to wait for. */
async function flushChange(key: string): Promise<void> {
  const prev = pendingChanges.get(key);
  if (!prev) return;
  window.clearTimeout(prev.timer);
  pendingChanges.delete(key);
  await prev.send();
}

/** The Monaco model for a path, or null before Monaco has loaded. */
function modelFor(path: string): Monaco.editor.ITextModel | null {
  const m = loadedMonaco();
  if (!m) return null;
  const uri = m.Uri.file(path).toString();
  return m.editor.getModels().find((mo) => mo.uri.toString() === uri) ?? null;
}

let registered = false;

/** Register Monaco's providers once; they answer only for owned models. */
function registerLspProviders(monaco: MonacoModule): void {
  if (registered) return;
  registered = true;
  const doc = (o: Owner) => ({ uri: o.path });

  monaco.languages.registerHoverProvider("*", {
    async provideHover(model, position) {
      const o = ownerOf(model);
      if (!o) return null;
      try {
        const r = await api.lspRequest(o.scope, o.id, o.path, "textDocument/hover", {
          textDocument: doc(o),
          position: toLspPosition(position),
        });
        const res = r.result as { contents?: unknown; range?: unknown } | null;
        const md = hoverMarkdown(res?.contents);
        if (!md) return null;
        return {
          contents: [{ value: md }],
          range: res?.range ? toMonacoRange(res.range as never) : undefined,
        };
      } catch (e: unknown) {
        // No answer draws nothing; the reason is the log's, not the editor's.
        log.debug("lsp", "a language request did not answer", { scope: o.scope, id: o.id, path: o.path, ...errorFields(e) });
        return null;
      }
    },
  });

  monaco.languages.registerCompletionItemProvider("*", {
    triggerCharacters: COMPLETION_TRIGGERS,
    async provideCompletionItems(model, position, _context, token) {
      const o = ownerOf(model);
      if (!o || !completionOn) return null;
      // The word under the caret is what an item with no edit of its own replaces.
      const word = model.getWordUntilPosition(position);
      const range = { startLineNumber: position.lineNumber, startColumn: word.startColumn, endLineNumber: position.lineNumber, endColumn: word.endColumn };
      try {
        // The server answers for the text it holds: what was typed since the last send goes first.
        await flushChange(ownerKey(o));
        if (token.isCancellationRequested) return null;
        const r = await api.lspRequest(o.scope, o.id, o.path, "textDocument/completion", {
          textDocument: doc(o),
          position: toLspPosition(position),
        });
        if (token.isCancellationRequested) return null;
        return toCompletionItems(r.result, range) as Monaco.languages.CompletionList;
      } catch (e: unknown) {
        // No answer draws nothing; the reason is the log's, not the editor's.
        log.debug("lsp", "a language request did not answer", { scope: o.scope, id: o.id, path: o.path, ...errorFields(e) });
        return null;
      }
    },
  });

  monaco.languages.registerDefinitionProvider("*", {
    async provideDefinition(model, position) {
      const o = ownerOf(model);
      if (!o) return null;
      try {
        const r = await api.lspRequest(o.scope, o.id, o.path, "textDocument/definition", {
          textDocument: doc(o),
          position: toLspPosition(position),
        });
        const locs = toLocations(r.result);
        if (locs.length === 0) return null;
        // Same file: hand Monaco the location. Elsewhere: open the file
        // there through the workbench — Monaco cannot open our documents.
        const here = locs.find((l) => l.path === o.path);
        if (here) {
          return { uri: model.uri, range: { startLineNumber: here.line, startColumn: here.column, endLineNumber: here.line, endColumn: here.column } };
        }
        openAt?.(locs[0].path, locs[0].line);
        return null;
      } catch (e: unknown) {
        // No answer draws nothing; the reason is the log's, not the editor's.
        log.debug("lsp", "a language request did not answer", { scope: o.scope, id: o.id, path: o.path, ...errorFields(e) });
        return null;
      }
    },
  });

  monaco.languages.registerReferenceProvider("*", {
    async provideReferences(model, position) {
      const o = ownerOf(model);
      if (!o) return null;
      try {
        const r = await api.lspRequest(o.scope, o.id, o.path, "textDocument/references", {
          textDocument: doc(o),
          position: toLspPosition(position),
          context: { includeDeclaration: true },
        });
        const locs = toLocations(r.result);
        // The peek is this document's: Monaco can preview only a model it
        // holds. A reference elsewhere is not dropped — when none is here,
        // the first elsewhere opens through the workbench, as a definition
        // elsewhere does (ide/10 §Bounds).
        const here = locs.filter((l) => l.path === o.path);
        if (here.length === 0) {
          const elsewhere = locs.find((l) => l.path !== o.path);
          if (elsewhere) openAt?.(elsewhere.path, elsewhere.line);
          return null;
        }
        return here.map((l) => ({ uri: model.uri, range: { startLineNumber: l.line, startColumn: l.column, endLineNumber: l.line, endColumn: l.column } }));
      } catch (e: unknown) {
        // No answer draws nothing; the reason is the log's, not the editor's.
        log.debug("lsp", "a language request did not answer", { scope: o.scope, id: o.id, path: o.path, ...errorFields(e) });
        return null;
      }
    },
  });

  monaco.languages.registerDocumentSymbolProvider("*", {
    async provideDocumentSymbols(model) {
      const o = ownerOf(model);
      if (!o) return null;
      try {
        const r = await api.lspRequest(o.scope, o.id, o.path, "textDocument/documentSymbol", { textDocument: doc(o) });
        return toDocumentSymbols(r.result) as Monaco.languages.DocumentSymbol[];
      } catch (e: unknown) {
        // No answer draws nothing; the reason is the log's, not the editor's.
        log.debug("lsp", "a language request did not answer", { scope: o.scope, id: o.id, path: o.path, ...errorFields(e) });
        return null;
      }
    },
  });
}

const CHANGE_DEBOUNCE_MS = 300;

/**
 * The documents this window keeps synchronised, each with how to hear that
 * its server changed — what a person's *Restart* is said through, since the
 * node says nothing on the stream for a server that was not running.
 */
const followers = new Set<(change: ServerChange) => void>();

/**
 * A person asked for a language's server again and the node answered
 * (`POST /ide/lsp/{scope}/{id}/restart`): every open document of that root
 * and language on no server is opened again (`lspModel.documentsToReopen`).
 */
export function lspRestarted(scope: FileScope, id: string, language: string): void {
  for (const hear of [...followers]) hear({ scope, id, language, event: "restarted" });
}

/**
 * Keep one open document synchronised: `didOpen` on mount, `didChange`
 * debounced as the buffer moves, `didClose` on unmount; diagnostics for it
 * land on its model's markers. `language` is what the node answered — the
 * language the path is, `null` when none applies and nothing more is sent.
 * The node also says whether a server `following` the document took it: a
 * language named with none — servers off, none configured, a crash loop
 * given up on — is remembered all the same, so the document is opened again
 * the moment that server starts.
 *
 * A server that stopped — a person's *Restart*, a crash, an `lsp.*` setting
 * that changed — took what it knew of the document with it: the document is
 * opened again, with the buffer as it stands, on the server that comes next
 * (`lspModel.documentsToReopen`), and the diagnostics of the one that went
 * are cleared rather than left standing for nobody.
 */
export function useLspDocument(
  scope: FileScope,
  id: string,
  /** `null` for a document with no file behind it yet — an untitled one — which no server follows. */
  path: string | null,
  text: string,
  onLanguage: (language: string | null) => void,
): void {
  // The caller's callback as it stands at the moment of telling, never the one captured at the mount — a server restart tells the latest.
  const onLanguageRef = useRef(onLanguage);
  onLanguageRef.current = onLanguage;
  const languageRef = useRef<string | null | undefined>(undefined);
  // The open's answer as state, so the change effect below re-runs once the
  // server holds the document — a ref would leave the first edit unsent.
  const [language, setLanguage] = useState<string | null>(null);
  const latest = useRef(text);
  // The switch over completion, kept for the provider as the settings arrive.
  const { resolved } = useResolvedSettings(null);
  useEffect(() => {
    completionOn = boolOf(resolved, "lsp.completion", true);
  }, [resolved]);
  latest.current = text;
  /** What a change of this document's server does to it — set while the document is mounted. */
  const onServer = useRef<((change: ServerChange) => void) | null>(null);
  /** An open is out: an edit waits for its answer rather than race it to a server that is still starting. */
  const opening = useRef(false);

  useEffect(() => {
    if (path === null) {
      onLanguageRef.current(null);
      return;
    }
    let alive = true;
    /** Opens asked for so far: only the newest's answer is taken, so a late answer about a server that has since gone writes nothing. */
    let asked = 0;
    /** Whether the node's server follows this document, an open is out, or none does. */
    let following: FollowedDocument["following"] = "asking";
    languageRef.current = undefined;
    setLanguage(null);
    const clearMarkers = () => {
      const m = loadedMonaco();
      const model = m ? modelFor(path) : null;
      if (m && model) m.editor.setModelMarkers(model, "lsp", []);
    };
    const open = (m: MonacoModule): Promise<void> => {
      asked += 1;
      const mine = asked;
      following = "asking";
      opening.current = true;
      const carried = latest.current;
      // What the answer means is the model's (`openAnswered`, `openFailed`):
      // this hook only writes what it says.
      return api.lspOpen(scope, id, path, carried).then(
        (r) => {
          const verdict = openAnswered(languageRef.current, r, { alive, newest: mine === asked });
          // The document closed while the server was opening it: the node
          // now holds a document nobody edits — close it rather than leak it.
          if (verdict.closeOnNode) void api.lspClose(scope, id, path).catch((e: unknown) => log.debug("lsp", "a document that closed while opening could not be closed on the node", { path, ...errorFields(e) }));
          if (verdict.following === null) return;
          opening.current = false;
          following = verdict.following;
          if (verdict.language !== undefined) languageRef.current = verdict.language;
          if (verdict.say !== undefined) onLanguageRef.current(verdict.say);
          if (verdict.following === "yes" && verdict.language) {
            setLanguage(verdict.language);
            owners.set(m.Uri.file(path).toString(), { scope, id, path, language: verdict.language });
            // What was typed while the open was out was held back: it goes now, whole.
            if (verdict.sendHeld && latest.current !== carried) void api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug("lsp", "an edit did not reach the language server; the next one carries the whole text", { path, ...errorFields(e) }));
          }
        },
        (e: unknown) => {
          const verdict = openFailed(languageRef.current, { alive, newest: mine === asked });
          if (verdict.following === null) return;
          opening.current = false;
          following = verdict.following;
          log.debug("lsp", "a document could not be opened on its language server", { path, ...errorFields(e) });
          if (verdict.language !== undefined) languageRef.current = verdict.language;
          if (verdict.say !== undefined) onLanguageRef.current(verdict.say);
        },
      );
    };
    onServer.current = (change) => {
      const m = loadedMonaco();
      if (!m) return;
      const mine = { scope, id, language: languageRef.current, following };
      // The server that published them is gone, or gave up: its diagnostics are nobody's.
      if (change.event !== "started" && change.event !== "restarted" && sameServer(change, mine)) clearMarkers();
      if (documentsToReopen(change, [mine]).length === 0) return;
      // The open carries the buffer as it stands: an edit still waiting to be sent is in it.
      cancelChange(ownerKey({ scope, id, path }));
      void open(m);
    };
    const hear = (change: ServerChange) => onServer.current?.(change);
    followers.add(hear);
    // The providers are Monaco's, so registration waits for the on-demand load
    // — which the editor beside this document has already begun.
    void loadMonaco().then(
      (m) => {
        if (!alive) return;
        registerLspProviders(m);
        return open(m);
      },
      (e: unknown) => {
        if (!alive) return;
        log.debug("lsp", "the editor did not load, so no document is followed", { path, ...errorFields(e) });
        languageRef.current = null;
        onLanguageRef.current(null);
      },
    );
    return () => {
      alive = false;
      opening.current = false;
      followers.delete(hear);
      onServer.current = null;
      cancelChange(ownerKey({ scope, id, path }));
      const m = loadedMonaco();
      if (m) owners.delete(m.Uri.file(path).toString());
      clearMarkers();
      if (languageRef.current) void api.lspClose(scope, id, path).catch((e: unknown) => log.debug("lsp", "a document could not be closed on the node", { path, ...errorFields(e) }));
    };
    // One open per document: the caller's `onLanguage` is read through a
    // ref (`onLanguageRef`), so a caller that hands a new function each
    // render neither reopens the document nor is told through a stale one.
  }, [scope, id, path]);

  useEffect(() => {
    if (!language || path === null) return;
    const key = ownerKey({ scope, id, path });
    // Debounced in the module's map, so a completion asked for meanwhile can send it first.
    scheduleChange(
      key,
      () => {
        // An open is out and carries the buffer; what moved since goes when it answers.
        if (opening.current) return;
        return api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug("lsp", "an edit did not reach the language server; the next one carries the whole text", { path, ...errorFields(e) }));
      },
      CHANGE_DEBOUNCE_MS,
    );
    return () => cancelChange(key);
  }, [text, scope, id, path, language]);

  // The bus came back after a gap. A node that restarted in it started no
  // server and said nothing of the ones it lost, so the document is sent
  // again, whole, as a change: the node opens it on a server that holds no
  // such document, and one that still does takes the text it already has.
  useReloadOnReconnect(() => {
    if (path === null || !languageRef.current || opening.current) return;
    void api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug("lsp", "a document could not be sent again after the node came back; the next edit carries the whole text", { path, ...errorFields(e) }));
  });

  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type !== "lsp" || p.scope !== scope || p.id !== id) return;
    const change = serverChange(p);
    if (change) {
      onServer.current?.(change);
      return;
    }
    if (p.method !== "textDocument/publishDiagnostics") return;
    const params = p.params as { uri?: string | null; diagnostics?: unknown[] } | null;
    if (path === null || !params || params.uri !== path) return;
    const mod = loadedMonaco();
    if (!mod) return;
    const m = modelFor(path);
    if (m) mod.editor.setModelMarkers(m, "lsp", toMarkers(params.diagnostics) as Monaco.editor.IMarkerData[]);
  });
}
