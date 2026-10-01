/**
 * The bytes a rendered document draws from (ide/03 §Rendered documents): one
 * file under a root, fetched from the node with the bearer header —
 * `GET /ide/raw/{scope}/{id}?path=` — into a `Uint8Array` and a blob URL, the
 * way an artifact's bytes are (`artifactBytes.ts`). Never a node URL an
 * `<img>` or a `<video>` is pointed at: the viewer holds the bytes and hands
 * them to pdf.js, SheetJS, mammoth or a blob URL as the kind needs.
 *
 * Nothing is cached across documents: a file changes under an editor, and a
 * `version` the caller bumps on a `file_changed` frame reads it again. The
 * blob URL is revoked when the document leaves or the bytes are replaced.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import type { FileScope } from "../../types";
import { bytesFailure } from "./fileBytesModel.mjs";
import type { BytesFailure } from "./fileBytesModel.mjs";

export type FileBytesState = { state: "loading" } | { state: "ready"; bytes: Uint8Array; url: string; size: number } | BytesFailure;

export function useIdeFileBytes(scope: FileScope, id: string, path: string, mime: string, version = 0): FileBytesState {
  const [state, setState] = useState<FileBytesState>({ state: "loading" });
  useEffect(() => {
    const ac = new AbortController();
    let url: string | null = null;
    setState({ state: "loading" });
    api
      .ideRaw(scope, id, path, ac.signal)
      .then((bytes) => {
        if (ac.signal.aborted) return;
        url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
        setState({ state: "ready", bytes, url, size: bytes.byteLength });
      })
      .catch((e: unknown) => {
        if (ac.signal.aborted) return;
        // What the refusal means — over the size the node serves, by the limit it said — is the model's.
        setState(bytesFailure(e));
      });
    return () => {
      ac.abort();
      if (url) URL.revokeObjectURL(url);
    };
  }, [scope, id, path, mime, version]);
  return state;
}
