/**
 * Everything the Inbox *decides*, with no React and no DOM in it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, for the same reason
 * `ui/fileTreeModel.mjs` is: `node --test` imports the real module rather
 * than a transcription of it, and there is no jsdom here to render a
 * component into. Which rows a filter shows, what a row's state is called,
 * what order rows come in and — the one that used to be a bug — which row
 * stays selected are all decisions, and decisions belong somewhere a test can
 * reach. The view above is then paint and effects.
 *
 * # The vocabulary
 *
 * A row is a **thing** — a goal, a channel, a direct channel, a conversation, a workstream,
 * a project, a workflow — never an event. It carries **asks** (`needs_action`:
 * a gate or a question, what is owed, answered here) and **notices**
 * (`notices`: what happened to the thing — a run finished or failed, a
 * step blocked, a start event that could not start a run, a script that
 * failed, a pull request opened — read here, opened there). A notice is
 * worded by the same `pulseLine` a Pulse row gets, so the two screens never
 * say one fact two ways. What is *owed* is the asks alone: a notice never
 * counts as owed.
 *
 * # Why the screen filters, and not only the node
 *
 * `GET /inbox` takes the same `?filter=`/`?kind=` this module implements, and
 * the desktop still asks it for everything and narrows here. That is not
 * duplication for its own sake: the `inbox` SSE channel patches a row *in
 * place*, so a row's read/handled state changes between fetches. A
 * server-side filter would mean the row you just read has to be re-requested
 * to find out it no longer matches — which is the wholesale array replace
 * this whole change exists to remove. Holding every row and re-deciding
 * locally is what lets a row change under you without moving.
 */

import { pulseLine } from "../../activityModel.mjs";
import { isAnswerAsk } from "../../askModel.mjs";
import { namesAdoption } from "../_goal/proposalRouting.mjs";
import { settingsSearch } from "../_settings/settingsLink.mjs";
import { originIcon, originTakesId, originWords } from "./conversationsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The primary buckets. Three, not five: what is owed, what is new, and
 * everything you keep.
 */
const FILTERS = [
  { id: "needs_you", label: t("studio-inbox-needs") },
  { id: "unread", label: t("studio-inbox-unread") },
  { id: "all", label: t("studio-inbox-all") },
];

/**
 * The sources, which narrow *within* a bucket rather than replacing it:
 * what a row is **about** — the messages (channels, direct channels and a
 * conversation about nothing in particular), the projects (workstreams,
 * projects and a conversation about a checkout), the workflows, the goals
 * (and a conversation about one), the people. The node says each row's
 * (`row.source`); nothing here mirrors its rule. A start event that could
 * not start a run is a notice on its workflow's row, or on the row of the
 * goal that listens.
 */
/** The source tabs, in the order they stand, each with its glyph as the key of `ui/icons.ts` it is drawn from — the mark before the word, and the whole tab on a window too narrow for the words. */
export const SOURCES = [
  { id: "any", label: t("studio-inbox-any"), icon: "inbox" },
  { id: "messages", label: t("studio-inbox-messages"), icon: "dm" },
  { id: "projects", label: t("studio-inbox-projects"), icon: "project" },
  { id: "workflows", label: t("studio-inbox-workflows"), icon: "workflow" },
  { id: "goals", label: t("studio-inbox-goals"), icon: "goal" },
  { id: "people", label: t("studio-inbox-people"), icon: "members" },
];

/**
 * The bucket the address names, *Needs you* for anything else — a link
 * written by hand, an older build's word.
 * @param {unknown} raw
 * @returns {"needs_you" | "unread" | "all"}
 */
export function filterOf(raw) {
  return FILTERS.some((f) => f.id === raw) ? raw : "needs_you";
}

/**
 * The source the address names, *Any* for anything else.
 * @param {unknown} raw
 * @returns {"any" | "goals" | "projects" | "workflows" | "messages" | "people"}
 */
export function sourceIdOf(raw) {
  return SOURCES.some((s) => s.id === raw) ? raw : "any";
}

/**
 * Where a browser tab opened beside an Inbox row is at home (ide/18): the
 * goal, channel, message, conversation, workstream or workflow the row is;
 * nothing for a person, a session or a project.
 * @param {{kind: string, key: string} | null | undefined} row
 * @returns {{scope: string, id: string} | null}
 */
export function browserHomeOf(row) {
  if (!row) return null;
  switch (row.kind) {
    case "goal":
    case "channel":
    case "dm":
    case "conversation":
    case "workstream":
    case "workflow":
      return { scope: row.kind, id: row.key };
    default:
      return null;
  }
}

/**
 * What a conversation row is about, in words — *about goal Dark mode*,
 * *about workstream cart in project web* — through the conversations' own
 * `originWords`; `null` for a row that is no conversation, or one about
 * nothing in particular (the node, the workspace): its tab already says.
 * @param {{kind?: string, origin?: {kind: string, id?: string, project?: string} | null} | null | undefined} row
 * @param {{goal?: (id: string) => string | null, project?: (id: string) => string | null, workstream?: (id: string) => string | null, workflow?: (id: string) => string | null}} [names]
 */
export function aboutWords(row, names = {}) {
  const origin = row?.kind === "conversation" ? row.origin : null;
  if (!origin || !originTakesId(origin.kind)) return null;
  return t("studio-inbox-about", { what: originWords(origin, names) });
}

/** The glyph key per kind — `ui/icons.ts`'s `INBOX_KIND_ICON` reads the same names. */
const KIND_GLYPH = Object.freeze({
  goal: "goal",
  channel: "channel",
  dm: "dm",
  conversation: "conversation",
  workstream: "workstream",
  project: "project",
  workflow: "workflow",
  people: "members",
  session: "harness",
});

/**
 * The glyph a row wears — the key of `ui/icons.ts`: its kind's, except a
 * conversation about a thing, which wears that thing's, so a conversation
 * about a goal reads as the goal's and never as a direct message's.
 * @param {{kind?: string, origin?: {kind: string} | null} | null | undefined} row
 * @returns {string}
 */
export function rowGlyph(row) {
  const origin = row?.kind === "conversation" ? row.origin : null;
  if (origin && originTakesId(origin.kind)) return originIcon(origin.kind);
  return KIND_GLYPH[row?.kind] ?? "inbox";
}

/**
 * A row's state, as one word.
 *
 * The screen had no vocabulary for read at all — a row was either present or
 * deleted — so this is where the three states it now has are named once:
 *
 * - `waiting`: something is owed. Outranks everything, including read: you
 *   can have read a gate and still not have decided it.
 * - `unread`: new to you and nothing is owed.
 * - `read`: you have seen everything here.
 *
 * `handled` rides alongside rather than as a fourth state, because it answers
 * a different question — *the workspace* decided something here — and a
 * handled row can perfectly well be unread to you.
 */
export function rowState(row) {
  if (needsOf(row).length > 0 || joinOf(row) || waitingOf(row)) return "waiting";
  return row.read ? "read" : "unread";
}

/**
 * The harness waiting on you in its terminal, on a session row (ide/06
 * §Reporting): what it waits on, in the roster's words, and where it
 * stands. The one reader of `waiting`.
 */
export function waitingOf(row) {
  return row?.waiting ?? null;
}

/**
 * What the session card says: the harness, by the label the catalog gives
 * it, and the wait — *Claude Code is waiting on you — permission: Bash*.
 * @param {object} row @param {Record<string, string>} [labels] harness labels by id
 */
export function waitingWords(row, labels = {}) {
  const w = waitingOf(row);
  if (!w) return null;
  return t("studio-inbox-waiting", { harness: labels[w.harness] ?? w.harness, words: w.words });
}

/**
 * The claim waiting on you on a people row (14-collaboration): somebody
 * claimed an invitation under `collab.join = ask`. The one reader of `join`.
 */
export function joinOf(row) {
  return row?.join ?? null;
}

/** What the join card says: who asks, as what. */
export function joinWords(row) {
  const j = joinOf(row);
  if (!j) return null;
  return t("studio-inbox-asks-join", { title: j.label || row.title, role: j.role });
}

/**
 * What a row says is waiting on you — the one reader of `needs_action`, so
 * a row the node sent without it (or a frame that trimmed it) is a row
 * with nothing owed, never a throw above the error boundary.
 */
export function needsOf(row) {
  return Array.isArray(row?.needs_action) ? row.needs_action : [];
}

/**
 * What tells one owed decision from the next in a row's list, for a list
 * drawn once each: the live gate's id; for one rebuilt from durable state —
 * a waiting step, an adoption — the step it completes, else the gate's kind
 * and its question. Never its place in the list, which moves as one is
 * decided and would hand a card the state of the one before it.
 * @param {{gate_id?: string | null, run?: string | null, step?: string | null, gate_kind: string, question: string}} action
 */
export function needsKey(action) {
  if (action.gate_id) return action.gate_id;
  if (action.run && action.step) return `${action.run}:${action.step}`;
  return `${action.gate_kind}:${action.question}`;
}

/**
 * What happened to a row — the one reader of `notices`, so a row the node
 * sent without them is a row nothing happened to, never a throw.
 */
export function noticesOf(row) {
  return Array.isArray(row?.notices) ? row.notices : [];
}

/** How many of a row's notices are after your watermark. */
export function unreadNoticesOf(row) {
  const n = row?.unread_notices;
  return typeof n === "number" && n > 0 ? Math.min(n, noticesOf(row).length) : 0;
}

/** Whether a decision is on record and nothing is outstanding. */
export function isHandled(row) {
  return row.handled === true && needsOf(row).length === 0;
}

/** Does this row belong in the bucket? */
export function matchesFilter(row, filter) {
  switch (filter) {
    case "needs_you":
      // The node's rule: an ask, a join waiting to be admitted, a harness
      // waiting in its terminal.
      return needsOf(row).length > 0 || joinOf(row) !== null || waitingOf(row) !== null;
    case "unread":
      return !row.read;
    default:
      return true;
  }
}

/** Does this row sit under the source? The row's own word — the node's. */
export function matchesSource(row, source) {
  return source === "any" || row?.source === source;
}

/**
 * Sort bucket. Mirrors the node's, so a client-side re-sort after a patched
 * row does not reshuffle the list into a different order than a refetch
 * would have produced.
 */
export function rank(row) {
  const state = rowState(row);
  if (state === "waiting") return 0;
  if (state === "unread") return 1;
  return 2;
}

/** What is owed, then what is new, then everything kept; newest first inside. */
export function compareRows(a, b) {
  const byRank = rank(a) - rank(b);
  if (byRank !== 0) return byRank;
  return b.latest_at - a.latest_at;
}

/** The visible list: filtered by bucket and source, then ordered. */
export function visibleRows(rows, filter, source) {
  return rows
    .filter((r) => matchesFilter(r, filter) && matchesSource(r, source))
    .sort(compareRows);
}

/**
 * The counts the filter strip wears: every bucket over every row, and every
 * source within the bucket that is on — so a badge says what pressing the
 * tab would show.
 * @param {readonly object[]} rows
 * @param {"needs_you" | "unread" | "all"} filter
 */
export function counts(rows, filter) {
  const buckets = { needs_you: 0, unread: 0, all: rows.length };
  const sources = { any: 0, messages: 0, projects: 0, workflows: 0, goals: 0, people: 0 };
  for (const r of rows) {
    if (matchesFilter(r, "needs_you")) buckets.needs_you += 1;
    if (matchesFilter(r, "unread")) buckets.unread += 1;
    if (!matchesFilter(r, filter)) continue;
    sources.any += 1;
    const source = r.source;
    if (source && source in sources) sources[source] += 1;
  }
  return { buckets, sources };
}

/**
 * The bucket segments the strip draws: each bucket's word, with its count
 * after a dot when it has one.
 * @param {Record<"needs_you" | "unread" | "all", number>} buckets
 */
export function filterSegments(buckets) {
  return FILTERS.map((f) => ({ id: f.id, label: buckets[f.id] > 0 ? `${f.label} · ${buckets[f.id]}` : f.label }));
}

/**
 * The source tabs the strip draws: each source's word, its glyph key and
 * its count — the badge, and the tooltip when the strip folds.
 * @param {Record<"any" | "messages" | "projects" | "workflows" | "goals" | "people", number>} sources
 */
export function sourceTabs(sources) {
  return SOURCES.map((s) => ({ id: s.id, label: s.label, icon: s.icon, count: sources[s.id] ?? 0 }));
}

/** The three groups the list is drawn in, in order, empty ones left out. */
const GROUPS = Object.freeze([
  { id: "waiting", title: t("studio-inbox-waiting-2") },
  { id: "new", title: t("studio-chat-new") },
  { id: "kept", title: t("studio-inbox-kept") },
]);

/**
 * The visible rows in their groups — what is owed, what is new, what you
 * keep — each already ordered by `visibleRows`.
 * @param {readonly object[]} visible
 */
export function groupRows(visible) {
  const by = { waiting: [], new: [], kept: [] };
  for (const r of visible) {
    const state = rowState(r);
    by[state === "waiting" ? "waiting" : state === "unread" ? "new" : "kept"].push(r);
  }
  return GROUPS.filter((g) => by[g.id].length > 0).map((g) => ({ ...g, rows: by[g.id] }));
}

/**
 * What stays selected after the list changes.
 *
 * The rule is the whole point of this wave: **the selection never jumps.** It
 * used to be "if the selected key is not in `rows`, select `rows[0]`", and
 * because a row was deleted the moment you read it, reading a conversation
 * silently moved you to a different one — usually somebody else's.
 *
 * So a selection is only ever *chosen* when there is none, and only ever
 * *replaced* when the conversation itself is gone from the workspace, not
 * merely filtered out of view. `known` is the unfiltered set for exactly that
 * distinction: switching to "Needs you" must not re-point the detail pane.
 */
export function nextSelection(visible, known, selected) {
  if (selected && known.some((r) => r.key === selected)) return selected;
  if (visible.length > 0) return visible[0].key;
  return known.length > 0 ? known[0].key : null;
}

/**
 * Apply one `inbox` SSE frame to the rows held on screen.
 *
 * A frame for a conversation with no row yet is dropped rather than turned
 * into a half-built row: the frame carries state, not identity — no title, no
 * kind — and a row that says "undefined" until the next fetch is worse than a
 * row that appears a moment later.
 */
export function applyDelta(rows, frame) {
  let changed = false;
  const next = rows.map((r) => {
    if (r.key !== frame.key) return r;
    changed = true;
    // A session row is its wait: a frame that says the wait is over drops
    // the row — nothing else about it can change in place.
    if (r.kind === "session") return frame.waiting === false ? null : r;
    const patched = {
      ...r,
      unread_count: frame.unread_count,
      latest_at: frame.latest_at,
      read: frame.read,
      handled: frame.handled,
      // The frame counts what is waiting; it does not carry the questions
      // themselves. Trimming a list we cannot refill would lose the text of a
      // gate, so the count only ever *clears* the list.
      needs_action: frame.needs_action_count === 0 ? [] : needsOf(r),
    };
    // The notice counts ride only when the cause could have moved them; a
    // frame without them leaves what is held. The same rule: a count of
    // zero clears, a count the row cannot fill is `needsReload`'s to say.
    if (typeof frame.notice_count === "number") {
      patched.notices = frame.notice_count === 0 ? [] : noticesOf(r);
      patched.unread_notices = frame.unread_notices ?? 0;
    }
    return patched;
  });
  return changed ? next.filter((r) => r !== null) : rows;
}

/**
 * Whether a frame says more than the row on screen can show — a question or
 * a notice the frame counts but does not carry — so the list is read again.
 * A frame for a row nobody holds is the list's to fetch too.
 */
export function needsReload(rows, frame) {
  const row = rows.find((r) => r.key === frame.key);
  // A session's frame for a row nobody holds is a new wait only while it
  // waits; a wait that ended before the row was ever shown moves nothing.
  if (!row) return frame.kind !== "session" || frame.waiting === true;
  if (row.kind === "session") return false;
  if (frame.needs_action_count > needsOf(row).length) return true;
  return typeof frame.notice_count === "number" && frame.notice_count > noticesOf(row).length;
}

/**
 * The row's second line: what this conversation wants, in one phrase.
 *
 * Returns an icon *name* rather than a component, so this module stays free
 * of the UI kit and the test can assert on it.
 */
/**
 * A gate's one line, from its subject.
 *
 * `adopt:<workflow>@<rev>` is the Workflow Agent's proposal; `amend:<run>@…`
 * an amendment; `approval:<run>/<step>` or `step:<run>/<step>` a step of a
 * run. Anything else — a permission, a publish — names the kind and, when it
 * has one, the subject.
 */
function gateText(action) {
  const subject = action.subject ?? "";
  if (namesAdoption(subject)) return t("studio-inbox-adopt-proposed-workflow");
  if (subject.startsWith("amend:")) return t("studio-inbox-approve-amendment");
  if (subject.startsWith("release:")) return t("studio-inbox-release-held-step");
  if (action.step) return t("studio-inbox-gate-step", { gate: action.gate_kind, step: action.step });
  const m = /^(?:approval|step):[^/]+\/(.+)$/.exec(subject);
  if (m) return t("studio-inbox-gate-step", { gate: action.gate_kind, step: m[1] });
  return subject ? t("studio-inbox-gate-subject", { gate: action.gate_kind, subject }) : t("studio-needs-action-gate", { gate_kind: action.gate_kind });
}

export function summarize(row) {
  const waiting = waitingOf(row);
  if (waiting) return { text: t("studio-inbox-waiting-3", { words: waiting.words }), icon: "waiting", urgent: true, tone: "accent" };
  const needs = needsOf(row);
  if (needs.length > 1) {
    return { text: t("studio-inbox-waiting-4", { needs: needs.length }), icon: "waiting", urgent: true, tone: "accent" };
  }
  if (needs.length === 1) {
    const one = needs[0];
    if (isAnswerAsk(one.expects)) {
      // A question, not a clock: the point is that somebody asked you
      // something, which the clock would flatten into "this is taking time".
      return { text: t("studio-inbox-question"), icon: "question", urgent: true, tone: "accent" };
    }
    // The subject is what is actually being decided. "Approval gate" names
    // the ceremony; the subject names the thing — and the engine's subjects
    // have a grammar this can read: an adoption, an amendment, a step.
    return { text: gateText(one), icon: `gate:${one.gate_kind}`, urgent: true, tone: "accent" };
  }
  // The newest notice you have not read: what happened, in the Pulse's
  // words and tone, so a failure reads as a failure here too.
  const fresh = unreadNoticesOf(row);
  if (fresh > 0) {
    const line = noticeLine(noticesOf(row)[0]);
    const more = fresh > 1 ? ` (+${fresh - 1})` : "";
    return { text: `${line.text}${more}`, icon: `notice:${noticesOf(row)[0].notice}`, urgent: false, tone: line.tone };
  }
  if (isHandled(row)) {
    const d = row.decided;
    const gate = d?.gate_kind ?? t("studio-inbox-decision");
    return { text: d?.approve === false ? t("studio-inbox-gate-declined", { gate }) : t("studio-inbox-gate-approved", { gate }), icon: "handled", urgent: false, tone: "ok" };
  }
  return { text: kindLabel(row.kind), icon: `kind:${row.kind}`, urgent: false, tone: "dim" };
}

/**
 * The chip an ask's card wears: *question* for one a person answers in
 * words, else what its gate is about, from its subject — an adoption, an
 * amendment, a step held or to approve, a call the guard or a permission
 * put to you — and the gate's kind when the subject says nothing.
 * @param {{ subject?: string | null, gate_kind?: string }} action
 * @param {boolean} isQuestion `askModel.isAnswerAsk(action.expects)`
 */
export function askTitle(action, isQuestion) {
  if (isQuestion) return t("studio-needs-action-question");
  const s = action?.subject ?? "";
  const last = () => s.split("/").pop() ?? "";
  if (namesAdoption(s)) return t("studio-needs-action-adopt-workflow");
  if (s.startsWith("amend:")) return t("studio-needs-action-approve-amendment");
  if (s.startsWith("release:")) return t("studio-needs-action-release-step", { s: last() });
  if (s.startsWith("approval:")) return t("studio-needs-action-approval-step", { s: last() });
  // The guard put a tool call to you: the question carries the redacted command.
  if (s.startsWith("guard:")) return t("studio-needs-action-allow", { guard: s.slice("guard:".length) });
  if (s.startsWith("permission:")) return t("studio-needs-action-allow-2", { permission: s.slice("permission:".length) });
  return t("studio-needs-action-gate", { gate_kind: action?.gate_kind });
}

/**
 * The two verbs an ask's decision wears. A held step's release has one —
 * there is nothing to decline; a decision names Approve and Decline.
 * @param {{ subject?: string | null }} action
 * @returns {{ approve: string, decline: string | null }}
 */
export function askVerbs(action) {
  if ((action.subject ?? "").startsWith("release:")) return { approve: t("studio-inbox-release"), decline: null };
  return { approve: t("studio-pending-ask-approve"), decline: t("studio-pending-ask-decline") };
}

/**
 * A line under the verbs saying what the answer will do beyond this one
 * call — for the guard's asks, that the goal remembers it. `null` for every
 * other ask.
 * @param {Pick<NeedsAction, "subject">} action
 */
export function askHint(action) {
  if ((action.subject ?? "").startsWith("guard:")) return t("studio-inbox-answer-kept-goal-same-call-will");
  return null;
}

/**
 * One notice as a line: the Pulse's words for the fact, its tone, the door
 * it opens. `notice` is the node's `NoticeDto` — a Pulse row with its kind.
 */
export function noticeLine(notice) {
  const line = pulseLine(notice);
  return { key: line.key, at: notice.at, text: line.text, tone: line.tone, icon: line.icon, notice: notice.notice, source: notice.source };
}

/**
 * The door a row opens — the route and the search that goes with it. A
 * project's is its primary workstream (the two share one id); a workflow's
 * the page of the run of the workspace that asks, else the designer — where
 * a start event that could not start a run is mended; a goal's, the goal's
 * page, where its listening stands.
 */
export function doorOf(row) {
  switch (row.kind) {
    case "goal":
      return { route: { name: "goal", id: row.key }, search: null };
    case "channel":
      return { route: { name: "channel", id: row.key }, search: null };
    case "dm":
      return { route: { name: "dm", id: row.key }, search: null };
    case "workstream":
    case "project":
      return { route: { name: "workbench", scope: "workstream", id: row.key }, search: null };
    case "conversation":
      return { route: { name: "conversation", id: row.key }, search: null };
    case "workflow": {
      // A run of the workspace owes the ask: its page is where it is
      // answered and followed. Else the designer, which remembers its pane.
      const run = (row.needs_action ?? []).map((a) => a.home).find((h) => h?.home === "run");
      return run ? { route: { name: "run", id: run.run }, search: null } : { route: { name: "workflow", id: row.key }, search: null };
    }
    case "people":
      return { route: { name: "settings" }, search: settingsSearch("people") };
    case "session": {
      // The workstream the harness stands in; the screen adds the terminal
      // tab, which only it knows. A session with no workstream has no door.
      const w = waitingOf(row)?.workstream ?? null;
      return w ? { route: { name: "workbench", scope: "workstream", id: w }, search: null } : null;
    }
    default:
      return null;
  }
}

/**
 * The door's words, by the kind of thing a row is — *Open goal*, *Open in
 * the IDE*, *Open the terminal*; *Open* for a kind this list has no word for.
 * @param {string | null | undefined} kind
 */
export function doorLabel(kind) {
  switch (kind) {
    case "goal":
      return t("screens-inbox-open-goal");
    case "workflow":
      return t("screens-inbox-open-workflow");
    case "people":
      return t("screens-inbox-open-people");
    case "conversation":
      return t("screens-inbox-open-conversation");
    case "workstream":
    case "project":
      return t("screens-inbox-open-ide");
    case "session":
      return t("screens-inbox-open-terminal");
    default:
      return t("screens-inbox-open");
  }
}

/**
 * What an empty list says: by the bucket first — nothing waits, nothing is
 * new — and, under *All*, by the source it was narrowed to.
 * @param {"needs_you" | "unread" | "all"} filter
 * @param {"any" | "goals" | "projects" | "workflows" | "messages" | "people"} source
 * @returns {{title: string, hint: string}}
 */
export function emptyWords(filter, source) {
  if (filter === "needs_you") return { title: t("screens-goals-nothing-waits"), hint: t("screens-inbox-questions-from-agents-gates-held-steps") };
  if (filter === "unread") return { title: t("screens-inbox-nothing-new"), hint: t("screens-inbox-run-finished-script-failed-message-named") };
  const from = SOURCES.find((s) => s.id === source && s.id !== "any");
  return {
    title: from ? t("screens-inbox-nothing-from-yet", { source: from.label.toLowerCase() }) : t("screens-inbox-nothing-here-yet"),
    hint: t("screens-inbox-everything-has-ever-concerned-keeps-row"),
  };
}

/**
 * The message stream a row has, when it has one: a goal's thread, a channel,
 * a direct channel, a conversation. A workstream, a project and a workflow
 * have none of their own — their rows are notices.
 */
export function conversationOf(row) {
  switch (row.kind) {
    case "goal":
      return { kind: "goal", scope: row.key };
    case "channel":
      return { kind: "channel", scope: row.key };
    case "dm":
      return { kind: "dm", scope: row.key };
    case "conversation":
      return { kind: "conversation", scope: row.key };
    default:
      return null;
  }
}

/**
 * Whether selecting a row is what reads it. A conversation marks itself read
 * as it is shown — the thread's own doing, wherever it is mounted
 * (`readModel.mjs`, `useReadAsShown`); a row with none — a workflow, a
 * project — has nothing else to do it, so the screen does, after the same beat.
 */
/**
 * Which kind of scope an id names in the workspace's two lists — a direct
 * channel, a standing channel — or none.
 * @param {string} id
 * @param {readonly {channel: {id: string}}[]} channels
 * @param {readonly {channel: {id: string}}[]} dms
 * @returns {"channel" | "dm" | null}
 */
export function scopeKindOf(id, channels, dms) {
  if (dms.some((d) => d.channel.id === id)) return "dm";
  if (channels.some((c) => c.channel.id === id)) return "channel";
  return null;
}

/**
 * The channel an inbox row stands for — a channel or a direct channel row
 * alone — as the address tray's seed; none for any other kind of row.
 * @template C
 * @param {{kind: string, key: string} | null | undefined} row
 * @param {readonly {channel: C & {id: string}}[]} channels
 * @param {readonly {channel: C & {id: string}}[]} dms
 * @returns {(C & {id: string}) | null}
 */
export function channelOfRow(row, channels, dms) {
  if (!row || (row.kind !== "channel" && row.kind !== "dm")) return null;
  return [...channels, ...dms].find((e) => e.channel.id === row.key)?.channel ?? null;
}

export function readOnSelect(row) {
  return conversationOf(row) === null;
}

/** The rows in view that a *Mark all read* would mark: the unread ones. */
export function unreadKeys(visible) {
  return visible.filter((r) => !r.read).map((r) => r.key);
}

/** How many marks a *Mark all read* sends at once: a full Inbox is not a burst of a thousand requests. */
export const MARKS_AT_ONCE = 8;

/**
 * The keys of a *Mark all read* in the groups they are sent in, each key
 * once, `MARKS_AT_ONCE` at a time.
 * @param {readonly string[]} keys
 * @param {number} [size]
 * @returns {string[][]}
 */
export function markBatches(keys, size = MARKS_AT_ONCE) {
  const each = [...new Set(keys)];
  const n = Number.isInteger(size) && size > 0 ? size : MARKS_AT_ONCE;
  const out = [];
  for (let i = 0; i < each.length; i += n) out.push(each.slice(i, i + n));
  return out;
}

/**
 * Whether the answer of a read of the list is the one to draw: the newest
 * read asked for. Two reads may be out at once — a frame's, a mark's, the
 * bus coming back — and the older one's answer, landing last, would put back
 * what the newer one replaced.
 * @param {number} asked the read's own number
 * @param {number} newest the number of the last read asked for
 */
export function isNewestRead(asked, newest) {
  return asked === newest;
}

/**
 * What a key does on the list, or nothing. `j`/`k` move, `Enter` and `o`
 * open, `a` focuses the ask, `e` marks read, `u` marks unread, `Escape`
 * clears the selection. Nothing while typing, nothing inside an ask's own
 * controls, nothing with a modifier held.
 * @param {string} key the `KeyboardEvent.key`
 * @param {{inInput: boolean, inAsk: boolean, modifier: boolean, hasSelection: boolean}} ctx
 * @returns {"next" | "prev" | "open" | "focus_ask" | "mark_read" | "mark_unread" | "clear" | null}
 */
export function keyAction(key, ctx) {
  if (ctx.inInput || ctx.inAsk || ctx.modifier) return null;
  switch (key) {
    case "j":
      return "next";
    case "k":
      return "prev";
    case "Enter":
    case "o":
      return ctx.hasSelection ? "open" : null;
    case "a":
      return ctx.hasSelection ? "focus_ask" : null;
    case "e":
      return ctx.hasSelection ? "mark_read" : null;
    case "u":
      return ctx.hasSelection ? "mark_unread" : null;
    case "Escape":
      return ctx.hasSelection ? "clear" : null;
    default:
      return null;
  }
}

export const KIND_LABEL = {
  goal: t("studio-inbox-goal"),
  channel: t("studio-inbox-channel"),
  dm: t("studio-inbox-direct-message"),
  conversation: t("studio-inbox-conversation"),
  workstream: t("studio-inbox-workstream"),
  project: t("studio-inbox-project"),
  workflow: t("studio-inbox-workflow"),
  people: t("studio-inbox-person"),
  session: t("studio-inbox-terminal"),
};

/** The word for a row's kind — the kind itself for one this list has no word for. */
export function kindLabel(kind) {
  return KIND_LABEL[kind] ?? String(kind ?? "");
}

/**
 * The engine frames that change what is owed or what happened, and so are
 * worth a list reload when their `inbox` frame patched nothing — a new row.
 * Not every frame: an agent streaming tokens moves nothing here. Every tag
 * the node reads a notice from (`bisa_engine::notices::NOTICE_TAGS`) is one
 * of them, or a notice would wait for an unrelated reload to be seen.
 */
export const RELOAD_ON = Object.freeze([
  "gate_opened",
  "question_asked",
  "gate_decided",
  "run_started",
  "run_queued",
  "run_finished",
  "run_cancelled",
  "step_changed",
  "goal_closed",
  "workflow_proposed",
  "result_accepted",
  // A start event heard, refused, or turned on or off: a workflow's row or a
  // listening goal's moves with it.
  "listener_fired",
  "listener_failed",
  "listening_changed",
  "execution_ended",
  "guided",
  "committer_needed",
  "workstream_script_ran",
  "workstream_publish_failed",
  "workstream_changed",
  "project_created",
  "guard_decided",
  "people_changed",
  "invite_changed",
  "message_held",
  "workflow_changed",
  "workflow_archived",
]);

/** How long frames are gathered before one reload answers them all. */
export const RELOAD_DEBOUNCE_MS = 300;

/**
 * Whether a frame that is not the `inbox` stream's is worth reading the list
 * again: a conversation's snapshot (its thread was rewritten), or an engine
 * fact of {@link RELOAD_ON}. A frame this build cannot read moves nothing.
 * @param {{stream?: string, payload?: any} | null | undefined} frame
 */
export function wantsList(frame) {
  if (!frame || typeof frame !== "object") return false;
  if (frame.stream === "conversation") return frame.payload?.snapshot === true;
  if (frame.stream === "engine") return RELOAD_ON.includes(frame.payload?.payload?.type);
  return false;
}

/**
 * The rows after a person marked one read or unread, before the node says
 * so: the row changes where it sits, the others are the same objects. Read
 * clears what was new — the messages and the notices — and never what is
 * owed; unread only flips the mark, since what is new again is the node's to
 * count. The node's `inbox` frame for the marker follows and is the truth;
 * a key nobody holds changes nothing.
 * @param {readonly object[]} rows
 * @param {string} key
 * @param {boolean} read
 */
export function withReadMark(rows, key, read) {
  let changed = false;
  const next = rows.map((r) => {
    if (r.key !== key || r.read === read) return r;
    changed = true;
    return read ? { ...r, read: true, unread_count: 0, unread_notices: 0 } : { ...r, read: false };
  });
  return changed ? next : rows;
}

