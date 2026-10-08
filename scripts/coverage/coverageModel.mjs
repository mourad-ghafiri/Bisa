/**
 * Line coverage, judged (docs/contributing/coverage.md §Line coverage): an
 * lcov file from either tool — cargo-llvm-cov for a Rust tree, Node's own
 * reporter for the desktop's models — read into lines hit and lines found per
 * file; every file placed in its tree (a crate, the shell, a desktop
 * directory) or in none (a test, a bench, a build script); the files the
 * exclusion list names left out; a desktop model no test loaded counted as
 * bare; one percentage per tree, floored to a tenth; each tree judged against
 * the committed baseline — held, rose, fell, new or gone — and the next
 * baseline, which only ever rises. No I/O here: `node --test` reads it, and
 * `check-coverage.mjs` beside it does the reading and the writing.
 */

/** Where every tree is going. */
export const TARGET = 100;

/** The file every tree is held to, named in the report. */
export const BASELINE_FILE = "scripts/coverage/baseline.json";

/** A desktop model's tree is its directory, cut this many segments below `desktop/src`. */
const DESKTOP_DEPTH = 2;

/**
 * An lcov text — `SF:` opens a file, `DA:<line>,<count>` is one line's hits,
 * `end_of_record` closes it; everything else (functions, branches, the
 * totals) is read past, since the totals are computed here. A file recorded
 * twice — two test binaries of one crate, or two runs merged — keeps the
 * higher count on each line.
 *
 * @param {string} text
 * @returns {Map<string, Map<number, number>>} by the path as the tool wrote it
 */
export function parseLcov(text) {
  const files = new Map();
  let current = null;
  for (const raw of String(text).split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith("SF:")) {
      const path = line.slice(3);
      if (!files.has(path)) files.set(path, new Map());
      current = files.get(path);
    } else if (line.startsWith("DA:") && current) {
      const [no, count] = line.slice(3).split(",");
      const n = Number(no);
      const c = count === undefined || count.trim() === "" ? NaN : Number(count);
      if (!Number.isInteger(n) || n < 1 || !Number.isFinite(c)) continue;
      current.set(n, Math.max(current.get(n) ?? 0, c));
    } else if (line === "end_of_record") {
      current = null;
    }
  }
  return files;
}

/**
 * Several parsed lcov files as one: a path in two of them keeps the higher
 * count per line, as `parseLcov` does within one.
 *
 * @param {Iterable<Map<string, Map<number, number>>>} parsed
 */
export function mergeLcov(parsed) {
  const out = new Map();
  for (const files of parsed) {
    for (const [path, lines] of files) {
      if (!out.has(path)) out.set(path, new Map());
      const into = out.get(path);
      for (const [n, c] of lines) into.set(n, Math.max(into.get(n) ?? 0, c));
    }
  }
  return out;
}

/**
 * A path as the tool wrote it — cargo-llvm-cov's absolute under the root,
 * Node's relative to the folder it ran in (`src/shell/x.mjs` from
 * `desktop/`) — as the repository's own path with forward slashes, or null
 * when it is not under the root (a dependency's source).
 *
 * @param {string} path
 * @param {string} root the repository root, absolute
 * @param {string} [base] the folder a relative path is relative to, as the repository's own path (`desktop`)
 */
export function relativize(path, root, base = "") {
  const slashed = String(path).replace(/\\/g, "/");
  const top = String(root).replace(/\\/g, "/").replace(/\/+$/, "");
  if (slashed.startsWith(`${top}/`)) return slashed.slice(top.length + 1);
  if (slashed.startsWith("/") || /^[A-Za-z]:\//.test(slashed)) return null;
  const rel = slashed.replace(/^\.\//, "");
  const under = String(base).replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  return under ? `${under}/${rel}` : rel;
}

/**
 * The tree a repository path is measured in, or null for a path nobody
 * measures: a crate's `src/` is the crate; the shell's `src/` is the shell;
 * a desktop model is its directory cut to two segments below `desktop/src`;
 * a test, a bench, a build script, a `.d.mts` and a `.test.mjs` are nobody's.
 *
 * @param {string} rel
 * @returns {string | null}
 */
export function placeOf(rel) {
  const path = String(rel).replace(/\\/g, "/");
  let m = /^crates\/([^/]+)\/src\/.+\.rs$/.exec(path);
  if (m) return `crates/${m[1]}`;
  if (/^desktop\/src-tauri\/src\/.+\.rs$/.test(path)) return "desktop/src-tauri";
  m = /^desktop\/src\/(.+)\.mjs$/.exec(path);
  if (m && !path.endsWith(".test.mjs")) {
    const dirs = m[1].split("/").slice(0, -1).slice(0, DESKTOP_DEPTH);
    return ["desktop/src", ...dirs].join("/");
  }
  return null;
}

/**
 * Lines found and lines hit per file, each placed in its tree; a path that is
 * nobody's (a test source the tool reported too) is dropped.
 *
 * @param {Map<string, Map<number, number>>} byPath relativized paths
 * @returns {{ path: string, tree: string, found: number, hit: number, unmeasured: boolean }[]}
 */
export function fileStats(byPath) {
  const out = [];
  for (const [path, lines] of byPath) {
    const tree = placeOf(path);
    if (!tree) continue;
    let hit = 0;
    for (const c of lines.values()) if (c > 0) hit += 1;
    out.push({ path, tree, found: lines.size, hit, unmeasured: false });
  }
  return out.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

/**
 * The files the exclusion list names, left out — and which of the list's
 * paths named nothing that was measured, so the judge can say so.
 *
 * @param {{ path: string }[]} files
 * @param {Iterable<string>} excluded repository paths
 */
export function applyExclusions(files, excluded) {
  const set = new Set(excluded);
  return files.filter((f) => !set.has(f.path));
}

/**
 * Node reports only the files a test loaded: a desktop model nothing imports
 * is invisible rather than bare. Every `.mjs` on disk that the lcov did not
 * name joins the list as unmeasured — hit 0 over its physical lines, which is
 * what Node counts as a file's lines once a test loads it, so the denominator
 * does not jump at that moment. A Rust file is never completed this way: a
 * file of `pub mod` lines has no coverable line and is not 0 %.
 *
 * @param {{ path: string, tree: string, found: number, hit: number, unmeasured: boolean }[]} files
 * @param {Iterable<string>} onDisk repository paths of every desktop `.mjs` that is not a test
 * @param {(path: string) => number} linesOf the file's physical line count
 */
export function completeDesktop(files, onDisk, linesOf) {
  const seen = new Set(files.map((f) => f.path));
  const added = [];
  for (const path of onDisk) {
    if (seen.has(path)) continue;
    const tree = placeOf(path);
    if (!tree || !tree.startsWith("desktop/src")) continue;
    added.push({ path, tree, found: linesOf(path), hit: 0, unmeasured: true });
  }
  return [...files, ...added].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

/** A percentage floored to a tenth: integer arithmetic first, so 2 of 3 is 66.6 and a run re-judged never reads lower than it was written. */
export function percent(hit, found) {
  if (found <= 0) return TARGET;
  return Math.floor((hit * 1000) / found) / 10;
}

/**
 * One row per tree: lines found and hit over its files, the percentage, the
 * files ascending by their own percentage with the unmeasured first. A tree
 * with no coverable line is not a row.
 *
 * @param {{ path: string, tree: string, found: number, hit: number, unmeasured: boolean }[]} files
 * @returns {Map<string, { found: number, hit: number, pct: number, files: object[] }>}
 */
export function treeTotals(files) {
  const trees = new Map();
  for (const f of files) {
    if (!trees.has(f.tree)) trees.set(f.tree, { found: 0, hit: 0, pct: 0, files: [] });
    const t = trees.get(f.tree);
    t.found += f.found;
    t.hit += f.hit;
    t.files.push({ ...f, pct: percent(f.hit, f.found) });
  }
  const out = new Map();
  for (const tree of [...trees.keys()].sort()) {
    const t = trees.get(tree);
    if (t.found === 0) continue;
    t.pct = percent(t.hit, t.found);
    t.files.sort((a, b) => {
      if (a.unmeasured !== b.unmeasured) return a.unmeasured ? -1 : 1;
      if (a.pct !== b.pct) return a.pct - b.pct;
      return a.path < b.path ? -1 : a.path > b.path ? 1 : 0;
    });
    out.set(tree, t);
  }
  return out;
}

/**
 * Every tree against the baseline: `held` at its figure, `rose` above it,
 * `fell` below it, `new` with no figure yet, `gone` a figure with no tree —
 * the last two on a full judge alone, since a judge of some trees knows
 * nothing of the rest. The judgement is ok when nothing fell, is new or is
 * gone: a new or a gone tree is the baseline drifting from the tree, and one
 * `--write` ends it; a rise passes with a note, since a run is not a sentence
 * count and a rise that had to be written would make a timing-dependent
 * branch a red gate.
 *
 * @param {Map<string, { found: number, hit: number, pct: number }>} totals
 * @param {Record<string, number>} baseline
 * @param {string[] | null} only the trees asked about, or null for every one
 */
export function judge(totals, baseline, only = null) {
  const rows = [];
  const asked = only ? new Set(only) : null;
  const trees = new Set([...totals.keys(), ...(asked ? [] : Object.keys(baseline))]);
  for (const tree of [...trees].sort()) {
    if (asked && !asked.has(tree)) continue;
    const t = totals.get(tree);
    const was = Object.hasOwn(baseline, tree) ? baseline[tree] : null;
    let verdict;
    if (!t) verdict = "gone";
    else if (was === null) verdict = "new";
    else if (t.pct < was) verdict = "fell";
    else if (t.pct > was) verdict = "rose";
    else verdict = "held";
    rows.push({ tree, pct: t ? t.pct : null, found: t ? t.found : 0, hit: t ? t.hit : 0, baseline: was, verdict });
  }
  const ok = rows.every((r) => r.verdict === "held" || r.verdict === "rose");
  return { rows, ok };
}

/**
 * The baseline after this run: every judged tree at the higher of its figure
 * and its reading, a tree without a figure added — never lowered; a gone
 * tree dropped on a full judge alone. Keys sorted, so the file diffs by tree.
 *
 * @param {Record<string, number>} baseline
 * @param {Map<string, { pct: number }>} totals
 * @param {string[] | null} only
 */
export function nextBaseline(baseline, totals, only = null) {
  const next = { ...baseline };
  const asked = only ? new Set(only) : null;
  for (const [tree, t] of totals) {
    if (asked && !asked.has(tree)) continue;
    next[tree] = Math.max(Object.hasOwn(next, tree) ? next[tree] : 0, t.pct);
  }
  if (!asked) for (const tree of Object.keys(next)) if (!totals.has(tree)) delete next[tree];
  return Object.fromEntries(Object.entries(next).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

const WORDS = {
  held: "held",
  rose: "rose — `just coverage-write` holds it",
  fell: "FELL",
  new: "new — `just coverage-write` adds it",
  gone: "GONE — no tree measured; `just coverage-write` drops it",
};

function pad(s, n) {
  const str = String(s);
  return str.length >= n ? str : str + " ".repeat(n - str.length);
}

function lpad(s, n) {
  const str = String(s);
  return str.length >= n ? str : " ".repeat(n - str.length) + str;
}

function pctWords(p) {
  return p === null ? "—" : `${p.toFixed(1)} %`;
}

/**
 * The report as text: one line per tree with its reading, its figure and its
 * verdict, then one sentence saying what fell, rose, is new or gone, and the
 * target. With `files`, the named trees' files follow, ascending, the
 * unmeasured first and marked — the work list a hardening pass starts from.
 *
 * @param {{ rows: object[], ok: boolean }} judgement
 * @param {{ totals?: Map<string, { files: object[] }>, files?: string[], verdicts?: boolean }} [options]
 */
export function formatReport(judgement, options = {}) {
  const { rows, ok } = judgement;
  const verdicts = options.verdicts ?? true;
  const width = Math.max(10, ...rows.map((r) => r.tree.length));
  const out = [];
  out.push(
    verdicts
      ? `coverage: ${rows.length} trees against ${BASELINE_FILE}`
      : `coverage: ${rows.length} trees measured by their own suites (a report, not a verdict)`
  );
  for (const r of rows) {
    const line = [
      "  ",
      pad(r.tree, width),
      "  ",
      lpad(pctWords(r.pct), 8),
      "  ",
      lpad(r.hit, 7),
      " / ",
      pad(r.found, 7),
    ];
    if (verdicts) {
      line.push("  baseline ", pad(r.baseline === null ? "—" : r.baseline.toFixed(1), 6), "  ", WORDS[r.verdict]);
    }
    out.push(line.join("").replace(/\s+$/, ""));
  }
  if (verdicts) {
    const fell = rows.filter((r) => r.verdict === "fell");
    const rose = rows.filter((r) => r.verdict === "rose");
    const fresh = rows.filter((r) => r.verdict === "new");
    const gone = rows.filter((r) => r.verdict === "gone");
    const parts = [];
    if (fell.length) parts.push(`${fell.length} fell (${fell.map((r) => `${r.tree} by ${(r.baseline - r.pct).toFixed(1)}`).join(", ")})`);
    if (rose.length) parts.push(`${rose.length} rose`);
    if (fresh.length) parts.push(`${fresh.length} new`);
    if (gone.length) parts.push(`${gone.length} gone`);
    const at = rows.filter((r) => r.pct === TARGET).length;
    out.push(
      `${parts.length ? parts.join(", ") : "every tree held"}; ${at} of ${rows.length} at ${TARGET}. The end is ${TARGET} in every row.`
    );
    out.push(ok ? "coverage: the gate holds" : "coverage: the gate does not hold");
  }
  for (const tree of options.files ?? []) {
    const t = options.totals?.get(tree);
    out.push("");
    if (!t) {
      out.push(`${tree}: no measured file`);
      continue;
    }
    out.push(`${tree}: ${t.files.length} files, ascending`);
    const fw = Math.max(10, ...t.files.map((f) => f.path.length));
    for (const f of t.files) {
      out.push(
        `  ${pad(f.path, fw)}  ${lpad(pctWords(f.pct), 8)}  ${lpad(f.hit, 6)} / ${pad(f.found, 6)}${f.unmeasured ? "  (no test loads it)" : ""}`.replace(
          /\s+$/,
          ""
        )
      );
    }
  }
  return `${out.join("\n")}\n`;
}

/**
 * The bare lines of one file, as ranges — `12`, `40-43` — read from the parsed
 * lcov: what a test has to reach.
 *
 * @param {Map<number, number>} lines
 */
export function bareRanges(lines) {
  const bare = [...lines.entries()]
    .filter(([, c]) => c === 0)
    .map(([n]) => n)
    .sort((a, b) => a - b);
  const out = [];
  for (const n of bare) {
    const last = out[out.length - 1];
    if (last && last.end === n - 1) last.end = n;
    else out.push({ start: n, end: n });
  }
  return out.map((r) => (r.start === r.end ? `${r.start}` : `${r.start}-${r.end}`));
}

// ---------------------------------------------------------------------------
// What a Rust source excuses on its own: its test items, and a line that
// names an invariant.
// ---------------------------------------------------------------------------

/**
 * The `{`, `}` and `;` of a Rust source that are code — outside a string,
 * a character, a comment — each with its line, so an item's end can be
 * found without a `"}"` in a test closing it early.
 *
 * @param {string} source
 * @returns {{ ch: string, line: number }[]}
 */
export function codeBraces(source) {
  const out = [];
  const s = String(source);
  let i = 0;
  let line = 1;
  const n = s.length;
  while (i < n) {
    const c = s[i];
    if (c === "\n") {
      line += 1;
      i += 1;
      continue;
    }
    // A line comment runs to the end of its line.
    if (c === "/" && s[i + 1] === "/") {
      while (i < n && s[i] !== "\n") i += 1;
      continue;
    }
    // A block comment nests in Rust.
    if (c === "/" && s[i + 1] === "*") {
      let depth = 1;
      i += 2;
      while (i < n && depth > 0) {
        if (s[i] === "/" && s[i + 1] === "*") {
          depth += 1;
          i += 2;
        } else if (s[i] === "*" && s[i + 1] === "/") {
          depth -= 1;
          i += 2;
        } else {
          if (s[i] === "\n") line += 1;
          i += 1;
        }
      }
      continue;
    }
    // A raw string: r"…", r#"…"#, br"…".
    if ((c === "r" || (c === "b" && s[i + 1] === "r")) && /^b?r#*"/.test(s.slice(i, i + 8))) {
      const start = c === "b" ? i + 2 : i + 1;
      let hashes = 0;
      while (s[start + hashes] === "#") hashes += 1;
      const close = `"${"#".repeat(hashes)}`;
      let j = start + hashes + 1;
      while (j < n && s.slice(j, j + close.length) !== close) {
        if (s[j] === "\n") line += 1;
        j += 1;
      }
      i = j + close.length;
      continue;
    }
    // A string: "…" with escapes; b"…" the same.
    if (c === '"' || (c === "b" && s[i + 1] === '"')) {
      let j = c === "b" ? i + 2 : i + 1;
      while (j < n && s[j] !== '"') {
        if (s[j] === "\\") {
          // An escape takes the next character with it — a newline too.
          if (s[j + 1] === "\n") line += 1;
          j += 1;
        } else if (s[j] === "\n") line += 1;
        j += 1;
      }
      i = j + 1;
      continue;
    }
    // A character literal — '{', '\n', '\'' — against a lifetime 'a.
    if (c === "'") {
      if (s[i + 1] === "\\") {
        let j = i + 2;
        while (j < n && s[j] !== "'") j += 1;
        i = j + 1;
        continue;
      }
      if (s[i + 2] === "'") {
        i += 3;
        continue;
      }
      i += 1;
      continue;
    }
    if (c === "{" || c === "}" || c === ";") out.push({ ch: c, line });
    i += 1;
  }
  return out;
}

/**
 * The lines of a Rust source that belong to an item under `#[cfg(test)]` —
 * a `mod tests { … }`, a helper `fn`, an `impl`, a `use` — as `[first,
 * last]` pairs, inclusive. Test code is not the product: the meter leaves
 * it out as it leaves out `tests/` and `.test.mjs`. The attribute must be
 * exactly `#[cfg(test)]` on a line of its own; the item is the next line
 * that is not an attribute, a doc comment or blank, and it ends at the `;`
 * that closes it or at the `}` matching its first `{`.
 *
 * @param {string} source
 * @returns {[number, number][]}
 */
export function testItemSpans(source) {
  const lines = String(source).split("\n");
  // A trailing newline ends the last line; it is not a line of its own.
  const lineCount = String(source).endsWith("\n") ? lines.length - 1 : lines.length;
  const braces = codeBraces(source);
  const spans = [];
  for (let i = 0; i < lines.length; i += 1) {
    if (lines[i].trim() !== "#[cfg(test)]") continue;
    const first = i + 1;
    let j = i + 1;
    while (j < lines.length) {
      const t = lines[j].trim();
      if (t === "" || t.startsWith("#[") || t.startsWith("///") || t.startsWith("//!")) j += 1;
      else break;
    }
    const itemLine = j + 1;
    let depth = 0;
    let last = null;
    let opened = false;
    for (const b of braces) {
      if (b.line < itemLine) continue;
      if (b.ch === ";" && !opened) {
        last = b.line;
        break;
      }
      if (b.ch === "{") {
        depth += 1;
        opened = true;
      } else if (b.ch === "}") {
        depth -= 1;
        if (opened && depth === 0) {
          last = b.line;
          break;
        }
      }
    }
    // A block the file ends inside — a broken file — runs to its end.
    if (last === null) last = opened ? lineCount : itemLine;
    spans.push([first, Math.max(last, itemLine)]);
    i = last - 1;
  }
  return spans;
}

/** The standard lcov markers: one line, or a block. Each must say why. */
const EXCL_LINE = /LCOV_EXCL_LINE\b(.*)$/;
const EXCL_START = /LCOV_EXCL_START\b(.*)$/;
const EXCL_STOP = /LCOV_EXCL_STOP\b/;

/**
 * A reason after a marker: a colon or a dash, then at least three words.
 *
 * @param {string} rest what follows the marker on its line
 */
function reasonOf(rest) {
  const words = String(rest)
    .replace(/^\s*[:—-]\s*/, "")
    .trim();
  return words.split(/\s+/).filter(Boolean).length >= 3 ? words : null;
}

/**
 * The lines a Rust source excuses by name — the lcov markers
 * `LCOV_EXCL_LINE` (its own line) and `LCOV_EXCL_START` … `LCOV_EXCL_STOP`
 * (a block, both marker lines included) — each with the reason it carries.
 * A marker without a reason of at least three words is a fault, and so is
 * a block left open: the judge refuses the run rather than guess.
 *
 * @param {string} source
 * @returns {{ lines: Set<number>, excused: { line: number, reason: string }[], faults: string[] }}
 */
export function excludedLines(source) {
  const lines = new Set();
  const excused = [];
  const faults = [];
  const text = String(source).split("\n");
  let open = null;
  for (let i = 0; i < text.length; i += 1) {
    const no = i + 1;
    const l = text[i];
    if (EXCL_STOP.test(l)) {
      if (open === null) faults.push(`line ${no}: LCOV_EXCL_STOP with no block open`);
      else {
        for (let k = open.line; k <= no; k += 1) lines.add(k);
        open = null;
      }
      continue;
    }
    const start = EXCL_START.exec(l);
    if (start) {
      const reason = reasonOf(start[1]);
      if (!reason) faults.push(`line ${no}: LCOV_EXCL_START says no reason`);
      if (open !== null) faults.push(`line ${no}: LCOV_EXCL_START inside a block opened at line ${open.line}`);
      open = { line: no };
      excused.push({ line: no, reason: reason ?? "" });
      continue;
    }
    const one = EXCL_LINE.exec(l);
    if (one) {
      const reason = reasonOf(one[1]);
      if (!reason) faults.push(`line ${no}: LCOV_EXCL_LINE says no reason`);
      lines.add(no);
      excused.push({ line: no, reason: reason ?? "" });
    }
  }
  if (open !== null) faults.push(`line ${open.line}: LCOV_EXCL_START never stopped`);
  return { lines, excused, faults };
}

/**
 * A measured file's lines without what its source excuses: the test items
 * and the marked lines. The counts of the rest are untouched.
 *
 * @param {Map<number, number>} lines the file's `DA` counts
 * @param {string} source the file as it is on disk
 * @returns {{ lines: Map<number, number>, excused: { line: number, reason: string }[], faults: string[], testLines: number }}
 */
export function withoutExcused(lines, source) {
  const out = new Map(lines);
  let testLines = 0;
  for (const [first, last] of testItemSpans(source)) {
    for (let k = first; k <= last; k += 1) if (out.delete(k)) testLines += 1;
  }
  const marked = excludedLines(source);
  for (const k of marked.lines) out.delete(k);
  return { lines: out, excused: marked.excused, faults: marked.faults, testLines };
}
