/**
 * What a row of the explorer wears when git has something to say about it
 * (ide/03 §The Files occupant): the **standing** of a file — untracked,
 * added, modified, renamed, deleted, in conflict — and, folded upward, of
 * every folder holding one. The facts arrive from the Git panel's read
 * (`gitFiles.treeStandings`, the letters git prints made into kinds); this
 * model owns what the tree does with them: which ancestors carry the fold,
 * which tone a kind wears, which mark, which words the tooltip reads.
 *
 * The tones are the theme's three status roles, the same the Changes view's
 * chips wear: `ok` for what is new (added, untracked), `warn` for what moved
 * (modified, renamed, deleted), `danger` for a conflict. A folder wears the
 * strongest kind under it, in that order, so a conflict three levels down
 * is red at the top. Plain `.mjs`: `node --test` reads the rule with no DOM.
 */

import { t } from "../i18n/l10n.mjs";

/** The kinds a file can stand in, strongest first. */
export const STANDING_KINDS = Object.freeze(["conflict", "deleted", "renamed", "modified", "added", "untracked"]);

const TONE = Object.freeze({
  conflict: "danger",
  deleted: "warn",
  renamed: "warn",
  modified: "warn",
  added: "ok",
  untracked: "ok",
});

/** The glyph a kind wears at the row's right — the marks the Changes view uses, restated here so the two trees agree by test. */
const MARK = Object.freeze({
  conflict: "!",
  deleted: "−",
  renamed: "→",
  modified: "~",
  added: "+",
  untracked: "?",
});

const WORD = Object.freeze({
  conflict: "conflict",
  deleted: "deleted",
  renamed: "renamed",
  modified: "modified",
  added: "added",
  untracked: "untracked",
});

/** How strong a kind is: lower is stronger; a word off the list is weakest. */
function rank(kind) {
  const at = STANDING_KINDS.indexOf(kind);
  return at === -1 ? STANDING_KINDS.length : at;
}

/**
 * Every path that wears a standing, the files as given and every folder
 * above them as `holds` with the strongest kind beneath. A folder that is
 * also a changed path itself (never, in git — but the rule is total) keeps
 * its own kind.
 * @param {readonly {path: string, kind: string, staged?: boolean}[]} files
 * @returns {Map<string, {kind: string, staged: boolean, strongest: string}>}
 */
export function foldStandings(files) {
  const out = new Map();
  for (const f of files ?? []) {
    const kind = STANDING_KINDS.includes(f.kind) ? f.kind : "modified";
    out.set(f.path, { kind, staged: !!f.staged, strongest: kind });
  }
  for (const f of files ?? []) {
    const kind = out.get(f.path)?.strongest ?? "modified";
    let at = f.path.lastIndexOf("/");
    while (at > 0) {
      const dir = f.path.slice(0, at);
      const held = out.get(dir);
      if (!held) out.set(dir, { kind: "holds", staged: false, strongest: kind });
      else if (held.kind === "holds" && rank(kind) < rank(held.strongest)) out.set(dir, { ...held, strongest: kind });
      at = f.path.lastIndexOf("/", at - 1);
    }
  }
  return out;
}

/** The theme role a standing's name is painted in: `ok`, `warn` or `danger`. @param {{kind: string, strongest: string} | null | undefined} standing */
export function standingTone(standing) {
  if (!standing) return null;
  const kind = standing.kind === "holds" ? standing.strongest : standing.kind;
  return TONE[kind] ?? "warn";
}

/** The glyph at the row's right; a folder wears none — its colour says enough. @param {{kind: string} | null | undefined} standing */
export function standingMark(standing) {
  if (!standing || standing.kind === "holds") return null;
  return MARK[standing.kind] ?? "~";
}

/** The tooltip's words: *modified · staged*, *untracked*, *holds changes*. @param {{kind: string, staged: boolean, strongest: string} | null | undefined} standing */
export function standingHint(standing) {
  if (!standing) return null;
  if (standing.kind === "holds") return t("ui-file-standing-holds-changes");
  const word = WORD[standing.kind] ?? "changed";
  return standing.staged ? t("ui-file-standing-staged", { word }) : word;
}
