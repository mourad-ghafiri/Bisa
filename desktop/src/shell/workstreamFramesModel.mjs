/**
 * Which engine facts move a workstream (ide/07): its record — the state, the
 * name, the note, the pin, the place on the Board — and what the node says
 * of its checkout (`WorkstreamStatus`, which carries the record's state and
 * name beside git's facts). One list, read by everything that re-reads on
 * them — the status store the rail and the Board share
 * (`workstreamStatusStore.ts`) and the one checkout the Workstreams panel
 * stands in (`useWorkstream.ts`) — so a surface cannot follow a fact another
 * one misses: a rename once reached the panel and not the rail's statuses,
 * because each spelled its own list.
 *
 * A commit is a commit whichever door made it: the node moves the record and
 * says `workstream_changed` and `workstream_committed` for the Changes view's
 * commit as for the workstream's own, so nothing here tells the doors apart.
 *
 * Facts only, no React and no store. Plain `.mjs`, so `node --test` reads it.
 */

/** The facts that move one workstream's record, each naming it as `workstream`. */
export const RECORD_FRAMES = Object.freeze(["workstream_opened", "workstream_changed", "workstream_committed", "workstream_edited"]);

/**
 * The facts that move what every checkout of a project says: who commits is
 * the repository's, shared by every worktree of it, and the project's own
 * record carries the publishing policy, the default branch and whether the
 * folder is a repository at all. Each names the project as `project`.
 */
export const PROJECT_FRAMES = Object.freeze(["committer_set", "project_changed"]);

/**
 * Whether a fact of this type is worth reading every workstream's status
 * again.
 * @param {unknown} type
 */
export function movesStatuses(type) {
  return typeof type === "string" && (RECORD_FRAMES.includes(type) || PROJECT_FRAMES.includes(type));
}

/**
 * Whether a fact moves **this** workstream: one that names it, or one about
 * the project it is a checkout of. A fact about another workstream, or
 * another project, moves nothing here.
 * @param {{type?: unknown, workstream?: unknown, project?: unknown} | null | undefined} payload an engine event's payload
 * @param {string} wid the workstream
 * @param {string | null | undefined} project its project, once the record was read
 */
export function movesWorkstream(payload, wid, project) {
  const type = payload?.type;
  if (typeof type !== "string") return false;
  if (RECORD_FRAMES.includes(type)) return payload.workstream === wid;
  if (PROJECT_FRAMES.includes(type)) return payload.workstream === wid || (typeof project === "string" && project !== "" && payload.project === project);
  return false;
}
