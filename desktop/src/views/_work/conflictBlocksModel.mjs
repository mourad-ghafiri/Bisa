/**
 * A conflicted file as blocks (ide/04 §Conflicts, continued): the text git
 * wrote on disk, its markers read into the runs everybody agrees on and
 * the conflicts between them — each with what *ours* and *theirs* hold, and
 * the base when git wrote it (the `diff3` and `zdiff3` styles) — the choice
 * a person makes on each, and the file the choices compose. The result is
 * what *Mark resolved* saves; a conflict with no choice yet keeps its
 * markers, so an unfinished file is never saved as finished. The sides are
 * git's; which is *mine* is `conflictSidesModel`'s one rule, passed in as
 * `swapped`. Pure, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** What a person can do with one conflict: a side, both in either order, or their own text. */
export const CHOICES = Object.freeze(["mine", "theirs", "both", "both_reversed", "edit"]);

const OPEN = /^<{7}(?: .*)?$/;
const BASE = /^\|{7}(?: .*)?$/;
const MID = /^={7}$/;
const CLOSE = /^>{7}(?: .*)?$/;

/** The lines of `text`, each keeping its newline, so joining them gives the text back. */
function linesOf(text) {
  const out = [];
  let at = 0;
  const s = String(text ?? "");
  while (at < s.length) {
    const nl = s.indexOf("\n", at);
    if (nl === -1) {
      out.push(s.slice(at));
      break;
    }
    out.push(s.slice(at, nl + 1));
    at = nl + 1;
  }
  return out;
}

const bare = (line) => line.replace(/\r?\n$/, "");

/**
 * Read the markers. A `<<<<<<<` opens a conflict, `=======` divides ours
 * from theirs, `>>>>>>>` closes it; between the open and the divide a
 * `|||||||` starts the base (git's `diff3` and `zdiff3` styles). A run
 * outside any conflict is text. A marker out of place — an open never
 * closed, a divide with no open — is a torn file: the whole text is one
 * run and `problem` says so, for a person who edited the markers by hand.
 * @param {string} text
 * @returns {{segments: ({kind: "text", text: string} | {kind: "conflict", id: string, ours: string, base: string | null, theirs: string, raw: string})[], problem: string | null}}
 */
export function parseConflicts(text) {
  const src = String(text ?? "");
  const lines = linesOf(src);
  const segments = [];
  let run = [];
  let n = 0;
  const flush = () => {
    if (run.length) segments.push({ kind: "text", text: run.join("") });
    run = [];
  };
  let i = 0;
  while (i < lines.length) {
    const line = bare(lines[i]);
    if (MID.test(line) || CLOSE.test(line) || BASE.test(line)) return torn(src, t("work-conflict-blocks-marker-no-before", { line: line.slice(0, 7) }));
    if (!OPEN.test(line)) {
      run.push(lines[i]);
      i += 1;
      continue;
    }
    const raw = [lines[i]];
    const ours = [];
    const base = [];
    const theirs = [];
    let part = "ours";
    let closed = false;
    i += 1;
    while (i < lines.length) {
      const l = bare(lines[i]);
      raw.push(lines[i]);
      if (OPEN.test(l)) return torn(src, t("work-conflict-blocks-marker-inside-conflict"));
      if (part === "ours" && BASE.test(l)) part = "base";
      else if ((part === "ours" || part === "base") && MID.test(l)) part = "theirs";
      else if (part === "theirs" && CLOSE.test(l)) {
        closed = true;
        i += 1;
        break;
      } else if (part === "theirs" && (BASE.test(l) || MID.test(l))) return torn(src, t("work-conflict-blocks-marker-after", { l: l.slice(0, 7) }));
      else if (part === "ours") ours.push(lines[i]);
      else if (part === "base") base.push(lines[i]);
      else theirs.push(lines[i]);
      i += 1;
    }
    if (!closed) return torn(src, t("work-conflict-blocks-marker-never-closed"));
    flush();
    n += 1;
    segments.push({ kind: "conflict", id: `c${n}`, ours: ours.join(""), base: part === "theirs" && raw.some((r) => BASE.test(bare(r))) ? base.join("") : null, theirs: theirs.join(""), raw: raw.join("") });
  }
  flush();
  return { segments, problem: null };
}

function torn(text, why) {
  return { segments: text ? [{ kind: "text", text }] : [], problem: t("work-conflict-blocks-conflict-markers-file-torn-fix-them", { why }) };
}

/** The conflicts among the segments, in order. */
export function conflictsOf(segments) {
  return (Array.isArray(segments) ? segments : []).filter((s) => s && s.kind === "conflict");
}

/** Whether `text` still carries a marker line — what keeps *Mark resolved* off. */
export function hasMarkers(text) {
  return /^(<{7}|={7}|>{7}|\|{7})(?: |$)/m.test(String(text ?? ""));
}

/**
 * The text a choice makes of one conflict, in git's sides mapped through
 * `swapped` (mine is git's theirs under a rebase). `edit` is the person's
 * text; no choice is the conflict as git wrote it, markers and all.
 * @param {{ours: string, theirs: string, raw: string}} conflict
 * @param {{choice: string, text?: string} | null | undefined} made
 * @param {boolean} swapped
 */
export function textFor(conflict, made, swapped) {
  if (!made || !CHOICES.includes(made.choice)) return conflict.raw;
  const mine = swapped ? conflict.theirs : conflict.ours;
  const theirs = swapped ? conflict.ours : conflict.theirs;
  switch (made.choice) {
    case "mine":
      return mine;
    case "theirs":
      return theirs;
    case "both":
      return joined(mine, theirs);
    case "both_reversed":
      return joined(theirs, mine);
    default:
      return String(made.text ?? "");
  }
}

/** Two runs one after the other, a newline between when the first lacks one. */
function joined(a, b) {
  if (!a) return b;
  if (!b) return a;
  return a.endsWith("\n") ? a + b : `${a}\n${b}`;
}

/**
 * The whole file the choices compose: every text run as it is, every
 * conflict as chosen — or as git wrote it, when not yet.
 * @param {readonly object[]} segments
 * @param {Readonly<Record<string, {choice: string, text?: string}>>} choices by conflict id
 * @param {boolean} swapped
 */
export function compose(segments, choices, swapped) {
  return (Array.isArray(segments) ? segments : []).map((s) => (s.kind === "conflict" ? textFor(s, choices?.[s.id], swapped) : s.text)).join("");
}

/** A choice made on one conflict; `text` only for `edit`. */
export function choose(choices, id, choice, text) {
  if (!CHOICES.includes(choice)) return choices ?? {};
  return { ...(choices ?? {}), [id]: choice === "edit" ? { choice, text: String(text ?? "") } : { choice } };
}

/** A choice taken back. */
export function unchoose(choices, id) {
  const next = { ...(choices ?? {}) };
  delete next[id];
  return next;
}

/** One choice on every conflict — *Keep all mine*, *Keep all theirs*. */
export function chooseAll(segments, choice) {
  const out = {};
  for (const c of conflictsOf(segments)) out[c.id] = { choice };
  return out;
}

/** How far the file is: conflicts settled, of all. */
export function progress(segments, choices) {
  const all = conflictsOf(segments);
  const settled = all.filter((c) => choices?.[c.id] && CHOICES.includes(choices[c.id].choice)).length;
  return { settled, total: all.length };
}

/** The ids of the conflicts still without a choice, in order. */
export function unsettledIds(segments, choices) {
  return conflictsOf(segments)
    .filter((c) => !choices?.[c.id])
    .map((c) => c.id);
}

/**
 * The next conflict to look at after `from` — the first unsettled one
 * after it, wrapping to the first; any conflict when every one is settled;
 * none when there are none.
 * @param {readonly object[]} segments
 * @param {Readonly<Record<string, object>>} choices
 * @param {string | null} from a conflict id
 */
export function nextUnsettled(segments, choices, from) {
  return step(segments, choices, from, 1);
}

/** The one before `from`, the same way. */
export function previousUnsettled(segments, choices, from) {
  return step(segments, choices, from, -1);
}

function step(segments, choices, from, dir) {
  const ids = conflictsOf(segments).map((c) => c.id);
  if (ids.length === 0) return null;
  const open = unsettledIds(segments, choices);
  const pool = open.length ? open : ids;
  // From nothing, the next is the first and the previous the last; an id the
  // file no longer has counts as nothing too.
  const known = from === null ? -1 : ids.indexOf(from);
  const at = known >= 0 ? known : dir > 0 ? -1 : ids.length;
  const n = ids.length;
  for (let k = 1; k <= n; k += 1) {
    const i = (((at + dir * k) % n) + n) % n;
    if (pool.includes(ids[i])) return ids[i];
  }
  return pool[0] ?? null;
}

/** The lines a run has, for the fold's words. */
export function lineCount(text) {
  const s = String(text ?? "");
  if (!s) return 0;
  const n = s.split("\n").length;
  return s.endsWith("\n") ? n - 1 : n;
}

/** What a folded run says: *… 42 unchanged lines …*. */
export function foldWords(lines) {
  return t("work-conflict-blocks-unchanged-line-lines", { lines });
}

/** A run short enough to show whole rather than fold. */
export const FOLD_UNDER = 6;

/**
 * What a settled conflict's card says: *kept mine — main*, *kept both, mine
 * first*, *edited*.
 * @param {{choice: string} | null | undefined} made
 * @param {{mine: {name: string}, theirs: {name: string}}} sides
 */
export function blockWords(made, sides) {
  switch (made?.choice) {
    case "mine":
      return t("work-conflict-blocks-kept-mine", { mine: sides.mine.name });
    case "theirs":
      return t("work-conflict-blocks-kept-theirs", { theirs: sides.theirs.name });
    case "both":
      return t("work-conflict-blocks-kept-both-mine-first");
    case "both_reversed":
      return t("work-conflict-blocks-kept-both-theirs-first");
    case "edit":
      return "edited";
    default:
      return t("work-conflict-blocks-not-settled-yet");
  }
}

/**
 * What the whole document says at the top: how many conflicts, how many
 * settled; a file with no markers left is already merged — by a hand edit
 * elsewhere, or by a take.
 * @param {{settled: number, total: number}} p
 */
export function documentWords(p) {
  if (p.total === 0) return t("work-conflict-blocks-no-conflict-markers-left-file-review");
  const left = p.total - p.settled;
  if (left === 0) return t("work-conflict-blocks-every-conflict-settled-one-one-review", { total: p.total });
  return t("work-conflict-blocks-conflict-conflicts-file-settled-go", { total: p.total, settled: p.settled, left });
}
