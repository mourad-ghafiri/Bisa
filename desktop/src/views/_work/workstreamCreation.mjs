/**
 * Opening a workstream, as facts: whether a project can take one right now
 * (and, when not, why and what would fix it); the **sources** a workstream
 * can start from and the wire body each makes (ide/07 §Where a workstream
 * starts); the branch name the engine will settle on, previewed before the
 * click; which branches other checkouts already stand on; and the toast once
 * it is open. The dialog draws these, every door into it inherits the same
 * guard, and `node --test` checks them.
 *
 * The name rules are ports of `bisa_core::branch_name_for`,
 * `sanitize_ref_component` (`crates/bisa-core/src/workstream.rs`),
 * `typed_branch_name` and `branch_for_ulid`
 * (`crates/bisa-engine/src/projects.rs`): forty characters of the label,
 * then the six-character tail of the workstream's id — which only the engine
 * knows, so the preview shows dots for it. The test vectors are the Rust
 * tests' own.
 */

import { refNameProblem } from "./commitActionsModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** How much of the label rides in a derived branch name. */
export const BRANCH_SLUG_CHARS = 40;
/** Stands in for the id tail the engine appends. */
export const TAIL_PLACEHOLDER = "······";

/**
 * The sources, in the order the dialog offers them, each with the sentence
 * under its fields.
 */
export const SOURCES = Object.freeze([
  { id: "new", label: tr("work-new-branch-dialog-new-branch"), hint: tr("work-workstream-creation-branch-own-one-derived-from-label") },
  { id: "branch", label: tr("work-new-workstream-dialog-branch"), hint: tr("work-workstream-creation-branch-repository-already-has-checked-out") },
  { id: "remote", label: tr("work-new-workstream-dialog-remote-branch"), hint: tr("work-workstream-creation-branch-remote-fetched-then-local-branch") },
  { id: "tag", label: tr("work-new-workstream-dialog-tag"), hint: tr("work-workstream-creation-new-branch-tag-one-repository-has") },
  { id: "pr", label: tr("work-new-workstream-dialog-pull-request"), hint: tr("work-workstream-creation-open-pull-request-code-host-branch") },
]);

/**
 * Whether a workstream can be opened on this project now.
 * @param {{exists: boolean} | null | undefined} project the project row (`exists`: its root is on disk)
 * @param {{git: boolean, exists: boolean, head?: string | null} | null | undefined} gitStatus the primary's git status, `null` while loading
 */
export function canOpenWorkstream(project, gitStatus) {
  if (project && project.exists === false) {
    return { ok: false, reason: "missing", remedy: tr("work-workstream-creation-folder-not-disk-put-back-adopt") };
  }
  if (!gitStatus) return { ok: false, reason: "loading", remedy: null };
  if (!gitStatus.git) return { ok: true, mode: "copy" };
  if (!gitStatus.exists) {
    return { ok: false, reason: "missing", remedy: tr("work-workstream-creation-folder-not-disk-put-back-adopt") };
  }
  if (!gitStatus.head) {
    return {
      ok: false,
      reason: "unborn",
      remedy: tr("work-workstream-creation-repository-has-no-commits-yet-so"),
    };
  }
  return { ok: true, mode: "branch" };
}

/** One component of a ref, made safe the way git wants it — never refused. */
export function sanitizeRefComponent(s, fallback) {
  let out = "";
  for (const ch of String(s ?? "")) {
    const c = ch.toLowerCase();
    out += /^[a-z0-9._-]$/.test(c) ? c : "-";
  }
  while (out.includes("..")) out = out.replaceAll("..", ".");
  while (out.includes("--")) out = out.replaceAll("--", "-");
  out = out.replace(/^[-._]+|[-._]+$/g, "");
  while (out.endsWith(".lock")) out = out.slice(0, -".lock".length).replace(/[-._]+$/g, "");
  return out === "" ? fallback : out;
}

/** `work/<slug>` — the engine's rule for a derived branch. */
export function branchNameFor(kind, slug) {
  return `${sanitizeRefComponent(kind, "work")}/${sanitizeRefComponent(slug, "item")}`;
}

/**
 * A branch name a person typed, made safe the way the engine makes it
 * (`typed_branch_name`): `kind/slug` when they typed a slash, `work/<slug>`
 * otherwise. Never refused.
 * @param {string | null | undefined} typed
 */
export function typedBranchName(typed) {
  const t = String(typed ?? "").trim().replace(/^\/+|\/+$/g, "");
  const slash = t.indexOf("/");
  if (slash > 0 && slash < t.length - 1) return branchNameFor(t.slice(0, slash), t.slice(slash + 1));
  return branchNameFor("work", t);
}

/**
 * The branch the engine will stand the checkout on, for a source and what
 * was typed or picked — before the click. `derived` is true when the engine
 * appends the id's tail, which the preview shows as dots.
 * @param {{label?: string, project?: string, source?: SourceFields}} input `project` is the slug the engine uses when the label is empty; `source` absent is a new derived branch
 * @returns {{branch: string, derived: boolean}}
 */
export function previewBranch({ label, project, source }) {
  const s = source ?? { kind: "new" };
  switch (s.kind) {
    case "branch":
      return { branch: String(s.branch ?? ""), derived: false };
    case "remote":
      return { branch: String(s.branch ?? ""), derived: false };
    case "tag": {
      const own = String(s.branch ?? "").trim();
      if (own) return { branch: typedBranchName(own), derived: false };
      const tag = s.newTag ? s.tagName : s.tag;
      return { branch: tag ? branchNameFor("from", tag) : "", derived: false };
    }
    case "pr":
      return { branch: String(s.head ?? ""), derived: false };
    default: {
      const own = String(s.name ?? "").trim();
      if (own) return { branch: typedBranchName(own), derived: false };
      const head = [...(String(label ?? "").trim() || String(project ?? ""))].slice(0, BRANCH_SLUG_CHARS).join("");
      // The placeholder stands for the id tail the engine appends *after* it
      // has made the slug safe, so it is appended after sanitising here too.
      return { branch: `${branchNameFor("work", head)}-${TAIL_PLACEHOLDER}`, derived: true };
    }
  }
}

/**
 * @typedef {{kind: "new", name?: string, start?: string}
 *   | {kind: "branch", branch?: string}
 *   | {kind: "remote", remote?: string, branch?: string}
 *   | {kind: "tag", tag?: string, newTag?: boolean, tagName?: string, tagAt?: string, branch?: string}
 *   | {kind: "pr", pr?: number | null, head?: string}} SourceFields
 *   what the dialog holds for a source: the fields of that source only.
 */

/**
 * The wire `source` for what was typed or picked, or the one problem with
 * it — an empty pick, a tag name git would refuse. A typed branch name is
 * never a problem: the engine makes it safe.
 * @param {SourceFields} f
 * @returns {{source: object, problem: null} | {source: null, problem: string}}
 */
export function sourceBody(f) {
  const clean = (v) => {
    const s = String(v ?? "").trim();
    return s === "" ? null : s;
  };
  switch (f.kind) {
    case "branch": {
      const name = clean(f.branch);
      return name ? { source: { source: "local_branch", name }, problem: null } : { source: null, problem: tr("work-workstream-creation-pick-branch") };
    }
    case "remote": {
      const remote = clean(f.remote);
      const name = clean(f.branch);
      return remote && name ? { source: { source: "remote_branch", remote, name }, problem: null } : { source: null, problem: tr("work-workstream-creation-pick-remote-branch") };
    }
    case "tag": {
      const branch = clean(f.branch);
      if (f.newTag) {
        const name = clean(f.tagName);
        if (!name) return { source: null, problem: tr("work-workstream-creation-name-tag") };
        const bad = refNameProblem(name);
        if (bad) return { source: null, problem: bad };
        const at = clean(f.tagAt);
        if (!at) return { source: null, problem: tr("work-workstream-creation-say-where-tag-goes-branch-tag") };
        return { source: { source: "tag", name, branch, create_at: at }, problem: null };
      }
      const name = clean(f.tag);
      return name ? { source: { source: "tag", name, branch, create_at: null }, problem: null } : { source: null, problem: tr("work-workstream-creation-pick-tag") };
    }
    case "pr": {
      const number = Number(f.pr);
      return Number.isInteger(number) && number > 0
        ? { source: { source: "pull_request", number }, problem: null }
        : { source: null, problem: tr("work-workstream-creation-pick-pull-request") };
    }
    default:
      return { source: { source: "new_branch", name: clean(f.name), start: clean(f.start) }, problem: null };
  }
}

/**
 * Whether the dialog shows the label's field: a copy is listed by it, and a
 * new branch derives its name from it. Every other source stands on a branch
 * that already has a name.
 * @param {boolean} git the project is a repository
 * @param {SourceFields} f
 */
export function labelShown(git, f) {
  return !git || f.kind === "new";
}

/**
 * The body of `POST /projects/{pid}/workstreams` (`NewWorkstreamBody`) for
 * what the dialog holds, or the one problem with it. Keys by name, and only
 * what the person can see: the label while its field shows — a label typed
 * under *New branch* does not ride along once another source is chosen — the
 * source of a repository, and the base when it is not the project's default
 * and the source brings none of its own (a pull request does).
 * @param {{git: boolean, label?: string, fields: SourceFields, base?: string, defaultBranch?: string | null}} form
 * @returns {{body: {label?: string, source?: object, base?: string}, problem: null} | {body: null, problem: string}}
 */
export function openBody({ git, label, fields, base, defaultBranch }) {
  const clean = (v) => String(v ?? "").trim();
  const body = {};
  if (labelShown(git, fields) && clean(label) !== "") body.label = clean(label);
  if (!git) return { body, problem: null };
  const made = sourceBody(fields);
  if (made.problem !== null) return { body: null, problem: made.problem };
  body.source = made.source;
  if (fields.kind !== "pr" && clean(base) !== "" && clean(base) !== clean(defaultBranch)) body.base = clean(base);
  return { body, problem: null };
}

/**
 * The branches other checkouts already stand on — git refuses two worktrees
 * on one branch — each with who: a workstream's name or its branch, and the
 * primary's current branch as *the primary*.
 * @param {readonly {name?: string | null, kind: {kind: string, branch?: string}, state: {state: string}}[]} workstreams
 * @param {string | null | undefined} primaryBranch the branch the primary is on
 * @returns {Map<string, string>} branch → who stands on it
 */
export function takenBranches(workstreams, primaryBranch) {
  const out = new Map();
  if (primaryBranch) out.set(primaryBranch, tr("work-workstream-creation-primary"));
  for (const w of workstreams ?? []) {
    if (w.kind.kind !== "worktree" || w.state.state === "closed" || !w.kind.branch) continue;
    out.set(w.kind.branch, w.name || w.kind.branch);
  }
  return out;
}

/**
 * The fields a door presets — a branch row, a tag row, a ref chip — as the
 * dialog's source fields.
 * @param {SourcePreset | null | undefined} preset
 * @returns {SourceFields}
 */
export function fieldsOf(preset) {
  switch (preset?.kind) {
    case "branch":
      return { kind: "branch", branch: preset.branch };
    case "tag":
      return { kind: "tag", tag: preset.tag, newTag: false };
    case "remote":
      return { kind: "remote", remote: preset.remote, branch: preset.branch };
    case "pr":
      return { kind: "pr", pr: preset.pr };
    default:
      return { kind: "new", name: "", start: preset?.kind === "new" ? preset.start ?? "" : "" };
  }
}

/**
 * @typedef {{kind: "new", start?: string} | {kind: "branch", branch: string} | {kind: "remote", remote: string, branch: string} | {kind: "tag", tag: string} | {kind: "pr", pr: number}} SourcePreset
 */

/**
 * The toast once the workstream is open, naming what it stands on.
 * @param {SourceFields | null} f the source, or `null` for a copy
 * @param {string} branch the branch the engine settled on
 */
export function openWords(f, branch) {
  if (f === null) return tr("work-workstream-creation-copied-into-workstream");
  switch (f.kind) {
    case "remote":
      return tr("work-workstream-creation-opened-tracking", { branch, remote: f.remote, branch2: f.branch });
    case "tag":
      return tr("work-workstream-creation-opened-at-tag", { branch, tag: f.newTag ? f.tagName : f.tag });
    case "pr":
      return tr("work-workstream-creation-opened-from-pull-request-lifecycle-picks", { branch, pr: f.pr });
    default:
      return tr("work-workstream-creation-opened", { branch });
  }
}
