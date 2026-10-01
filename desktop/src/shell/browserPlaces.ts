/**
 * The names every list of browser tabs uses (ide/18): one index over the
 * workspace the shell already holds — goals, workstreams, projects,
 * channels, direct messages, the Inbox's conversations — and the workflow
 * rows, read once here and again when a workflow changes. The rules are
 * `browserPlacesModel.mjs`'s; this hook only gathers the rows.
 */

import { useMemo } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useAsync } from "../views/_work/useAsync";
import { browserPlaces } from "./browserPlacesModel.mjs";
import type { BrowserPlaces } from "./browserPlacesModel.mjs";
import { useWorkspace } from "./useWorkspaceData";

export function useBrowserPlaces(): BrowserPlaces {
  const ws = useWorkspace();
  const workflows = useAsync((s) => api.workflows({ scope: "all" }, s).then((r) => r.workflows), []);
  useEngineEvents((e) => {
    if (e.payload.type === "workflow_changed" || e.payload.type === "workflow_deleted" || e.payload.type === "workflow_archived") workflows.reload();
  });
  return useMemo(
    () => browserPlaces({ goals: ws.goals, workstreams: ws.workstreams, projects: ws.projects, channels: ws.channels, dms: ws.dms, inbox: ws.inbox }, workflows.data ?? []),
    [ws.goals, ws.workstreams, ws.projects, ws.channels, ws.dms, ws.inbox, workflows.data],
  );
}
