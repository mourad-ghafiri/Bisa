# bisa-security

The Redactor, the Tool & Commands Guard and the Classifier's contract, as pure functions over text
([11 — Security](../11-security.md)). Nothing here does I/O: the engine hands this crate a string and
a policy and gets a string and a verdict back. That is what lets every rule be tested against a
synthetic token and a command that is only ever matched, never run.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the re-exports and the contract the engine holds on top: a placeholder is restored only at an execution point on this machine |
| `redact.rs` | `RedactRule { id, label, enabled, detector, origin }`, `Detector::{Pattern { regex }, EnvValue { name }}` (the `secret` named group is the secret when present; an environment variable's *value* is read when the redactor is built and never stored), `Origin::{Builtin, User}`, `Placeholder { kind, tag }` (`«secret:<kind>:<tag>»`, `parse` only whole), `Vault` (`new(nonce)`, `placeholder_for` — one placeholder per secret per process, a tag collision lengthens the newer tag; `restore` → `Restored { text, restored, unresolved }`; `restore_value` over string leaves; a `Debug` that prints a count), `Redactor::compile(rules, env) -> (Redactor, Vec<BadRule>)`, `redact` → `Redaction { text, count, kinds }` (longest match first, idempotent on placeholders), `redact_value` (string leaves only, shape untouched), `has_placeholder`, `MIN_ENV_SECRET_LEN` (4) for a rule a person wrote, `AUTO_ENV_MIN_LEN` (8) for a built-in one armed from a variable's name |
| `guard.rs` | `GuardRule { id, label, enabled, action, matcher, hint?, applies_to?, origin }` — `hint` the sentence a refusal adds after the label (`GuardRule::refusal`), `applies_to: Option<Host>` where the rule applies (`Host::{Platform, Terminal}` — who hosts the harness a call comes from; unset is everywhere, and a rule stored before the field reads as everywhere); `Action::{Allow, Deny, Ask, Classify}`, `Matcher::{Command { regex }, Path { glob }, Tool { name }, Any}` (a tool name is exact, or a prefix ending in `*` — `Guard::tool_matches`), `ToolCall { tool, input, cwd, home, host, canonical? }` (`command()` from `command`/`cmd`, whitespace-normalised; `paths()` from the path keys and the command's path-like words, `~` and relative paths tried in every form; `host` who makes the call; `canonical` the tool's harness-neutral name when the harness's own differs — `fetch` for `WebFetch` — which a `Tool` rule reads beside the raw name), `Guard::compile(rules) -> (Guard, Vec<BadRule>)`, `evaluate(&ToolCall) -> Verdict::{Allow, Deny { rule, reason }, Ask, Classify, Fallthrough}` — first enabled match wins, a rule for one host alone passed over for a call from the other; `evaluate_lines(script, cwd, home) -> Option<RefusedLine { line, rule, reason }>` — each non-comment line as a `sh` call, the first deny names its line, an ask or a classify on a line is not a refusal; `unresolved_deny`, `UNRESOLVED_RULE`; a unit test scans the module for `std::process` and finds none |
| `builtin.rs` | `redact_rules()` — private-key blocks, AWS, GitHub, GitLab, Slack, Anthropic, OpenAI, Google, Stripe, npm, Hugging Face, SendGrid, DigitalOcean, age, JWT, bearer, a credential in a URL, a secret-looking assignment; `env_rules(names)` — one `EnvValue` rule `env:<NAME>` (`Origin::Builtin`, switchable by id) per variable whose name matches `SECRET_NAME` (token, secret, password, api/access/private key, credential, auth key), sorted and deduplicated, names only — the value meets the redactor at compile time and nowhere else; `guard_rules()` — the refusals (`command`, `path`; `refuse_with_hint` for a refusal that says what to do instead — `machine_browser`, whose `BROWSER_HINT` names the browser tools; `browser_test_runner` asks about a project's own end-to-end suite before it; both `platform_only` — `applies_to: Some(Host::Platform)` — as are `harness_fetch` and `harness_web_search`: the four steer an agent to the platform's own tools and pass a terminal harness over, which has none of them; `store_submission` asks before an app goes to the App Store or Google Play — `altool`, `notarytool`, `iTMSTransporter`, `fastlane deliver` and its kin, a Gradle publish — and `simulator_wipe` before every simulator is erased or deleted, ide/19) (recursive root deletes, `find -delete`, privilege escalation, download-to-shell, disk tools, force pushes — `--force-with-lease` too — hard resets, `git clean`, tree discards and history rewrites, mass permissions, signals to every process, system control, fork bombs, startup and scheduling files, the credential paths — `~/.config/gh`, `~/.config/glab-cli`, `~/.docker/config.json`, `~/.config/sops/age` among them, so an agent never reads a CLI's own store) then the classify rules (network — `gh` and `glab` are network words — remote, containers, publishing — `gh pr create\|merge`, `gh release create`, `gh repo create\|delete` and their `glab` equivalents). Stable ids, `Origin::Builtin`, switchable, undeletable. Its test module is the only place in the tree the destructive fixtures are spelled, and they are matched, never run; every path refusal has one fixture by name; `harness_fetch` and `harness_web_search` ask before the harness's own web tools with `HARNESS_WEB_HINT` (`tool_ask_with_hint`), since what they fetch is not screened |
| `classify.rs` | what the classifier is asked, whoever reads it — a generative reader's one line, or the Decision-Making Agent's typed choice ([15 — The Decision-Making Agent](../15-decision-making-agent.md)): `Subject { tool, summary, paths, cwd }`, `MessageSubject { author, role, scope, text }` (a message from another node), `prompt(&Subject)` / `message_prompt(&MessageSubject)` (the reviewer's instructions, the redacted call or message, *answer with one line*), `parse_verdict(text) -> Result<Verdict::{Safe, Harmful { reason }}, NoVerdict>` — the first non-empty line, fences and markers stripped, must be `SAFE` or start with `HARMFUL:`; `TOOL_HARMS` (`none` plus six named harms — destroys data, exfiltrates, escalates, changes the machine outside the project, publishes without consent, reaches an unexpected remote) and `MESSAGE_HARMS` (`none` plus four — injects instructions, asks for secrets, tells an agent to fetch and run something, impersonates the owner or the platform), each a decision model's options with the harm's own sentence as its meaning; `TOOL_QUESTION` / `MESSAGE_QUESTION` — the one instruction a decision model reads; `verdict_of_choice(harms, choice) -> Result<Verdict, NoVerdict>` — `NO_HARM` reads `Safe`, a harm in the list reads `Harmful` with its sentence, anything else is no verdict; `subject_digest` / `message_digest` for the per-process cache and the answer memory — tool, summary, paths and the working directory, so the same call in another checkout is another subject; `subject_block` / `message_block` / `content_block` — the state a decision model is shown; `PageSubject`, `CONTENT_BRIEF`, `CONTENT_HARMS`, `CONTENT_QUESTION`, `content_prompt`, `content_digest` for content an agent reads from outside ([11 § What an agent reads from outside](../11-security.md#what-an-agent-reads-from-outside)) |
| `net.rs` | the one outbound-network rule the platform's own HTTP obeys: `HostPolicy { allow, deny }` (the two `security.net.*` lists, concatenated across the workspace and the machine), `parse_hosts(&Value) -> (hosts, problems)` (an entry that is not `host[:port]` or `*.suffix` is a `HostProblem` by index), `host_matches(pattern, host)`, `decide_host(policy, declared, host) -> HostVerdict::{Allow { by: Declared · Policy }, Deny { reason }}` — deny wins, then a connector's own declared hosts, then the allow list, else refused with the connector's list in the reason — `HostPolicy`, `HostVerdict`, `HostAllowedBy`, `HostProblem`, `parse_hosts`, `host_matches`, `decide_host` |
| `policy.rs` | `SettingsLayer { redact_rules, redact_builtins_off, guard_rules, guard_builtins_off }` with `from_values` (a rule that is not a rule is a `Problem` by index, a list that is not a list is empty), `SecurityPolicy::from_layers(layers, env, env_names) -> (SecurityPolicy, Vec<Problem>)` — built-ins first (the patterns, then `env_rules(env_names)` — the names the engine passes while `security.redactor.env_auto` is on, none when it is off), switched off by the union of every layer's `builtins_off`, then each layer's own rules in order, `Origin::User` forced on them; `env_detectors` — how many built-in `EnvValue` rules are armed, a count for the status; `builtin(env)` (no environment read); `Problem { feature, rule, reason }`, `Feature::{Redactor, Guard}` |

---

## Entry points

`SecurityPolicy::from_layers` (the engine's `security::SecurityState::load`), `Redactor::redact` /
`redact_value`, `Vault::restore` / `restore_value`, `Guard::evaluate`, `classify::prompt` and
`parse_verdict`. The engine (`crates/bisa-engine/src/security.rs`) owns the seams; the node names
the rule types through the engine's re-exports.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| Redaction then restore is identity; the same secret reads the same; a text already redacted is left alone; an unknown placeholder is left and named | `redact.rs` tests |
| A JSON value keeps its shape — only string leaves change, keys never | `redact.rs::a_json_value_keeps_its_shape_and_only_its_strings_change` |
| The vault's `Debug` and a compiled redactor's never print a secret or an environment value | `redact.rs::the_vault_debug_prints_a_count_and_never_a_secret` |
| The first enabled rule wins; a disabled rule never matches; a bad regex or glob is a named problem and the rest still stand | `guard.rs` tests, `policy.rs::what_cannot_be_read_or_compiled_is_a_named_problem_not_a_failure` |
| A path rule sees file-tool inputs and command words alike, in every form — as written, absolute under the cwd, `~/…` under the home | `guard.rs::a_path_rule_sees_inputs_and_command_words_in_every_form`, `builtin.rs::path_rules_read_the_inputs_of_file_tools_too` |
| Every built-in compiles; ids are unique; every destructive shape is refused by the rule named for it — the machine's browser and a headless one among them, the refusal naming the browser tools — `browser_open`, `browser_snapshot`, the acts by ref — a project's own end-to-end suite asked instead; the everyday commands fall through; network, remote and publishing go to the classifier; from a terminal the four rules that steer to the platform's tools say nothing while every refusal that protects the machine holds, and the harness's own `WebFetch` is asked under its neutral name | `builtin.rs` tests (`the_rules_that_steer_to_the_platforms_tools_apply_to_its_sessions_alone`, `the_harnesss_own_web_tools_are_asked_and_pointed_at_the_browser_tools`) |
| A refusal reads the rule's label, and its hint after a dash when the rule carries one | `guard.rs::a_refusal_names_the_rule_and_its_hint_when_it_carries_one` |
| A rule for one host alone is passed over for a call from the other and the rules after it still read the call; a tool rule reads the harness's own name and its harness-neutral one, a prefix pattern the raw name alone; a rule without `applies_to` is stored as it always was | `guard.rs::a_rule_for_one_host_is_skipped_for_the_other`, `a_tool_rule_reads_the_harnesss_own_name_and_the_neutral_one`, `policy.rs::a_layer_rule_round_trips_through_json` |
| A tool pattern ending in `*` is a prefix — one MCP server's every tool, or every MCP tool — and an exact name is exact | `guard.rs::a_tool_pattern_ending_in_a_star_is_a_prefix` |
| A person's rule cannot claim to be built in; workspace rules come before this machine's; a switched-off built-in is listed off and redacts nothing | `policy.rs::layers_switch_builtins_off_and_add_rules_in_order` |
| Only the two answers parse; anything else is no verdict | `classify.rs::the_two_answers_parse_and_nothing_else_does` |
| Every brief and question the classifier is sent passes the built-in redactor untouched — one that spelt a placeholder's shape as `secret:kind:tag` reached the model mangled | `classify.rs::every_brief_and_question_passes_the_redactor_untouched` |
| The guard never spawns a process | `guard.rs::this_module_never_spawns_a_process` |
| Only a name that says secret becomes an environment detector; a value shorter than the auto minimum arms nothing; a layer switches one off by id; no name, no detector | `builtin.rs::env_rules_arm_only_the_names_that_say_secret_and_never_read_a_value`, `policy.rs::the_environment_names_become_switchable_builtin_detectors` |
| A script is read line by line, comments skipped, and the first refusal names its line; an ask on a line is not a refusal | `guard.rs::a_script_is_read_line_by_line_and_the_first_refusal_names_its_line` |
| The same call in another working directory is another subject | `classify.rs::the_digest_is_stable_and_distinguishes_calls` |

---

## Errors

`redact::BadRule` and `guard::BadRule` (`{ rule, reason }`) — a rule that did not compile — become a
`policy::Problem` the status route shows; `classify::NoVerdict(line)` — the engine turns it into a
question for the person. Nothing here panics on a person's rule.

---

## Extension points

| To add… | Touch, in order |
|---|---|
| a built-in rule | `builtin.rs` (a stable id, a label, the pattern or glob) → a fixture in its test module → the list in [11 — Security](../11-security.md) |
| a host rule | `net.rs` (`decide_host`'s order is the rule; a new pattern shape goes in `host_matches`) → the engine's `PolicyHostJudge` → [11 — Security](../11-security.md#outbound-hosts) |
| a detector or matcher kind | the enum arm in `redact.rs` or `guard.rs` → `compile` and the walk → `securityRules.mjs` (`detectorWords` / `matcherWords`, the editor's `Select`) → the guide |

---

## Tests

Unit tests beside every module; fixtures are synthetic (`ghp_` and thirty-six `x`, `AKIA` and
sixteen `A`, a PEM header with `FAKE` inside, an environment closure answering `fake-value-0001`, a
list of variable *names* with no process behind it).
No integration test directory: the crate is pure, and the engine's `tests/it/security.rs` drives it
through the seams.

---

## What this crate refuses to do

- depend on any crate of ours beyond the leaf `bisa-netrules`, or perform any I/O;
- run, spawn or shell out to anything it judges;
- keep a secret in a rule — a rule names a pattern or a variable, never a value;
- decide for the engine what a verdict *does* — it answers `Fallthrough`, not `Allow`, when no rule
  has an opinion;
- carve an exception for the platform's own SSH surface — `~/.ssh/**` stays denied to every agent;
  the `/git/ssh` routes are a person's, behind the bearer token, reached by no tool, and they read
  public key material only ([11 — Security](../11-security.md)).
