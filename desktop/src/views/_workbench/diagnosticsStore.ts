/**
 * The diagnostics a root's language servers published (ide/10), kept per
 * root for the Problems list and the rail's mark: every `publishDiagnostics`
 * the node relays on the engine stream — for the documents open in a
 * workbench, and whatever more a server reports — a pair's list replaced
 * whole by the next frame, a server's word dropped when it stops or fails,
 * and the root's whole list dropped when the bus comes back after the node
 * was away (the node started no server in the gap; the documents are
 * opened again and the servers speak afresh). The rules are
 * `problemsModel.mjs`'s; this file holds the arrays and hears the frames.
 *
 * Fed while a hook is mounted, as `shell/pathIndexStore.ts` is: the
 * workbench mounts one for its root's rail mark, the Problems list another
 * — a frame heard by both moves the array once, since the model answers the
 * same array for a list it already holds.
 */

import { useEffect, useState } from "react";
import { useEngineEvents } from "../../bus";
import type { FileScope } from "../../types";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { serverChange } from "./lspModel.mjs";
import { withDiagnostics, withoutLanguage } from "./problemsModel.mjs";
import type { DiagnosticEntry } from "./problemsModel.mjs";

const NONE: readonly DiagnosticEntry[] = Object.freeze([]);
const byRoot = new Map<string, readonly DiagnosticEntry[]>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const l of listeners) l();
}

function put(key: string, next: readonly DiagnosticEntry[]): void {
  const have = byRoot.get(key) ?? NONE;
  if (next === have) return;
  if (next.length === 0) byRoot.delete(key);
  else byRoot.set(key, next);
  notify();
}

/** The entries a root holds right now — the same array until a frame moves it. */
function diagnosticsOf(scope: string, id: string): readonly DiagnosticEntry[] {
  return byRoot.get(`${scope}:${id}`) ?? NONE;
}

/** Forget a root's list whole: the node that relayed it was away, and started no server in the gap. */
function clearDiagnostics(scope: string, id: string): void {
  put(`${scope}:${id}`, NONE);
}

/** A frame of the engine stream, as the hook hears it. */
function hear(p: { type: string; scope?: string; id?: string; language?: string; method?: string; params?: unknown }): void {
  if (p.type !== "lsp" || typeof p.scope !== "string" || typeof p.id !== "string") return;
  const key = `${p.scope}:${p.id}`;
  const change = serverChange(p);
  if (change) {
    if (change.event === "stopped" || change.event === "failed") put(key, withoutLanguage(byRoot.get(key) ?? NONE, change.language));
    return;
  }
  if (p.method !== "textDocument/publishDiagnostics" || typeof p.language !== "string") return;
  const params = p.params as { uri?: unknown; diagnostics?: unknown } | null;
  if (!params || typeof params.uri !== "string") return;
  const diagnostics = Array.isArray(params.diagnostics) ? (params.diagnostics as readonly unknown[]) : [];
  put(key, withDiagnostics(byRoot.get(key) ?? NONE, { language: p.language, uri: params.uri, diagnostics }));
}

/**
 * The diagnostics for a root, kept current while this hook is mounted —
 * `[]` for a root nobody spoke about, or no root.
 */
export function useDiagnostics(scope: FileScope | null, id: string | null): readonly DiagnosticEntry[] {
  const [, bump] = useState(0);
  useEffect(() => {
    const l = () => bump((n) => n + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, []);
  useEngineEvents((e) => hear(e.payload as Parameters<typeof hear>[0]));
  // The bus came back after a gap: what the servers said before it is nobody's word now.
  useReloadOnReconnect(() => {
    if (scope && id) clearDiagnostics(scope, id);
  });
  return scope && id ? diagnosticsOf(scope, id) : NONE;
}
