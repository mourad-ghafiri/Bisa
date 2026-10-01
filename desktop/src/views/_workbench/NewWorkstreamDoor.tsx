/**
 * The one door to a new workstream, wherever it is knocked on: the rail's `+`,
 * a project's menu, the header's Add menu, ⌘⇧W, the palette, the Git doors on
 * a ref, the Workstreams panel's menu — every one fires `NEW_WORKSTREAM`
 * naming a project (else the current root's), and this component listens,
 * resolves the project against the **whole** workspace — the projects on
 * screen and the ones put away, the same lists the rail draws — mounts the
 * dialog, and lands the new checkout on its own panel (ide/07). A project the
 * workspace no longer has is a toast, never a click that does nothing.
 *
 * Mounted by the workbench and by `#/projects`, so the rail's doors work on
 * both screens, with or without a root open.
 */

import { useEffect, useState } from "react";
import { navigate } from "../../router";
import { NEW_WORKSTREAM, onDoor, type NewWorkstreamRequest } from "../../shell/shortcuts";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { toggleCollapsed, useCollapsedUnder, useToast } from "../../ui";
import { NewWorkstreamDialog } from "../_work/NewWorkstreamDialog";
import type { SourcePreset } from "../_work/workstreamCreation.mjs";
import { projectOf } from "./projectRailModel.mjs";
import { showRightPanel } from "./rightPanelStore";
import { rootKey } from "./workbenchModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function NewWorkstreamDoor({ defaultPid }: { defaultPid: string | null }) {
  const ws = useWorkspace();
  const toast = useToast();
  const railFolds = useCollapsedUnder("rail.");
  const [request, setRequest] = useState<{ pid: string; preset: SourcePreset | null } | null>(null);
  useEffect(
    () =>
      onDoor(NEW_WORKSTREAM, (raw) => {
        const detail = raw as NewWorkstreamRequest | undefined;
        const pid = detail?.pid ?? defaultPid;
        if (pid) setRequest({ pid, preset: detail?.preset ?? null });
      }),
    [defaultPid],
  );
  const project = request ? projectOf(ws.projects, ws.archivedProjects, request.pid)?.project ?? null : null;
  // A door that named a project the workspace no longer has — a row drawn
  // before a delete landed, a stale palette entry — says so and closes.
  useEffect(() => {
    if (request && !project) {
      toast.error(t("workbench-new-workstream-door-project-not-workspace-any-more"));
      setRequest(null);
    }
  }, [request, project, toast]);
  if (!request || !project) return null;
  return (
    <NewWorkstreamDialog
      open
      onClose={() => setRequest(null)}
      pid={project.id}
      projectName={project.name}
      projectSlug={project.slug}
      preset={request.preset}
      onOpened={(wid) => {
        ws.refresh();
        // The new row is under its project in the rail: make sure the project is open.
        for (const key of railFolds) if (key.startsWith(`rail.project.${project.id}.`)) toggleCollapsed(key);
        // The new checkout lands on its own panel.
        showRightPanel("workstreams", rootKey("workstream", wid));
        navigate({ name: "workbench", scope: "workstream", id: wid });
      }}
    />
  );
}
