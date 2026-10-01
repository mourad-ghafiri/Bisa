/**
 * One checkout, live: the workstream record with its checkout facts, and the
 * git status the node reads for it. What the Workstreams panel reads once for
 * the checkout it manages; the panel's lifecycle asks the code host on top of
 * it through `usePullRequest(base)`, and nothing here does.
 *
 * Follows the bus: a commit — the workstream's own or Git › Changes', the
 * record moves the same — a push from the CLI, an agent's write, a rename;
 * which facts those are is `workstreamFramesModel.movesWorkstream`, the list
 * the status store reads too, narrowed to this checkout and its project. A
 * `file_changed` burst — a save writes a temp file and renames it — is one
 * status read.
 */

import { useEffect, useMemo, useRef } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { movesWorkstream } from "../../shell/workstreamFramesModel.mjs";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import type { WorkstreamDetail, WorkstreamStatus } from "../../types";
import { useAsync } from "./useAsync";

const STATUS_COALESCE_MS = 300;

export interface WorkstreamData {
  /** The workstream this is the read of. */
  wid: string;
  detail: WorkstreamDetail | null;
  status: WorkstreamStatus | null;
  projectId: string | null;
  /** The first load only; refreshes keep the old data on screen. */
  loading: boolean;
  error: string | null;
  /** The record and the status again, now. */
  reload: () => void;
  /** The status alone — the tree moved, the record did not. */
  reloadStatus: () => void;
}

export function useWorkstream(wid: string): WorkstreamData {
  const detail = useAsync((s) => api.workstream(wid, s), [wid]);
  const status = useAsync((s) => api.workstreamStatus(wid, s), [wid]);
  const projectId = detail.data?.workstream.project ?? null;

  const reload = () => {
    detail.reload();
    status.reload();
  };
  const latest = useRef({ reload, status: status.reload, projectId });
  latest.current = { reload, status: status.reload, projectId };

  useReloadOnReconnect(() => latest.current.reload());
  const flush = useRef<number | null>(null);
  useEffect(() => () => window.clearTimeout(flush.current ?? undefined), []);
  useEngineEvents((e) => {
    const p = e.payload;
    // This checkout's record moved, or its project's — a plain folder became a repository, say.
    if (movesWorkstream(p, wid, latest.current.projectId)) {
      latest.current.reload();
      return;
    }
    if (p.type === "file_changed" && p.scope === "workstream" && p.id === wid && flush.current === null) {
      flush.current = window.setTimeout(() => {
        flush.current = null;
        latest.current.status();
      }, STATUS_COALESCE_MS);
    }
  });

  return useMemo(
    () => ({
      wid,
      detail: detail.data,
      status: status.data?.status ?? null,
      projectId,
      loading: detail.loading && !detail.data,
      error: detail.error ?? (status.error && !status.data ? status.error : null),
      reload,
      reloadStatus: status.reload,
    }),
    // `reload` is remade every render over two stable doors; the object is
    // rebuilt on the facts that change what it says.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [wid, detail.data, detail.loading, detail.error, status.data, status.error, projectId],
  );
}
