/**
 * The Pulse's rules (docs/guide/the-desktop.md §Pulse): the concepts a
 * reader narrows the feed to — the seven the core names, and *All* — with
 * their words and glyphs; what a frame of the feed asks of the tab that is
 * open; the page a scrolled window asks for next; how a page
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
 * Whether a tab wants a row of the feed: *All* wants every row; a concept
 * wants the rows filed under it. A frame that names no concept — the index
 * was rebuilt, or this window fell behind the stream — is everybody's.
 * @param {string} concept the tab
 * @param {string | null | undefined} of the concept the row is filed under (`PulseFrame.concept`)
 */
export function wantsNudge(concept, of) {
  return concept === "all" || of == null || of === concept;
}

/**
 * What a frame of the feed (`{stream: "pulse"}`) asks of the tab that is
 * open: nothing, to read its head page again, or to start over.
 *
 * The node says every row as it lands — once it can be read, with the
 * concept it is filed under — so the screen places a frame by the frame's
 * own word, never by guessing from an engine payload.
 *
 * - A row of another concept: `"none"`.
 * - A row the head page will show: `"head"`.
 * - `{concept: null}` — the index was rebuilt and every row may wear another
 *   `seq`, or this window fell behind: `"restart"`, since what is held can
 *   no longer be joined by `seq`.
 * - A row older than the head page reaches — a fact another node wrote, and
 *   synced late, lands where it happened and not at the head: `"restart"`,
 *   or it would never be read into a feed already loaded past it.
 * @param {string} concept the tab
 * @param {{concept?: string | null, at?: number} | null | undefined} frame
 * @param {readonly {at: number}[]} shown what the tab holds, newest first
 * @param {number} [size] a page
 * @returns {"none" | "head" | "restart"}
 */
export function nudgeOf(concept, frame, shown, size = PAGE) {
  if (!frame || frame.concept == null) return "restart";
  if (!wantsNudge(concept, frame.concept)) return "none";
  const edge = shown.length >= size ? shown[size - 1] : null;
  return edge && typeof frame.at === "number" && frame.at < edge.at ? "restart" : "head";
}

/**
 * Whether the window the list draws has come close enough to the end of
 * what is loaded to ask for the next page: within `PAGE_AHEAD` rows of the
 * last, with a page still to ask for and none in flight. A feed that holds
 * no row and still has a page to ask for asks at once: the page it read was
 * all rows this build cannot draw, and the feed goes on behind it.
 */
export function wantsMore({ last, total, next, inFlight }) {
  return next !== null && !inFlight && (total === 0 || last >= total - PAGE_AHEAD);
}

/** Whether two cursors name the same place — both none, or the same second and row. */
export function sameCursor(a, b) {
  if (!a || !b) return !a && !b;
  return a.at === b.at && a.seq === b.seq;
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
 * A row that is the echo of another (`itemOf`) is passed over.
 * @param {readonly {key: string, at: number, title?: string, echo?: boolean}[]} items newest first
 */
export function withDividers(items) {
  const out = [];
  let day = null;
  let heading;
  for (const item of items) {
    // An engine's fact that only repeats a journal fact is held, for the
    // joins, and never drawn: the journal's row says the happening.
    if (item.echo) continue;
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
 * (whose id is the project's), a workflow, an agent. This node and the
 * workspace itself have no screen of their own: no door — and neither has a
 * thing that is gone (`gone`): a row about a goal since deleted says what
 * happened and opens nothing, where it used to open a page that could only
 * say *not found*.
 * @param {{kind: string, id: string} | null | undefined} source
 * @param {(id: string) => "channel" | "dm" | null} channelKind which kind a channel id is
 * @param {boolean} [gone] the node no longer holds what the row is about
 */
export function linkOf(source, channelKind, gone = false) {
  if (!source || !source.id || gone) return null;
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
      return { name: "agent", id: source.id };
    default:
      return null;
  }
}

/**
 * A row as a screen holds it: the line's words (`activity.pulseLine`) beside
 * the feed's facts a door and a merge need — `seq`, `at`, `concept`,
 * `source` — and two things the node says of the row: whether it only
 * repeats another (`echo`, from `echo_of`: the engine's fact of a happening
 * its journal already tells), and whether what it is about is gone (`gone`).
 * One builder for the Pulse's rows and an Inbox row's notices, which are the
 * same rows.
 * @param {{seq: number, at: number, concept: string, source: unknown, title?: unknown, echo_of?: unknown}} row
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
    echo: row.echo_of != null,
    gone: isGone(row),
  };
}

/** The kinds of thing that always have a name: a row about one with no title is about one the node no longer holds. */
const ALWAYS_TITLED = Object.freeze(["goal", "workflow", "project"]);

/**
 * Whether what a row is about is gone. The node titles a row by reading the
 * record it is about; a goal, a workflow and a project always have a name,
 * so one of them with no title is one the node could not read — deleted.
 * A conversation nobody titled and a checkout nobody named have none
 * either, and are still there.
 */
function isGone(row) {
  return ALWAYS_TITLED.includes(row.source?.kind) && !row.title;
}

/**
 * What the screen holds after a fresh head page: the rows, and the cursor
 * the next older page is asked with.
 *
 * The node's own word says whether the page is the whole feed: `next` is
 * absent on the last page and present otherwise — the page's length says
 * nothing, since the node leaves out a row this build cannot type after it
 * has counted, and a page of fewer rows than asked for may still have a
 * feed behind it.
 *
 * - Nothing shown yet: the page is the feed, its cursor the tail.
 * - The page is the whole feed (no `next`): it *is* what is held, and the
 *   feed is at its end.
 * - The page reaches what is shown — a `seq` in both: it joins the head,
 *   and is the truth for the span it covers — a row held inside that span
 *   that the page no longer carries was taken back (a retracted message)
 *   and goes. The tail stays where the last older page left it, `null`
 *   included: a feed read to its end is not read again from its head.
 * - A page that reaches nothing shown and is not the whole feed: more
 *   landed since the last read than a page holds. Joining it would leave a
 *   hole nothing ever fills, so the feed starts over from this page and
 *   older rows are paged in again.
 * @param {{shown: readonly {seq: number, at: number}[], page: readonly {seq: number, at: number}[], next: unknown, tail: unknown}} facts
 */
export function joinHead({ shown, page, next, tail }) {
  if (shown.length === 0 || next == null) return { items: [...page], next: next ?? null, restarted: false };
  const held = new Set(shown.map((r) => r.seq));
  if (!page.some((r) => held.has(r.seq))) return { items: [...page], next, restarted: true };
  // Below the page's last row, what is held stands; inside its span, the page does.
  const last = page[page.length - 1];
  const below = shown.filter((r) => r.at < last.at || (r.at === last.at && r.seq < last.seq));
  return { items: mergePage(below, page, "head"), next: tail ?? null, restarted: false };
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

