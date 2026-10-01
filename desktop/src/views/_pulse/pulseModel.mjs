/**
 * The Pulse's rules (docs/guide/the-desktop.md §Pulse): the concepts a
 * reader narrows the feed to — the seven the core names, and *All* — with
 * their words and glyphs; which engine facts are in the feed at all, and
 * under which concept; the page a scrolled window asks for next; how a page
 * joins what is already shown, and how much of it is held; the day dividers
 * and the headings a run of rows on one thing shares; and the door a row
 * opens. Pure, so `node --test` pins every one; the view paints.
 */

import { t } from "../../i18n/l10n.mjs";
import { countValue } from "../../shell/viewValuesModel.mjs";

/** The local calendar day a divider falls on — the kit's `dayKey`, word for word, so a model needs no React file. */
export function dayKey(unixSeconds) {
  return new Date(unixSeconds * 1000).toDateString();
}

/** The concepts, in the order the tabs draw them — *All* first, then the core's seven. */
export const CONCEPTS = Object.freeze(["all", "workspace", "goals", "workflows", "projects", "channels", "agents", "node"]);

/** A page of the feed: enough to fill a screen twice, small enough to arrive at once. */
export const PAGE = 80;
/** How close to the end of what is loaded the window comes before the next page is asked for. */
export const PAGE_AHEAD = 20;
/** How long the head page waits after a frame before reading again — a burst is one read. */
export const NUDGE_MS = 300;

const WORDS = Object.freeze({
  all: { label: t("pulse-pulse-all"), icon: "pulse", empty: t("pulse-pulse-nothing-has-happened-yet"), hint: t("pulse-pulse-create-goal-post-channel-everything-shows") },
  workspace: { label: t("pulse-pulse-workspace"), icon: "members", empty: t("pulse-pulse-workspace-has-not-changed-shape-yet"), hint: t("pulse-pulse-goal-made-note-written-lands-here") },
  goals: { label: t("pulse-pulse-goals"), icon: "goal", empty: t("pulse-pulse-no-goal-has-moved-yet"), hint: t("pulse-pulse-notes-questions-decisions-progress-results-runs") },
  workflows: { label: t("pulse-pulse-workflows"), icon: "workflow", empty: t("pulse-pulse-no-workflow-has-been-designed-changed"), hint: t("pulse-pulse-workflow-agent-s-standing-proposals-edits") },
  projects: { label: t("pulse-pulse-projects"), icon: "project", empty: t("pulse-pulse-no-project-has-moved-yet"), hint: t("pulse-pulse-projects-made-edited-workstreams-opened-committed") },
  channels: { label: t("pulse-pulse-channels"), icon: "dm", empty: t("pulse-pulse-nobody-has-posted-yet"), hint: t("pulse-pulse-messages-channels-direct-messages-goal-threads") },
  agents: { label: t("pulse-pulse-agents"), icon: "agent", empty: t("pulse-pulse-no-agent-has-worked-yet"), hint: t("pulse-pulse-claims-replies-model-switches-guard-s") },
  node: { label: t("pulse-pulse-node"), icon: "node", empty: t("pulse-pulse-node-has-not-changed-yet"), hint: t("pulse-pulse-settings-git-setup-paused-resumed") },
});

/** The tab's word and glyph for a concept, and the empty state's sentence when it has nothing. */
export function conceptWords(concept) {
  return WORDS[concept] ?? WORDS.all;
}

/** The concept the URL names, or *All* for anything else. */
export function parseConcept(raw) {
  return CONCEPTS.includes(raw) ? raw : "all";
}

/**
 * The engine facts that are no activity — a per-frame or per-token signal
 * whose home is the roster or the watcher, never the feed. The engine's own
 * list (`bisa_engine::activity::concept_of`, its `None` arm), held equal by
 * the test, which reads the Rust.
 */
export const NOT_ACTIVITY = Object.freeze([
  "session",
  "session_state",
  "session_gone",
  "agent_thinking",
  "agent_streamed",
  "file_changed",
  "browser_request",
  "drawing_request",
  "mobile_development_changed",
  "mcp_probed",
  "changes_moved",
  "ask_opened",
  "ask_settled",
  "lsp",
]);

/** The concept each engine fact is filed under — the same function's other arms. */
export const FACT_CONCEPT = Object.freeze({
  goal_created: "workspace",
  note_changed: "workspace",
  drawing_changed: "workspace",
  gate_opened: "goals",
  question_asked: "goals",
  gate_decided: "goals",
  result_accepted: "goals",
  run_started: "goals",
  run_queued: "goals",
  run_finished: "goals",
  run_cancelled: "goals",
  step_changed: "goals",
  document_added: "goals",
  goal_archived: "goals",
  goal_deleted: "goals",
  goal_closed: "goals",
  boundary_fired: "goals",
  guided: "workflows",
  workflow_proposed: "workflows",
  workflow_changed: "workflows",
  workflow_deleted: "workflows",
  workflow_archived: "workflows",
  project_created: "projects",
  project_changed: "projects",
  committer_needed: "projects",
  committer_set: "projects",
  workstream_opened: "projects",
  workstream_edited: "projects",
  workstream_changed: "projects",
  workstream_committed: "projects",
  workstream_script_ran: "projects",
  workstream_publish_failed: "projects",
  server_changed: "projects",
  project_archived: "projects",
  project_deleted: "projects",
  changes_settled: "projects",
  attachment_changed: "projects",
  scheduled: "agents",
  execution_ended: "agents",
  agent_replied: "agents",
  model_switched: "agents",
  guard_decided: "agents",
  judged: "agents",
  content_screened: "agents",
  redacted: "agents",
  conversation_created: "channels",
  conversation_changed: "channels",
  people_changed: "channels",
  invite_changed: "channels",
  message_held: "channels",
  message_released: "channels",
  hosted_changed: "channels",
  settings_changed: "node",
  git_setup_changed: "node",
  connectors_changed: "node",
  addons_changed: "node",
  relays_changed: "node",
  paused: "node",
  resumed: "node",
});

/** The facts a listener's host decides the concept of: a library workflow's are the workflows', a goal's the goals'. */
export const HOSTED_FACTS = Object.freeze(["signal_received", "listener_fired", "listener_failed", "listening_changed"]);

/** The concept of a host in its wire form — `goal:<goal>` or `workspace:<workflow>`, with or without `/<step>` after it. */
function hostConcept(host) {
  return typeof host === "string" && host.startsWith("goal:") ? "goals" : "workflows";
}

/**
 * Where an engine fact is filed in the feed: its concept; `false` for a fact
 * that is no activity — it nudges nobody; `null` for one this build cannot
 * place — it nudges every tab, since a read too many costs less than a stale
 * screen.
 * @param {{type?: unknown, listener?: unknown, host?: unknown} | null | undefined} payload an `EnginePayload`
 * @returns {string | false | null}
 */
export function conceptOfFact(payload) {
  const type = payload?.type;
  if (typeof type !== "string") return null;
  if (NOT_ACTIVITY.includes(type)) return false;
  if (HOSTED_FACTS.includes(type)) return hostConcept(type === "listening_changed" ? payload.host : payload.listener);
  return FACT_CONCEPT[type] ?? null;
}

/**
 * Whether a concept's feed wants a frame's fact: *All* wants every one that
 * is in the feed; a concept wants the facts placed under it. A frame is only
 * a nudge to read the head page again, so a false positive costs one read
 * and a false negative a stale screen — the concept's own facts are what
 * matter. A fact that is no activity (`false`) nudges nobody: an agent
 * streaming its words would otherwise have every open Pulse read its head
 * three times a second for rows that never come.
 * @param {string} concept the tab
 * @param {string | false | null | undefined} of the frame's concept (`conceptOfFact`); null nudges every tab
 */
export function wantsNudge(concept, of) {
  if (of === false) return false;
  return concept === "all" || of == null || of === concept;
}

/**
 * Whether the window the list draws has come close enough to the end of
 * what is loaded to ask for the next page: within `PAGE_AHEAD` rows of the
 * last, with a page still to ask for and none in flight.
 */
export function wantsMore({ last, total, next, inFlight }) {
  return next !== null && !inFlight && total > 0 && last >= total - PAGE_AHEAD;
}

/**
 * The rows after a page joined them: the head page replaces what it
 * overlaps by `seq` and keeps the rest; a further page appends what is new.
 * Newest first throughout, every `seq` once.
 * @param {readonly {seq: number, at: number}[]} shown
 * @param {readonly {seq: number, at: number}[]} page
 * @param {"head" | "older"} where
 */
export function mergePage(shown, page, where) {
  const seen = new Set(page.map((r) => r.seq));
  const kept = shown.filter((r) => !seen.has(r.seq));
  const merged = where === "head" ? [...page, ...kept] : [...kept, ...page];
  return merged.sort((a, b) => b.at - a.at || b.seq - a.seq);
}

/** The cursor a page answers with, as the next request's `before`. */
export function cursorOf(page) {
  return page.next ?? null;
}

/**
 * Dividers between calendar days, and a heading above each run of rows on
 * the same thing. The heading is why the node ships `title` as a field
 * instead of writing it into the sentence: forty consecutive rows that each
 * began "ship the thing:" spent forty lines saying one thing once.
 * @param {readonly {key: string, at: number, title?: string}[]} items newest first
 */
export function withDividers(items) {
  const out = [];
  let day = null;
  let heading;
  for (const item of items) {
    const d = dayKey(item.at);
    if (d !== day) {
      out.push({ slot: "day", key: `d:${d}`, at: item.at });
      day = d;
      heading = undefined;
    }
    const showHeading = item.title && item.title !== heading;
    out.push({ slot: "row", item, heading: showHeading ? item.title : undefined });
    heading = item.title;
  }
  return out;
}

/**
 * The door a row opens, by what it is about: a goal, a channel or a direct
 * message, a conversation, a workstream in the IDE, a project as its primary
 * (whose id is the project's), a workflow, the Agents screen. This node and
 * the workspace itself have no screen of their own: no door.
 * @param {{kind: string, id: string} | null | undefined} source
 * @param {(id: string) => "channel" | "dm" | null} channelKind which kind a channel id is
 */
export function linkOf(source, channelKind) {
  if (!source || !source.id) return null;
  switch (source.kind) {
    case "goal":
      return { name: "goal", id: source.id };
    case "channel": {
      const kind = channelKind(source.id);
      return kind ? { name: kind, id: source.id } : null;
    }
    case "workstream":
    case "project":
      return { name: "workbench", scope: "workstream", id: source.id };
    case "conversation":
      return { name: "conversation", id: source.id };
    case "workflow":
      return { name: "workflow", id: source.id };
    case "agent":
      return { name: "agents" };
    default:
      return null;
  }
}

/**
 * A row as a screen holds it: the line's words (`activity.pulseLine`) beside
 * the feed's facts a door and a merge need — `seq`, `at`, `concept`,
 * `source`. One builder for the Pulse's rows and an Inbox row's notices,
 * which are the same rows.
 * @param {{seq: number, at: number, concept: string, source: unknown}} row
 * @param {{key: string, text: string, tone: string, author?: unknown, title?: unknown, icon?: unknown, detail?: unknown}} line
 */
export function itemOf(row, line) {
  return {
    key: line.key,
    seq: row.seq,
    at: row.at,
    text: line.text,
    tone: line.tone,
    concept: row.concept,
    source: row.source,
    author: line.author,
    title: line.title,
    icon: line.icon,
    detail: line.detail,
  };
}

/**
 * What the screen holds after a fresh head page: the rows, and the cursor
 * the next older page is asked with.
 *
 * - Nothing shown yet: the page is the feed, its cursor the tail.
 * - The page reaches what is shown — a `seq` in both — or is short, which is
 *   the whole feed: it joins the head (`mergePage`), and the tail stays where
 *   the last older page left it, `null` included — a feed read to its end is
 *   not read again from its head.
 * - A full page that reaches nothing shown: more landed since the last read
 *   than a page holds. Joining it would leave a hole nothing ever fills, so
 *   the feed starts over from this page and older rows are paged in again.
 * @param {{shown: readonly {seq: number, at: number}[], page: readonly {seq: number, at: number}[], next: unknown, tail: unknown, size?: number}} facts
 */
export function joinHead({ shown, page, next, tail, size = PAGE }) {
  if (shown.length === 0) return { items: [...page], next: next ?? null, restarted: false };
  const held = new Set(shown.map((r) => r.seq));
  const reaches = page.length < size || page.some((r) => held.has(r.seq));
  if (reaches) return { items: mergePage(shown, page, "head"), next: tail ?? null, restarted: false };
  return { items: [...page], next: next ?? null, restarted: true };
}

/** How many pages a feed is read back to after a restart: further than that, a person scrolls again. */
export const MAX_PAGES = 10;

/** How many rows a feed holds, at most, below what the window draws: ten pages. */
export const HELD_MAX = PAGE * MAX_PAGES;

/**
 * The feed as it is held after its head grew. A Pulse left open grows at
 * its head for as long as the workspace moves, so what is held is bounded:
 * past `HELD_MAX` rows — or a page below the last row the window draws, when
 * the reader is further down than that — the tail is let go, and the cursor
 * moves up to the last row kept, so scrolling down pages it in again. Never
 * a row under the reader's eyes, and a feed within the bound is the same
 * object.
 * @template {{seq: number, at: number}} T
 * @param {{items: T[], next: {at: number, seq: number} | null}} feed
 * @param {number} [shownLast] the last row the window draws
 * @param {number} [max]
 * @returns {{items: T[], next: {at: number, seq: number} | null}}
 */
export function heldFeed(feed, shownLast = 0, max = HELD_MAX) {
  const below = Number.isInteger(shownLast) && shownLast > 0 ? shownLast + PAGE : 0;
  const keep = Math.max(max, below);
  if (feed.items.length <= keep) return feed;
  const items = feed.items.slice(0, keep);
  const last = items[items.length - 1];
  return { items, next: { at: last.at, seq: last.seq } };
}

/**
 * How far a feed is read, as it is kept for a restart: its rows, never more
 * than `MAX_PAGES` pages of them — and nothing for a feed no deeper than its
 * head page, which every visit reads anyway.
 * @param {number} loaded the rows held
 * @param {number} [size] a page
 * @returns {number}
 */
export function depthOf(loaded, size = PAGE) {
  if (!Number.isInteger(loaded) || loaded <= size) return 0;
  return Math.min(loaded, size * MAX_PAGES);
}

/**
 * A depth read back from the memory: a whole number of rows, held to
 * `MAX_PAGES` pages — or `undefined` for what is no depth.
 * @param {unknown} raw
 * @param {number} [size] a page
 * @returns {number | undefined}
 */
export function parseDepth(raw, size = PAGE) {
  const rows = countValue(raw);
  return rows === undefined ? undefined : Math.min(rows, size * MAX_PAGES);
}

/**
 * Whether a feed being read back asks for its next page: it holds fewer
 * rows than it was left with, a page is still to ask for, and none is in
 * flight. A feed read to its end, or as deep as it was, asks for nothing.
 * @param {{loaded: number, wanted: number, next: unknown, inFlight: boolean}} state
 */
export function wantsDepth({ loaded, wanted, next, inFlight }) {
  return next !== null && !inFlight && loaded > 0 && loaded < wanted;
}

