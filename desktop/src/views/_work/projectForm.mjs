/**
 * The project views' arithmetic, with no React and no DOM in it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `ui/fileTreeModel.mjs`:
 * `node --test` imports the real module rather than a transcription of it, and
 * there is no jsdom in this repo to render a component into. What can actually
 * be *wrong* here is data — which slug a path suggests, which input a server
 * refusal is about, what the import counts add up to in a sentence — so all of
 * it lives here and the component is paint.
 *
 * # Provenance and placement are two questions
 *
 * The wire has five `kind`s. The dialog offers three ways a project comes into
 * existence — create, clone, import — and *import* then asks a second question
 * the other two do not have: whether the folder is copied in (`import`) or left
 * where it is (`adopt`). Those are one segment and one placement here, and
 * {@link bodyKind} is the single place that folds them back into a wire `kind`.
 * Two controls mapping onto three wire values is exactly the kind of thing that
 * drifts if each render decides it again.
 *
 * `link` is not among them, and neither is a goal. Attaching — a project that
 * already exists made visible to a goal — creates nothing and writes nothing on
 * disk, so it is not a way a project arrives; it is `AttachGoalDialog`'s
 * (`attachGoalModel.mjs`), opened from the project's About and the rail's menu.
 * This dialog asks nothing about goals at all: a project made here is the
 * workspace's, unless the door it was opened from stands in a goal, which is
 * then attached in the same call and said in one line, never as a control.
 */

import { t } from "../../i18n/l10n.mjs";

/** The three ways a project comes into existence, as the dialog offers them. */
export const PROVENANCE = ["new", "clone", "import"];

/** Where an imported folder ends up living, in the order the dialog offers them: linked in place (the default), then copied in. */
export const PLACEMENT = ["link", "copy"];

/**
 * The wire `kind` for a provenance and (for `import`) a placement.
 *
 * "Copy it in" is `import` — a managed root the goal owns. "Link it in
 * place" is `adopt`, the operation that has always been there and still writes
 * nothing into the folder. The default is copy, because "import this folder"
 * most often means the files are the work.
 */
export function bodyKind(provenance, placement) {
  if (provenance !== "import") return provenance;
  return placement === "link" ? "adopt" : "import";
}

/**
 * The slug a person has not typed yet, derived from the **name** they did
 *: the mirror of the core's `slug_for_goal` — lowercase, every run
 * of anything but `a-z0-9` becomes one `-`, never a leading one, cut at the
 * core's 64 and trimmed. What comes out is what `validate_slug` accepts, so a
 * suggestion is never something the server refuses.
 * @param {string | null | undefined} name
 */
export function slugFromName(name) {
  const out = (name ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .slice(0, 64)
    .replace(/[-_]+$/, "");
  return out;
}

/**
 * The name a clone or an import starts with: the tail of the URL or path,
 * as written (`Storefront.git` → `Storefront`), for the person to keep or
 * change; the slug then follows the name.
 * @param {string | null | undefined} from
 */
export function nameFromPath(from) {
  return (
    (from ?? "")
      .replace(/\.git$/, "")
      .replace(/[/\\]+$/, "")
      .split(/[/\\:]/)
      .filter(Boolean)
      .pop() ?? ""
  );
}

/**
 * The ways in the dialog offers, by how it was opened: creating offers all
 * three; importing (the rail's import button) offers the two that
 * bring something that already exists, Import first.
 * @param {"create" | "import"} mode
 */
export function provenancesFor(mode) {
  return mode === "import" ? ["import", "clone"] : [...PROVENANCE];
}

/**
 * The one consequence of *no goal* a person cannot guess from the field: a
 * project whose publishing is **gated** asks its goal before a push or a
 * pull request leaves the machine, so with no goal attached there is nobody
 * to ask and the push is refused by name (`publish_no_goal`). Said under
 * the Publishing field, and only then — `null` otherwise. A goal is never
 * required, and the dialog never asks for one: a project belongs to the
 * workspace, and attaching it is one record, from its About tab or the rail's
 * menu, later.
 * @param {"auto" | "gated" | "manual" | string} publish
 * @param {boolean} hasGoal the door the dialog was opened from fixed a goal
 */
export function publishCaveat(publish, hasGoal) {
  if (publish !== "gated" || hasGoal) return null;
  return t("work-project-form-gated-asks-goal-before-publishing-no");
}

/**
 * The input each of the node's refusals of a creation is about, by the
 * refusal's **id** — the message it travels as (`ErrorBody.text.id`,
 * `ApiError.refusal`), the same whatever language its sentence is said in.
 * The sentence is never read: it is this window's language, and a word
 * matched in English would place nothing in any other.
 *
 * The node validates a slug and a source path deliberately — both are
 * path-traversal boundaries — so *invalid project slug* is not a mishap to be
 * toasted away. It is an answer about one input and belongs beside it.
 *
 * `import` and `adopt` refuse a folder through one function on the node
 * (`resolve_source_path`), the verb an argument of the message, so one id
 * answers for both. What the table leaves out is the form's: an assignee
 * nobody has, a goal that is gone, git's own words for a clone that failed.
 */
export const FIELD_OF_REFUSAL = Object.freeze({
  // The slug: its shape, a name `projects/` keeps for itself, one already taken.
  "error-core-invalid-slug": "slug",
  "error-store-invalid-reserved-name-under-projects": "slug",
  "error-store-invalid-project-named-already-exists": "slug",
  "error-store-invalid-project-name-must-not-be-blank": "name",
  // A clone with nothing to clone — refused by the route, and by the engine for any other caller.
  "error-node-projects-clone-needs-url": "url",
  "error-engine-invalid-clone-needs-url": "url",
  // The folder an import copies or a link adopts: the four rules a source passes…
  "error-node-projects-source-needs-absolute-path": "path",
  "error-node-projects-source-cannot": "path",
  "error-node-projects-source-not-directory": "path",
  "error-node-projects-source-inside-workspace": "path",
  // …a link with no folder, a folder another project already is, holds or sits in, one that is gone by the time it is read…
  "error-engine-invalid-adopting-needs-path-folder-adopts": "path",
  "error-store-invalid-adopted-project-needs-path-folder-adopts": "path",
  "error-store-invalid-folder-project-folder-one-project-s": "path",
  "error-engine-invalid-project-points-which-not-directory": "path",
  // …and what an import refuses to copy: over what is there, or past its limits.
  "error-engine-invalid-already-exists-refusing-import-into-rather-than": "path",
  "error-engine-invalid-refusing-import-holds-more-than-files-folders": "path",
  "error-engine-invalid-refusing-import-holds-more-than-import-limit": "path",
});

/**
 * Put the node's refusal on the input it is about; one the form knows no
 * input for — or that carries no id — is the form's own.
 * @param {string | null | undefined} refusal the refusal's message id
 * @returns {"slug" | "name" | "url" | "path" | "form"}
 */
export function fieldFor(refusal) {
  return (refusal && FIELD_OF_REFUSAL[refusal]) || "form";
}

/**
 * The inputs the dialog shows for the way in: the name and the slug for
 * every way, a clone its URL, an import its folder. The form itself always
 * has a place for a word.
 * @param {"new" | "clone" | "import"} provenance
 * @returns {("slug" | "name" | "url" | "path" | "form")[]}
 */
export function shownFields(provenance) {
  return ["name", "slug", ...(provenance === "clone" ? ["url"] : []), ...(provenance === "import" ? ["path"] : []), "form"];
}

/**
 * Where the node's refusal is said, as the dialog's errors: beside the input
 * it is about (`fieldFor`, by the refusal's id) **when the form shows that
 * input**, else on the form — never on an input that is not on screen, where
 * nobody would read it. A refusal about nothing the form holds — an assignee
 * nobody has, a goal that is gone — is the form's, in the node's own
 * sentence. Only an answer about an input (a 4xx) is placed; anything else
 * is the node failing, and the form's too.
 * @param {{message: string, refusal?: string | null, status?: number | null}} failure the node's sentence, the id it travels as, and the answer's status when there was an answer
 * @param {"new" | "clone" | "import"} provenance the way in the form shows
 * @returns {Partial<Record<"slug" | "name" | "url" | "path" | "form", string>>}
 */
export function refusalPlace(failure, provenance) {
  const said = String(failure?.message ?? "");
  const status = failure?.status;
  const aboutAnInput = typeof status === "number" && status >= 400 && status < 500;
  const field = aboutAnInput ? fieldFor(failure?.refusal) : "form";
  return { [shownFields(provenance).includes(field) ? field : "form"]: said };
}

/**
 * The commit button's words — what is about to happen, not *Create* for
 * everything: the two import placements do materially different things, one
 * copies a tree onto the disk and the other writes nothing at all, and the
 * button is the last thing read before either.
 * @param {"new" | "clone" | "import"} provenance @param {"copy" | "link"} placement
 */
export function primaryLabel(provenance, placement) {
  if (provenance !== "import") return t("work-agent-editor-create");
  return placement === "copy" ? t("work-new-project-dialog-copy") : t("work-new-project-dialog-link-place");
}

/**
 * What the dialog says while the node works — the commit button's own verb,
 * going on: a clone, a copy and a link take very different times, and
 * *Working…* says none of them.
 * @param {"new" | "clone" | "import"} provenance @param {"copy" | "link"} placement
 */
export function busyLabel(provenance, placement) {
  if (provenance === "clone") return t("work-new-project-dialog-cloning");
  if (provenance === "import") return placement === "copy" ? t("work-new-project-dialog-copying") : t("work-new-project-dialog-linking");
  return t("work-new-project-dialog-creating");
}

/**
 * The account select's first row: no pin — the host's own choice, named
 * when the node suggested one.
 * @param {string | null | undefined} suggested the login the node would use
 */
export function hostChoiceWords(suggested) {
  return suggested ? t("work-project-form-host-own-choice-suggested", { suggested }) : t("work-project-form-host-own-choice");
}

/**
 * What an import actually moved, as a sentence — or `null` when there is
 * nothing worth saying.
 *
 * The skipped counts are the point. A copy that silently dropped every symlink
 * looks like a faithful import until a build fails a week later, so a tree that
 * leaned on links is told at the moment it happens.
 */
/** The words of the workspace's `git.committer`, in the registry's order (`CommitterPolicy`). */
export const COMMITTER_POLICIES = Object.freeze(["inherit", "pin", "ask"]);

/**
 * How the dialog treats the repository's git config at creation — the
 * repository's local layer, written only when something is typed. Offered
 * for every way a project arrives (a linked-in-place import included: the
 * engine writes an adopted repository's local config when the request asks,
 * and only then); nothing this dialog does creates nothing, so there is no
 * hidden state.
 *
 * `note`: nothing is asked — one footnote says who the repository will commit
 * as (the global pair, inherited or pinned by the workspace's `git.committer`)
 * with *Change…* to type local values. `open`: the identity fields are open
 * from the start — the policy is `ask`, or no identity resolves globally, so
 * the repository is born with an author rather than asking a moment later.
 * The value a person types is carried in both states — gating the payload on
 * the global identity is how a typed name and email were once silently dropped.
 * @param {"inherit" | "pin" | "ask"} policy the resolved `git.committer`
 * @param {boolean} globalResolved whether a global identity resolves
 * @returns {"note" | "open"}
 */
export function gitConfigSection(policy, globalResolved) {
  if (policy === "ask" || !globalResolved) return "open";
  return "note";
}

/**
 * The footnote under the form in the `note` state: who the repository will
 * commit as, and whether the pair is inherited or pinned into it.
 * @param {"inherit" | "pin" | "ask"} policy
 * @param {string} ident the global pair, formatted
 * @returns {string}
 */
export function committerNote(policy, ident) {
  return policy === "pin" ? t("work-project-form-commits-global-git-config-pinned-into", { ident }) : t("work-project-form-commits-global-git-config", { ident });
}

/**
 * The `git_config` a creation body carries: every key the person set, or
 * nothing. Only `set` travels — nothing is local before the repository exists,
 * so there is nothing to unset. Independent of what resolves globally and of
 * which state the section is shown in: typed behind *Change…* travels too.
 * @param {{set: Record<string, string>, unset: string[]}} write
 * @returns {Record<string, string> | null}
 */
export function creationGitConfig(write) {
  const set = write?.set ?? {};
  return Object.keys(set).length > 0 ? { ...set } : null;
}

/**
 * The body of `POST /projects` (`NewProjectBody`) for what the dialog holds:
 * the kind's own keys **by name** — a clone's URL, an import's or an
 * adoption's folder — beside what every kind carries. A `git_config` travels
 * only when the person set something.
 * @param {{provenance: "new" | "clone" | "import", placement: "copy" | "link", slug: string, name: string, publish: string, tags: readonly string[], gitConfig: Record<string, string> | null, url: string, path: string}} form
 */
export function creationBody(form) {
  const common = {
    slug: form.slug.trim(),
    name: form.name.trim(),
    publish: form.publish,
    tags: [...form.tags],
    ...(form.gitConfig ? { git_config: { ...form.gitConfig } } : {}),
  };
  switch (form.provenance) {
    case "clone":
      return { ...common, url: form.url.trim(), kind: "clone" };
    case "import":
      // One control decides `import` vs `adopt`; see `bodyKind`.
      return { ...common, path: form.path.trim(), kind: bodyKind("import", form.placement) };
    default:
      return { ...common, kind: "new" };
  }
}

export function importSummary(stats) {
  if (!stats) return null;
  const parts = [t("work-project-form-files-copied-in", { files: stats.files })];
  if (stats.skipped_symlinks > 0) {
    parts.push(
      t("work-project-form-symlink-symlinks-skipped", { skipped_symlinks: stats.skipped_symlinks }),
    );
  }
  if (stats.skipped_special > 0) {
    parts.push(t("work-project-form-special-file-files-skipped", { skipped_special: stats.skipped_special }));
  }
  return `${parts.join(", ")}.`;
}

