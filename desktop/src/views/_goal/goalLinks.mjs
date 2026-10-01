/**
 * Where a path said about a goal is looked up (ide/17 §Paths): the primary
 * checkouts of the projects attached to it. One rule, read wherever the
 * goal's thread is drawn — its Conversation tab.
 */

/**
 * @param {readonly {exists: boolean, goals: readonly string[], project: {id: string}}[]} projects the workspace's projects
 * @param {string} goal
 * @returns {{scope: "workstream", id: string}[]}
 */
export function goalLinkRoots(projects, goal) {
  return (projects ?? []).filter((p) => p.exists && p.goals.includes(goal)).map((p) => ({ scope: "workstream", id: p.project.id }));
}
