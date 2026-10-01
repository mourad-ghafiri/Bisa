/**
 * The right panel's vocabulary — the one place that says what may occupy it,
 * how the occupants are grouped on the rail, and what is available for a
 * given root. `rightPanelStore.ts` keeps the state, `OccupantRail.tsx` and
 * `RightPanel.tsx` paint; the facts are here, with tests, so a wrong answer is
 * a failing test and not a missing icon.
 *
 * One kind of occupant, one kind of door. The panel's column shows one
 * occupant at a time — Files · Git · Workstreams · Agent · About — and every
 * one is an icon on a vertical **rail** at the panel's right edge, in two
 * groups: what you look at while editing (Files · Git) and what you open on
 * purpose (Workstreams · Agent · About). A rail icon is
 * under one press rule (open · switch · close). The rail never leaves the
 * screen and never loses a tab: an occupant a root cannot show is drawn
 * muted, and pressing it asks the workbench the way any door does. Pressing
 * the showing occupant closes the panel's column and the rail stays. Any
 * occupant is remembered per root.
 *
 * Workstreams is one occupant for the whole checkout (ide/07, ide/08): what
 * it is called, its sessions, closing it (its path only copied, never
 * printed) — and, on a branch beside
 * the primary, under those, the branch's way to its base: the pull
 * request, its checks, its reviews, the merge. One occupant because the
 * lifecycle is the branch's story and a person who opens a workstream came
 * for it; the one act under the current step is the only button it shows, so
 * nobody meets a merge button before its time. It lists no other checkout —
 * that is the project rail's, where every workstream already has a row — and
 * opens a new one from its menu through the workbench's one dialog, as the
 * rail's `+` does.
 */

/** The rail, top to bottom, in its two groups — the order alone; the rail draws them at one spacing, with neither a gap nor a rule between. */
import { PATCH_VIEWS } from "../_work/patchViewModel.mjs";
import { pressRailTab } from "../../ui/iconRailModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export const RAIL_GROUPS = Object.freeze([
  Object.freeze(["files", "git"]),
  Object.freeze(["workstreams", "agents", "about"]),
]);
/** Every occupant, in the rail's order. */
export const OCCUPANTS = Object.freeze(RAIL_GROUPS.flat());

export const OCCUPANT_LABEL = Object.freeze({
  files: t("workbench-right-panel-files"),
  git: t("workbench-right-panel-git"),
  agents: t("workbench-agent-pane-agent"),
  about: t("workbench-right-panel-about"),
  workstreams: t("workbench-right-panel-workstreams"),
});

/**
 * Whose scrollport the panel body is for each occupant (ide/03 §The
 * explorer). An occupant that holds its own scrollport — the explorer's
 * `VirtualList`, the Agent pane's conversation — gets the body as a flex
 * column that clips, so it fills the column by flex and scrolls inside its
 * own box however many rows a folder opens; an occupant that scrolls as one
 * — Git › Changes under its sticky composer, and every `PanelBody` — gets
 * the body as the scrollport. One fact, read once, never a class typed twice.
 */
export const OCCUPANT_SCROLL = Object.freeze({
  files: "own",
  git: "panel",
  agents: "own",
  about: "panel",
  workstreams: "panel",
});

/** `"own"` when the occupant scrolls itself, `"panel"` when the body scrolls as one. */
export function occupantScroll(tab) {
  return OCCUPANT_SCROLL[tab] ?? "panel";
}

/** The keymap command that shows each occupant (`shell/keymapModel.mjs`). */
export const OCCUPANT_COMMAND = Object.freeze({
  files: "panel_files",
  git: "git_panel",
  agents: "panel_agents",
  about: "panel_about",
  workstreams: "panel_workstreams",
});

/**
 * The Git tab's views, in strip order — what a person does to the tree:
 * Changes, Branches, History and the Stashes (the repository's list, parked
 * work a person reaches for on purpose). Nothing here is a setting or a
 * fact about the repository: the connection, the remotes, who commits and
 * the project's own settings are About's Settings view (`ABOUT_VIEWS`).
 */
export const GIT_VIEWS = Object.freeze(["changes", "branches", "history", "stashes"]);
export const GIT_VIEW_LABEL = Object.freeze({
  changes: t("workbench-right-panel-changes"),
  branches: t("workbench-right-panel-branches"),
  history: t("workbench-right-panel-history"),
  stashes: t("workbench-right-panel-stashes"),
});

/**
 * About's views: the **project** — its identity and relations, on every
 * checkout; this **checkout** — the repository as it reaches it (the
 * connection, the facts, the remotes, who commits); and the project's
 * **settings** — what is saved on the project itself (publishing, git,
 * workstreams, workstream scripts, editor and terminal). Two short views to
 * set things up, each with one Save at its top, so the Git tab is only what a
 * person does to the tree.
 */
export const ABOUT_VIEWS = Object.freeze(["project", "checkout", "settings"]);
export const ABOUT_VIEW_LABEL = Object.freeze({
  project: t("workbench-right-panel-project"),
  checkout: t("workbench-right-panel-checkout"),
  settings: t("workbench-right-panel-settings"),
});

/**
 * The Changes view's two layouts: the changed files as folders (the default —
 * a folder row acts on everything under it) or as a flat list of paths. A
 * layout, not a view: it is remembered once for every checkout beside the
 * occupants' views, and it is no occupant's door.
 */
export const CHANGES_LAYOUTS = Object.freeze(["tree", "list"]);
export const CHANGES_LAYOUT_LABEL = Object.freeze({
  tree: t("workbench-right-panel-tree"),
  list: t("workbench-right-panel-list"),
});

/**
 * The Changes view's filter (ide/04): which changed files the tree draws —
 * every one, or one standing. Six words, *all* first and the default;
 * *unstaged* and *untracked* never overlap (git sets both on a new file,
 * the filter keeps them apart), *tracked* is everything git knows, and
 * *modified* is a content change — the letter `M` on either side, so an
 * added, deleted or renamed file is not one. The predicate is
 * `views/_work/changesFilterModel.mjs`'s; the word is remembered here,
 * once for every checkout, beside the layout. It narrows the rows and
 * never the verbs: *Stage all* means all whatever is on screen.
 */
export const CHANGE_FILTERS = Object.freeze(["all", "conflicted", "staged", "unstaged", "tracked", "untracked", "modified"]);
export const CHANGE_FILTER_LABEL = Object.freeze({
  all: t("workbench-right-panel-all"),
  conflicted: t("workbench-right-panel-conflicted"),
  staged: t("workbench-right-panel-staged"),
  unstaged: t("workbench-right-panel-unstaged"),
  tracked: t("workbench-right-panel-tracked"),
  untracked: t("workbench-right-panel-untracked"),
  modified: t("workbench-right-panel-modified"),
});

/**
 * The Remotes section's two layouts (ide/04 §Remotes): a remote's branches
 * as folders in the explorer's shape, or as a flat list newest first — the
 * same two words as the Changes layout, remembered on their own.
 */
export const REMOTE_LAYOUTS = Object.freeze(["tree", "list"]);
export const REMOTE_LAYOUT_LABEL = Object.freeze({
  tree: t("workbench-right-panel-tree"),
  list: t("workbench-right-panel-list"),
});

/**
 * Every remembered choice under `bisa.ide.views`, each with what it may
 * be: the two occupants' views, the Changes layout, the Remotes layout and
 * the view a changed file's patch opens in (`patchViewModel.mjs`).
 */
const VIEW_CHOICES = Object.freeze({ git: GIT_VIEWS, about: ABOUT_VIEWS, changes: CHANGES_LAYOUTS, changesFilter: CHANGE_FILTERS, remotes: REMOTE_LAYOUTS, patch: PATCH_VIEWS });
/** What each choice opens on. */
export const DEFAULT_VIEWS = Object.freeze({ git: "changes", about: "project", changes: "tree", changesFilter: "all", remotes: "tree", patch: "hunks" });
export function isOccupant(v) {
  return typeof v === "string" && OCCUPANTS.includes(v);
}

/** Whether `view` is one of the choice's — an occupant's views, or the Changes layouts. */
export function isViewOf(choice, view) {
  return typeof view === "string" && (VIEW_CHOICES[choice] ?? []).includes(view);
}

/**
 * The remembered choices, read back from storage: a value that is not one of
 * that choice's — or no object at all — is the default. Never a throw, never
 * a ladder from an older word.
 * @returns {{git: string, about: string, changes: string, changesFilter: string, remotes: string}}
 */
export function parseViews(value) {
  const raw = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  const out = {};
  for (const choice of Object.keys(VIEW_CHOICES)) {
    const v = raw[choice];
    out[choice] = isViewOf(choice, v) ? v : DEFAULT_VIEWS[choice];
  }
  return out;
}

/**
 * What a root can show. Files and About answer for every root — a goal's
 * folder, a work item's, a checkout. Git, Agents and Workstreams are a
 * project's, so only a workstream with a project has them — the primary
 * included: whether a checkout has a branch to take to a base is the
 * Workstreams panel's to say, under its header, not the rail's.
 *
 * `centre` is what the middle of the screen is showing: when it already is
 * the conversation (Agent Mode, ide/09), the Agent occupant is not offered
 * — the same thread twice on one screen would be two trays and two
 * composers for one draft.
 * @param {{scope: string, hasProject: boolean, centre?: "documents" | "conversation"}} root
 */
export function availableOccupants({ scope, hasProject, centre = "documents" }) {
  const projectBound = scope === "workstream" && hasProject === true;
  return OCCUPANTS.filter((o) => (o === "files" || o === "about" || projectBound) && !(o === "agents" && centre === "conversation"));
}

/**
 * What About shows for a root. About is a project's identity and relations
 * on **every** checkout of it, the primary or another:
 * a checkout's own facts — branch, changes, pull request — are the
 * Workstreams occupant's. A work item's folder shows the item; a goal's, a
 * note pointing at the goal; a checkout still resolving its project, nothing
 * yet.
 * @param {{scope: string, hasProject: boolean}} root
 * @returns {"project" | "work_item" | "goal" | "none"}
 */
export function aboutBody({ scope, hasProject }) {
  if (scope === "work_item") return "work_item";
  if (scope === "goal") return "goal";
  return hasProject === true ? "project" : "none";
}

/** The occupant to show: what was asked for when it is available, else Files. */
export function resolveOccupant(wanted, available) {
  return isOccupant(wanted) && available.includes(wanted) ? wanted : "files";
}

/**
 * The rail: the groups whole, in the model's order, whatever the root can
 * show — a tab that is not available is drawn muted rather than dropped, so
 * the rail reads the same on every root and a person always knows where a
 * thing is.
 * @returns {string[][]}
 */
export function railGroups() {
  return RAIL_GROUPS.map((g) => [...g]);
}

/**
 * A rail icon pressed: closed → open on it; open elsewhere → switch; open on
 * it already → close the column. One control, one meaning — "is this thing
 * showing". The rail itself stays. The rule is every rail's
 * (`ui/iconRailModel.mjs`).
 * @param {{open: boolean, tab: string}} state
 */
export function pressOccupant(state, target) {
  return pressRailTab(state, target);
}

/**
 * The root changed: what the panel shows there. The same root is left alone
 * (a re-render is not a navigation); another root recalls the occupant it
 * last had, or Files — never another root's.
 * @param {{tab: string, root: string | null, byRoot: Record<string, string>}} state
 * @param {string} nextRoot
 */
export function recallFor({ tab, root, byRoot }, nextRoot) {
  if (root === nextRoot) return { tab, root };
  return { tab: byRoot[nextRoot] ?? "files", root: nextRoot };
}

/**
 * The remembered per-root occupants, read back from storage. A word that is
 * not an occupant is dropped rather than mapped — there is no ladder from an
 * older vocabulary; a root that remembered nothing usable opens on
 * Files. Remembering any occupant is safe now that the rail always shows.
 */
export function parseRememberedTabs(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const out = {};
  for (const [root, tab] of Object.entries(value)) if (isOccupant(tab)) out[root] = tab;
  return out;
}
