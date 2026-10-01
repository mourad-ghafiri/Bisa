/**
 * What is gone is forgotten (`crates/desktop.md` §Routing): the paths a
 * fact on the bus takes out of the place memory. A remembered place must
 * never lead to a dead page — so a goal deleted, a workflow deleted, a
 * project deleted, a conversation deleted, a checkout closed and a hosted
 * workspace left each name the paths that stood on them. An **archived**
 * thing is still there, and is kept.
 *
 * Facts only, no React and no store: `App.tsx` hears the bus and forgets
 * what this names. What went while the app was closed raises no fact; the
 * screen that opens on it finds nothing and leaves (`useGonePlace.ts`).
 * Plain `.mjs`, so `node --test` reads it.
 */

import { href } from "../routeModel.mjs";

/** A route's path, without its hash mark. */
function pathFor(route) {
  return href(route).slice(1);
}

/**
 * The paths an engine fact takes out of the memory; none for a fact that
 * removes nothing.
 * @param {{type?: string} & Record<string, unknown>} payload an engine event's payload
 * @returns {string[]}
 */
export function gonePaths(payload) {
  const text = (v) => (typeof v === "string" && v ? v : null);
  switch (payload?.type) {
    case "goal_deleted": {
      const id = text(payload.goal);
      return id ? [pathFor({ name: "goal", id }), pathFor({ name: "workbench", scope: "goal", id })] : [];
    }
    case "workflow_deleted": {
      const id = text(payload.workflow);
      return id ? [pathFor({ name: "workflow", id })] : [];
    }
    case "project_deleted": {
      const id = text(payload.project);
      return id ? [pathFor({ name: "workbench", scope: "workstream", id })] : [];
    }
    case "conversation_changed": {
      const id = text(payload.id);
      return id && payload.change === "deleted" ? [pathFor({ name: "conversation", id })] : [];
    }
    case "workstream_changed": {
      const id = text(payload.workstream);
      const closed = payload.state && typeof payload.state === "object" && payload.state.state === "closed";
      return id && closed ? [pathFor({ name: "workbench", scope: "workstream", id })] : [];
    }
    default:
      return [];
  }
}

/**
 * The paths of the hosted workspaces a person is no longer in: every host
 * a remembered place names that the memberships no longer list as one they
 * are a member of.
 * @param {readonly string[]} remembered the paths the memory keeps
 * @param {readonly string[]} hosts the hosts the person is a member of
 * @returns {string[]} one path a host, `/hosts/<host>`
 */
export function leftHosts(remembered, hosts) {
  const out = new Set();
  for (const path of remembered) {
    const m = /^\/hosts\/([^/]+)\//.exec(path);
    if (!m) continue;
    let host = m[1];
    try {
      host = decodeURIComponent(host);
    } catch {
      // A host that is no escape is no host: its place goes.
    }
    if (!hosts.includes(host)) out.add(`/hosts/${m[1]}`);
  }
  return [...out];
}

/**
 * Whether the memberships the shell holds are the node's answer: the
 * workspace was read, the node is there, and the last read of the hosts
 * answered. Only then does a host missing from the list mean the person is
 * no longer in it — a read that failed leaves the list as it was, or empty
 * at a first load, and says nothing of who left.
 * @param {{ready: boolean, offline: string | null, hostsRead: boolean}} workspace
 */
export function membershipsKnown(workspace) {
  return !!workspace.ready && !workspace.offline && !!workspace.hostsRead;
}
