/**
 * Hunks of a unified diff, and the patches the IDE builds from them
 * (ide/04 §6).
 *
 * Plain JavaScript so `node --test` runs it without a build. Two jobs:
 *
 * 1. **Cut a patch into hunks** the way git drew them — the file header once,
 *    then one entry per `@@` line, each carrying its own lines with the old and
 *    new line numbers worked out, so a row can be picked and a note can name
 *    the lines it is about.
 * 2. **Build the patch for a selection.** Staging one hunk is the header plus
 *    that hunk. Staging *some lines* of a hunk is the same hunk with every
 *    unpicked addition dropped and every unpicked deletion turned back into
 *    context — that is the transformation `git add -p`'s `e` asks a person to
 *    do by hand, and the counts in the `@@` header are recomputed so the
 *    patch is valid on its own (the node also passes `--recount`, belt and
 *    braces).
 *
 * `\ No newline at end of file` markers travel with the line before them.
 */

import { t } from "../../i18n/l10n.mjs";

const HUNK_HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$/;

/**
 * @param {string} diff
 * @returns {{header: string, hunks: import("./hunkModel.d.mts").Hunk[]}}
 */
export function parseHunks(diff) {
  const header = [];
  /** @type {import("./hunkModel.d.mts").Hunk[]} */
  const hunks = [];
  let current = null;
  let oldLine = 0;
  let newLine = 0;
  // `split("\n")` leaves one trailing "" for a diff that ends in a newline;
  // it is not a line.
  const raw = diff.split("\n");
  if (raw.length > 0 && raw[raw.length - 1] === "") raw.pop();
  for (const line of raw) {
    const m = HUNK_HEADER.exec(line);
    if (m) {
      current = {
        index: hunks.length,
        header: line,
        oldStart: Number(m[1]),
        oldCount: m[2] === undefined ? 1 : Number(m[2]),
        newStart: Number(m[3]),
        newCount: m[4] === undefined ? 1 : Number(m[4]),
        context: m[5].trim(),
        lines: [],
        text: "",
      };
      oldLine = current.oldStart;
      newLine = current.newStart;
      hunks.push(current);
      continue;
    }
    if (!current) {
      header.push(line);
      continue;
    }
    if (line.startsWith("\\")) {
      current.lines.push({ kind: "meta", text: line, oldLine: null, newLine: null });
    } else if (line.startsWith("+")) {
      current.lines.push({ kind: "add", text: line, oldLine: null, newLine: newLine++ });
    } else if (line.startsWith("-")) {
      current.lines.push({ kind: "del", text: line, oldLine: oldLine++, newLine: null });
    } else {
      // Context. git writes a leading space; an empty context line can arrive
      // as "" from tools that strip trailing whitespace, and means the same.
      current.lines.push({ kind: "context", text: line, oldLine: oldLine++, newLine: newLine++ });
    }
  }
  for (const h of hunks) {
    h.text = [h.header, ...h.lines.map((l) => l.text)].join("\n") + "\n";
  }
  return { header: header.length ? header.join("\n") + "\n" : "", hunks };
}

/** The header line for a hunk whose lines are `lines`. */
function headerFor(hunk, lines) {
  let oldCount = 0;
  let newCount = 0;
  for (const l of lines) {
    if (l.kind === "context") {
      oldCount++;
      newCount++;
    } else if (l.kind === "del") oldCount++;
    else if (l.kind === "add") newCount++;
  }
  const range = (start, count) => (count === 1 ? `${start}` : `${start},${count}`);
  const tail = hunk.context ? ` ${hunk.context}` : "";
  return `@@ -${range(hunk.oldStart, oldCount)} +${range(hunk.newStart, newCount)} @@${tail}`;
}

/**
 * The whole hunk as a patch git can apply on its own.
 * @param {string} header
 * @param {import("./hunkModel.d.mts").Hunk} hunk
 */
export function hunkPatch(header, hunk) {
  return header + hunk.text;
}

/**
 * A patch of only the picked change lines of one hunk. `picked` holds indexes
 * into `hunk.lines`. Unpicked deletions become context; unpicked additions are
 * dropped. `null` when nothing that changes anything is picked.
 *
 * @param {string} header
 * @param {import("./hunkModel.d.mts").Hunk} hunk
 * @param {Iterable<number>} picked
 * @returns {string | null}
 */
export function linesPatch(header, hunk, picked) {
  const set = new Set(picked);
  const out = [];
  let changes = 0;
  for (let i = 0; i < hunk.lines.length; i++) {
    const l = hunk.lines[i];
    if (l.kind === "context") {
      out.push(l);
    } else if (l.kind === "meta") {
      // Belongs to the line before it; keep it only if that line was kept.
      if (out.length > 0 && out[out.length - 1] === hunk.lines[i - 1]) out.push(l);
    } else if (set.has(i)) {
      out.push(l);
      changes++;
    } else if (l.kind === "del") {
      out.push({ kind: "context", text: " " + l.text.slice(1), oldLine: l.oldLine, newLine: null });
    }
    // an unpicked "add" is dropped
  }
  if (changes === 0) return null;
  return header + [headerFor(hunk, out), ...out.map((l) => l.text)].join("\n") + "\n";
}

/**
 * Which change lines of a hunk are pickable — the indexes of its `add` and
 * `del` lines.
 * @param {import("./hunkModel.d.mts").Hunk} hunk
 * @returns {number[]}
 */
export function pickable(hunk) {
  const out = [];
  hunk.lines.forEach((l, i) => {
    if (l.kind === "add" || l.kind === "del") out.push(i);
  });
  return out;
}

/**
 * The 1-based inclusive line range a note on this hunk is about, on the new
 * side of the file. A hunk of pure deletions has no new lines; the note then
 * points at the line the deletion sits before, which is where the reader's
 * eye lands.
 * @param {import("./hunkModel.d.mts").Hunk} hunk
 * @returns {{start: number, end: number}}
 */
export function hunkRange(hunk) {
  const start = Math.max(1, hunk.newStart);
  const end = Math.max(start, hunk.newStart + hunk.newCount - 1);
  return { start, end };
}

/**
 * The scope a note gets for the patch a person is looking at.
 * @param {boolean} staged
 * @returns {{scope: "staged"} | {scope: "unstaged"}}
 */
export function noteScope(staged) {
  return staged ? { scope: "staged" } : { scope: "unstaged" };
}

/**
 * A short label for a note's scope, for a chip.
 * @param {import("../../types").DiffScope} scope
 */
export function scopeLabel(scope) {
  if (scope.scope === "branch") return `vs ${scope.base}`;
  return scope.scope;
}

/**
 * Notes still waiting on an agent: not sent, or edited since, and not resolved.
 * @param {import("../../types").ReviewNote[]} notes
 */
export function unsentNotes(notes) {
  return notes.filter((n) => n.sent_at == null && n.resolved_at == null);
}

/**
 * Whether a refused edit, resolve or delete of a review note means the note
 * is gone — the node's `404`, read by status and never by its words. The
 * list on screen is then stale by that row: it is read again, so the row
 * leaves instead of refusing the same click for ever.
 * @param {number | null | undefined} status
 */
export function reviewNoteGone(status) {
  return status === 404;
}

/** *hunk 2 of 5* — where a hunk stands among the patch's. @param {number} index zero-based @param {number} total */
export function hunkPosition(index, total) {
  return t("work-hunk-model-hunk-of", { n: index + 1, total });
}
