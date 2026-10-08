#!/usr/bin/env node
/**
 * The coverage judge (docs/contributing/coverage.md §Line coverage): the lcov
 * files `scripts/coverage/measure` left under target/coverage/ read into one
 * figure per tree, each held to scripts/coverage/baseline.json — a fall, a
 * tree with no figure and a figure with no tree each fail; a rise passes
 * with a note. `--write` records what the run measured, raising and never
 * lowering. `--crate <name>` (repeatable) reports the named crates alone,
 * without a verdict — the loop of a hardening pass; `--files <tree>`
 * (repeatable) lists a tree's files ascending with their bare lines, the
 * work list. The summary is written to target/coverage/summary.json.
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASELINE_FILE,
  applyExclusions,
  bareRanges,
  completeDesktop,
  fileStats,
  formatReport,
  judge,
  mergeLcov,
  nextBaseline,
  parseLcov,
  relativize,
  treeTotals,
} from "./coverageModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const out = join(root, "target", "coverage");
/** Each half's lcov, and the folder its tool ran in — what a relative path in it is relative to. */
const HALVES = { rust: "rust.lcov", shell: "shell.lcov", desktop: "desktop.lcov" };
const BASES = { rust: "", shell: "", desktop: "desktop" };

const args = process.argv.slice(2);
const halves = new Set(Object.keys(HALVES).filter((h) => args.includes(`--${h}`)));
const crates = [];
const files = [];
let write = false;
let report = false;
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--crate") crates.push(args[++i]);
  else if (args[i] === "--files") files.push(args[++i]);
  else if (args[i] === "--write") write = true;
  else if (args[i] === "--report") report = true;
}
const only = crates.length ? crates.map((c) => (c.startsWith("bisa-") ? `crates/${c}` : `crates/bisa-${c}`)) : null;
if (crates.length) {
  report = true;
  halves.add("rust");
}
if (halves.size === 0) for (const h of Object.keys(HALVES)) if (existsSync(join(out, HALVES[h]))) halves.add(h);

const parsed = [];
for (const h of halves) {
  const file = join(out, HALVES[h]);
  if (!existsSync(file)) {
    console.error(`coverage: ${relative(root, file)} is not there — run scripts/coverage/measure ${h} first`);
    process.exit(2);
  }
  // Relativized per half, since each tool writes its paths from where it ran.
  const own = new Map();
  for (const [path, lines] of parseLcov(readFileSync(file, "utf8"))) {
    const rel = relativize(path, root, BASES[h]);
    if (rel) own.set(rel, lines);
  }
  parsed.push(own);
}
if (parsed.length === 0) {
  console.error("coverage: nothing under target/coverage/ to judge — run scripts/coverage/measure first");
  process.exit(2);
}

const byPath = mergeLcov(parsed);

const exclusions = JSON.parse(readFileSync(join(root, "scripts", "coverage", "exclusions.json"), "utf8")).map((e) => e.path);
let stats = applyExclusions(fileStats(byPath), exclusions);

/** Every desktop model on disk, so one no test loads is bare rather than invisible. */
function desktopModels(dir, acc) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) desktopModels(path, acc);
    else if (name.endsWith(".mjs") && !name.endsWith(".test.mjs")) acc.push(relative(root, path).replace(/\\/g, "/"));
  }
  return acc;
}
if (halves.has("desktop")) {
  const onDisk = desktopModels(join(root, "desktop", "src"), []).filter((p) => !exclusions.includes(p));
  stats = completeDesktop(stats, onDisk, (p) => readFileSync(join(root, p), "utf8").split("\n").length);
}

const totals = treeTotals(stats);
const baselinePath = join(root, BASELINE_FILE);
const baseline = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, "utf8")) : {};

/** Which trees this run can speak for: every one it measured, or the crates asked for. */
const judged = only ?? (halves.size === Object.keys(HALVES).length ? null : [...totals.keys()]);
const judgement = judge(totals, baseline, judged);
process.stdout.write(formatReport(judgement, { totals, files, verdicts: !report }));

if (files.length) {
  for (const tree of files) {
    const t = totals.get(tree);
    if (!t) continue;
    for (const f of t.files) {
      if (f.unmeasured || f.pct === 100) continue;
      const lines = byPath.get(f.path);
      if (lines) process.stdout.write(`    ${f.path}: bare ${bareRanges(lines).join(", ")}\n`);
    }
  }
}

mkdirSync(out, { recursive: true });
writeFileSync(
  join(out, "summary.json"),
  `${JSON.stringify(
    {
      halves: [...halves],
      trees: Object.fromEntries([...totals].map(([tree, t]) => [tree, { found: t.found, hit: t.hit, pct: t.pct, files: t.files.map((f) => ({ path: f.path, found: f.found, hit: f.hit, pct: f.pct, unmeasured: f.unmeasured })) }])),
      judgement: report ? null : judgement,
    },
    null,
    2
  )}\n`
);

if (write) {
  if (report) {
    console.error("coverage: --write takes a judged run, not a crate report");
    process.exit(2);
  }
  const next = nextBaseline(baseline, totals, judged);
  writeFileSync(baselinePath, `${JSON.stringify(next, null, 2)}\n`);
  console.log(`coverage: ${BASELINE_FILE} written — ${Object.keys(next).length} trees`);
  process.exit(0);
}
if (!report && !judgement.ok) process.exit(1);
