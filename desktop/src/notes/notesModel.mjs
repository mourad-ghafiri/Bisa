/**
 * The notes overlay's facts, where they can be tested without a DOM.
 *
 * Each is here because getting it wrong produces a wrong *fact* rather than
 * a wrong pixel: text spliced at the wrong offset, a tab that lists another
 * kind's notes, a new note filed somewhere you were not, a search that drops
 * a note that says the words.
 *
 * Plain `.mjs` with a `.d.mts` beside it — `node --test` imports it with no
 * build step, and TypeScript reads the declarations.
 */

import { t as tr } from "../i18n/l10n.mjs";

/**
 * What the toolbar buttons do to a selection.
 *
 * `wrap` puts the same marker on both sides of the selection; `line` puts a
 * prefix at the start of every selected line. They are genuinely different
 * operations and a single "insert this string" would get lists wrong.
 */
export const MARKS = {
  bold: { kind: "wrap", mark: "**", empty: tr("notes-notes-bold-text") },
  italic: { kind: "wrap", mark: "_", empty: tr("notes-notes-italic-text") },
  code: { kind: "wrap", mark: "`", empty: "code" },
  link: { kind: "link", empty: "text" },
  heading: { kind: "line", prefix: "## " },
  bullet: { kind: "line", prefix: "- " },
  quote: { kind: "line", prefix: "> " },
};

/** The start of the line `at` sits on. */
function lineStart(text, at) {
  const nl = text.lastIndexOf("\n", Math.max(0, at - 1));
  return nl === -1 ? 0 : nl + 1;
}

/** The end of the line `at` sits on, exclusive of the newline. */
function lineEnd(text, at) {
  const nl = text.indexOf("\n", at);
  return nl === -1 ? text.length : nl;
}

/**
 * Apply a mark to `[from, to)` and say where the caret goes.
 *
 * Returns `{ text, from, to }` — a *range*, not a point, because the useful
 * result of pressing Bold with nothing selected is placeholder text that is
 * already selected so the next keystroke replaces it. Collapsing that to a
 * caret is the difference between a working toolbar and one that makes you
 * reach for the mouse after every click.
 *
 * Toggling is deliberately absent: pressing Bold on already-bold text wraps it
 * again rather than unwrapping. Detecting "already bold" means parsing
 * markdown, and a toolbar that is wrong about what it is looking at removes
 * markers somebody typed on purpose.
 */
export function wrapSelection(text, from, to, kind) {
  const spec = MARKS[kind];
  if (!spec) return { text, from, to };
  // A backwards drag is a real selection; the browser reports it either way
  // round and every offset below assumes it is ordered.
  if (from > to) [from, to] = [to, from];
  from = Math.max(0, Math.min(from, text.length));
  to = Math.max(from, Math.min(to, text.length));

  if (spec.kind === "line") {
    const start = lineStart(text, from);
    const end = lineEnd(text, to);
    const block = text.slice(start, end);
    const marked = block
      .split("\n")
      .map((line) => (line.startsWith(spec.prefix) ? line : spec.prefix + line))
      .join("\n");
    return {
      text: text.slice(0, start) + marked + text.slice(end),
      from: start,
      to: start + marked.length,
    };
  }

  const selected = text.slice(from, to) || spec.empty;
  if (spec.kind === "link") {
    const body = `[${selected}](url)`;
    const out = text.slice(0, from) + body + text.slice(to);
    // Select `url`, because that is the part you still have to supply.
    const at = from + selected.length + 3;
    return { text: out, from: at, to: at + 3 };
  }

  const body = spec.mark + selected + spec.mark;
  return {
    text: text.slice(0, from) + body + text.slice(to),
    from: from + spec.mark.length,
    to: from + spec.mark.length + selected.length,
  };
}


/* ---------------------------------------------------------------------------
   Tabs, targets, and the words for a scope

   The panel stands on its own: which notes it lists is its **tab**, kept in
   the store and changed only by a hand on the strip, never by the route. The
   route has exactly one say — where a *new* note is filed when you press New
   on a tab that admits more than one place.
--------------------------------------------------------------------------- */

/** The tabs, in the order the strip draws them. `all` is first because it is the default. */
export const NOTE_TABS = ["all", "workspace", "projects", "goals", "workflows", "channels", "node"];

/** What the strip calls each tab. */
export const NOTE_TAB_LABEL = {
  all: tr("notes-notes-all"),
  workspace: tr("notes-notes-workspace"),
  projects: tr("notes-notes-projects"),
  goals: tr("notes-notes-goals"),
  workflows: tr("notes-notes-workflows"),
  channels: tr("notes-notes-channels"),
  node: tr("notes-notes-node"),
};

/**
 * The scope kind a tab lists, as the wire spells it — `null` for *All*,
 * which is no kind at all. A plural tab is a kind, not a scope: *Projects*
 * is every project's notes.
 */
const TAB_KIND = {
  all: null,
  workspace: "workspace",
  projects: "project",
  goals: "goal",
  workflows: "workflow",
  channels: "channel",
  node: "node",
};

/**
 * Which tab a stored preference selects. Anything unknown is *All*, for the
 * reason `noteView` gives: the value outlives the name, and the first tab is
 * always a reasonable place to land.
 */
export function noteTab(raw) {
  return NOTE_TABS.includes(raw ?? "") ? raw : NOTE_TABS[0];
}

/** The scope kind a tab lists, or `null` for every kind. */
export function tabKind(tab) {
  return TAB_KIND[noteTab(tab)];
}

/**
 * The query string a tab's listing sends to `GET /notes`: nothing for *All*,
 * the kind alone for a plural tab, the kind for a standalone one. A kind
 * without an id is the node's word for *every note of that kind*.
 */
export function tabQuery(tab) {
  const kind = tabKind(tab);
  return kind ? `?${new URLSearchParams({ scope: kind }).toString()}` : "";
}

/** Whether a note of `kind` belongs on `tab` — which frames redraw the list, which rows it shows. */
export function tabAdmits(tab, kind) {
  const wanted = tabKind(tab);
  return wanted === null || wanted === kind;
}

/**
 * The frames of a place leaving with its notes and drawings: a project's, a
 * goal's and a workflow's deletion removes every note and drawing under it in
 * the store, and no `note_changed` or `drawing_changed` says so. Archiving
 * and closing remove nothing. A channel's deletion emits nothing on either
 * stream — the one place a count catches up only on the next frame or when
 * the bus comes back.
 */
export const OWNER_GONE_FRAMES = Object.freeze(["project_deleted", "goal_deleted", "workflow_deleted"]);

/**
 * Whether a fact of this type is a place leaving with its notes and drawings.
 * @param {unknown} type an engine frame's `type`
 */
export function ownerGone(type) {
  return typeof type === "string" && OWNER_GONE_FRAMES.includes(type);
}

/**
 * Whether a fact of this type moves how many notes there are — what the
 * dock's count is read again on, whether or not the panel is open. A
 * `note_changed` is a note created, deleted or appended to by an agent (an
 * editor's own PATCH is silent by design, and moves no count); the rest are
 * a place gone with its notes.
 * @param {unknown} type an engine frame's `type`
 */
export function movesNoteCount(type) {
  return type === "note_changed" || ownerGone(type);
}

/**
 * The scope the route stands on — the one place a *new* note goes without
 * asking.
 *
 * A goal's screen is its goal; a workflow's its workflow; a channel's its
 * channel; the workbench is a project (a workstream is a *place inside* a
 * project, and a note filed under one would vanish when it closed, so a
 * workstream root files under its project — the primary's id is the
 * project's, any other is resolved by the caller, an unknown one falls to
 * the workspace). Everything else is the workspace.
 */
export function routeTarget(route, projectOf = () => null) {
  if (!route || typeof route !== "object") return { scope: "workspace" };
  if (route.name === "goal" && route.id) return { scope: "goal", id: route.id };
  if (route.name === "workflow" && route.id) return { scope: "workflow", id: route.id };
  if (route.name === "channel" && route.id) return { scope: "channel", id: route.id };
  if (route.name === "workbench" && route.scope === "workstream" && route.id) {
    const project = projectOf(route.id) ?? route.id;
    return { scope: "project", id: project };
  }
  return { scope: "workspace" };
}

/**
 * The scopes a new note may take under a tab, each with its words and
 * whether it is where you are.
 *
 * `names` is the workspace's directory — `{projects, goals, workflows,
 * channels}`, each a list of `{id, name}`. A standalone tab has one target;
 * a plural tab has every record of its kind, the route's own first; *All*
 * has the workspace, the node, and the route's own when that is a record.
 * One target means a plain *New* button; several mean a menu.
 */
export function noteTargets(tab, route, names, projectOf) {
  const here = routeTarget(route, projectOf);
  const record = (kind, list) =>
    (list ?? []).map((r) => ({
      scope: { scope: kind, id: r.id },
      label: r.name,
      here: sameScope(here, { scope: kind, id: r.id }),
    }));
  const standalone = (kind) => ({
    scope: { scope: kind },
    label: NOTE_TAB_LABEL[kind],
    here: sameScope(here, { scope: kind }),
  });
  const byKind = {
    workspace: () => [standalone("workspace")],
    node: () => [standalone("node")],
    projects: () => record("project", names?.projects),
    goals: () => record("goal", names?.goals),
    workflows: () => record("workflow", names?.workflows),
    channels: () => record("channel", names?.channels),
  };
  const t = noteTab(tab);
  if (t !== "all") {
    const out = byKind[t]();
    // The place you are standing goes first, so the default is the obvious one.
    return out.sort((a, b) => Number(b.here) - Number(a.here));
  }
  const out = [standalone("workspace"), standalone("node")];
  if (here.id) {
    const list = { project: names?.projects, goal: names?.goals, workflow: names?.workflows, channel: names?.channels }[here.scope];
    const found = (list ?? []).find((r) => r.id === here.id);
    if (found) out.unshift({ scope: here, label: found.name, here: true });
  }
  return out;
}

/**
 * The words a row wears for its scope — the record's name, or the
 * standalone kind's — so a list across scopes says what each note is about.
 * A record the directory does not know keeps its kind as its word rather
 * than an id nobody can read.
 */
export function scopeWords(scope, names) {
  if (!scope) return tr("notes-notes-workspace");
  if (scope.scope === "workspace") return tr("notes-notes-workspace");
  if (scope.scope === "node") return tr("notes-notes-node");
  const list = { project: names?.projects, goal: names?.goals, workflow: names?.workflows, channel: names?.channels }[scope.scope];
  const found = (list ?? []).find((r) => r.id === scope.id);
  if (found?.name) return found.name;
  return { project: tr("notes-notes-project"), goal: tr("notes-notes-goal"), workflow: tr("notes-notes-workflow"), channel: tr("notes-notes-channel") }[scope.scope] ?? tr("notes-notes-workspace");
}

/** A row's scope, in the shape the model reads, from the wire's flat pair. */
export function scopeOfRow(row) {
  return row?.scope_id ? { scope: row.scope, id: row.scope_id } : { scope: row?.scope ?? "workspace" };
}

/**
 * The notes a search keeps: a case-insensitive match on the title or the
 * body, order kept. Every word of the query has to appear somewhere, so
 * *cache ttl* finds a note that says both without saying them together.
 * An empty query keeps everything.
 */
export function filterNotes(notes, query) {
  const words = String(query ?? "")
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return notes;
  return notes.filter((n) => {
    const hay = `${n.title ?? ""}\n${n.body ?? ""}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** The match the bar stands on, or `null` when the index names none. */
export function matchAt(matches, index) {
  if (!Array.isArray(matches) || !Number.isInteger(index) || index < 0) return null;
  return matches[index] ?? null;
}

/**
 * What to do when the server's copy of a note changed while the editor is open.
 *
 * This is the rule the first version got wrong, and getting it wrong is what
 * made the overlay unusable: it re-seeded the textarea from every incoming
 * copy, including the echo of the editor's own save, so text typed since that
 * save was replaced by an older version of itself — and the difference was
 * then saved, producing another echo.
 *
 * The rule has one job: **your text is never replaced by anything.**
 *
 * - The incoming copy is what you already have → `same`: nothing happened.
 *   Returning `body: local` unchanged matters, because a caller that sets
 *   state unconditionally here re-renders on every frame.
 * - The incoming copy **extends** what was last saved → `appended`: an agent
 *   added a block at the end (`note_append`), and the block lands under
 *   whatever you have typed since — live, with no banner — since nothing of
 *   yours is in its way. With nothing typed, that is simply the incoming copy.
 * - Something else arrived — an agent rewrote the note (`note_write`) — over
 *   a buffer with nothing typed since the last save → `taken`: the rewrite
 *   shows, and the caller keeps what was confirmed so the person can have
 *   their version back in one click.
 * - A rewrite over text you have typed and not saved → `conflict`: keep what
 *   you have and say so. Merging for you would be guessing; the caller offers
 *   *Take theirs* and *Keep mine*.
 * @param {string} local what the buffer holds
 * @param {string} confirmed what the server last confirmed for this editor
 * @param {string} incoming what the server holds now
 * @returns {{body: string, outcome: "same" | "appended" | "taken" | "conflict"}}
 */
export function adoptIncoming(local, confirmed, incoming) {
  if (incoming === local) return { body: local, outcome: "same" };
  if (incoming.startsWith(confirmed)) return { body: local + incoming.slice(confirmed.length), outcome: "appended" };
  if (local === confirmed) return { body: incoming, outcome: "taken" };
  return { body: local, outcome: "conflict" };
}

/**
 * A parked draft as the storage holds it: the text, and the hash of the note
 * it was typed against — so a draft restored later is known to be typed over
 * a note that has moved since, or not. An older build parked the bare text;
 * it is read as a draft of unknown base.
 * @param {string} raw
 * @returns {{body: string, base_hash: string | null}}
 */
export function parseDraft(raw) {
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && typeof parsed.body === "string") {
      return { body: parsed.body, base_hash: typeof parsed.base_hash === "string" ? parsed.base_hash : null };
    }
  } catch {
    // Not JSON: an older build's bare text.
  }
  return { body: raw, base_hash: null };
}

/**
 * What the editor opens with when a draft was parked for the note: no draft,
 * or one that says what the note says, is the note; a draft typed against
 * **this** text is restored and may save as it is typed into; a draft typed
 * against another text — or against nobody knows what — is restored as a
 * conflict, for the person to decide, and never autosaved: a draft from a
 * window that closed must not erase what an agent wrote since.
 * @param {{body: string, base_hash: string | null} | null | undefined} draft
 * @param {{body: string, hash: string}} note
 * @returns {{body: string, conflict: boolean, restored: boolean}}
 */
export function restoredDraft(draft, note) {
  if (!draft || draft.body === note.body) return { body: note.body, conflict: false, restored: false };
  if (draft.base_hash === note.hash) return { body: draft.body, conflict: false, restored: true };
  return { body: draft.body, conflict: true, restored: true };
}

/**
 * The newer of two rows of one note: the one the editor's save answered, and
 * the one a list read brought — the saved row stands unless the read's is
 * strictly newer. A read asked for before a save and answered after it must
 * not take the row back; a tie in a seconds clock goes to the save, and the
 * next frame's read corrects it.
 * @template {{updated_at: number}} R
 * @param {R} read @param {R} saved
 * @returns {R}
 */
export function latestNote(read, saved) {
  return read.updated_at > saved.updated_at ? read : saved;
}

/**
 * A list read as it lands beside the saves that landed while it was out: the
 * rows those saves answered stand where the read's are older
 * (`latestNote`). The same array back when nothing was saved meanwhile.
 * @template {{id: string, updated_at: number}} R
 * @param {readonly R[]} rows what the read answered
 * @param {ReadonlyMap<string, R>} savedSince the rows saves answered since the read began, by id
 * @returns {readonly R[]}
 */
export function landedNotes(rows, savedSince) {
  if (savedSince.size === 0) return rows;
  return rows.map((r) => {
    const saved = savedSince.get(r.id);
    return saved ? latestNote(r, saved) : r;
  });
}

/**
 * Where a note's unsent text is parked, following `Composer`'s convention.
 *
 * Colons, like `bisa:draft:<scope>` — this is the same kind of thing and
 * the odd one out in this app either way, so it matches its sibling rather
 * than the dot-separated keys that store window preferences.
 */
export function draftKey(id) {
  return `bisa:draft:note:${id}`;
}

/** Two scopes name the same thing. */
export function sameScope(a, b) {
  return Boolean(a) && Boolean(b) && a.scope === b.scope && (a.id ?? null) === (b.id ?? null);
}

/** The query string for a scope, as `GET /notes` takes it. */
export function scopeQuery(scope) {
  const params = new URLSearchParams({ scope: scope.scope });
  if (scope.id) params.set("id", scope.id);
  return `?${params.toString()}`;
}

/* ---------------------------------------------------------------------------
   Preferences

   The vocabulary and the bounds live here rather than in the store, for the
   reason every other model in this app gives: `node --test` imports this file
   with no build step, and what a stored preference is *allowed to be* is a
   fact worth a test. The store reads `localStorage`, which a person can edit
   by hand and an older build can have written — so these clamp rather than
   validate. A note editor that refuses to draw because a preference will not
   parse is a worse outcome than one at the default.
--------------------------------------------------------------------------- */

/** The three ways to look at a note, in the order the segmented control draws. */
export const NOTE_VIEWS = ["write", "split", "read"];

/**
 * Which view a stored preference selects.
 *
 * Anything unknown is `write`, matching how `settingsTab` treats a `?tab=`
 * nobody recognises: the value outlives the name, and the first option is
 * always a reasonable place to land.
 */
export function noteView(raw) {
  return NOTE_VIEWS.includes(raw ?? "") ? raw : NOTE_VIEWS[0];
}

/**
 * How long the editor waits, after you stop typing, before it saves.
 *
 * **The floor is the interesting number.** Below about 300ms the timer starts
 * firing inside ordinary typing rhythm — the pause between two words is enough
 * — and every one of those is a PATCH, a `NoteChanged` on the bus and a
 * re-render. That is precisely the request storm that made this feature
 * unusable before the save effect was rewritten, and a preference is not
 * allowed to reintroduce it. The ceiling is where *Saved* stops being a useful
 * thing to have read.
 */
export const SAVE_MIN_MS = 300;
export const SAVE_MAX_MS = 3000;
export const SAVE_STEP_MS = 100;
export const SAVE_DEFAULT_MS = 700;

/**
 * Numeric input from storage or a control, or `NaN` when there is no number in
 * it at all.
 *
 * `Number()` alone is not enough and the difference is not academic:
 * `Number(null)`, `Number("")` and `Number([])` are all **0**, so an absent
 * preference would clamp to the *minimum* rather than fall back to the
 * default — the smallest legal value, silently, for anyone whose storage was
 * empty. Only a real number or a string with something in it counts.
 */
function numeric(raw) {
  if (typeof raw === "number") return raw;
  if (typeof raw === "string" && raw.trim() !== "") return Number(raw);
  return NaN;
}

/**
 * The one word the editor's footer says, in order of weight: a conflict, an
 * error, saving, unsaved, saved — and before them all, that the note is gone.
 * @param {{gone?: string | null, conflict?: string | null, error?: string | null, saving?: boolean, dirty?: boolean}} s
 */
export function noteStatusWords(s) {
  if (s.gone) return s.gone;
  if (s.conflict) return s.conflict;
  if (s.error) return s.error;
  if (s.saving) return tr("notes-notes-saving");
  if (s.dirty) return tr("notes-notes-unsaved");
  return tr("notes-notes-saved");
}

/**
 * What the node's refusal of a note's read or write means for the note on
 * screen, by its **status** — never its words, which are the reader's
 * language: a `409` is a save that lost to another writer (*conflict*); a
 * `404` is a note that is no longer there — deleted under the editor, by an
 * agent or another window (*gone*: nothing more can be saved to it, and a
 * delete of it has already happened); anything else *failed* and may be
 * tried again.
 * @param {number | null | undefined} status
 * @returns {"conflict" | "gone" | "failed"}
 */
export function noteRefusal(status) {
  if (status === 409) return "conflict";
  if (status === 404) return "gone";
  return "failed";
}

/** What the editor says over a note that is gone: the node's own sentence, and that the text typed stays to be copied. @param {string} why */
export function noteGoneWords(why) {
  return tr("notes-note-editor-note-gone", { why });
}

/**
 * What a keystroke does to the parked draft: text equal to the confirmed body
 * is no draft, so the key goes; anything else is parked.
 * @param {string} body @param {string} confirmedBody
 * @returns {"forget" | "park"}
 */
export function draftAction(body, confirmedBody) {
  return body === confirmedBody ? "forget" : "park";
}

/** A stored or typed delay, made safe to hand to `setTimeout`. */
export function clampSaveDelay(raw) {
  const n = numeric(raw);
  if (!Number.isFinite(n)) return SAVE_DEFAULT_MS;
  return Math.min(Math.max(Math.round(n), SAVE_MIN_MS), SAVE_MAX_MS);
}

/**
 * The open note once the list has loaded. The open note is kept across a
 * restart, and what it names may have gone while the app was closed: a note
 * that came back from the last window and that the list does not hold opens
 * nothing. A note opened in this window is left alone — the list is about to
 * hold it, or another tab does.
 * @param {string | null} active the open note
 * @param {string | null} restored the note that came back from the last window, until the list has spoken
 * @param {readonly string[]} listed the ids the list holds
 * @returns {string | null}
 */
export function listedNote(active, restored, listed) {
  if (active === null || active !== restored) return active;
  return listed.includes(active) ? active : null;
}
