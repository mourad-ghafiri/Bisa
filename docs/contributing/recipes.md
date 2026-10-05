# Recipes — to add or change something

Every recipe is the files in order, then the gate that fails if one is missed. The crate pages
([architecture/crates/](../architecture/crates/README.md)) say what each file owns; this page is the
sequence across them.

---

## 1. Add an HTTP route

1. The request and response types in `crates/bisa-node/src/dto.rs`, deriving `JsonSchema`
   (a tri-state `PATCH` field uses the `clearable` deserialiser).
2. The handler in the owning node module. **If it mutates, it calls an engine function** — a new
   one in the matching engine module if none exists (`crates/bisa-node/tests/it/layering.rs`
   derives the store's writers by verb and denies them in the node).
3. A `RouteDoc { method, path, summary }` in the module's `ROUTES` table and the `.route(...)` in
   its `routes()`. A refusal the handler makes is a `text!` — `bad_request(text!("error-node-<module>-…"))`
   with the message in `locales/en/errors.ftl` — never a sentence ([26](#26-say-something-to-a-person)).
4. `just gen-types` — `desktop/api-schema.json` and `desktop/src/types.gen.ts`, never edited.
5. The `api.ts` function, in the section group that matches the module; a URL template is a
   documented route.
6. `just gen-api-docs` — `docs/reference/http-api.md`.

Gates: `crates/bisa-node/tests/it/routes.rs` (every documented route is mounted; every route module
has a table; every `api.ts` URL is documented), `check-types` and `check-api-docs` in CI.

## 2. Add a setting

1. A `def!(…)` in `REGISTRY` (`crates/bisa-core/src/settings.rs`) with its `Kind`, default and
   `ScopeSet` — which scopes may hold it — and its words in `locales/en/settings.ftl`:
   `setting-<key>` (the dots as dashes) `= <label>`, `.help = <the sentence under it>`, and for a
   `Choice` one `.choice-<value> = <word>` per value. The registry carries no words; the node, the
   CLI and the reference page render the catalog's ([17](../architecture/17-internationalisation.md)).
2. The reader: an engine `setting(key, scope)` call, or the desktop panel that renders the group
   (`desktop/src/views/_settings/`, generated from the registry by key prefix). A key nobody reads
   is a lying panel — `diagrams.export.scale` reaches `MermaidView` through `EditorDoc.tsx` the
   way `diagrams.theme` does.
3. `just gen-settings-docs` — `docs/reference/settings-keys.md`.

Gates: `check-settings-docs`; `crates/bisa-core/tests/it/docs.rs` refuses a key quoted in prose
that is not registered; `resolve_all_covers_the_registry` in the core; `scripts/test crate i18n`
refuses a key without its message, a choice without its word, or a word for a value the key does
not take.

## 3. Add a start event or a topic

- A **topic** is what a `platform` start or wait names. It is an `EnginePayload` variant's word in
  `EnginePayload::topic` (`crates/bisa-engine/src/events.rs`): add the variant (recipe 4), its
  word, its fields in `fields` and its row in `TOPICS` together, then the row in
  [guide/events.md](../guide/events.md#platform).
- A **start event** is a `StartOn` variant in `crates/bisa-core/src/start.rs` — its row in
  `fields_of`, `as_str` and `NAMES`, its arm in `resolve`, and its answer to `arms_unattended` (may
  an auto adoption arm it with nobody looking?) and `rate_limited`. What it hears is a filter in
  `crates/bisa-core/src/listen.rs` with a pure `hears(&Heard)`; validation's arm is in
  `crates/bisa-core/src/workflow.rs` (what it names must exist; a `ProblemKind` if a new rule needs
  one). In the engine, `listen/registry.rs` arms it and one door observes it — the ticker
  (`listen/sources.rs`) for what is only seen by looking, the ear (`listen/ear.rs`) for what the bus
  or a conversation says — and every occurrence sets a `dedupe_key`. On the desktop:
  `START_EVENTS` in `desktop/src/views/_workflow/stepKinds.mjs`, its fields in
  `desktop/src/views/_workflow/forms/startForm.mjs` and `StartStepForm.tsx`, its words in
  `listeningModel.mjs` and `workflowRunsModel.mjs`. Then its row in
  [guide/events.md](../guide/events.md#start-events) and
  [03 — Workflows](../architecture/03-workflows.md#start-events), and the Workflow Agent's prompt
  (`library/core/workflow-agent.toml`) if it should reach for it.
- A **catch** is the same filter under `WaitFor` (`workflow.rs`), armed by
  `crates/bisa-engine/src/waits.rs`; a **boundary event** or **act** is a `BoundaryOn` or
  `BoundaryAct` variant in `crates/bisa-core/src/boundary.rs`, armed by `waits::sync_boundaries` and
  performed by `effects::boundary_act`, drawn by `forms/boundaryModel.mjs`.
- There is no *action* to add to an event: an event starts a run, diverts a step or acts beside it,
  and what happens is the workflow's steps. What a workflow cannot express is a step kind
  (recipe 16).

Gates: the exhaustive matches over `StartOn`, `WaitFor`, `BoundaryOn` and `BoundaryAct`;
`crates/bisa-engine/tests/it/events.rs`, `boundaries.rs` and `waits.rs`;
`crates/bisa-engine/tests/it/docs.rs` (a topic quoted in the docs exists);
`desktop/src/views/_workflow/stepKinds.test.mjs` (the desktop's lists equal the Rust enums).

## 4. Add an engine event

1. The `EnginePayload` variant in `crates/bisa-engine/src/events.rs`.
2. Its topic in `EnginePayload::topic`, its fields in `EnginePayload::fields`, and its row in
   `TOPICS` — a `platform` start or wait may name it from then on.
3. The emit site (`inner.emit(EngineEvent::…)`).
4. The mirror in `desktop/src/types.hand.ts`, the rendered line in `desktop/src/activityModel.mjs`, and
   one instance in `activityModel.test.mjs`'s fixture; the CLI's line in `crates/bisa-cli/src/activity.rs`.
5. A sentence on the [engine page](../architecture/crates/engine.md).

Gates: `activityModel.test.mjs` (one of every variant renders), `crates/bisa-engine/tests/it/docs.rs`
(the page names every variant), the `topic` match (exhaustive) and
`every_topic_is_listed_once_and_the_list_names_only_topics` beside it.

## 5. Add an MCP tool

1. The `Op` variant and its arm in `crates/bisa-engine/src/intake.rs`; `core_agent_only` with
   the agents allowed if only the General Agent or the Workflow Agent may call it. A refusal is an error, never a result.
2. The `#[tool(name = …)]` in `crates/bisa-mcp/src/server.rs`, in the router it belongs to
   (common, worker, goal, the set the General Agent and the Workflow Agent share, the General
   Agent's, the Workflow Agent's). The Decision-Making Agent runs no session and has no router.
3. A row in [reference/mcp-tools.md](../reference/mcp-tools.md).
4. Catalog prompts may now name it.
5. A tool the **desktop** performs — a browser's, a drawing's — parks its request on a
   `parked::Desk` in the engine, puts a frame on the bus, and is answered over a node route
   (`crates/bisa-engine/src/drawings.rs` is the model); the engine says *nobody home* itself.

Gates: `crates/bisa-mcp/tests/it/docs.rs` (the reference equals the manifest);
`crates/bisa-store/tests/it/catalog.rs` (a prompt names only real tools).

## 6. Add a keymap command

1. A `COMMANDS` entry in `desktop/src/shell/keymapModel.mjs`: `id`, `label`, `when`, a chord per
   preset (`always: true` only for a palette chord that must work inside a composer).
2. The dispatch arm in `desktop/src/shell/shortcuts.ts`.
3. `just gen-keymap-docs` — `docs/reference/keymap.md`; the id in the list in
   [ide/15](../architecture/ide/15-keymap.md).

Gates: `keymapModel.test.mjs` (no conflicts), `keymapDocs.test.mjs` (the list equals the model),
`check-keymap-docs`.

## 7. Add a desktop model

`x.mjs` beside the component that uses it, `x.d.mts` for its types, `x.test.mjs` for its facts. A
model that mirrors something the Rust owns reads the Rust source in its test
(`ui/tagVocabulary.test.mjs`, `views/_work/assigneeWire.test.mjs`). Components are not tested; a
wrong pixel is visible, a wrong fact is not. A store that remembers a choice reads and writes it
through `shell/storedPrefModel.mjs` (never `localStorage` by hand), a diagnostic goes through
`log.ts` (never `console`), and an exported value is imported by a source or read by a test —
`logDoor.test.mjs` and `deadExports.test.mjs` hold the three.

**A screen's state that must come back** — a filter's text, the rows opened, the step picked — is
`useViewState(place, name, initial, parse)` from `shell/viewMemoryStore.ts` in place of `useState`:
the place is the route's (`placeOf(route)`) or a name for what is no route, `initial` is a module
constant, and `parse` is a function of the screen's model that answers `undefined` for what the
model does not know (`shell/viewValuesModel.mjs` has the common ones; a parse of the screen's own
gets a test). A scroll comes back by marking the scrollport `data-scroll-keep="<name>"` under a root
bound with `useViewScroll`; where one path shows several things the name carries the selection. A
read that must not leave the screen empty on return takes `useAsync(load, deps, { keep:
readKey(…) })`, and a detail screen hands its `missing` to `useGonePlace`. A dialog's or a form's
fields stay `useState`: they start empty. No new storage key is needed, and none is added.

## 8. Add a harness adapter

1. `crates/bisa-adapters/src/<id>.rs` implementing `HarnessAdapter` and `HarnessSession`;
   the wire is read here and nowhere else.
2. `register_all` in `lib.rs`; a Cargo feature.
3. The id in `BUILTIN_IDS` (`crates/bisa-harness/src/catalog.rs`) — a preset may not shadow it.
4. Dead-model patterns in `util.rs`: conjunctions of literal fragments with exclusions, error
   channels only.
5. `crates/bisa-adapters/tests/it/adapters.rs`.
6. Declare `HarnessCaps::TOOL_GUARD` only when the adapter stops before a tool runs and obeys the
   engine's `Deny` — the flag is what lets an installed MCP server ride on the harness
   (`executor::mounts_for_harness`); an adapter that takes MCP servers pre-allows the `Platform`
   mounts alone and puts every other tool to the engine (`claude_code::allowed_tools` is the model).
7. Effort, when the harness has a control for how hard a model works
   ([06 § Effort](../architecture/06-agents-and-teams.md#effort)): declare `HarnessCaps::EFFORT`;
   answer `efforts(model)` from a table read from the harness's **own documentation**, cited with
   the date it was read, lowest first — for a model the table does not know, only the levels every
   model takes, since a level the harness refuses fails the launch; pass `SessionSpec.effort` in the
   harness's own flag or command, held to that table once more; mint the `ResumeToken` with the
   model and the effort (`util::Shared::resume_token`) and hand both back at `attach`. A harness
   that documents no control declares nothing and is sent nothing. The harness's own word for it
   stays in the adapter: above it the word is *effort*. The interactive launch passes neither a
   model nor an effort.
8. `recommended_plan` and `recommended_judge`, when the harness has models the platform should
   lead with: what the setup gate's fixes write ([16](../architecture/16-setup-gate.md)). A
   harness whose models can retire under the platform recommends none, and lists what its own
   command prints (`grok.rs`).
9. How a person installs it and signs in, in its official page's words:
   an `InstallHint` and its `install_hint` arm in `crates/bisa-harness/src/install.rs` — a line
   for every platform, how to check it landed, the sign-in line. Shown to copy, never run.
10. The probe. `util::probe_binary` for a name nothing else answers to; `util::probe_versioned`
    when another tool installs a binary of the same name (an editor's `copilot` launcher) — the
    harness is there only when it answers with a version.
11. In a terminal: `interactive()` names the bare command and the harness's own word for *continue
    the latest session here*; `interactive_reporting` is a recipe under `hooks/` **only when the
    harness takes a hook, an extension or a plugin for one launch** — files under the session's
    own run folder, arguments appended to the command, nothing written in the harness's own
    configuration or in a project — with its `translate` arm in `hooks/mod.rs`; and, when its
    pre-execution hook can wait for a verdict, the guard as a second entry and the harness's own
    verdict shape in `hooks::guard_output`. A harness that takes none returns the empty plan and
    opens as a plain terminal; say so on [feature status](../feature-status.md).
12. Its mark: `MARK_IDS` in `desktop/src/ui/harnessMarkModel.mjs`, the component and its `BRAND`
    entry in `harnessMarks.tsx`, the set it came from in `NOTICES.md` —
    `harnessMarkModel.test.mjs` fails until every built-in id wears one.

### A harness that speaks ACP

It has no wire of its own, so it gets no wire code: **the protocol is `acp.rs`; the harness is
its facts.** Two ways in, by what the platform should know of it:

- **A generic target** — one line in `register_all`: `AcpAdapter::new("acp:<x>", label, program,
  args)`. Four words: no probe beyond `PATH`, no models, no terminal form, the protocol's plug for
  a mark.
- **An id of its own** (`copilot.rs`, `grok.rs`, `gemini.rs` are the models) — `src/<id>.rs` with a
  `HarnessAdapter` whose `launch` is `acp::open(self.command(), &spec, None)` and whose `attach`
  is `acp::revival(ADAPTER_ID, token)` then `acp::open(…, Some(native_id))`; `command()` is a pure
  function returning an `AcpCommand` — the words that put the binary in protocol mode, and
  nothing else. Then steps 2, 3 and 5 to 12 above; the Cargo feature implies `acp`.

What such an adapter never does:

- **name a model or an effort on the command line.** `acp::open` sets both the protocol's way —
  the session's `model` option first, then its `thought_level` option, fitted to the levels the
  model's own answer offers — and a model the session does not offer ends the launch as
  `ModelUnavailable`, so the plan walks on. A flag would say it twice, and differently.
- **pass a word that allows a tool unasked**, or let a variable that does reach the process
  (`copilot.rs` puts `COPILOT_ALLOW_ALL` in `env_remove`): `TOOL_GUARD` is true only while the
  agent asks.
- **write a frame.** A protocol step an agent needs — a capability to check, an option to set —
  is added to `acp.rs`, for every agent at once.

Tests: the adapter's own facts beside it (the command, the levels, the models, the terminal
form); a stub agent driven through it in `tests/it/adapters.rs`; and the scripted agent placed
under its program's name in a journey (`Sealed::install_agent_as`, `e2e/copilot_grok_and_gemini.rs`).

## 9. Add a GEP kind

1. The constant in `crates/bisa-core/src/kind.rs` and its membership in `GEP_KINDS` and the
   policy sets (`ADDRESSABLE_KINDS`, `ENCRYPT_TO_OWNER_KINDS`, `EPHEMERAL_KINDS`,
   `AUTHORITY_CHECKED_KINDS`, `AUDIENCE_SCOPED_KINDS`). Always the next free number, `33416`: from
   0.1.0 a retired number is never reissued ([Keeping compatibility](compatibility.md)) — `33407` and
   `33401` were the two reissued before it.
2. A route arm in `crates/bisa-store/src/ingest.rs`; a snapshot or journal path through
   `crates/bisa-store/src/paths.rs`.
3. [09 — GEP](../architecture/09-protocol-gep.md) and [reference/gep.md](../reference/gep.md).

Gates: `kind.rs` tests — the counts, the addressable set and the free number.

## 10. Add a workspace path

An accessor in `crates/bisa-store/src/paths.rs`; the name in `OWNED_NAMES`
(`crates/bisa-store/tests/it/layout.rs`); the line in
[reference/workspace-layout.md](../reference/workspace-layout.md) and the tree in
[04](../architecture/04-workspace-project-goal.md). Gate: `layout.rs` (a `join("<name>")` outside
`paths.rs`, in a crate that knows the workspace, fails).

## 11. Change the index schema

`SCHEMA` in `crates/bisa-store/src/index.rs` and a bump of `SCHEMA_VERSION`; the `reindex_*`
walker in the owning store module; the SQL block in [08 — Persistence](../architecture/08-persistence.md).
**Never a migration**: the next open discards the index and rebuilds it from truth.
Gate:
`crates/bisa-store/tests/it/schema_doc.rs`.

## 12. Add a CLI verb

The `Command` variant in `crates/bisa-cli/src/main.rs`; its subcommand enum in the owning
module; `client.rs` if it needs the daemon; `output.rs` for its shape — every line it says is
`out.say(&text!("cli-<module>-…"))` with the message in `locales/en/cli.ftl`, a refusal
`bail!(text!(…))`, and its help — the doc comments — mirrored there too as `cli-cmd-<path>`
(`.about`) and `cli-arg-<path>-<id>` (`.help`), which `crates/bisa-cli/src/localize.rs`'s test holds
equal ([17](../architecture/17-internationalisation.md)); a sentence in
[reference/cli.md](../reference/cli.md). Gate: `crates/bisa-cli/tests/it/docs.rs`.

## 13. Add a Tauri command

`#[tauri::command]` in `desktop/src-tauri/src/main.rs` and its `invoke_handler`; a typed wrapper in
`desktop/src/terminal/session.ts` or `desktop/src/api.ts`; `capabilities/default.json` if it needs a
plugin. The webview names a scope and an id, never a path; a harness id, never a program. A word
from the shell to the webview is a `pub const NAME: &str = "namespace:word"` and `app.emit`, with the
same string in a `.mjs` model the webview listens by (`shell/trayModel.TRAY_EVENTS`,
`scenarios/tray.test.mjs` holds them equal). The shell holds the window's two ways out —
`CloseRequested` and `ExitRequested` — and the webview decides (`shell/useCloseGuard.ts`); a
command that ends the app calls `quit_app`, never `exit` of its own.

## 14. Add a screen or a settings panel

A screen: its pattern in `ROUTE_TABLE`, its path and its `section` in `desktop/src/routeModel.mjs`
(the `Route` variant in `routeModel.d.mts`; `router.ts` navigates); a lazy case in `App.tsx`'s
`Screen`; a `Sidebar` destination — or none, for a page reached only by its doors, as a run's page
(`#/runs/<id>`) is; a row for the route in `REMEMBERED` (`shell/placeMemoryModel.mjs`) naming the
query keys that are the screen's state — **every query key is classified**: the screen's state, the
Details pane's, or handed over once (`ONE_SHOT`), and the model's test fails on a key read by
`useSearchValue` or written by `setSearch` that is in no table. A settings panel: a component under `views/_settings/`, or a `RegistryPanel`
over a settings group when the panel is only its keys (Settings › Automation › Budgets over
`budget.*`), and a stable tab id in `settingsLink.mjs`.

## 15. Add a catalog workflow template

A file `library/catalog/workflows/<slug>.toml` — `[workflow]` with `name`, `description` and
`tags`, then `[[workflow.inputs]]` and `[[workflow.steps]]` in the core's own shape (a step's
`kind` and its fields flattened; `then = ["x"]` or `[{ to, branch }]`; assignees as
`{ agent = "<slug>" }`, never a pubkey; people and projects as inputs). It begins at an explicit
`start` step — `on = { event = "manual" }`, and one more `start` per event it may begin on, each
mapping the event onto the template's inputs and reading what it needs from an input, so the
template stays generic; it installs Off. Its `agent` steps name
catalog agents by slug and are installed with it. `just gen-catalog-docs` regenerates
[reference/catalog.md](../reference/catalog.md) — never edit it by hand — and the count in the
Workflow Agent's prompt (`library/core/workflow-agent.toml`) changes with it. Gate:
`check-catalog-docs`; `crates/bisa-store/tests/it/catalog.rs` (every template validates against the
catalog's agents with no problems; tags are in the vocabulary),
`crates/bisa-store/src/catalog.rs::every_template_begins_at_a_start_and_says_what_it_starts_on`
and `every_template_runs_in_the_workspace` — no step reads
`{goal.statement}` or `{goal.title}`: a goal's run is told its goal in the first prompt, and a run in
the workspace has none. The template's name and description are the file's words in every language unless a
`locales/<tag>/catalog.ftl` translates them (`catalog-workflow-<slug>`, recipe 25).

## 16. Add a step kind

The eighteen kinds are a closed set, and a nineteenth is a change in every layer that names them —
in this order, because each gate catches the next miss:

1. The `StepKind` variant and its fields in `crates/bisa-core/src/workflow.rs`, in `NAMES`
   (palette order) and `fields_of` (the wire's closed vocabulary), `as_str`, `family` (an event, a
   gateway, a loop or a task — the palette's group), `summary`, `templates`,
   `input_refs`; `branches()` and `loop_branches()` if it labels its flows; `may_carry_boundaries` in
   `boundary.rs` (may it be stopped mid-flight?); its validation arm (what
   must be upstream, which inputs it may reference, what its `then` may carry); a `ProblemKind` if a
   new rule needs one; a sample in `step_kind_field_table_matches_the_wire`.
2. `crates/bisa-core/src/run.rs`: what entering it does (`Running` with an effect, `Waiting`
   with an effect, or `Done` at once — a loop kind keeps a `LoopCursor` and resets its body), which
   `RunEvent`s it accepts (`accepts`), a `RunEffect` if the engine must act, and its side in
   `dies_with_the_process` and `waiting_holder` (both exhaustive). A unit test per rule.
3. The interpreter's arm in `crates/bisa-engine/src/effects.rs`, and a `waits.rs` arm if it
   waits on the world; the engine's other exhaustive matches (`staff.rs`, `projects.rs`,
   `intake.rs`, `ops.rs`).
4. If a person completes it: a node route under `/runs/{rid}/steps/{step}/…`
   (`crates/bisa-node/src/runs.rs`, recipe 1 — any run, a goal's or one in the workspace), the
   engine facade function taking the run's id, and a `step` verb in `crates/bisa-cli/src/step.rs`
   (recipe 12), which takes a goal's id or a run's (`crates/bisa-cli/src/target.rs`). `crates/bisa-cli/src/activity.rs`'s
   `step_kind_label` is exhaustive either way.
5. The desktop: a row in `desktop/src/views/_workflow/stepKinds.mjs` (`STEP_KINDS`, `blankStep`
   defaults, `branchesOf` when it branches), an entry in `STEP_KIND_ICON` (`desktop/src/ui/icons.ts`),
   a form under `desktop/src/views/_workflow/forms/` wired into `Inspector.tsx`, the `StepNode`
   handle arm, `stepActions` in `desktop/src/views/_workflow/runView.mjs`, `KIND_LABEL` in
   `views/_goals/goalStripModel.mjs` and `stepHolder` in `views/_goal/progressModel.mjs`.
6. The kinds table in [03 — Workflows](../architecture/03-workflows.md#the-eighteen-kinds) and
   [guide/workflows.md](../guide/workflows.md); the `run_steps.kind` check in `SCHEMA` and a bump of
   `SCHEMA_VERSION` (recipe 11); the Workflow Agent's prompt (`library/core/workflow-agent.toml`) and
   `DESIGN_DIRECTIVE` if it should reach for the kind.

Gates: the exhaustive matches in `run.rs`, `effects.rs` and the desktop's `switch`es fail to compile
or type-check; `desktop/src/views/_workflow/stepKinds.test.mjs` and `runView.test.mjs` read the
Rust enums; `crates/bisa-store/tests/it/catalog.rs` if a template uses it; `just gen-types` and
`npm run build`.

## 17. Add a built-in security rule

A rule the platform ships — a token shape the redactor recognises, a command or path the guard
refuses or sends to the classifier ([11 — Security](../architecture/11-security.md)).

1. `crates/bisa-security/src/builtin.rs`: a `redact(id, label, regex)` or a `command(id, label,
   action, regex)` / `path(id, label, glob)` entry, with a **stable id** — a setting names it to
   switch it off, so an id is never renamed. A redaction pattern that should keep part of its match
   names the secret with a group called `secret`.
2. The test module in the same file: one synthetic fixture the rule catches (`ghp_` and thirty-six
   `x`, never a real value) and, for a guard rule, the benign command that must still fall through.
   A destructive fixture is spelled here and nowhere else in the tree — the scan in
   `crates/bisa-core/tests/it/layering.rs` refuses it under any `tests/` directory.
3. The list in [11 — Security](../architecture/11-security.md) and, when the rule's words change
   what Settings › Security says, `desktop/src/views/_settings/securityRules.mjs`.

Gates: `cargo test -p bisa-security` (every built-in compiles, ids are unique, every fixture
lands on the rule named for it, the everyday commands fall through); `crates/bisa-core/tests/it/docs.rs`.

A person's own rule is a setting — `security.redactor.rules`, `security.guard.rules` — and needs no
code at all: Settings › Security writes it, and the node compiles it at the next read.

## 18. Add a theme

A family is a stylesheet, a material and an id in two lists; the suite says whether it is readable.

1. `desktop/src/theme/themes/<name>.css`: a `:root[data-theme="<name>"]` block and a
   `:root[data-theme="<name>-dark"]` block, each stating `color-scheme` and every role between the
   `@roles` markers of `tokens.css`, colours in `oklch()`. Build for hours of reading: light `bg`
   around L 0.975 with `surface` above it (never pure white behind text), dark `bg` around L 0.2
   (never black), `text-dim` that still clears 4.5:1, `surface-2` a visible step off `bg`, and an
   accent at least 30° of hue from `warn` and `danger`. Answer the material roles for what the
   family is: an opaque or matte family gives `--material-blur: 0px`, `--material-saturate: 1` and
   no alpha in its surfaces (Suede answers `--shadow-raised: none`); a glass family carries alpha in
   `bg`, `surface`, `surface-2` and `border`, a non-zero blur, and words that stay solid — and every
   bar is then measured over the composite, on a white ground and a black one.
2. `@import` it in `desktop/src/styles.css` after the other families. A glass family's ids go into
   `material.css`'s selectors and the `GLASS_FAMILIES` list in `theme/themes.test.mjs`.
3. The id list, three times: `THEMES` and a `FAMILIES` row (with its `material`) in
   `desktop/src/shell/theme.ts`; the `appearance.theme` `Choice` in
   `crates/bisa-core/src/settings.rs`; the file name in `theme/themes.test.mjs`'s expected
   list.
4. The docs: the `theme/` row in [crates/desktop.md](../architecture/crates/desktop.md) and the
   Settings paragraph in [guide/the-desktop.md](../guide/the-desktop.md); regenerate
   `settings-keys.md`.

Gates: `node --test desktop/src/theme/roles.test.mjs desktop/src/theme/themes.test.mjs` — the second
computes every contrast bar from the stylesheet and fails on the first pairing under its floor;
`cargo test -p bisa-core --lib settings`.

## 19. Change the platform's mark

The mark is one file, `logo/logo.svg`, shown by the app and rasterised into the OS icon; a redraw
is a new file at that path and one command. Nothing in code draws or generates a mark.

1. Replace `logo/logo.svg`. Keep it one self-contained `<svg>`: a `viewBox`, `<title>Bisa</title>`,
   gradients and clips as you like, **no filter** (`tauri icon` rasterises with resvg), no script, no
   image, no reference outside the file. Every id it declares is used.
2. `just app-icon`: `tauri icon` over the file into `desktop/src-tauri/icons/`. Look at `icon.png`
   and `32x32.png` before moving on — the drawing must read at both.
3. The menu bar's mark is its sibling, `logo/tray-mark.svg` — the same B and rail in one ink, no
   squircle, no gradient — rasterised by `just tray-icon` into `desktop/src-tauri/icons/tray/36x36.png`
   (18 pt at 2×), which the shell ships and tints at run time (`src-tauri/src/tray/glyph.rs`). A
   redraw of the B is a redraw of both files.
4. The docs: `NOTICES.md` says the mark is our own (and `just gen-notices` carries it into
   `THIRD-PARTY-NOTICES.md`); the `ui/` row in
   [crates/desktop.md](../architecture/crates/desktop.md) if what the drawing *is* changed.
5. **macOS keeps the icon it first saw for a bundle id** — in the Dock, in ⌘Tab and on every
   notification — until the app is re-registered. After a new mark, on your own Mac: unregister the
   stale copies (`/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u <old .app>`
   for each build under `target/`, `dist-previous/` and any renamed predecessor), re-register the
   current one (`lsregister -f dist/Bisa.app`), clear the icon cache
   (`sudo rm -rf /Library/Caches/com.apple.iconservices.store`) and restart the Dock and the
   notification centre (`killall Dock NotificationCenter usernoted`, or log out and in). If a
   notification still wears the old mark, remove Bisa under System Settings › Notifications; it
   re-registers on the next one. The same registry keys the `bisa://` link claim and the app's
   folders under `~/Library` by the **bundle identifier**, `dev.bisa.bisa` (`tauri.conf.json`): a
   build under the earlier `dev.bisa.desktop` keeps its claim on the link until it is unregistered
   the same way, and its window state and fallback log stay in its own folders, read by nothing. Judge the icon on a **bundled** app only: under `tauri dev` the
   notification plugin posts as Terminal, so a dev build's notifications wear Terminal's icon by
   design.

Gates: `node --test desktop/src/ui/platformMark.test.mjs` — the file is self-contained, the
component shows it and spells no path, the recipe reads the same file, every icon the bundle names
exists.

## 20. Log something

The diagnostic log ([crates/log](../architecture/crates/log.md)) is written by every `tracing`
call site; a new line is a call, at the right level, with the right words.

1. The level. `error!` is a malfunction — what a person attaches to a bug report at the default
   setting; `warn!` a fault the platform absorbed (`warn_on_err`); `info!` a fact of the
   platform's timeline (the engine's activity facts already are one — do not repeat them); `debug!`
   a step a developer follows; `trace!` a frame.
2. The words. Fields are ids, kinds, statuses, durations and a typed error's sentence (`%e`), with
   a `target:` naming the module. **Never a prompt, a message body, a file's text, a token or an
   environment value** ([11 — Security](../architecture/11-security.md#the-diagnostic-log)); a
   request's query is never recorded.
3. The desktop: `log.error/warn/info/debug(target, message, fields)` from `desktop/src/log.ts`,
   with `errorFields(e)` for an error — the shaper bounds the rest. A component reaches it through
   a prop (`ErrorBoundary`'s `onError`, `onReveal`), never by importing the app's log into the kit.
4. A new setting on the log is a `logging.*` key (recipe 2) read by `logging::config_from` in the
   engine and `configFrom` in `desktop/src/logModel.mjs`; a new level or rotation word is the
   crate's `WORDS` and the registry's `Choice` together.
5. A death that is not a panic — a supervised child gone, a state the process cannot go on from —
   is `handle.report_crash(CrashReport::new(kind, words))`: the handle stamps the process, the
   build, the pid, the moment and the flight recorder's last lines. A panic needs nothing: the hook
   writes the report. A process that means to end says `handle.goodbye(reason)` on its way out, or
   the next start reports it as ended without one.

Gates: `crates/bisa-engine/tests/it/logging.rs` (the registry's words are the crate's),
`desktop/src/logModel.test.mjs`; `scripts/lint-terminology`.

## 21. Add a connector

A built-in connector is one TOML file under `library/catalog/connectors/<slug>.toml` — a
`[connector]` table with every field of `bisa_core::Connector` but `id`, `origin` and
`created_at` ([03 — Workflows § Connectors](../architecture/03-workflows.md#connectors)): `name`,
`description`, `tags` (from the vocabulary), `base_url`, `hosts`, `auth`, `params`, `operations`
(each with a `description`, typed `params` with a `doc`, an `output.select` where the useful part
is nested, `writes` when it changes something, and — when the platform's documentation names them —
`timeout_secs` for its own deadline, `idempotency = { header = "…" }` on a write the platform can
tell a resend from, `page = { cursor_param, next_cursor, max_pages }` on a read that selects a list
and pages), and `check` — an operation with no required
parameter that writes nothing, the one Settings presses.

1. The file, then its row in `CATALOG_CONNECTORS` (`crates/bisa-store/src/catalog.rs`) and the
   slug in `the_catalog_ships_fifteen_connectors`.
2. `every_connector_is_well_formed` runs `Connector::validate` on it, checks its tags, that every
   operation and parameter is described, that `check` is set and does not write, and installs it
   into a scratch workspace — so a placeholder that names an undeclared parameter, a reserved
   header, a base host missing from `hosts`, or `insecure_tls` off loopback fails here.
3. `just gen-catalog-docs`; a paragraph in [`guide/connectors.md`](../guide/connectors.md) saying
   what to create at the platform and where the credential goes.

A custom connector is the same TOML through `bisa connector new --from <file>` or the same
JSON through `POST /connectors` — validated first, refused by name, and removable only while no
account and no workflow step names it. A body says its `kind` (`json`, `form`, `multipart`, `raw`),
a parameter may be a `file` the run's checkout holds, and `jwt` is the scheme for a platform that
hands out a private key ([the guide](../guide/connectors.md#bodies-files-and-signed-tokens)).

A new **way a call holds up** — a field of the operation like `timeout_secs`, `idempotency` or
`page` — is a change in `crates/bisa-core/src/connector.rs` (the field, its `validate` rules), in
`bisa-connectors` (`CallSpec` in `spec.rs` and its use in `client.rs`), in the engine's `call_spec`,
in the CLI's `connector show`, in the guide and in [`architecture/crates/connectors.md`](../architecture/crates/connectors.md#extension-points).

A new **auth scheme**, **body kind** or **parameter kind** is a change in
`crates/bisa-core/src/connector.rs` (the variant, its `validate` rules, its `templates()` sites) and
in `bisa-connectors` — a signer in `src/auth/<scheme>.rs` behind `AuthSigner` and its arm in
`signer_for`; an encoder in `src/body.rs` behind `BodyEncoder` and its arm in `encoder_for`; a kind
in `bind_params` — then the engine's shape maps (`connectors.rs::{auth_spec, body_of, kind_of}`),
the desktop's mirror (`connectorsModel.mjs` for a scheme's secret fields, the step form for a kind),
the guide and the two crate pages ([connectors](../architecture/crates/connectors.md#extension-points)).
`just gen-types`, `gen-api-docs` and `check-catalog-docs` follow.

## 22. Regenerate and verify

```sh
just gen-types gen-api-docs gen-settings-docs gen-catalog-docs gen-keymap-docs   # the generated artefacts
just i18n-baseline                                                              # the translation ratchet's baselines, after a sentence moved into the catalog
just verify                                                                     # the whole gate — see Release
```

Without `just`: `cargo run -q -p bisa-node --bin api-schema > desktop/api-schema.json && (cd
desktop && npx json-schema-to-typescript@15 api-schema.json -o src/types.gen.ts)`, `cargo run -q -p
bisa-node --bin api-docs > docs/reference/http-api.md`, `cargo run -q -p bisa-node --bin
settings-docs > docs/reference/settings-keys.md`, `cargo run -q -p bisa-node --bin catalog-docs
> docs/reference/catalog.md`, `node scripts/gen-keymap-docs.mjs`.

## 23. Add a built-in pet

A built-in pet is one folder under `library/pets/<slug>/` in Codex's pet format: `pet.json` (the
manifest — `id` ending in `.<slug>`, `displayName`, `description`, `spritesheetPath`, and for a pack
drawn for the platform `animations` — one row per state with its `frames` and `frameDurationsMs` —
and an `x-bisa-pets` block with a `tagline`, an `archetype` and a `mood`) and `spritesheet.webp`
(1536×1872, 8×9 cells of 192×208, one row per state in the sheet's order). A `sheet.svg` beside them
is the drawing's source and does not ship.

1. The folder, then its row in `CATALOG_PETS` (`crates/bisa-store/src/catalog.rs`, `pet!("<slug>")`)
   and the slug in `the_catalog_ships_nine_pets`.
2. `every_pet_is_well_formed` holds the manifest to `Pet::validate_animations`, the id to the folder,
   the tagline, the archetype and the mood, and the sheet to WebP of the grid's exact size under the
   cap — a pack that will not draw fails here, not on a person's machine.
3. `just gen-catalog-docs`; a line in [`guide/the-desktop.md`](../guide/the-desktop.md#the-pet)
   saying who the pet is.

A pet ships listed and drawn from the binary: nobody installs it, nobody removes it, and a person's
own pack may not take its id. The default when the pet is turned on with none chosen is
`petModel.DEFAULT_PET_ID` (Moonrice) — change it there and in the guide together.

## 24. Add a tray platform

The menu bar icon (`desktop/src-tauri/src/tray/`) is one module on every OS; what differs sits behind
`platform::Platform` — `has_dock`, `set_dock_visible`, `reveal` — with `MacOs` under the one
`target_os` cfg and `Elsewhere` answering *unsupported*. Linux or Windows is a `Platform` of its own
and nothing else:

1. `struct Linux` / `struct Windows` in `platform.rs`, chosen in `current()` by cfg. No Dock on
   either (`has_dock` false — the menu's *Show in Dock* line is not built); `reveal` is a no-op unless
   the OS hides whole apps.
2. What the OS cannot draw, drawn otherwise: Windows draws no title beside a notification-area icon,
   so the count goes through `Window::set_overlay_icon` — a second `glyph` composition, the number on
   a disc — from `Tray::present`; Linux draws the title and no tooltip. Linux emits no click on the
   icon (tauri-apps/tray-icon), so the menu is the only door — *Open Bisa* is already a line in it.
3. The words never move: `TrayReport` is the webview's and `report.rs` paints it; a platform adds
   how, not what. `scenarios/tray.test.mjs` and `tray::tests` hold the ids and the events; a new
   platform adds its own facts beside `platform::tests`.
4. The guide's [§The menu bar icon](../guide/the-desktop.md#the-menu-bar-icon) and the row in
   [feature-status.md](../feature-status.md) say which platforms ship.

## 25. Add a locale

A language is a folder ([17 — Internationalisation](../architecture/17-internationalisation.md)).

1. `locales/<tag>/` — a copy of `locales/en/`, every file, every id, translated; the tag is a
   language identifier (`fr`, `pt-BR`).
2. The tag in `AVAILABLE` twice — `crates/bisa-i18n/src/locale.rs` and
   `desktop/src/i18n/localeModel.mjs` — and each namespace's file in `Namespace::source`'s match
   (`crates/bisa-i18n/src/catalog.rs`), so the crates carry it compiled in; the desktop finds the
   folder by its glob.
3. The tag among `appearance.language`'s choices (`crates/bisa-core/src/settings.rs`) and its word
   in `locales/en/settings.ftl` (`.choice-<tag>`), then `just gen-settings-docs`.
4. The guide's Appearance paragraph names the language.
5. Optional: the catalog's display fields in `locales/<tag>/catalog.ftl` — `catalog-<kind>-<slug>`
   with `.description`, `catalog-pet-<id>` with `.tagline` — for the entries worth translating; a
   field with no message keeps the file's words.

Gates: `scripts/test crate i18n` (every file parses, the language ships whole, the two `AVAILABLE`
lists agree), `node --test desktop/src/i18n/localeModel.test.mjs` (the setting's choices equal the
model's), `desktop/src/scenarios/i18n.test.mjs`.

## 26. Say something to a person

A sentence the platform says is a message of the catalog, said by its id; the words live in
`locales/en/*.ftl`, once ([17](../architecture/17-internationalisation.md)).

- **In the desktop**: the message in `locales/en/desktop/<area>.ftl` — attributes for a widget's
  several strings, a selector for a plural — and `t("area-thing", { n })` where the model or the
  component says it (`attr("area-field", "placeholder")` for an attribute, `tx(text)` for a `Text` the
  node sent). A model's test asserts the English sentence, which `npm test` preloads
  (`node --test --import ./src/i18n/preload.mjs` for one file).
- **In the node or the engine**: `text!("error-…", id = goal.id)` (`bisa_core::text!`) and the
  message in `locales/en/errors.ftl` (or `problems.ftl`, `engine.ftl`); an `ApiError` carries the
  `Text`, the edge renders it. A model's or an agent's sentence is not a `Text` — it stays English.
- **In the CLI**: `out.say(text!("cli-…"))` and the message in `locales/en/cli.ftl`.

Gates: `scripts/test crate i18n` (every `text!` names a message with exactly its arguments; every
message is said; the ratchet — a bare sentence that stayed a literal raises a file's count and
fails), `desktop/src/scenarios/i18n.test.mjs` (the same for `t()`/`attr()` and the desktop's
ratchet). Both baselines are empty and stay so; `cargo run -q -p bisa-i18n --bin i18n-ratchet -- --list`
and `cd desktop && node src/i18n/ratchet.mjs --list` name each offending line. A word that is not
for a person — for the agent, for the machine, for the log — is marked so on its line or the comment
line above (`// for the agent`), and the ratchet leaves it out.

## 27. Add a built-in addon

A built-in addon is a folder under `library/addons/<slug>/` ([18](../architecture/18-addons.md)):
`addon.json` whose `id` **is the slug**, `index.html` loading `<script src="bisa-addon.js">` and its
own `main.js`, a `style.css`, and any other file of an allowed kind — nothing remote, no `fetch`,
`localStorage` or `eval` (the library is the door). `crates/bisa-store/build.rs` embeds every folder
at compile time, so nothing is listed by hand; the store's `the_catalog_ships_thirteen_addons` pins
the folders — add the slug there — and `every_addon_is_well_formed` holds each to the manifest's
rules, the listing's rules and an install; `desktop/src/scenarios/addons.test.mjs` reads the same
folders. Then `just gen-catalog-docs` (the *Addons* section of `reference/catalog.md`). A display
field a language may translate is `catalog-addon-<slug>` (`.description`) in
`locales/en/catalog.ftl`'s rule; English ships none. `addons/template/` is the starter a developer
copies; what an addon may ask is `reference/addon-api.md`.

## 28. Add a dependency

A dependency is a licence the platform ships ([Release § Licence](release.md#licence)), so adding one
is three steps, the first a reading.

1. Read its licence. Permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, CC0, 0BSD, Unlicense,
   MIT-0) needs no thought; an `OR` with a permissive branch is fine (the branch is taken); a
   file-level copyleft (MPL-2.0) is fine unmodified, and the notices will point at its source;
   GPL, LGPL, AGPL, SSPL, EUPL or a share-alike licence with no alternative means another package.
   A licence not on `deny.toml`'s list that you have read and judged permissive goes on the list —
   **both** `deny.toml` and `desktop/src-tauri/deny.toml`, with a comment saying why.
2. `just licence-gate`: cargo-deny where installed, and `scripts/licences/check-licences.mjs` on
   every machine — it names the package and the expression when a branch cannot be taken.
3. `just gen-notices`, and commit `THIRD-PARTY-NOTICES.md` with the change; `just check-notices`
   fails otherwise. A font or an asset that arrives without its licence file (Excalidraw's fonts)
   is a line in `NOTICES.md` by hand.

Gates: `node --test scripts/licences/licencesModel.test.mjs`,
`node --test desktop/src/scenarios/licences.test.mjs`, `just check-notices`.
