import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { cardChips, cardTitle, renameBody, renamedWords, renames, stateLabel, stateWord } from "./workstreamCardModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CHIP = readFileSync(join(HERE, "../../ui/Chip.tsx"), "utf8");
const TONES = [...CHIP.match(/export type Tone = ([^;]+);/)[1].matchAll(/"([a-z]+)"/g)].map((m) => m[1]);

const ws = (extra = {}) => ({ id: "01HWS", project: "p", name: null, kind: { kind: "worktree", branch: "work/x", base: "main" }, state: { state: "open" }, ...extra });
const st = (extra = {}) => ({
  workstream: "01HWS",
  project: "p",
  kind: { kind: "worktree", branch: "work/x", base: "main" },
  name: null,
  publish: "gated",
  default_branch: "main",
  exists: true,
  git: true,
  branch: "work/x",
  base: "main",
  ahead_of_base: 0,
  behind_base: 0,
  upstream: "origin/work/x",
  ahead: 0,
  behind: 0,
  staged: 0,
  unstaged: 0,
  untracked: 0,
  conflicted: 0,
  clean: true,
  in_progress: null,
  pr: null,
  running_agents: 0,
  state: { state: "open" },
  ...extra,
});

test("the title is the label, else the branch, else the primary's live branch, else the copy's tail", () => {
  assert.equal(cardTitle(ws({ name: "Dark mode" }), st()), "Dark mode");
  assert.equal(cardTitle(ws(), st()), "work/x");
  assert.equal(cardTitle(ws({ kind: { kind: "primary" } }), st({ branch: "main" })), "main");
  assert.equal(cardTitle(ws({ kind: { kind: "primary" } }), null), "primary");
  assert.equal(cardTitle(ws({ kind: { kind: "copy" }, id: "01HABCDEF" }), null), "copy · ABCDEF");
});

test("the title is one rule for every surface: a blank label is no label, and the rail, the Board and the footer ask this model", () => {
  assert.equal(cardTitle(ws({ name: "  Dark mode  " }), st()), "Dark mode", "trimmed");
  assert.equal(cardTitle(ws({ name: "   " }), st()), "work/x", "a blank label gives the branch its title back");
  assert.equal(cardTitle(ws({ name: "" }), null), "work/x");
  assert.equal(cardTitle(ws({ kind: { kind: "worktree", branch: "work/x", base: "main" } }), st({ branch: "elsewhere" })), "work/x", "a branch beside the primary is titled by the branch its record stands on");
  assert.equal(cardTitle(ws({ kind: { kind: "copy" }, id: "01HABCDEF" }), st({ branch: null })), "copy · ABCDEF");
  assert.equal(cardTitle(null, null), "copy · ", "nothing read yet is nothing to throw on");
  for (const rel of ["../_workbench/projectRailModel.mjs", "../_board/boardModel.mjs", "../../shell/footerSessionsModel.mjs"]) {
    const source = readFileSync(join(HERE, rel), "utf8");
    assert.ok(/import \{[^}]*\bcardTitle\b[^}]*\} from "[^"]*workstreamCardModel\.mjs"/.test(source), `${rel} reads the one rule`);
    assert.ok(!/copy · \$\{/.test(source), `${rel} spells no title of its own`);
  }
});

test("a rename is one rule: the name trimmed, null for an empty one, nothing written when nothing changed", () => {
  assert.deepEqual(renameBody("  Dark mode "), { name: "Dark mode" });
  assert.deepEqual(renameBody(""), { name: null });
  assert.deepEqual(renameBody("   "), { name: null }, "blanks are no name: the branch titles it again");
  assert.deepEqual(renameBody(null), { name: null });
  assert.equal(renames(null, ""), false, "no name, nothing typed");
  assert.equal(renames(null, "   "), false);
  assert.equal(renames("Dark mode", " Dark mode "), false, "the same name with air around it");
  assert.equal(renames("Dark mode", "Dark"), true);
  assert.equal(renames("Dark mode", ""), true, "cleared");
  assert.equal(renames(null, "Dark"), true);
  assert.equal(renamedWords({ name: "Dark" }), "Workstream renamed.");
  assert.equal(renamedWords({ name: null }), "Workstream named by its branch again.");
  // The three doors of a rename hand the node this body and no other.
  for (const rel of ["./RenameWorkstream.tsx", "../_workbench/ProjectRail.tsx"]) {
    const source = readFileSync(join(HERE, rel), "utf8");
    assert.ok(source.includes("renameBody(value)"), `${rel} builds the body through the model`);
    assert.ok(!/patchWorkstream\([^)]*\{\s*name:/.test(source), `${rel} spells no rename body of its own`);
  }
});

test("a state is said in the catalog's word, a state this build does not know as the node spelled it", () => {
  assert.deepEqual(["open", "dirty", "committed", "pushed", "pr_open", "merged", "closed"].map(stateWord), ["open", "dirty", "committed", "pushed", "PR open", "merged", "closed"]);
  assert.equal(stateWord("under_review"), "under review");
  assert.equal(stateWord(null), "");
  assert.equal(stateLabel({ state: "committed" }), "committed");
  assert.equal(stateLabel(null), "");
  const chip = cardChips(ws({ state: { state: "committed" } }), null)[0];
  assert.deepEqual([chip.id, chip.text, chip.tone, chip.title], ["state", "committed", "warn", "the workstream is committed"]);
  const upstream = cardChips(ws(), st({ ahead: 2, behind: 1 })).find((c) => c.id === "upstream");
  assert.deepEqual([upstream.text, upstream.title], ["↑2 ↓1", "against origin/work/x"]);
});

test("a clean, quiet, pushed checkout says almost nothing; every fact is one chip, once, in order", () => {
  assert.deepEqual(cardChips(ws(), st()), [], "open, clean, up to date: nothing to say");
  const chips = cardChips(
    ws({ state: { state: "pr_open", number: 12, url: "https://github.com/o/r/pull/12" } }),
    st({
      state: { state: "pr_open", number: 12, url: "https://github.com/o/r/pull/12" },
      pr: { number: 12, url: "https://github.com/o/r/pull/12" },
      running_agents: 2,
      ahead_of_base: 3,
      behind_base: 1,
      ahead: 1,
      behind: 2,
      staged: 1,
      unstaged: 2,
      untracked: 4,
      conflicted: 1,
      in_progress: "merge",
      clean: false,
    }),
  );
  assert.deepEqual(
    chips.map((c) => c.id),
    ["state", "pr", "in_progress", "ahead_base", "behind_base", "upstream", "staged", "modified", "untracked", "conflicts"],
  );
  assert.deepEqual(chips.map((c) => c.text), ["PR open", "#12", "merge in progress", "+3", "−1", "↑1 ↓2", "S 1", "M 2", "? 4", "1 conflict"]);
  assert.equal(chips.find((c) => c.id === "pr").href, "https://github.com/o/r/pull/12");
  for (const c of chips) {
    assert.ok(TONES.includes(c.tone), `${c.id}: ${c.tone} is a chip tone`);
    assert.ok(c.title.length > 3, `${c.id} explains itself on hover`);
  }
  assert.equal(new Set(chips.map((c) => c.id)).size, chips.length, "each fact once");
});

test("the pull request comes from the status, else from the record; a branch never pushed says so", () => {
  const fromRecord = cardChips(ws({ state: { state: "pr_open", number: 7, url: "u" } }), null);
  assert.deepEqual(fromRecord.map((c) => c.id), ["state", "pr"], "no status yet: the record still names the PR");
  assert.equal(fromRecord[1].text, "#7");
  const unpushed = cardChips(ws({ state: { state: "committed" } }), st({ upstream: null, ahead_of_base: 2, state: { state: "committed" } }));
  assert.deepEqual(unpushed.map((c) => c.id), ["state", "ahead_base", "no_upstream"]);
  assert.equal(unpushed[0].tone, "warn", "committed but not out: worth a look");
});

test("a missing checkout or a plain folder stops the git facts", () => {
  assert.deepEqual(cardChips(ws(), st({ exists: false, staged: 3 })).map((c) => c.id), ["missing"]);
  // The harnesses standing here are shown live as rows now, not a chip; a plain folder has no git facts.
  assert.deepEqual(cardChips(ws({ kind: { kind: "copy" } }), st({ git: false, running_agents: 1, staged: 3 })).map((c) => c.id), []);
});

test("a merged record keeps its pull request chip — how the work landed is not thrown away — and the state word names an open one", () => {
  const merged = cardChips({ id: "W1", kind: { kind: "worktree", branch: "work/x" }, state: { state: "merged", number: 9, url: "https://github.com/o/r/pull/9" } }, null);
  assert.deepEqual(merged.map((c) => c.id), ["state", "pr"]);
  assert.equal(merged[1].text, "#9");
  assert.equal(merged[1].href, "https://github.com/o/r/pull/9");
  assert.equal(stateLabel({ state: "pr_open", number: 12 }), "PR #12");
  assert.equal(stateLabel({ state: "merged", number: 12 }), "merged");
  assert.equal(stateLabel(null), "");
});
