/**
 * Where the Project IDE stands when nobody chose (D9): **a home is an open
 * workstream of a listed project**. `#/projects` lands on the workstream you
 * were last in while it is one, else the first project's primary (by name),
 * else nowhere — which the workbench renders as the empty IDE with its one
 * door, *New project*.
 *
 * The same rule decides where the IDE goes when the project under the person
 * is archived or removed — by the rail, by About, by the command line or by
 * another window: the home computed from the facts in hand with that project
 * taken out (`homeAfterLeaving`), the same moment, by `replace`. An archived
 * project's checkouts are nobody's home: the live `projects` list is the one
 * consulted, and `GET /workstreams` lists every project's checkouts, an
 * archived one's too.
 */

/** @typedef {{scope: "workstream", id: string}} Home */

/**
 * The open workstreams of the listed projects: what a home is chosen among.
 * @param {readonly import("../../types").ProjectRow[] | null | undefined} projects
 * @param {readonly import("../../types").WorkstreamRef[] | null | undefined} workstreams
 */
function homes(projects, workstreams) {
  const listed = new Set((projects ?? []).map((p) => p.project.id));
  return (workstreams ?? []).filter((w) => w.workstream.state?.state !== "closed" && listed.has(w.workstream.project));
}

/**
 * @param {string | null | undefined} lastRoot — a workstream id remembered in localStorage
 * @param {readonly import("../../types").ProjectRow[] | null | undefined} projects the live list — archived projects are not on it
 * @param {readonly import("../../types").WorkstreamRef[] | null | undefined} workstreams
 * @returns {Home | null}
 */
export function homeRoot(lastRoot, projects, workstreams) {
  const open = homes(projects, workstreams);
  if (lastRoot && open.some((w) => w.workstream.id === lastRoot)) return { scope: "workstream", id: lastRoot };
  const sorted = [...(projects ?? [])].sort((a, b) => a.project.name.localeCompare(b.project.name, undefined, { sensitivity: "base" }));
  for (const p of sorted) {
    // The primary's id is the project's.
    if (open.some((w) => w.workstream.id === p.project.id)) return { scope: "workstream", id: p.project.id };
  }
  const first = sorted[0];
  return first ? { scope: "workstream", id: first.project.id } : null;
}

/**
 * The home once `pid` is archived or removed: `homeRoot` over the lists with
 * the project and its checkouts taken out, the remembered root kept only if
 * it stands off the project. Computed from the facts in hand — before the
 * node answers and before the lists are read again — so the person is moved
 * the moment the act is done, to a place that is still there.
 * @param {string} pid the project leaving
 * @param {string | null | undefined} lastRoot
 * @param {readonly import("../../types").ProjectRow[] | null | undefined} projects
 * @param {readonly import("../../types").WorkstreamRef[] | null | undefined} workstreams
 * @returns {Home | null}
 */
export function homeAfterLeaving(pid, lastRoot, projects, workstreams) {
  const remaining = (workstreams ?? []).filter((w) => w.workstream.project !== pid);
  const stands = remaining.some((w) => w.workstream.id === lastRoot);
  return homeRoot(
    stands ? lastRoot : null,
    (projects ?? []).filter((p) => p.project.id !== pid),
    remaining,
  );
}

/**
 * Whether an engine fact takes the root from under the person: the project
 * the root belongs to was deleted, or archived (`archived: true` — an
 * unarchive brings nothing down). Another project's fact and a root that is
 * no workstream leave nobody. The root's project is read from the workspace's
 * list; a root the list does not know yet is left standing — the read of the
 * root itself answers *not found* if it went (`useGonePlace`).
 * @param {{type: string, project?: string | null, archived?: boolean}} payload
 * @param {{scope: string, id: string} | null | undefined} root the root the workbench is on
 * @param {readonly import("../../types").WorkstreamRef[] | null | undefined} workstreams
 * @returns {string | null} the project leaving, when the fact leaves the root
 */
export function leavesRoot(payload, root, workstreams) {
  if (root?.scope !== "workstream") return null;
  const deleted = payload.type === "project_deleted";
  const archived = payload.type === "project_archived" && payload.archived === true;
  if (!deleted && !archived) return null;
  const project = (workstreams ?? []).find((w) => w.workstream.id === root.id)?.workstream.project ?? null;
  return project !== null && project === payload.project ? project : null;
}

/**
 * The route a home is: the workbench on it, or the landing when there is
 * none. The one translation the rail, About and the workbench share.
 * @param {Home | null} home
 * @returns {{name: "workbench", scope: "workstream", id: string} | {name: "projects"}}
 */
export function homeRoute(home) {
  return home ? { name: "workbench", scope: home.scope, id: home.id } : { name: "projects" };
}
