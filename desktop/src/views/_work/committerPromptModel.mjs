/**
 * The queue behind the one "who commits?" dialog.
 *
 * Every project the engine could not find a committer for arrives here —
 * seeded from `GET /git/committer` when the shell mounts, then moved by
 * `committer_needed` and `committer_set` frames — and the dialog shows the
 * head of the queue. One entry per project: a second ask for the same
 * project updates its reason and nothing else. Answering (the frame says so,
 * whichever door the answer came through) removes it; *Not now* skips it for
 * this session only, so the next launch asks again. Nothing here persists:
 * the desk is the engine's, and a restart re-seeds from it.
 */

/** @typedef {{project: string, slug: string, workstream: string, reason: string, origin: {origin: string} | null, global: {name: string, email: string} | null}} Ask */

export function emptyQueue() {
  return { asks: [], skipped: [] };
}

/** One ask, normalised from a frame or an overview row. */
function askOf(row) {
  return {
    project: String(row.project),
    slug: String(row.slug ?? ""),
    workstream: String(row.workstream ?? row.project),
    reason: String(row.reason ?? "created"),
    origin: row.origin ?? null,
    global: row.global ?? null,
  };
}

/** Add or refresh one ask. A skipped project stays skipped. */
function upsert(queue, ask) {
  if (queue.skipped.includes(ask.project)) return queue;
  const i = queue.asks.findIndex((a) => a.project === ask.project);
  if (i < 0) return { ...queue, asks: [...queue.asks, ask] };
  const asks = queue.asks.slice();
  // Keep the pair the first frame offered when a later one carries none.
  asks[i] = { ...asks[i], ...ask, global: ask.global ?? asks[i].global, origin: ask.origin ?? asks[i].origin };
  return { ...queue, asks };
}

/**
 * The overview's `pending` rows — at mount, and again when the bus comes
 * back: what the engine is asking that this window has not heard. A row
 * carries its origin and global pair like a frame does; `global` is the
 * overview's pair for a row without one.
 */
export function seed(queue, pending, global = null) {
  let q = queue;
  for (const row of pending ?? []) q = upsert(q, askOf({ ...row, global: row.global ?? global }));
  return q;
}

/** An engine frame arrived. */
export function onFrame(queue, payload) {
  if (!payload) return queue;
  switch (payload.type) {
    case "committer_needed":
      return upsert(queue, askOf(payload));
    case "committer_set":
      return remove(queue, String(payload.project));
    default:
      return queue;
  }
}

/** The question was answered — by this dialog or elsewhere. */
export function remove(queue, project) {
  if (!queue.asks.some((a) => a.project === project)) return queue;
  return { ...queue, asks: queue.asks.filter((a) => a.project !== project) };
}

/** *Not now*: gone for this session, back after a relaunch. */
export function skip(queue, project) {
  return {
    asks: queue.asks.filter((a) => a.project !== project),
    skipped: queue.skipped.includes(project) ? queue.skipped : [...queue.skipped, project],
  };
}

/** What the dialog shows: the oldest unanswered ask, or nothing. */
export function head(queue) {
  return queue.asks[0] ?? null;
}

/** How many are waiting behind the one shown. */
export function remaining(queue) {
  return Math.max(0, queue.asks.length - 1);
}
