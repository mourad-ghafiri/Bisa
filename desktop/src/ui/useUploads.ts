/**
 * Files chosen for a request, uploaded the moment they are chosen.
 *
 * Now rather than on submit, so the wait is where you can see it and a slow
 * upload does not sit between pressing the button and the thing appearing.
 * Each file is tracked from the moment it is chosen, so a failure is attached
 * to the file that failed rather than to the request. The composer and the
 * New Goal dialog both hold files this way; the strip that shows them is
 * `PendingFiles`.
 *
 * **Not persisted.** A `File` is not serializable and an uploaded descriptor
 * without the surface that chose it is a file nobody remembers attaching.
 */

import { useCallback, useMemo, useState } from "react";
import { api } from "../api";
import type { AttachmentRef } from "../types";
import { failureReason } from "./failure";

/** One file held, and how its upload is going. */
export interface PendingFile {
  key: string;
  name: string;
  size: number;
  state: "uploading" | "done" | "failed";
  /** The descriptor, once the upload answered. */
  file?: AttachmentRef;
  error?: string;
}

export interface Uploads {
  pending: PendingFile[];
  /** Take files from a picker, a drop or a paste, and upload them now. */
  take: (list: FileList | File[] | null) => void;
  remove: (key: string) => void;
  /** Change one entry — a caller's own flag on it, say. */
  patch: (key: string, change: Partial<PendingFile> & Record<string, unknown>) => void;
  clear: () => void;
  /** The descriptors of every upload that answered. */
  uploaded: AttachmentRef[];
  /** At least one upload is still in flight. */
  uploading: boolean;
}

export function useUploads(): Uploads {
  const [pending, setPending] = useState<PendingFile[]>([]);

  const take = useCallback((list: FileList | File[] | null) => {
    const chosen = Array.from(list ?? []);
    if (chosen.length === 0) return;
    for (const file of chosen) {
      const key = `${file.name}:${file.size}:${file.lastModified}:${Math.random()}`;
      setPending((p) => [...p, { key, name: file.name, size: file.size, state: "uploading" }]);
      void api
        .uploadAttachment(file)
        .then((r) => setPending((p) => p.map((x) => (x.key === key ? { ...x, state: "done", file: r } : x))))
        .catch((e: unknown) =>
          setPending((p) => p.map((x) => (x.key === key ? { ...x, state: "failed", error: failureReason("uploads", "an attachment did not upload", e) } : x))), // for the log
        );
    }
  }, []);

  const remove = useCallback((key: string) => setPending((p) => p.filter((x) => x.key !== key)), []);
  const patch = useCallback(
    (key: string, change: Partial<PendingFile> & Record<string, unknown>) => setPending((p) => p.map((x) => (x.key === key ? { ...x, ...change } : x))),
    [],
  );
  const clear = useCallback(() => setPending([]), []);

  const uploaded = useMemo(() => pending.filter((p) => p.file).map((p) => p.file as AttachmentRef), [pending]);
  const uploading = pending.some((p) => p.state === "uploading");

  return { pending, take, remove, patch, clear, uploaded, uploading };
}
