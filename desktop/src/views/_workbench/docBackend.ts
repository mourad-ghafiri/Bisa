/**
 * Where a document's bytes come from and go to (ide/03): one seam the editor
 * reads and writes through, chosen from its `DocSource`, so the editor never
 * asks which side of the trust boundary a file is on.
 *
 * - A **file under the root** is the node's: `GET`/`PUT /ide/file` as
 *   `(scope, id, relative)`, watched, searched and followed by a language
 *   server (ide/01 — its blast radius is the workspace).
 * - A **loose file** — anywhere on this machine, by absolute path — is the
 *   shell's: the same caps, the same hash, the same compare-and-swap, and no
 *   watcher or language server (its blast radius is the machine).
 * - An **untitled document** has no backend to read and is saved through
 *   *Save to…*; its write here is a refusal that says so.
 */

import { api, looseFile, writeLooseFile } from "../../api";
import type { FileScope } from "../../types";
import type { DocSource, LoadedFile } from "./editorModel.mjs";

export interface DocBackend {
  /** A file on this machine at all — read on mount, saved in place by ⌘S, autosaved. */
  readonly onDisk: boolean;
  /** Under the root the node watches, searches and runs a language server over. */
  readonly inRoot: boolean;
  read(): Promise<LoadedFile>;
  /** Compare-and-swap; `null` creates. A 409 `ApiError` carries `current_hash` and `current_text`. */
  write(text: string, baseHash: string | null): Promise<{ hash: string }>;
}

/** The backend a source reads and writes through. */
export function backendFor(source: DocSource, scope: FileScope, id: string): DocBackend {
  switch (source.kind) {
    case "file":
      return {
        onDisk: true,
        inRoot: true,
        read: () => api.ideFile(scope, id, source.path),
        write: (text, baseHash) => api.ideWriteFile(scope, id, source.path, text, baseHash ?? undefined),
      };
    case "loose":
      return {
        onDisk: true,
        inRoot: false,
        read: () => looseFile(source.path),
        write: (text, baseHash) => writeLooseFile(source.path, text, baseHash),
      };
    default:
      return {
        onDisk: false,
        inRoot: false,
        read: () => Promise.reject(new Error("an untitled document has nothing to read")),
        write: () => Promise.reject(new Error("an untitled document is saved through Save to…")),
      };
  }
}
