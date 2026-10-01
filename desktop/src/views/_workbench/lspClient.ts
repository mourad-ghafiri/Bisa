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
import {
  documentsToReopen,
  type FollowedDocument,
  hoverMarkdown,
  sameServer,
  serverChange,
  type ServerChange,
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
  const languageRef = useRef<string | null | undefined>(undefined);
  // The open's answer as state, so the change effect below re-runs once the
  // server holds the document — a ref would leave the first edit unsent.
  const [language, setLanguage] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const latest = useRef(text);
  latest.current = text;
  /** What a change of this document's server does to it — set while the document is mounted. */
  const onServer = useRef<((change: ServerChange) => void) | null>(null);
  /** An open is out: an edit waits for its answer rather than race it to a server that is still starting. */
  const opening = useRef(false);

  useEffect(() => {
    if (path === null) {
      onLanguage(null);
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
      return api.lspOpen(scope, id, path, carried).then(
        (r) => {
          if (!alive) {
            // The document closed while the server was opening it: the node
            // now holds a document nobody edits — close it rather than leak it.
            if (r.following) void api.lspClose(scope, id, path).catch((e: unknown) => log.debug("lsp", "a document that closed while opening could not be closed on the node", { path, ...errorFields(e) }));
            return;
          }
          if (mine !== asked) return;
          opening.current = false;
          if (r.following && r.language) {
            following = "yes";
            languageRef.current = r.language;
            setLanguage(r.language);
            onLanguage(r.language);
            owners.set(m.Uri.file(path).toString(), { scope, id, path, language: r.language });
            // What was typed while the open was out was held back: it goes now, whole.
            if (latest.current !== carried) void api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug("lsp", "an edit did not reach the language server; the next one carries the whole text", { path, ...errorFields(e) }));
            return;
          }
          following = "no";
          // No server took it. The language the node named is kept — it is
          // which server's start opens the document again, and what its chip
          // says the server became; a path no language applies to is `null`,
          // and nothing more is sent for it.
          if (languageRef.current === undefined || r.language !== null) {
            languageRef.current = r.language;
            onLanguage(r.language);
          }
        },
        (e: unknown) => {
          if (!alive || mine !== asked) return;
          opening.current = false;
          following = "no";
          log.debug("lsp", "a document could not be opened on its language server", { path, ...errorFields(e) });
          if (languageRef.current === undefined) {
            languageRef.current = null;
            onLanguage(null);
          }
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
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
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
        onLanguage(null);
      },
    );
    return () => {
      alive = false;
      opening.current = false;
      followers.delete(hear);
      onServer.current = null;
      if (timer.current !== null) window.clearTimeout(timer.current);
      const m = loadedMonaco();
      if (m) owners.delete(m.Uri.file(path).toString());
      clearMarkers();
      if (languageRef.current) void api.lspClose(scope, id, path).catch((e: unknown) => log.debug("lsp", "a document could not be closed on the node", { path, ...errorFields(e) }));
    };
    // One open per document: `onLanguage` is the caller's callback, and
    // following it would close and reopen the document on the server for a
    // caller that hands a new function each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, id, path]);

  useEffect(() => {
    if (!language || path === null) return;
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      // An open is out and carries the buffer; what moved since goes when it answers.
      if (opening.current) return;
      void api.lspChange(scope, id, path, latest.current).catch((e: unknown) => log.debug("lsp", "an edit did not reach the language server; the next one carries the whole text", { path, ...errorFields(e) }));
    }, CHANGE_DEBOUNCE_MS);
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
    };
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
