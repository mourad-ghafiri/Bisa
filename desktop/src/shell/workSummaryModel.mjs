/**
 * What the workspace is doing, in three counts — what waits on the person,
 * what is open for review, what is being worked on — from the facts the
 * shell already holds. The pet stands by them and an addon granted
 * `workspace_summary` is told them; one rule, so the companion in the corner
 * and a widget beside it never say two things about one moment.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

/**
 * @param {{inbox: readonly {needs_action?: readonly {gate_kind?: string | null}[] | null}[], waiting: number, working: Readonly<Record<string, readonly string[]>>}} workspace the shell's lists
 * @param {{waiting: number, working: number}} sessions the roster's tally (`sessionState.counts`)
 * @returns {{waiting: number, review: number, working: number, busyScope: string | null}}
 */
export function workSummary(workspace, sessions) {
  // Something finished, or was proposed, and is waiting to be looked at: an approval gate.
  const review = (workspace.inbox ?? []).filter((row) => (row.needs_action ?? []).some((ask) => ask.gate_kind === "approval")).length;
  // The conversations an agent is mid-turn in; the first is where a click lands.
  const busy = Object.entries(workspace.working ?? {}).filter(([, who]) => who.length > 0);
  return {
    // A worker running a step is work even when no conversation moves, and a session waiting on a person is a wait.
    waiting: (workspace.waiting ?? 0) + (sessions?.waiting ?? 0),
    review,
    working: busy.length + (sessions?.working ?? 0),
    busyScope: busy[0]?.[0] ?? null,
  };
}
