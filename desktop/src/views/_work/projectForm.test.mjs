/**
 * The New Project dialog's decisions, tested where they live.
 *
 * Nothing here renders — there is no jsdom in this repo, which is why the
 * decisions are a module rather than a pile of `useMemo`s. The three that can
 * actually be wrong in a way a person notices: the folder a segment plus a
 * placement adds up to on the wire, the slug a path suggests, and which input
 * a refusal from the node lands on.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { readFileSync } from "node:fs";

import {
  PLACEMENT,
  PROVENANCE,
  bodyKind,
  COMMITTER_POLICIES,
  committerNote,
  creationBody,
  creationGitConfig,
  gitConfigSection,
  nameFromPath,
  slugFromName,
  FIELD_OF_REFUSAL,
  fieldFor,
  publishCaveat,
  importSummary,
  provenancesFor,
  hostChoiceWords,
  primaryLabel,
  refusalPlace,
  shownFields,
  } from "./projectForm.mjs";

test("import defaults to copying the folder in, and can link it in place", () => {
  // The decision this whole feature implements: offer both, copy by default.
  assert.equal(bodyKind("import", "copy"), "import");
  assert.equal(bodyKind("import", "link"), "adopt");
  // The placement question only exists for import; the others ignore it.
  for (const placement of PLACEMENT) {
    assert.equal(bodyKind("new", placement), "new");
    assert.equal(bodyKind("clone", placement), "clone");
  }
  assert.deepEqual([...PROVENANCE], ["new", "clone", "import"]);
});

test("a suggested slug is one the server's allowlist would accept", () => {
  // The allowlist is lowercase ASCII alphanumerics plus - and _, first
  // character alphanumeric. A suggestion the node refuses reads as the app's
  // own idea and is worse than no suggestion at all.
  const legal = /^[a-z0-9][a-z0-9_-]*$/;
  const cases = [
    ["/Users/you/code/Storefront", "storefront"],
    ["/Users/you/code/storefront/", "storefront"],
    ["https://github.com/acme/storefront.git", "storefront"],
    ["git@github.com:acme/store-front.git", "store-front"],
    ["C:\\Users\\you\\My Project", "my-project"],
    ["/srv/  spaced  out  ", "spaced-out"],
    ["/tmp/__leading", "leading"],
    ["/tmp/café", "caf"],
  ];
  for (const [from, expected] of cases) {
    const slug = slugFromName(nameFromPath(from));
    assert.equal(slug, expected, `${from} → ${slug}`);
    assert.match(slug, legal, `${from} produced an illegal slug: ${slug}`);
  }
});

test("a refusal is put on its input by what it is — the id it travels as — never by its words, which are in the reader's language", () => {
  // `import` and `adopt` go through one function on the node, the verb an
  // argument of the message: one id answers for both, on the folder.
  for (const id of ["error-node-projects-source-needs-absolute-path", "error-node-projects-source-cannot", "error-node-projects-source-not-directory", "error-node-projects-source-inside-workspace"]) assert.equal(fieldFor(id), "path", id);
  assert.equal(fieldFor("error-engine-invalid-already-exists-refusing-import-into-rather-than"), "path");
  assert.equal(fieldFor("error-engine-invalid-refusing-import-holds-more-than-import-limit"), "path");
  assert.equal(fieldFor("error-store-invalid-folder-project-folder-one-project-s"), "path", "a folder another project already is");
  assert.equal(fieldFor("error-core-invalid-slug"), "slug");
  assert.equal(fieldFor("error-store-invalid-project-named-already-exists"), "slug");
  assert.equal(fieldFor("error-store-invalid-reserved-name-under-projects"), "slug");
  assert.equal(fieldFor("error-store-invalid-project-name-must-not-be-blank"), "name");
  assert.equal(fieldFor("error-node-projects-clone-needs-url"), "url");
  // A refusal about a project id is attaching's, which this form never does: the form's, whole.
  assert.equal(fieldFor("error-node-projects-not-project-id"), "form");
  assert.equal(fieldFor("error-engine-invalid-agent-not-found"), "form");
  assert.equal(fieldFor(null), "form", "no id: nothing to place by");
  assert.equal(fieldFor(undefined), "form");
  // The words are never read: a sentence handed in where an id goes places nothing, in English as in any language.
  assert.equal(fieldFor("invalid project slug: ../escape"), "form");
  assert.equal(fieldFor("clone needs a url"), "form");
  const model = readFileSync(new URL("./projectForm.mjs", import.meta.url), "utf8");
  const placing = model.slice(model.indexOf("export const FIELD_OF_REFUSAL"), model.indexOf("export function primaryLabel"));
  assert.ok(!/\.includes\("[a-z ]+"\)|toLowerCase\(\)|\.test\(/.test(placing), "nothing in the placing lowercases a message or looks inside one");
});

test("every id the form places is a message the node can say, from a call site on the way a project is made — and the folder's and the slug's refusals are all placed", () => {
  const errors = readFileSync(new URL("../../../../locales/en/errors.ftl", import.meta.url), "utf8");
  const ids = new Set([...errors.matchAll(/^(error-[a-z0-9_-]+) = /gm)].map((m) => m[1]));
  assert.ok(ids.size > 500, `errors.ftl is read: ${ids.size} messages`);
  const crate = (rel) => readFileSync(new URL(`../../../../crates/${rel}`, import.meta.url), "utf8");
  // The creation's path through the three layers, and the two files its refusals are worded in.
  const said = ["bisa-node/src/projects.rs", "bisa-engine/src/projects.rs", "bisa-engine/src/import.rs", "bisa-store/src/projects.rs", "bisa-core/src/error_text.rs"].map(crate).join("\n");
  for (const [id, field] of Object.entries(FIELD_OF_REFUSAL)) {
    assert.ok(ids.has(id), `${id} is a message of errors.ftl`);
    assert.ok(said.includes(`"${id}"`), `${id} is said where a project is made`);
    assert.ok(["slug", "name", "url", "path"].includes(field), `${id} lands on an input the dialog can draw`);
  }
  // Complete where a rule has a family: every refusal of a source folder, and the store's two of a slug.
  for (const id of ids) if (id.startsWith("error-node-projects-source-")) assert.equal(FIELD_OF_REFUSAL[id], "path", `${id} is placed`);
  const slugCheck = crate("bisa-store/src/projects.rs");
  const check = slugCheck.slice(slugCheck.indexOf("fn check_slug_free"), slugCheck.indexOf("pub fn create_project"));
  for (const m of check.matchAll(/"(error-[a-z0-9-]+)"/g)) assert.equal(FIELD_OF_REFUSAL[m[1]], "slug", `${m[1]} is placed`);
});

test("a refusal is said where the form shows it: beside its input when that input is on screen, else on the form — never swallowed", () => {
  assert.deepEqual(shownFields("new"), ["name", "slug", "form"]);
  assert.deepEqual(shownFields("clone"), ["name", "slug", "url", "form"]);
  assert.deepEqual(shownFields("import"), ["name", "slug", "path", "form"]);

  // A destination that already exists refuses an import under the folder's field; a way in that shows no folder says it on the form.
  const exists = { message: "/data/projects/storefront/tree already exists; refusing to import into it rather than merge with what is there", refusal: "error-engine-invalid-already-exists-refusing-import-into-rather-than", status: 400 };
  assert.deepEqual(refusalPlace(exists, "import"), { path: exists.message });
  assert.deepEqual(refusalPlace(exists, "new"), { form: exists.message }, "the refusal once landed on an input nobody could see");
  // The node checks a project's assignees where it is made: an agent or a team nobody has is its sentence, whole.
  assert.deepEqual(refusalPlace({ message: "agent not found: ghost", refusal: "error-engine-invalid-agent-not-found", status: 400 }, "new"), { form: "agent not found: ghost" });
  // A team whose id reads like a field's word is the form's all the same: the words place nothing.
  assert.deepEqual(refusalPlace({ message: "team not found: import-crew", refusal: "error-engine-invalid-team-not-found", status: 400 }, "import"), { form: "team not found: import-crew" });
  assert.deepEqual(refusalPlace({ message: "agent not found: url-checker", refusal: "error-engine-invalid-agent-not-found", status: 400 }, "clone"), { form: "agent not found: url-checker" });
  // What is about an input on screen stays beside it — in whatever language the sentence is said.
  assert.deepEqual(refusalPlace({ message: "slug de projet invalide : ../escape", refusal: "error-core-invalid-slug", status: 400 }, "new"), { slug: "slug de projet invalide : ../escape" });
  assert.deepEqual(refusalPlace({ message: "clone needs a url", refusal: "error-node-projects-clone-needs-url", status: 400 }, "clone"), { url: "clone needs a url" });
  assert.deepEqual(refusalPlace({ message: "clone needs a url", refusal: "error-node-projects-clone-needs-url", status: 400 }, "new"), { form: "clone needs a url" }, "a way in that shows no URL");
  // A goal a door fixed that is gone by the time the route reads it: nothing the form draws, so the form's, whole.
  assert.deepEqual(refusalPlace({ message: "goal not found: 01GONE", refusal: "error-store-goal-not-found", status: 404 }, "new"), { form: "goal not found: 01GONE" });
  // Anything but an answer about an input is the node failing: the form's, whatever it names.
  assert.deepEqual(refusalPlace({ message: "the import task did not finish", refusal: "error-node-projects-source-cannot", status: 500 }, "import"), { form: "the import task did not finish" });
  assert.deepEqual(refusalPlace({ message: "the node did not answer", refusal: null, status: null }, "clone"), { form: "the node did not answer" });
  assert.deepEqual(refusalPlace({ message: "it threw" }, "clone"), { form: "it threw" }, "what is no answer of the node's at all");
  // Every place a refusal can land is one the dialog draws — and no other.
  const dialog = readFileSync(new URL("./NewProjectDialog.tsx", import.meta.url), "utf8");
  for (const field of ["slug", "name", "url", "path"]) assert.ok(dialog.includes(`err("${field}")`), `the dialog draws ${field}'s refusal`);
  assert.ok(dialog.includes("errors.form"), "and the form's");
  assert.ok(!dialog.includes('err("project")'), "no project picker, so no place for a refusal about one");
  assert.ok(dialog.includes("setErrors(refusalPlace({ message: e instanceof Error ? e.message : String(e), refusal: answer?.refusal, status: answer?.status }, provenance));"), "the dialog places a refusal through the model, by the id the answer carries");
});

test("the dialog never asks about a goal: nothing it draws is a goal's, no door hands it a list of them, and attaching is one dialog elsewhere", () => {
  const here = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const dialog = here("./NewProjectDialog.tsx");
  // No picker, no attach mode, no call that attaches: a project made here is the workspace's, or the fixed goal's.
  for (const trace of ["attachProject", "work-new-project-dialog-attach", "setAttaching", "goals.map", "attachable"]) {
    assert.ok(!dialog.includes(trace), `the creation dialog still carries ${trace}`);
  }
  assert.ok(dialog.includes("api.createProject(") && dialog.includes("goal ?? null"), "the only goal it knows is the one a door fixed, handed to the route");
  assert.ok(dialog.includes("AttachGoalDialog"), "and it says where attaching lives");
  // Every opener: no `goals` list and no `attachable` rows go into the dialog.
  for (const rel of ["../Workbench.tsx", "../_workbench/ProjectRail.tsx"]) {
    const src = here(rel);
    const openings = src.split("<NewProjectDialog").slice(1).map((s) => s.slice(0, s.indexOf("/>")));
    assert.ok(openings.length > 0, `${rel} opens the dialog`);
    for (const props of openings) {
      assert.ok(!/\bgoals=/.test(props), `${rel} hands the dialog a goal list`);
      assert.ok(!/\battachable=/.test(props), `${rel} hands the dialog projects to attach`);
    }
  }
  // The rail's import door fixes a goal only from a goal heading; the toolbar's and the palette's name none, and nothing guesses from the tab.
  const rail = here("../_workbench/ProjectRail.tsx");
  assert.ok(rail.includes("setImporting({ goal: null })"), "the toolbar and the palette guess no goal");
  assert.ok(rail.includes("setImporting({ goal: row.id })"), "a goal heading's door names its goal");
  assert.ok(!rail.includes("importTarget"), "nothing guesses a goal from the tab");
});

test("the commit button says what is about to happen, and the account select's first row whose choice it is", () => {
  assert.equal(primaryLabel("new", "copy"), "Create");
  assert.equal(primaryLabel("clone", "link"), "Create");
  assert.equal(primaryLabel("import", "copy"), "Copy it in");
  assert.equal(primaryLabel("import", "link"), "Link it in place");
  assert.equal(hostChoiceWords(null), "— the host's own choice —");
  assert.equal(hostChoiceWords("ada"), "— the host's own choice (@ada) —");
});

test("an import says what it did not copy, not only what it did", () => {
  assert.equal(importSummary(null), null);
  assert.equal(
    importSummary({ files: 12, dirs: 3, bytes: 900, skipped_symlinks: 0, skipped_special: 0 }),
    "12 files copied in.",
  );
  assert.equal(
    importSummary({ files: 1, dirs: 1, bytes: 4, skipped_symlinks: 1, skipped_special: 0 }),
    "1 file copied in, 1 symlink was skipped.",
  );
  assert.equal(
    importSummary({ files: 9, dirs: 2, bytes: 40, skipped_symlinks: 3, skipped_special: 2 }),
    "9 files copied in, 3 symlinks were skipped, 2 special files skipped.",
  );
});



test("a name becomes a slug the server accepts, and a clone or import starts from the tail of its source", () => {
  assert.equal(slugFromName("Storefront"), "storefront");
  assert.equal(slugFromName("  Build a simple Landing Page!  "), "build-a-simple-landing-page");
  assert.equal(slugFromName("Ship the checkout — v2 (fast)"), "ship-the-checkout-v2-fast");
  assert.equal(slugFromName("—— !!! ——"), "", "nothing usable is nothing, so the field asks");
  assert.equal(slugFromName("x".repeat(80)).length, 64);
  assert.equal(slugFromName("Ünïcödé Nämé"), "n-c-d-n-m", "non-ASCII folds to separators, never a lookalike");
  assert.equal(slugFromName(null), "");
  for (const s of [slugFromName("Storefront"), slugFromName("Ship the checkout — v2 (fast)")]) {
    assert.match(s, /^[a-z0-9][a-z0-9_-]{0,63}$/, `${s} is what validate_slug accepts`);
  }
  assert.equal(nameFromPath("git@github.com:acme/Storefront.git"), "Storefront");
  assert.equal(nameFromPath("/Users/me/code/My Project/"), "My Project");
  assert.equal(nameFromPath("C:\\repos\\Thing"), "Thing");
  assert.equal(nameFromPath(""), "");
  assert.equal(slugFromName(nameFromPath("/Users/me/code/My Project/")), "my-project", "the slug follows the name");
});

test("the import dialog offers only the ways that bring something existing", () => {
  assert.deepEqual(provenancesFor("create"), ["new", "clone", "import"]);
  assert.deepEqual(provenancesFor("import"), ["import", "clone"], "Import first: it is what the button said");
});

test("a goal is never asked for; the one thing said about its absence is gated publishing's, under Publishing, and it says where attaching lives", () => {
  assert.match(publishCaveat("gated", false), /refused until one is/);
  assert.match(publishCaveat("gated", false), /attach it to a goal from its About tab, or choose automatic/);
  assert.equal(publishCaveat("gated", true), null, "a door fixed a goal to ask: nothing to say");
  assert.equal(publishCaveat("auto", false), null);
  assert.equal(publishCaveat("manual", false), null, "manual never asks anyone");
});

test("the dialog asks nothing by default: a footnote under a global identity, the fields only when the policy asks or nothing resolves — two states, since every way in here makes a repository to configure", () => {
  assert.equal(gitConfigSection("inherit", true), "note");
  assert.equal(gitConfigSection("pin", true), "note");
  assert.equal(gitConfigSection("ask", true), "open", "ask means ask, whatever resolves");
  assert.equal(gitConfigSection("inherit", false), "open", "no global identity: the repository is born with an author");
  assert.equal(gitConfigSection("pin", false), "open");
  for (const policy of COMMITTER_POLICIES) for (const resolved of [true, false]) assert.ok(["note", "open"].includes(gitConfigSection(policy, resolved)), "no hidden state: nothing this dialog does creates nothing");
  assert.match(committerNote("inherit", "Ada <ada@example.invalid>"), /^Commits as Ada <ada@example.invalid> — your global git config\.$/);
  assert.match(committerNote("pin", "Ada <ada@example.invalid>"), /pinned into the repository/);
});

test("the policy words mirror the engine's CommitterPolicy, in its order", () => {
  const rust = readFileSync(new URL("../../../../crates/bisa-core/src/git_settings.rs", import.meta.url), "utf8");
  const words = /pub const WORDS: \[&str; \d+\] = \[([^\]]+)\]/.exec(rust)[1].match(/"([a-z]+)"/g).map((w) => w.replaceAll('"', ""));
  assert.deepEqual([...COMMITTER_POLICIES], words);
});

test("the git config typed at creation is carried whatever resolves globally and whichever state the section shows", () => {
  const typed = { set: { "user.name": "Ada Lovelace", "user.email": "ada@example.invalid" }, unset: [] };
  assert.deepEqual(creationGitConfig(typed), { "user.name": "Ada Lovelace", "user.email": "ada@example.invalid" }, "typed in the open fields, or behind Change…: it travels");
  assert.notEqual(creationGitConfig(typed), typed.set, "a copy — the form's own map is not the body's");
  assert.equal(creationGitConfig({ set: {}, unset: [] }), null, "nothing typed means inherit the global config");
  assert.equal(creationGitConfig({ set: {}, unset: ["user.name"] }), null, "nothing is local before the repository exists, so an unset is nothing");
  assert.deepEqual(creationGitConfig({ set: { "core.autocrlf": "input" }, unset: [] }), { "core.autocrlf": "input" }, "any key the form offers travels, not only the identity");
});

test("the body a creation sends is the kind's own keys by name: a clone's URL, an import's folder, an adoption's — and a git config only when something was set", () => {
  const form = { provenance: "new", placement: "copy", slug: " web ", name: " Web ", publish: "gated", tags: ["frontend"], gitConfig: null, url: " https://example.com/acme/web.git ", path: " /Users/ada/web " };
  assert.deepEqual(creationBody(form), { slug: "web", name: "Web", publish: "gated", tags: ["frontend"], kind: "new" }, "a new project carries neither a URL nor a folder, whatever the form still holds");
  assert.deepEqual(creationBody({ ...form, provenance: "clone" }), { slug: "web", name: "Web", publish: "gated", tags: ["frontend"], url: "https://example.com/acme/web.git", kind: "clone" });
  assert.deepEqual(creationBody({ ...form, provenance: "import" }), { slug: "web", name: "Web", publish: "gated", tags: ["frontend"], path: "/Users/ada/web", kind: "import" });
  assert.deepEqual(creationBody({ ...form, provenance: "import", placement: "link" }), { slug: "web", name: "Web", publish: "gated", tags: ["frontend"], path: "/Users/ada/web", kind: "adopt" });
  const set = { "user.name": "Ada", "user.email": "ada@example.com" };
  const withConfig = creationBody({ ...form, gitConfig: set });
  assert.deepEqual(withConfig, { slug: "web", name: "Web", publish: "gated", tags: ["frontend"], git_config: set, kind: "new" });
  assert.notEqual(withConfig.git_config, set, "a copy, not the form's map");
  // A form that holds more than the body takes — the dialog's own state — leaves it behind.
  assert.deepEqual(Object.keys(creationBody({ ...form, target: "01GOAL", existing: "", overriding: true, busy: false })), ["slug", "name", "publish", "tags", "kind"]);
});
