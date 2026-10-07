### Bisa — refusals: `error-<crate>-<variant>` for a typed error's sentence, and
### the node's own ad hoc refusals. Rendered into `ErrorBody.error` by the
### request's Accept-Language, and by the desktop and the CLI from `ErrorBody.text`.

## core: crates/bisa-core

# AddonError
error-core-addon-entry-missing = the bundle has no { $v0 }
error-core-addon-reserved-file = { $v0 } is a name the platform serves itself; the bundle may not carry one
error-core-addon-not-served-file = { $v0 } is not a file a bundle may carry
error-core-addon-bad-bundle-path = { $v0 } is not a path a bundle may carry
error-core-addon-too-many-files = the bundle holds { $found } files; the most is { $max }
error-core-addon-file-too-large = { $name } is { $bytes } bytes; the most one file may be is { $max }
error-core-addon-too-large = the bundle is { $bytes } bytes; the most is { $max }
error-core-addon-grant-not-declared = { $v0 } was never declared by the addon, so it cannot be granted
error-core-addon-not-enabled = addon { $v0 } is not enabled
error-core-addon-files-absent = addon { $v0 } has no files on this machine
error-core-addon-not-granted = addon { $id } was not granted { $word }

# AgentError
error-core-agent-empty-name = an agent needs a name
error-core-agent-empty-harness = an agent needs a harness
error-core-agent-core-id-reserved = the core ids (`general-agent`, `workflow-agent`) and the core origin belong to each other and to nothing else
error-core-agent-decision-making-agent-id-reserved = `decision-making-agent` is the Decision-Making Agent's id, and no agent record may take it
error-core-agent-core-cannot-be-disabled = a platform agent (`general-agent`, `workflow-agent`) cannot be disabled
error-core-agent-core-cannot-be-removed = a platform agent (`general-agent`, `workflow-agent`) cannot be removed
error-core-agent-core-name-fixed = a platform agent's name is fixed: `General Agent` and `Workflow Agent`
error-core-agent-core-field-fixed = only its harness, its model plan and its decision-making switch are editable, and this update changes `{ $v0 }`
error-core-agent-immutable = `{ $v0 }` is immutable

# AnswerError
error-core-answer-empty = an answer must carry a selection, some text, or "I'm not sure" — this carried nothing
error-core-answer-selection-on-decision = a decision gate is approve/decline; it offers no options to select
error-core-answer-unsure-on-decision = a decision gate is approve/decline — leave it pending rather than approving it while unsure
error-core-answer-unknown-option = option { $v0 } was never offered by this question
error-core-answer-multi-not-offered = this question takes one answer, not several
error-core-answer-many-recommended = a question may recommend at most one option
error-core-answer-empty-option-id = option ids must be non-empty
error-core-answer-duplicate-option-id = option id { $v0 } appears twice

# BoardError
error-core-board-unknown-column = unknown board column { $v0 }: one of backlog, todo, doing, done, archived
error-core-board-bad-date = not a calendar day { $v0 }: a due date is YYYY-MM-DD

# ChannelError
error-core-channel-permanent = the { $id } channel cannot be deleted
error-core-channel-general-shape = the general channel must be standing, workspace-visible, rostered everyone and of core origin
error-core-channel-everyone-reserved = only the general channel may be rostered everyone (refused for { $id })
error-core-channel-core-reserved = only the general channel may be of core origin (refused for { $id })
error-core-channel-direct-needs-audience = a direct message needs a restricted audience

# ConversationError
error-core-conversation-blank-title = a conversation's title is a line of words: not blank
error-core-conversation-title-too-long = a conversation's title is at most { $max } characters, not { $v0 }

# DecisionContractError
error-core-decision-contract-no-questions = a request asks at least one question
error-core-decision-contract-too-large = a request of { $bytes } bytes is too large; the most is { $max } — a question, not a document
error-core-decision-contract-empty-question-id = a question's id is never empty
error-core-decision-contract-empty-instructions = question `{ $v0 }` has no instructions
error-core-decision-contract-option-count = choice `{ $question }` offers { $found } options; it takes 2 to 255
error-core-decision-contract-empty-option = choice `{ $v0 }` has an option with no name
error-core-decision-contract-level-count = score `{ $question }` has { $found } levels; it takes 2 to 10
error-core-decision-contract-empty-model = the response names no model
error-core-decision-contract-unanswered = question `{ $v0 }` was not answered
error-core-decision-contract-unasked-answer = the response answers `{ $v0 }`, which was not asked
error-core-decision-contract-wrong-type = question `{ $question }` is a { $asked } and was answered as a { $answered }
error-core-decision-contract-unknown-choice = choice `{ $question }` answered `{ $choice }`, which is not one of its options
error-core-decision-contract-unknown-outcome = question `{ $question }` gives a probability to `{ $outcome }`, which it does not have
error-core-decision-contract-choice-without-probability = choice `{ $v0 }` gives its own choice no probability
error-core-decision-contract-legend-mismatch = score `{ $question }` has a legend of { $found } for { $levels } levels
error-core-decision-contract-score-off-the-legend = score `{ $question }` answered { $score }, which is off its legend
error-core-decision-contract-out-of-range = question `{ $question }`: { $field } is { $value }, outside 0 to 1
error-core-decision-contract-not-a-distribution = question `{ $question }`: its probabilities sum to { $sum }, not to one

# CoreError
error-core-invalid-principal = invalid principal id (expected 64 lowercase hex chars): { $v0 }
error-core-unknown-decision-word = unknown { $what } { $value }
error-core-unknown-effort = unknown effort { $v0 }: expected auto, minimal, low, medium, high, xhigh or max
error-core-unknown-kind = unknown GEP kind: { $v0 }
error-core-unknown-gate = unknown gate { $v0 }: expected approval, escalation or publish
error-core-unknown-goal-status = unknown goal status { $v0 }: expected draft, running, waiting, done, failed or closed
error-core-unknown-holder = unknown holder { $v0 }: expected you, agents, world, finished or design
error-core-unknown-scope-kind = unknown message scope { $v0 }: expected channel, goal or conversation
error-core-thinking-too-large = a reply's thinking is { $bytes } bytes; the most kept is { $max }
error-core-text-too-large = a message of { $bytes } bytes is too long; the most is { $max } — send what is a file as an attachment
error-core-unknown-file-scope = unknown file scope { $v0 }: expected goal, workstream or work_item
error-core-unknown-artifact-kind = unknown artifact kind { $v0 }
error-core-unknown-activity-concept = unknown activity concept { $v0 }: expected workspace, goals, workflows, projects, channels, agents or node
error-core-too-many-artifacts = a message carries at most { $max } artifacts, not { $v0 }
error-core-invalid-artifact-name = invalid artifact name { $v0 }: a file name, not a path
error-core-invalid-artifact-title = invalid artifact title { $v0 }: a title is a line of words
error-core-artifact-title-too-long = artifact title is { $bytes } bytes; the most is { $max }
error-core-artifact-too-large = artifact is { $bytes } bytes; the most is { $max }
error-core-unknown-scope = unknown settings scope { $v0 }: expected machine, workspace or project
error-core-unknown-goal-mode = unknown goal mode { $v0 }: expected auto, guided or manual
error-core-invalid-assignee = invalid assignee { $v0 }: expected agent:<id>, human:<64 hex> or team:<id>
error-core-invalid-slug = invalid project slug { $v0 }: expected 1-64 chars of lowercase a-z, 0-9, '-' or '_', starting with a letter or digit
error-core-invalid-id = invalid { $what } id { $value }: expected 1-64 chars of a-z, 0-9, '-', '_', '.', ':', starting with a letter or digit
error-core-invalid-word = invalid { $what } { $value }: expected 1-32 chars of a-z, 0-9, '-', '_', starting with a letter
error-core-invalid-branch = invalid branch { $v0 }: expected 1-64 chars, not blank
error-core-invalid-rel-path = invalid relative path { $v0 }: no leading slash, no '..' component
error-core-invalid-hash = invalid sha256 { $v0 }: expected 64 lowercase hex chars
error-core-invalid-range = invalid line range { $start }..={ $end }: lines are 1-based and end >= start
error-core-context-too-large = message context is { $bytes } bytes; the cap is { $a0 }
error-core-invalid-tag = invalid tag { $v0 }: expected letters, digits and separators only, e.g. `engineering` or `go-to-market`
error-core-unknown-tag-entity = unknown tag entity { $v0 }: expected one of agent, team, channel, skill, mcp, project, goal, workflow, connector
error-core-too-many-tags = too many tags ({ $v0 }): at most { $max } per object

# GitProfileError
error-core-git-profile-empty-label = a profile needs a label
error-core-git-profile-host = { $v0 } is not a host name: letters, digits, dots and dashes, nothing else
error-core-git-profile-alias = { $v0 } is not an SSH host alias: letters, digits, dots, dashes and underscores
error-core-git-profile-owner = { $v0 } is not an owner: a user, an organization, a workspace or a group path like acme/platform — segments of letters, digits, dots, underscores and dashes joined by /, up to 255
error-core-git-profile-name = { $v0 } is not a name git will take: one line, not starting with a dash
error-core-git-profile-email = { $v0 } is not an email: one @ with something on both sides, no spaces
error-core-git-profile-ssh-key = { $v0 } is not a key path the platform will name: absolute, and only letters, digits and . _ / @ + -
error-core-git-profile-account = { $v0 } is not a code host login: letters, digits, dots, underscores and dashes, starting with a letter or digit, up to 255

# ClaimRefusal
error-core-claim-refusal-unknown-or-used = this invite code is unknown or was already used
error-core-claim-refusal-expired = this invite has expired — ask for a new one
error-core-claim-refusal-revoked = this invite was withdrawn
error-core-claim-refusal-awaiting-host = the host asked to admit people by hand; you will hear back

# InviteError
error-core-invite-owner-role = an invite names a hosted role, never the owner
error-core-invite-no-life = an invite expires after it is made

# McpError
error-core-mcp-empty-name = an MCP server needs a name
error-core-mcp-reserved-name = `{ $max }` is reserved: it is the platform's own MCP server name and cannot be registered
error-core-mcp-empty-command = a stdio server needs a command to run
error-core-mcp-relative-cwd = the working directory must be an absolute path, not `{ $v0 }`
error-core-mcp-invalid-url = the URL must be absolute and start with http:// or https://, not `{ $v0 }`
error-core-mcp-bad-header-name = `{ $v0 }` is not a header name: letters, digits and !#$%&'*+-.^_`|~ only
error-core-mcp-bad-header-value = the header `{ $v0 }` carries a control character in its value

# MemberError
error-core-member-owner-immutable = the workspace owner cannot be removed or demoted
error-core-member-second-owner = a workspace has exactly one owner

# NoteError
error-core-note-empty-title = a note needs a title
error-core-note-too-large = note body is { $v0 } bytes; the cap is { $max }
error-core-note-bad-front-matter = the note's front matter is not readable: { $v0 }
error-core-draw-empty-title = a drawing needs a title
error-core-draw-title-too-long = a drawing's title is { $chars } characters; the cap is { $max }
error-core-draw-too-large = the scene is { $bytes } bytes; the cap is { $max }
error-core-draw-too-many-elements = { $count } elements; the cap is { $max } in a drawing and { $per_call } in one call
error-core-draw-element-refused = the canvas draws no { $kind }: it is vector only
error-core-draw-bad-element = not an element the canvas draws: { $why }

# PetError
error-core-pet-animation = pet { $id }: { $why }

# PhotoRefusal
error-core-photo-refusal-not-a-hash = { $word }.sha256 is not a content hash
error-core-photo-refusal-not-held = { $word } is not on this machine — upload it first
error-core-photo-refusal-not-a-picture = { $word } is not a picture
error-core-photo-refusal-too-large = { $word } is { $bytes } bytes; the limit is { $max } — the desktop scales a { $word } to a { $edge } px square

# ReviewNoteError
error-core-review-note-empty-body = a review note needs a body

# RunError
error-core-run-not-started = the run has not started
error-core-run-no-start = the workflow has no start step; a run cannot begin
error-core-run-amend-changes-workflow = the amendment is workflow { $got }, but this run is running { $expected }
error-core-run-amend-needs-input = the amendment cannot bind this run's inputs: { $v0 }
error-core-run-already-started = the run has already started
error-core-run-finished = the run is finished; nothing moves
error-core-run-unknown-step = step `{ $step }` is not in this run
error-core-run-not-live = cannot { $event } step `{ $step }`: it is { $state }, not { $expected }
error-core-run-wrong-kind = cannot { $event } step `{ $step }`: it is { $kind }, and only { $expected } accepts that
error-core-run-bad-answer = step `{ $step }`: { $source }
error-core-run-amend-touches-started-step = an amendment may not change step `{ $step }`, which has started
error-core-run-stale-boundary = boundary event `{ $boundary }` of step `{ $step }` no longer applies
error-core-run-unknown-boundary = step `{ $step }` has no boundary event `{ $boundary }`

# SettingsError
error-core-settings-unknown-key = unknown setting `{ $v0 }`
error-core-settings-scope-not-allowed = `{ $key }` cannot be set at { $scope } scope; allowed: { $allowed }
error-core-settings-invalid-value = invalid value for `{ $key }`: { $why }

# SkillError
error-core-skill-empty-name = a skill needs a name
error-core-skill-empty-description = a skill needs a one-line description: it is the only part a model reads before opening it
error-core-skill-too-large = skill body is { $v0 } bytes; the cap is { $max }

# TeamError
error-core-team-empty-name = a team needs a name
error-core-team-nested-team = a team may not contain another team (`team:{ $v0 }`)
error-core-team-core-agent-stored = a platform agent is a participant of every team and is never stored in one

# TemplateError
error-core-template-unbalanced = unbalanced brace at byte { $at }: every `{"{"}` needs its `{"}"}`
error-core-template-unknown = unknown placeholder {"{"}{ $v0 }{"}"}: the roots are inputs.<name>, steps.<id>.output[.<path>], steps.<id>.answer, goal.statement and goal.title; to write a brace literally, double it ({"{"}{"{"} and {"}"}{"}"})
error-core-template-event-outside-mapping = {"{"}{ $v0 }{"}"} reads the event that began the run, which only a start step's input mapping reads: map it onto an input there and read {"{"}inputs.<name>{"}"} here
error-core-template-unknown-in-mapping = unknown placeholder {"{"}{ $v0 }{"}"}: a start step's input mapping reads event.<path> only; to write a brace literally, double it ({"{"}{"{"} and {"}"}{"}"})
error-core-template-unknown-in-start-event = unknown placeholder {"{"}{ $v0 }{"}"}: a start event's fields read inputs.<name> only — the inputs given when it was turned on; to write a brace literally, double it ({"{"}{"{"} and {"}"}{"}"})
error-core-template-unknown-in-connector = unknown placeholder {"{"}{ $v0 }{"}"}: a connector definition reads params.<name> and account.<name> only; to write a brace literally, double it ({"{"}{"{"} and {"}"}{"}"})
error-core-template-unresolved = {"{"}{ $key }{"}"} has no value in this run: { $why }

# InputError
error-core-input-wrong-kind = input `{ $input }` wants { $want }, got { $got }
error-core-input-missing = input `{ $input }` is required and has no default
error-core-input-unknown = the workflow declares no input named { $a0 }

# WorkItemError
error-core-work-item-illegal = cannot { $transition } a work item that is { $from }
error-core-work-item-terminal = the work item is { $v0 }; nothing moves

# WorkstreamError
error-core-workstream-illegal = cannot { $transition } a workstream that is { $from }
error-core-workstream-terminal = the workstream is closed; nothing moves
error-core-workstream-primary = cannot { $transition } the primary workstream: it is the project itself — remove the project instead
error-core-workstream-no-repository = workstream { $id } is a copy of a non-git project, not a git checkout
error-core-workstream-no-own-branch = workstream { $id } is the project's primary: it has no branch of its own to do that with

## store: crates/bisa-store

# StoreError
error-store-io = io error at { $path }: { $source }
error-store-key-store = key store error: { $v0 }
error-store-key-not-found = key not found: { $v0 }
error-store-nostr = nostr error: { $v0 }
error-store-encryption-required = encryption required for kind { $v0 } but no owner key available
error-store-sqlite = sqlite error: { $v0 }
error-store-serde = serialization error: { $v0 }
error-store-goal-not-found = goal not found: { $v0 }
error-store-work-item-not-found = work item not found: { $v0 }
error-store-session-not-found = session not found: { $v0 }
error-store-workstream-not-found = workstream not found: { $v0 }
error-store-project-not-found = project not found: { $v0 }
error-store-conversation-not-found = conversation not found: { $v0 }
error-store-workflow-not-found = workflow not found: { $v0 }
error-store-run-not-found = run not found: { $v0 }
error-store-definition-not-found = { $kind } not found: { $id }
error-store-run-not-finished = goal { $goal } has an unfinished run { $run }; finish or stop it first
error-store-run-not-queued = run { $run } of goal { $goal } is not queued
error-store-already-dispatched = signal { $signal } already began run { $run }
error-store-workflow-invalid = { $a0 }
error-store-revision-conflict = { $kind } { $id } is at revision { $actual }; you edited revision { $expected } — reload and merge
error-store-already-library = workflow { $v0 } is already in the library
error-store-unreadable = { $path }: not a { $what } this build can read ({ $reason }). The workspace was written by another shape of the code; move it aside with scripts/reset-dev-workspace and start fresh
error-store-gate-decision-invalid = gate decision { $v0 } not found or does not authorize this step
error-store-budget-exhausted = budget exhausted for goal { $v0 }
error-store-recall-conflict = recall conflict: the stored value changed (current hash { $current_hash })
error-store-edit-conflict = { $what } conflict: it changed since you read it (current hash { $current_hash })

## engine: crates/bisa-engine

# DesignRefusal
error-engine-design-refusal-manual-goal = goal { $v0 } is manual; design it on the Workflow tab, or ask the Workflow Agent in the conversation
error-engine-design-refusal-closed = goal { $v0 } is closed
error-engine-design-refusal-has-workflow = goal { $v0 } already has a workflow; adopt it, edit it, or clear it first
error-engine-design-refusal-has-run = goal { $v0 } has a run; a workflow is designed before a run, and repaired after a failed one
error-engine-design-refusal-busy = the Workflow Agent is already working on goal { $v0 }
error-engine-design-refusal-design-off = designing is off on this node

# InteractiveError
error-engine-interactive-unknown-harness = no harness { $v0 } in this workspace
error-engine-interactive-not-interactive = { $v0 } has no interactive form
error-engine-interactive-unknown-session = no interactive session { $v0 }
error-engine-interactive-bad-secret = the session secret does not match
error-engine-interactive-io = { $v0 }

# EngineError
error-engine-unknown-gate = unknown gate: { $v0 }
error-engine-gate-already-decided = gate already decided: { $v0 }
error-engine-io = io error: { $v0 }
error-engine-ssh-unavailable = SSH is not configured for this node: { $v0 }
error-engine-iso = isolation unavailable: { $v0 }
error-engine-iso-failed = isolation failed: { $v0 }
error-engine-security = refused by the guard: { $v0 }
error-engine-own-pull-request = @{ $author } opened this pull request, so the code host takes a comment from you but not an approval or a change request — ask a reviewer, or an agent
error-engine-nothing-to-commit = { $v0 } has nothing to commit
error-engine-project-ambiguous = step `{ $step }` names no project and the goal has { $count }; name one with a project input
error-engine-identity-unset = nobody is set to commit in project { $project }: set who commits in this repository — Git → Repository in the IDE, or `bisa project identity { $project } --name … --email …`
error-engine-repo-identity-unset = nobody is set to commit { $what }: set who commits under the panel's Who commits…, or your global git identity under Settings › Git & code hosts › Identity
error-engine-pull-not-offered = the { $what } repository offers no pull: its records arrive by sync
error-engine-publish-manual = project { $project } publishes manually; a person must { $what }
error-engine-publish-no-goal = project { $project } is gated and workstream { $workstream } belongs to no goal, so there is nobody to ask; { $what } yourself, attach the project to a goal, or set publishing to automatic
error-engine-publish-declined = the publish gate was declined: { $what }
error-engine-nothing-to-publish = { $branch } has no commits beyond { $base }; commit first (workstream { $workstream })
error-engine-pull-request-not-open = pull request #{ $number } is { $state }, not open — only an open pull request can be taken up as a workstream
error-engine-workstream-script = the { $phase } script { $reason }
error-engine-file-conflict = { $path } changed since you read it
error-engine-locked = another engine holds this workspace ({ $path }{ $a0 })

# LifecycleError

# RegistryError
error-engine-registry-cas-failed = agent already registered (or expectation mismatch)
error-engine-registry-stale = stale mutation: expected generation { $expected }, current { $current }
error-engine-registry-aborted = agent { $v0 } is aborted (terminal)
error-engine-registry-not-found = agent { $v0 } not found

# ScheduleRejection
error-engine-schedule-rejection-not-open = work item is not open (state: { $v0 })
error-engine-schedule-rejection-budget-exhausted = budget exhausted for { $v0 }
error-engine-schedule-rejection-goal-missing = goal not found: { $v0 }
error-engine-schedule-rejection-run-missing = run not found: { $v0 }
error-engine-schedule-rejection-harness-disabled = harness { $v0 } is disabled by configuration
error-engine-schedule-rejection-depth-exhausted = spawn depth exhausted
error-engine-schedule-rejection-spawn-not-allowed = agent { $agent } not in spawn allowlist { $allowlist }
error-engine-schedule-rejection-goal-closed = goal { $v0 } is closed
error-engine-schedule-rejection-run-finished = run { $v0 } is over
error-engine-schedule-rejection-no-step = work item is bound to no run step
error-engine-schedule-rejection-step-not-running = step `{ $step }` is { $state }, not running this item
error-engine-schedule-rejection-caps-closed = the concurrency caps are closed; nothing launches any more

## decision: crates/bisa-decision

# ProviderError
error-decision-provider-misconfigured = the decision provider is not set up: { $v0 }
error-decision-provider-refused = the decision provider refused the request ({ $status }): { $message }
error-decision-provider-busy = the decision provider is busy ({ $status })
error-decision-provider-unreachable = the decision provider cannot be reached: { $v0 }
error-decision-provider-unreadable = the decision provider's answer cannot be read: { $v0 }
error-decision-provider-contract = the decision provider's answer breaks the contract: { $v0 }
error-decision-provider-timed-out = the decision provider did not answer in time

## collab: crates/bisa-collab

# FaceRefusal
error-collab-face-refusal-too-long = the face is { $chars } characters of base64; the limit is { $max }
error-collab-face-refusal-not-base64 = the face is not base64
error-collab-face-refusal-hash-lies = the face's bytes do not hash to { $expected }
error-collab-face-refusal-not-a-face = the face is not a picture within the cap

# CollabError
error-collab-wrap = wrap: { $v0 }
error-collab-invite-code = invite code: { $v0 }
error-collab-relay = relay: { $v0 }

## guest: crates/bisa-guest

# GuestError
error-guest-store = guest store: { $v0 }
error-guest-not-hosted = not a member of that host
error-guest-refused = { $v0 }
error-guest-io = { $detail }
error-guest-json = { $detail }

## harness: crates/bisa-harness

# CatalogError
error-harness-catalog-reserved-id = custom harness id { $v0 } collides with a reserved builtin id
error-harness-catalog-invalid-descriptor = invalid descriptor { $path }: { $message }
error-harness-catalog-io = i/o error reading { $path }: { $message }

# HarnessError
error-harness-unavailable = harness unavailable: { $v0 }
error-harness-model-unavailable = model unavailable: { $model } ({ $reason })
error-harness-busy = session busy
error-harness-not-supported = capability not supported: { $v0 }
error-harness-protocol = protocol error: { $v0 }
error-harness-io = i/o error: { $v0 }
error-harness-terminated = session terminated

## net: crates/bisa-net

# NetError
error-net-config = net config: { $v0 }
error-net-relay = relay: { $v0 }
error-net-io = { $detail }

## mcp: crates/bisa-mcp

# IntakeError
error-mcp-intake-connect = intake socket unavailable at { $path }: { $source }
error-mcp-intake-io = intake connection broke mid-call: { $v0 }
error-mcp-intake-bad-reply = intake replied with invalid JSON: { $v0 }
error-mcp-intake-closed = intake closed the connection

## vcs: crates/bisa-vcs

# VcsError
error-vcs-not-available = vcs unavailable: { $v0 }
error-vcs-not-a-repository = { $a0 } is not a git repository
error-vcs-dirty = { $a0 } has uncommitted changes: { $details }
error-vcs-conflict = conflict: { $message }
error-vcs-not-fast-forward = not a fast-forward: { $ahead } ahead and { $behind } behind the upstream
error-vcs-in-progress = a { $v0 } is in progress; resolve or abort it first
error-vcs-nothing-to-stash = nothing to stash
error-vcs-nothing-to-discard = nothing to discard: the selected files have no change left here
error-vcs-stash-moved = stash@{"{"}{ $index }{"}"} no longer holds { $commit }; the stash list moved — reload it
error-vcs-no-remote = no usable remote: { $v0 }
error-vcs-not-authenticated = not authenticated: { $v0 }
error-vcs-identity-unset = git identity unset: { $v0 }
error-vcs-repository-busy = another git process is using { $a0 }; try again once it has finished
error-vcs-invalid-arg = invalid { $what }: { $value }
error-vcs-timeout = { $what } timed out after { $secs }s
error-vcs-command = { $what } (exit { $code }): { $stderr }
error-vcs-other = { $v0 }

## codehost: crates/bisa-codehost

# CliError

# CodeHostError
error-codehost-code-host-not-authenticated = not authenticated with the code host: { $v0 }
error-codehost-code-host-not-found = not found on the code host: { $v0 }
error-codehost-code-host-refused = the code host refused: { $v0 }
error-codehost-code-host-unsupported = unsupported by this code host: { $v0 }
error-codehost-code-host-transport = code host unreachable: { $v0 }

## connectors: crates/bisa-connectors

# ConnectorError
error-connectors-connector-bad-definition = the connector definition is unusable: { $v0 }
error-connectors-connector-bad-param = parameter `{ $name }`: { $why }
error-connectors-connector-unresolved = {"{"}{ $v0 }{"}"} has no value
error-connectors-connector-host-refused = { $host } is not a host this connector may reach ({ $allowed })
error-connectors-connector-not-authenticated = not authenticated: { $v0 }
error-connectors-connector-not-found = not found: { $v0 }
error-connectors-connector-refused = the service refused ({ $status }): { $reason }
error-connectors-connector-rate-limited = rate limited; try again in { $retry_after }s
error-connectors-connector-upstream = the service failed ({ $status }): { $reason }
error-connectors-connector-unreachable = could not connect: { $v0 }
error-connectors-connector-transport = the connection failed: { $v0 }
error-connectors-connector-timeout = timed out after { $v0 }
error-connectors-connector-open = { $host } failed { $a0 } calls in a row; calls to it are paused for { $until_secs }s
error-connectors-connector-select-missing = the answer has no `{ $path }`
error-connectors-connector-too-large = the answer is { $v0 } bytes; the cap is 1 MiB
error-connectors-connector-o-auth = OAuth: { $v0 }
error-connectors-connector-store = credential store: { $v0 }

## ssh: crates/bisa-ssh

# SshError
error-ssh-unavailable = ssh unavailable: { $v0 }
error-ssh-refused = { $v0 }
error-ssh-timeout = { $what } timed out after { $secs }s
error-ssh-failed = { $what }: { $detail }

## mobile-development: crates/bisa-mobile-development

# MobileDevelopmentError
error-mobile-development-not-installed = { $v0 } is not installed on this machine
error-mobile-development-timeout = { $what } timed out after { $secs }s
error-mobile-development-failed = { $what }: { $detail }
error-mobile-development-no-such-device = no device { $v0 }
error-mobile-development-unsupported = { $v0 }

## iso: crates/bisa-iso

# IsoError
error-iso-unavailable = isolation backend unavailable: { $v0 }
error-iso-other = { $v0 }

## lsp: crates/bisa-lsp

# LspError
error-lsp-spawn = could not start the language server: { $v0 }
error-lsp-protocol = the language server broke protocol: { $v0 }
error-lsp-closed = the language server is not running
error-lsp-timeout = the language server did not answer in time: { $v0 }
error-lsp-server-error = the language server refused: { $message } (code { $code })
error-lsp-no-server = no language server for { $v0 }

## The engine's and the store's own refusals — a `Text` where a sentence in a `String` was (crates/bisa-engine, crates/bisa-store).

error-engine-conflict-checkout-holds-refuse-so-tree-stays-commit = this checkout holds { $why }; `{ $dirty_close_key }` is `refuse`, so its tree stays — commit or discard the work, or close the workstream and keep the tree
error-engine-conflict-conversation-not-planning = this conversation is not in plan mode: there is no plan to build
error-engine-conflict-goal-has-no-run-restart-start-one = goal { $goal_id } has no run to restart; start one
error-engine-conflict-project-already-repository = project { $a0 } is already a repository
error-engine-conflict-restart-entry-gone = the run began at `{ $step }`, which { $workflow } no longer has; start a new run instead
error-engine-conflict-run-is-goal-s-act-from-goal = run { $run } is goal { $goal }'s; stop or restart it from its goal
error-engine-conflict-run-not-queued-only-queued-run-withdrawn = run { $run_id } is { $a0 }, not queued; only a queued run is withdrawn
error-engine-conflict-signal-not-held = signal { $id } is { $state }, not held; only a held signal is let through
error-engine-effects-refused = { $detail }
error-engine-ide-layout-refused = { $detail }
error-engine-ide-review-refused = { $detail }
error-engine-invalid-account-has-no-oauth-client-id-yet = account { $account } of { $connector } has no OAuth client id yet; add the client id (and secret) from the platform's developer console first
error-engine-invalid-adopted-folder-bisa-will-not-delete-delete = { $a0 } is an adopted folder; Bisa will not delete it. Delete the record without the tree and remove the folder yourself.
error-engine-invalid-adoption-needs-inputs = the adoption needs its inputs: { $e }
error-engine-invalid-adoption-was-already-decided = the adoption of { $subject } was already decided
error-engine-invalid-agent-missing-disabled-so-there-nobody-ask = agent { $name } is missing or disabled, so there is nobody to ask
error-engine-invalid-agent-not-found = agent not found: { $id }
error-engine-invalid-already-exists = { $relative } already exists
error-engine-invalid-already-exists-2 = { $to } already exists
error-engine-invalid-already-exists-refusing-import-into-rather-than = { $a0 } already exists; refusing to import into it rather than merge with what is there
error-engine-invalid-already-exists-send-base-hash-what-you = { $relative } already exists; send the base_hash of what you read to overwrite it
error-engine-invalid-amendment-can-no-longer-be-applied-propose = the amendment can no longer be applied: { $e }; propose it again
error-engine-invalid-amendment-subject-names-no-held-workflow = amendment subject { $subject } names no held workflow
error-engine-invalid-amendment-was-already-decided = the amendment { $subject } was already decided
error-engine-invalid-answer-does-not-fit-step-s-output = the answer of { $connector }.{ $operation } does not fit the step's output schema: { $a0 }
error-engine-invalid-api-key-never-empty = an API key is never empty
error-engine-invalid-asking-failed = asking { $agent } failed: { $e }
error-engine-invalid-author-no-longer-member-message-not-delivered = the author of { $event_id } is no longer a member; the message is not delivered
error-engine-invalid-branch-exists-open-workstream-from-branch-instead = branch { $intended } exists — open a workstream from the branch instead of a new one
error-engine-invalid-check-goes-http-https-url-not = a check goes to an http:// or https:// URL, not { $a0 }://
error-engine-invalid-check-needs-host = a check needs a host
error-engine-invalid-clone-needs-url = clone needs a url
error-engine-invalid-connection-was-already-completed = the connection was already completed
error-engine-invalid-connector-authenticates-with-not-oauth-set-secrets = connector { $a0 } authenticates with { $a1 }, not OAuth; set its secrets instead
error-engine-invalid-connector-catalog-s-not-edited-write-your = connector { $a0 } is the catalog's and is not edited; write your own definition under another id
error-engine-invalid-connector-has-no-account-here-add-one = connector { $a0 } has no account here; add one under Settings › Capabilities › Connectors › { $a1 }, or name one on the step
error-engine-invalid-connector-has-no-operation = connector { $connector } has no operation `{ $operation }`
error-engine-invalid-connector-s-permit-pool-closed = the connector's permit pool is closed
error-engine-invalid-could-not-move-trash = could not move { $a0 } to the trash: { $e }
error-engine-invalid-could-not-move-trash-several = could not move { $n } entries to the trash: { $e }
error-engine-invalid-device-answered-bytes-frame-most = the device answered { $a0 } bytes for a frame; the most is { $max_frame_bytes }
error-engine-invalid-did-not-answer-within-s = { $agent } did not answer within { $a0 }s
error-engine-invalid-stopped-before-answered = { $agent } was stopped before it answered
error-engine-invalid-directory = { $relative } is a directory
error-engine-invalid-directory-holding-entries-confirm-with-recursive-true = { $relative } is a directory holding { $n } entries; confirm with recursive=true
error-engine-invalid-ended-turn-without-saying-anything = { $agent } ended its turn without saying anything
error-engine-invalid-exclude-glob = exclude glob { $g }: { $e }
error-engine-invalid-gate-not-this-home-s = gate { $a0 } is not one of { $home }'s
error-engine-invalid-globs = globs: { $e }
error-engine-invalid-goal-already-has-design-revision-send-revision = goal { $goal_id } already has a design at revision { $a0 }; send the revision you edited
error-engine-invalid-goal-closed = goal { $goal_id } is closed
error-engine-invalid-goal-has-live-queued-run-amend-live = goal { $goal_id } has a live or a queued run; amend the live one, or stop the goal
error-engine-invalid-goal-has-no-run-amend = goal { $goal_id } has no run to amend
error-engine-invalid-goal-has-no-workflow-pick-one-let = goal { $goal_id } has no workflow; pick one, or let the Workflow Agent propose one
error-engine-invalid-goal-not-attached-project-attach-project-goal = goal { $goal } is not attached to project { $project }. Attach the project to the goal first.
error-engine-invalid-goal-points-no-workflow-adopt = goal { $goal_id } points at no workflow to adopt
error-engine-invalid-has-no-default-account-key = { $kind } has no default account key
error-engine-invalid-has-no-directory-yet-so-there-nothing = the { $a0 } { $id } has no directory yet, so there is nothing to search
error-engine-invalid-has-no-directory-yet-so-there-nothing-2 = the { $a0 } { $id } has no directory yet, so there is nothing to watch
error-engine-invalid-home-has-no-pending-gate = { $home } has no pending gate
error-engine-invalid-home-has-pending-gates-name-one = { $home } has { $a0 } pending gates; name one: { $a1 }
error-engine-invalid-human-step-needs-answer = a human step needs an answer
error-engine-invalid-import-task-did-not-finish = the import task did not finish: { $e }
error-engine-invalid-include-glob = include glob { $g }: { $e }
error-engine-invalid-input-has-no-value-run = input `{ $input }` has no value in this run
error-engine-invalid-input-not-assignee = input `{ $input }` is not an assignee: { $e }
error-engine-invalid-input-not-project-id = input `{ $input }` is not a project id: { $s }
error-engine-invalid-input-should-hold-assignee-got = input `{ $input }` should hold an assignee, got { $value }
error-engine-invalid-input-should-hold-cron-expression-got = input `{ $input }` should hold a cron expression, got { $value }
error-engine-invalid-input-should-hold-number-seconds-got = input `{ $input }` should hold a number of seconds, got { $value }
error-engine-invalid-input-should-hold-project-id-got = input `{ $input }` should hold a project id, got { $v }
error-engine-invalid-inside = { $to } is inside { $from }
error-engine-invalid-invite-not-waiting-you = invite { $id } is { $a0 }, not waiting on you
error-engine-invalid-layout-bytes-cap = the layout is { $a0 } bytes; the cap is { $max_layout_bytes }
error-engine-invalid-listening-input-kind = input `{ $name }` should be { $want }, got { $got }
error-engine-invalid-listening-needs-inputs = turning it on needs a value for { $names }
error-engine-invalid-listening-no-event-start = { $workflow } has no start on an event; there is nothing to listen for
error-engine-invalid-listening-not-public-hook = step `{ $step }` is not a public hook start
error-engine-invalid-listening-start-unresolved = the start `{ $step }` cannot listen with these inputs: { $e }
error-engine-invalid-listening-unknown-input = { $workflow } declares no input named `{ $name }`
error-engine-invalid-lsp-lock = lsp lock
error-engine-invalid-message-s-author-not-agent-only-agent = the message's author `{ $other }` is not an agent; only an agent speaks for a workflow
error-engine-invalid-mobile-development-task = mobile task: { $e }
error-engine-invalid-no-connection-was-started-account-start-first = no connection was started for account { $account } of { $connector }; start it first
error-engine-invalid-no-held-message = no held message { $event_id }
error-engine-invalid-no-language-server-applies = no language server applies to { $path }
error-engine-invalid-no-language-server-configured = no language server is configured for { $language }
error-engine-invalid-no-model-left-unavailable = no model left for { $agent }: { $a0 } is unavailable ({ $a1 })
error-engine-invalid-no-review-notes-send = no review notes to send
error-engine-invalid-no-session-agent = no session for agent { $agent }: { $a0 }
error-engine-invalid-no-such-path = no such path: { $from }
error-engine-invalid-no-such-path-2 = no such path: { $relative }
error-engine-invalid-no-such-turn = no such turn: { $turn }
error-engine-invalid-no-such-turn-changed-nothing-past-turns = no such turn: { $turn } — it changed nothing, or it is past the turns kept restorable
error-engine-invalid-nope = nope
error-engine-invalid-not-empty-refusing-clone-into = { $a0 } is not empty; refusing to clone into it
error-engine-invalid-not-goal-id = { $id } is not a goal id
error-engine-invalid-not-id-layout-can-be-filed-under = { $id } is not an id a layout can be filed under
error-engine-invalid-not-request-editor-may-proxy = { $method } is not a request the editor may proxy
error-engine-invalid-not-run-id = { $id } is not a run id
error-engine-invalid-not-text-keep-undo-whole-file = { $path } is not text: keep or undo the whole file
error-engine-invalid-not-url-check = not a URL to check: { $e }
error-engine-invalid-not-workflow-id = { $held } is not a workflow id
error-engine-invalid-nothing-named-delete = nothing was named to delete
error-engine-invalid-nothing-waiting-word = nothing of { $path } is waiting for a word
error-engine-invalid-only-goal-owes-this-decision = only a goal owes a decision on { $subject }; a run of the workspace has none to make
error-engine-invalid-patch-bytes-cap = patch is { $a0 } bytes; the cap is { $max_patch_bytes }
error-engine-invalid-patch-bytes-cap-2 = patch is { $a0 } bytes; the cap is { $a1 }
error-engine-invalid-patch-empty = patch is empty
error-engine-invalid-path-not-under-root = the path is not under the root
error-engine-invalid-pattern-not-valid-regex = the pattern is not a valid regex: { $e }
error-engine-invalid-project-archived-unarchive-before-opening-workstream = project { $a0 } is archived; unarchive it before opening a workstream
error-engine-invalid-project-archived-unarchive-before-step-runs = project { $a0 } is archived; unarchive it before a step runs in it
error-engine-invalid-project-has-external-root-importing-would-write = project { $a0 } has an external root; importing would write into a folder that was adopted  on the promise that Bisa never writes into it
error-engine-invalid-project-has-no-folder = project { $a0 } has no folder at { $a1 }
error-engine-invalid-project-points-which-not-directory = project { $a0 } points at { $a1 }, which is not a directory
error-engine-invalid-project-s-default-branch-forced-push-only = { $branch } is the project's default branch; a forced push is only for a workstream's own branch
error-engine-invalid-project-s-default-branch-not-deleted-from = { $branch } is the project's default branch; it is not deleted from here
error-engine-invalid-project-s-default-branch-not-renamed-from = { $from } is the project's default branch; it is not renamed from here
error-engine-invalid-provider-takes-no-api-key = the `{ $a0 }` provider takes no API key
error-engine-invalid-pull-request-s-branch-not-origin-branch = pull request #{ $number }'s branch { $a0 } is not on origin — a branch from a fork cannot be opened here ({ $e })
error-engine-invalid-refusing-import-holds-more-than-files-folders = refusing to import { $a0 }: it holds more than { $a1 } files and folders (the import limit).  Link it in place instead — a linked folder stays where it is, and Bisa never writes into it.
error-engine-invalid-refusing-import-holds-more-than-import-limit = refusing to import { $a0 }: it holds more than { $a1 } (the import limit).  Link it in place instead — a linked folder stays where it is, and Bisa never writes into it.
error-engine-invalid-remote-needs-url = a remote needs a URL
error-engine-invalid-review-note-needs-body = a review note needs a body
error-engine-invalid-run-finished-propose-new-workflow-start-another = run { $a0 } is finished; propose a new workflow and start another run instead
error-engine-invalid-run-names-device = a run names a device
error-engine-invalid-saved-layout-not-layout-file = the saved layout of the { $a0 } { $id } is not a layout file: { $e }
error-engine-invalid-search-pattern-empty = the search pattern is empty
error-engine-invalid-signal-name = `{ $name }` is not a signal name: dot-separated parts of lowercase letters, digits, `-` and `_`, at most 128 bytes (`report.ready`)
error-engine-invalid-signal-not-json = signal is not JSON: { $e }
error-engine-invalid-simulator-needs-name = a simulator needs a name
error-engine-invalid-ssh-task = ssh task: { $e }
error-engine-invalid-step-held-until-released-there-nothing = step `{ $step }` of { $home } is held until released; there is nothing to decline
error-engine-invalid-step-input-holds-no-account-id = step `{ $a0 }`: input `{ $input }` holds no account id
error-engine-invalid-step-names-no-connector = step `{ $a0 }` names no connector
error-engine-invalid-step-names-no-operation = step `{ $a0 }` names no operation of `{ $connector }`
error-engine-invalid-step-not-connector-step = step `{ $a0 }` is { $a1 }, not a connector step
error-engine-invalid-step-not-waiting-decision-answer = step `{ $named }` of { $home } is not waiting for a decision or an answer
error-engine-invalid-step-was-not-sent = step `{ $a0 }` was not sent: { $reason }
error-engine-invalid-steps-waiting-name-one-with = { $home } has { $a0 } steps waiting ({ $a1 }); name one with `step`
error-engine-invalid-team-not-found = team not found: { $id }
error-engine-invalid-test-event-does-not-map = the sample event does not map onto the run's inputs: { $e }
error-engine-invalid-too-large-review = { $rel } is too large to review
error-engine-invalid-url-inspect = a URL to inspect
error-engine-invalid-vcs-task-did-not-finish = vcs task did not finish: { $e }
error-engine-invalid-wait-moment = `{ $text }` is not a moment: give Unix seconds or an RFC 3339 time
error-engine-invalid-was-not-admitted = { $pubkey } was not admitted
error-engine-invalid-watcher = watcher: { $e }
error-engine-invalid-watching = watching { $a0 }: { $e }
error-engine-invalid-work-item-cannot-be-started = work item { $id } is { $a0 }; it cannot be started
error-engine-invalid-work-item-names-no-project-goal-has = work item { $a0 } names no project and the goal has { $count }
error-engine-invalid-workflow-archived-unarchive-before-running = workflow { $a0 } is archived; unarchive it before running it
error-engine-invalid-workflow-used-archive-point-them-elsewhere-first = workflow { $workflow } is used by { $a0 }; archive it, or point them elsewhere first
error-engine-invalid-workstream-closed-closed-workstream-archived-cannot-be = workstream { $id } is closed: a closed workstream is Archived and cannot be placed in { $column }
error-engine-invalid-workstream-copy-non-git-project-there-no = workstream { $id } is a copy of a non-git project; there is no branch to commit to
error-engine-invalid-workstream-copy-non-git-project-there-no-2 = workstream { $id } is a copy of a non-git project; there is no branch to push
error-engine-invalid-workstream-detached-head-check-out-branch-first = workstream { $id } is on a detached HEAD; check out a branch first
error-engine-invalid-workstream-detached-head-nothing-amend = workstream { $workstream } is on a detached HEAD; nothing to amend
error-engine-invalid-workstream-detached-head-nothing-push = workstream { $id } is on a detached HEAD; nothing to push
error-engine-invalid-workstream-has-no-open-pull-request = workstream { $a0 } has no open pull request
error-engine-invalid-workstream-has-no-pull-request = workstream { $a0 } has no pull request
error-engine-invalid-workstream-project-has-no-checkout = workstream { $a0 } of project { $a1 } has no checkout at { $a2 }
error-engine-lib-refused = { $detail }
error-engine-lsp-refused = { $detail }
error-engine-projects-refused = { $detail }
error-store-gate-policy-decision-signed-who-may-not-decide-gate = gate policy violation: decision { $a0 } is signed by { $a1 }, who may not decide the { $gate } gate under the  current governance policy
error-store-gate-policy-may-not-decide-gate-under-current-governance = gate policy violation: { $signer } may not decide the { $gate } gate under the current governance policy
error-store-ingest-refused = { $detail }
error-store-invalid-account-connector = account { $a0 } of connector { $a1 }: { $a2 }
error-store-invalid-agent-already-exists = agent { $id } already exists
error-store-invalid-attachment-bytes-limit = attachment is { $size } bytes; the limit is { $max_attachment_bytes }
error-store-invalid-attachment-does-not-match-address-asked-got = attachment does not match its address: asked for { $expected_sha256 }, got { $actual }
error-store-invalid-attachment-needs-name = an attachment needs a name
error-store-invalid-bad-goal-id-workflows-index = bad goal id in the workflows index: { $e }
error-store-invalid-bad-id-fts = bad id in fts: { $e }
error-store-invalid-bad-id-index = bad id in index: { $e }
error-store-invalid-bad-note-scope-index = bad note scope in index: { $kind } { $scope_id }
error-store-invalid-bad-project-id-index = bad project id in index: { $e }
error-store-invalid-bad-workflow-id-index = bad workflow id in index: { $e }
error-store-invalid-bad-workstream-id-index = bad workstream id in index: { $e }
error-store-invalid-cannot-address-agent-disabled-so-would-answer = cannot address { $token }: that agent is disabled, so it would  answer nothing — and naming it stops the room answering instead
error-store-invalid-cannot-address-not-pubkey-agent-id-team = cannot address { $token }: it is not a pubkey, an agent id, a team id,  or this conversation's own handle
error-store-invalid-cannot-address-team-disabled = cannot address { $token }: that team is disabled
error-store-invalid-cannot-be-used-as-file-name = { $what } { $s } cannot be used as a file name: { $why }
error-store-invalid-cannot-install-workspace-already-has-with-id = cannot install { $kind } { $slug }: this workspace already has a { $kind }  with that id, and it is { $holder }. Remove or rename it first.
error-store-invalid-cannot-rename-project-here-folder-must-move = cannot rename project { $a0 } -> { $a1 } here: the folder must move first
error-store-invalid-cannot-reply-not-message-conversation = cannot reply to { $parent }: it is not a message in this conversation
error-store-invalid-catalog-agent = catalog agent { $slug }: { $e }
error-store-invalid-catalog-channel = catalog channel { $slug }: { $e }
error-store-invalid-catalog-connector = catalog connector { $slug }: { $e }
error-store-invalid-catalog-has-no-called = the catalog has no { $kind } called { $slug }
error-store-invalid-catalog-skill = catalog skill { $slug }: { $e }
error-store-invalid-catalog-team = catalog team { $slug }: { $e }
error-store-invalid-catalog-workflow = catalog workflow { $slug }: { $e }
error-store-invalid-catalog-workflow-already-installed-as = the catalog workflow { $slug } is already installed as { $existing }
error-store-invalid-catalog-addon = catalog addon { $slug }: { $e }
error-store-invalid-catalog-addon-id-not-slug = catalog addon { $slug }: its manifest says its id is { $id }; a built-in's id is its folder's name
error-store-invalid-addon-holds-no-addon-json = { $a0 } holds no addon.json, so it is not an addon
error-store-invalid-addon-json-will-not-parse = addon.json will not parse: { $e }
error-store-invalid-addon-manifest-too-large = addon.json is { $a0 } bytes; the most is { $max }
error-store-invalid-addon-refused = the addon was refused: { $problems }
error-store-invalid-addon-ships-with-platform-give-your-own = an addon called { $a0 } ships with the platform; give your own another id
error-store-invalid-addon-already-installed = an addon called { $a0 } is already installed; remove it first
error-store-invalid-addon-id-reserved = { $id } is a name the addons folder keeps for itself
error-store-invalid-addon-symlink = { $path } is a symbolic link; a bundle carries files, never links
error-store-invalid-addon-dotfile = { $path } is hidden; a bundle carries no dotfiles
error-store-invalid-addon-name-not-utf8 = { $path } has a name that is not UTF-8
error-store-invalid-addon-not-a-file = { $path } is neither a file nor a folder
error-store-invalid-addon-too-deep = { $path } is nested too deep; a bundle's folders go { $max } deep at most
error-store-invalid-addon-folder-unreadable = { $path } could not be read: { $e }
error-store-invalid-addon-not-bundle-path = { $rel } is not a path inside a bundle
error-store-invalid-channel-already-exists = channel { $id } already exists
error-store-invalid-channel-needs-name = channel needs a name
error-store-invalid-channel-roster-names-unknown-agent = channel roster names unknown agent { $a }
error-store-invalid-channel-roster-names-unknown-team = channel roster names unknown team { $t }
error-store-invalid-channel-roster-names-who-not-person-workspace = channel roster names { $p }, who is not a person of this workspace
error-store-invalid-connector = connector { $a0 }: { $a1 }{ $a2 }
error-store-invalid-connector-already-exists = connector { $a0 } already exists
error-store-invalid-connector-has-no-secret-field = connector { $connector } ({ $a0 }) has no secret field `{ $a1 }`
error-store-invalid-cut-short = cut short
error-store-invalid-decision-making-agent-s-name-fixed = the Decision-Making Agent's name is fixed: `{ $name }`
error-store-invalid-directory-list-instead = { $relative } is a directory — list it instead
error-store-invalid-directory-list-instead-2 = { $shown } is a directory — list it instead
error-store-invalid-document-needs-name = a document needs a name
error-store-invalid-emoji-must-be-1-64-bytes = emoji must be 1..=64 bytes
error-store-invalid-empty-attachment = an empty attachment
error-store-invalid-empty-message = empty message
error-store-invalid-file-bytes-too-large-keep-review = a file of { $a0 } bytes is too large to keep for a review
error-store-invalid-folder-project-folder-one-project-s = { $a0 } { $how ->
    [same] is already
    [inside] is inside
   *[holds] holds
  } the folder of the project { $a1 } ({ $a2 }); a folder is one project's
error-store-invalid-goal-already-closed = goal { $goal_id } is already closed
error-store-invalid-goal-cannot-be-deleted-design-still-used = goal { $goal } cannot be deleted: its design { $a0 } is still used by { $a1 }
error-store-invalid-goal-closed = goal { $goal_id } is closed
error-store-invalid-goal-closed-nothing-runs = goal { $goal_id } is closed; nothing runs on it
error-store-invalid-goal-closed-nothing-runs-2 = goal { $a0 } is closed; nothing runs on it
error-store-invalid-goal-needs-statement = a goal needs a statement
error-store-invalid-goal-open-close-before-archiving = goal { $goal_id } is open; close it before archiving it
error-store-invalid-goal-running-workflow-run-made-while-busy = goal { $goal_id } is running workflow { $a0 }; a run made while it is busy is queued and must be of that workflow
error-store-invalid-has-no-directory-yet = { $scope } { $id } has no directory at { $a0 } yet
error-store-invalid-has-no-roster-people-edit = { $id } has no roster of people to edit
error-store-invalid-holds-no-pet-json-pet-package-manifest = { $a0 } holds no pet.json — a pet package is a manifest and a sprite sheet
error-store-invalid-invalid-nip-oa-conditions = invalid NIP-OA conditions: { $e }
error-store-invalid-invite-cannot-be-withdrawn = invite { $id } is { $a0 } and cannot be withdrawn
error-store-invalid-invite-not-waiting-you = invite { $id } is { $a0 }, not waiting on you
error-store-invalid-kind-ephemeral-must-never-be-journaled = kind { $wire_kind } is ephemeral and must never be journaled
error-store-invalid-kind-not-addressable = kind { $wire_kind } is not addressable
error-store-invalid-kind-not-conversation-fact = kind { $other } is not a conversation fact
error-store-invalid-library-core-decision-making-agent-toml = library/core/decision-making-agent.toml: { $e }
error-store-invalid-library-core-general-toml = library/core/general.toml: { $e }
error-store-invalid-listening-goal-closed = goal { $goal } is closed: a closed goal listens to nothing
error-store-invalid-listening-goal-design = workflow { $workflow } is a goal's design: its goal listens, never the design by itself
error-store-invalid-mcp-server-already-exists = mcp server { $a0 } already exists
error-store-invalid-message-content-not-body = message content is not a body: { $e }
error-store-invalid-neither-person-workspace-nor-one-agents = { $p } is neither a person of this workspace nor one of its agents
error-store-invalid-no-artifact-bytes-workspace = no artifact bytes { $a0 } in this workspace
error-store-invalid-no-attachment-workspace = no attachment { $a0 } in this workspace
error-store-invalid-no-attachment-workspace-2 = no attachment { $sha256 } in this workspace
error-store-invalid-no-attachment-workspace-upload-bytes-first = no attachment { $a0 } in this workspace — upload the bytes first
error-store-invalid-no-invite = no invite { $id }
error-store-invalid-no-sprite-sheet = no sprite sheet at { $a0 }
error-store-invalid-no-such-file = no such file: { $relative }
error-store-invalid-no-such-file-2 = no such file: { $shown }
error-store-invalid-not-digest = not a digest
error-store-invalid-not-file-name = not a file name: { $name }
error-store-invalid-not-file-path = { $a0 } is not a file path
error-store-invalid-not-goal-id = { $id } is not a goal id
error-store-invalid-not-member = { $pubkey } is not a member
error-store-invalid-not-run-id = { $id } is not a run id
error-store-invalid-not-settings-file = { $a0 }: not a settings file: { $e }
error-store-invalid-not-standing-channel-guest-can-be-put = { $c } is not a standing channel a guest can be put on
error-store-invalid-not-webp-file-pet-s-sprite-sheet = { $a0 } is not a WebP file — a pet's sprite sheet is a .webp, and the bytes decide
error-store-invalid-not-work-item-id = { $id } is not a work item id
error-store-invalid-not-workstream-id = { $id } is not a workstream id
error-store-invalid-note-would-be-bytes-limit = note would be { $a0 } bytes; the limit is { $max_note_bytes }
error-store-invalid-nothing-append = nothing to append
error-store-invalid-path-leaves = path { $relative } leaves { $a0 }
error-store-invalid-pet-already-installed = pet { $a0 } is already installed
error-store-invalid-pet-json-will-not-parse = pet.json will not parse: { $e }
error-store-invalid-pet-needs-displayname = a pet needs a displayName
error-store-invalid-pet-ships-with-platform-give-your-own = pet { $a0 } ships with the platform; give your own pack another id
error-store-invalid-pet-ships-with-platform-not-removed-put = pet { $id } ships with the platform and is not removed; put it away instead
error-store-invalid-project-archived-unarchive-before-attaching = project { $a0 } is archived; unarchive it before attaching it
error-store-invalid-project-name-must-not-be-blank = project name must not be blank
error-store-invalid-project-named-already-exists = a project named { $slug } already exists
error-store-invalid-project-scope-settings-need-project = project-scope settings need a project
error-store-invalid-reaction-has-no-e-tag = reaction has no e tag
error-store-invalid-recall-slug-must-be-non-empty-no = recall slug { $slug } must be non-empty, no whitespace
error-store-invalid-refused = { $label }: { $e }
error-store-invalid-remote-project-wants-slug-held-here = remote project { $a0 } wants slug { $a1 }, held here by { $a2 }
error-store-invalid-reserved-name-under-projects = { $slug } is a reserved name under projects/
error-store-invalid-retraction-has-no-e-tag = retraction has no e tag
error-store-invalid-run-entry-not-a-start = “{ $step }” is not a start of workflow { $workflow }: a run begins at one of its start steps
error-store-invalid-run-files-are-its-goal-s = run { $run } is goal { $goal }'s, and its files are that goal's: open the goal's folder
error-store-invalid-run-no-manual-entry = workflow { $workflow } has no start a person runs by hand: only its events begin it — test it by naming one of its starts
error-store-invalid-run-kept-moving-under-attempts-record-event = run { $run_id } kept moving under { $attempts } attempts to record an event on it
error-store-invalid-secret-field-empty = secret field `{ $a0 }` is empty
error-store-invalid-signal-needs-id = signal needs an id
error-store-invalid-signal-not-found = signal not found: { $id }
error-store-invalid-signal-payload-bytes-over-byte-cap = signal payload is { $a0 } bytes, over the { $max_signal_payload_bytes }-byte cap
error-store-invalid-skill-already-exists = skill { $a0 } already exists
error-store-invalid-skill-empty = skill { $a0 } is empty
error-store-invalid-snapshot-event-has-no-d-tag = snapshot event has no d tag
error-store-invalid-snapshot-has-no-d-tag = snapshot has no d tag
error-store-invalid-spawn-step-names-which-not-installed = { $label }: its spawn step names { $named }, which is not installed
error-store-invalid-sprite-sheet-bytes-limit = the sprite sheet is { $a0 } bytes; the limit is { $max_spritesheet_bytes }
error-store-invalid-spritesheetpath = spritesheetPath { $a0 }: { $e }
error-store-invalid-stale-snapshot-write-stored-rev-new-rev = stale snapshot write for { $wire_kind }:{ $d }: stored (rev={ $a0 }, at={ $a1 }) >= new (rev={ $revision }, at={ $at })
error-store-invalid-team-already-exists = team { $id } already exists
error-store-invalid-team-references-unknown-agent = team references unknown agent { $id }
error-store-invalid-unknown-catalog-kind = unknown catalog kind { $s }
error-store-invalid-unknown-event = unknown event: { $event_id }
error-store-invalid-unknown-event-2 = unknown event: { $target_event }
error-store-invalid-unknown-scope = unknown scope: { $scope }
error-store-invalid-unknown-usage-kind = unknown usage kind { $s }
error-store-invalid-was-not-admitted = { $by } was not admitted
error-store-invalid-work-item-let-settle-cancel-before-deleting = work item { $id } is { $a0 }; let it settle, or cancel it, before deleting it
error-store-invalid-work-item-state-changes-go-through-transition = work item { $a0 }: state changes go through transition_work_item
error-store-invalid-workflow-archived-unarchive-before-pointing-goal = workflow { $a0 } is archived; unarchive it before pointing a goal at it
error-store-invalid-workflow-archived-unarchive-before-running = workflow { $a0 } is archived; unarchive it before running it
error-store-invalid-workflow-was-designed-goal-promote-library-first = workflow { $a0 } was designed for goal { $designed_for }; promote it to the library first
error-store-invalid-workspace-run-never-queued = run { $run } is a run of the workspace: it started when it was made, and never waits in a queue
error-store-invalid-workstream-belongs-another-project = workstream { $ws } belongs to another project
error-store-invalid-workstream-belongs-project-not = workstream { $id } belongs to project { $a0 }, not { $project }
error-store-invalid-workstream-only-name-note-pinned-board-can = workstream { $a0 }: only name, note, pinned and the board can be edited
error-store-invalid-workstream-primary-project-s-id-one-fact = workstream { $a0 }: the primary and the project's id are one fact
error-store-invalid-workstream-state-changes-go-through-transition-workstream = workstream { $a0 }: state changes go through transition_workstream
error-store-invalid-workstream-workstream-s-kind-fixed = workstream { $a0 }: a workstream's kind is fixed
error-store-invalid-workstreams-cannot-be-filtered = workstreams cannot be filtered by { $col }
error-store-invites-refused = { $detail }
error-store-pets-refused = { $detail }
error-store-still-used-cannot-remove-still-used = cannot remove { $kind } { $id }: still used by { $a0 }. { $a1 }
error-store-still-used-listening-public-hook = “{ $step }” is a public hook workflow { $workflow } is listening on — someone outside calls it with its secret. Turn listening off first, then remove, rename or make it local
error-store-still-used-workflow-has-run-going = workflow { $a0 } has a run going ({ $run }); stop it, or retire the workflow, before deleting it
error-store-still-used-workflow-has-run-going-archive = workflow { $a0 } has a run going ({ $run }); stop it, or retire the workflow, before archiving it
error-engine-invalid-adopting-needs-path-folder-adopts = adopting needs the path of the folder it adopts
error-engine-invalid-check-way-out-machine-not-loopback-address = a check is for the way out of this machine, not a loopback address
error-engine-invalid-conversation-not-about-project-workstream-so-turns = this conversation is not about a project or a workstream, so its turns change no checkout
error-engine-invalid-goal-retired-archiving-deleting-keep-not-fate = a goal is retired by archiving or deleting it; keep is not a fate for it
error-engine-invalid-no-harness-named-so-there-nobody-ask = no harness is named, so there is nobody to ask
error-engine-invalid-repository-has-no-branch-checked-out = the { $what } repository has no branch checked out
error-engine-invalid-only-conversation-about-project-workstream-has-mode = only a conversation about a project or a workstream has a mode: elsewhere a turn changes no checkout
error-engine-invalid-question-no-longer-waiting-turn-ended-before = that question is no longer waiting: its turn ended before the answer arrived
error-engine-invalid-question-no-longer-waiting-was-answered-turn = that question is no longer waiting: it was answered, or its turn ended
error-engine-invalid-reply-needs-words-say-what-you-changed = a reply needs words — say what you changed, or what you think
error-engine-invalid-root-itself-not-something-delete = the root itself is not something to delete
error-engine-invalid-root-itself-not-something-move = the root itself is not something to move
error-engine-invalid-run-command-opened-from-ide-s-terminal = the run command is opened from the IDE's Terminal menu, never run by the engine
error-engine-invalid-unknown-expired-oauth-state-start-connection-again = unknown or expired OAuth state; start the connection again
error-engine-invalid-workflow-retired-archiving-deleting-keep-not-fate = a workflow is retired by archiving or deleting it; keep is not a fate for it
error-store-invalid-adopted-project-needs-path-folder-adopts = an adopted project needs the path of the folder it adopts
error-store-invalid-archiving-unarchiving-go-through-set-goal-archived = archiving and unarchiving go through set_goal_archived, never an edit
error-store-invalid-author-not-participant-conversation = author is not a participant of this conversation
error-store-invalid-closing-reopening-go-through-set-goal-closed = closing and reopening go through set_goal_closed, never an edit
error-store-invalid-direct-message-has-no-roster-topic-edit = a direct message has no roster or topic to edit
error-store-invalid-direct-message-needs-least-one-other-participant = a direct message needs at least one other participant
error-store-invalid-goal-s-origin-recorded-capture-never-edited = a goal's origin is recorded at capture and never edited
error-store-invalid-no-attachment-upload-bytes-first = no attachment { $a0 } for { $a1 } — upload the bytes first
error-store-invalid-only-author-may-retract-event = only the author may retract an event
error-store-invalid-project-s-root-fixed-create-new-project = a project's root is fixed; create a new project for a different folder
error-store-invalid-retraction-author-does-not-match-target-author = retraction author does not match target author
error-store-invalid-runs-started-through-create-run-never-edited = runs are started through create_run and never edited onto a goal
error-store-invalid-listening-set-through-set-listening = a goal begins or stops listening through set_listening, never through an edit
error-store-invalid-self-attestation-invalid-per-nip-oa = self-attestation is invalid per NIP-OA
error-store-invalid-workflow-goal-runs-set-through-set-goal = the workflow a goal runs is set through set_goal_workflow
error-store-invalid-workspace-has-exactly-one-owner-own-keypair = a workspace has exactly one owner: its own keypair
error-store-invalid-workstream-s-name-must-not-be-blank = a workstream's name must not be blank; clear it to use the branch

## Sentences a person and a model both hear — the model in English from the source, the person from here (crates/bisa-engine/src/mobile.rs holds a test that the two agree); and the store's one refusal of the primary workstream.

error-engine-conflict-mobile-development-off = mobile development is turned off in Settings › Capabilities › Mobile Development
error-engine-conflict-mobile-development-platform-off = that device's platform is off in Settings › Capabilities › Mobile Development (mobile_development.platforms)
error-engine-addons-switched-off = addons are switched off on this machine (`addons.enabled`)
error-engine-addons-url-malformed = { $url } is not a URL an addon may fetch: scheme://host/path, with no user info
error-engine-addons-https-only = an addon fetches over https only, not { $scheme }
error-engine-addons-loopback-refused = { $host } is this machine; an addon never reaches the node or anything else on it
error-engine-addons-host-not-declared = { $host } is not a host the addon declares (it declares { $declared })
error-engine-addons-host-refused = { $host } was refused: { $reason }
error-engine-addons-bad-accept = the Accept header an addon asks with must be printable ASCII
error-engine-addons-fetch-failed = the fetch failed: { $e }
error-engine-mobile-development-unavailable-no-tools = the mobile tools are not available — this node was started without them
error-store-invalid-primary-is-the-project = the primary workstream is the project itself; remove the project instead

## The node's own refusals (crates/bisa-node/src): what a handler says before anything is asked of the engine.

error-node-admin-session-has-no-transcript = session has no transcript
error-node-admin-transcript-file-missing = transcript file missing
error-node-admin-unknown-session = unknown session
error-node-agents_api-agent-needs-name-system-prompt = an agent needs a name and a system prompt
error-node-agents_api-bad-member-pubkey = bad member pubkey: { $e }
error-node-agents_api-no-session = no session { $id }
error-node-agents_api-not-session-id = { $id } is not a session id
error-node-agents_api-session-secret-required = a session secret is required
error-node-agents_api-team-member-must-be = team member must be {"{"}"human": pk{"}"} or {"{"}"agent": id{"}"}
error-node-agents_api-team-needs-name = a team needs a name
error-node-agents_api-unknown-mcp-server = unknown mcp server { $id }
error-node-agents_api-unknown-respond-policy = unknown respond policy { $other }
error-node-agents_api-unknown-scope-use-goal-workstream-work-item = unknown scope { $a0 }: use goal, workstream or work_item
error-node-agents_api-unknown-skill = unknown skill { $id }
error-node-attachments-no-attachment-with-hash-machine = no attachment with that hash on this machine
error-node-attachments-no-connected-peer-has = no connected peer has it
error-node-attachments-not-file-name = not a file name
error-node-attachments-not-sha-256-digest = not a sha-256 digest
error-node-attachments-peer-refused = { $detail }
error-node-body-unreadable = the request body does not fit: { $detail }
error-node-body-needs-json-content-type = the request body needs Content-Type: application/json
error-node-body-too-large = the request body is larger than this route accepts
error-node-query-unreadable = the address's query does not fit: { $detail }
error-node-browser-nothing-waits-under-id = nothing waits under that id
error-node-catalog-unknown-catalog-kind-use-one = unknown catalog kind { $raw }: use one of { $a0 }
error-node-changes-not-conversation-id = { $s } is not a conversation id
error-node-codehost-empty-token = an empty token
error-node-codehost-one-url-path = one of `url` or `path`
error-node-codehost-reply-needs-words-say-what-you-changed = a reply needs words — say what you changed, or what you think
error-node-codehost-unknown-kind = { $detail }
error-node-collab-bad-invite-id = bad invite id: { $e }
error-node-collab-bad-pubkey = bad pubkey: { $e }
error-node-collab-emoji-must-be-1-64-chars = emoji must be 1..=64 chars
error-node-collab-empty-message = an empty message
error-node-collab-failed = { $detail }
error-node-collab-message-needs-least-one-recipient = a message needs at least one recipient
error-node-collab-node-not-member-workspace = this node is not a member of that workspace
error-node-collab-node-runs-no-collaboration-pump-so-there = this node runs no collaboration pump, so there is nobody to ask
error-node-collab-paste-invite-link-code = paste the invite link or code
error-node-collab-person-another-node-admin-member-guest-never = a person on another node is an admin, a member or a guest — never the owner
error-node-collab-refused = { $detail }
error-node-collab-relay-ws-wss-url = a relay is a ws:// or wss:// URL
error-node-connectors-body-names-connector-but-path-names = the body names connector { $a0 } but the path names { $cid }
error-node-connectors-cannot-remove-connector-still-used-forget-accounts = cannot remove connector { $cid }: still used by { $a0 }. Forget its accounts and point the workflows' steps elsewhere first.
error-node-connectors-code-empty = the code is empty
error-node-connectors-connector-authenticates-with-has-no-secret-field = connector { $cid } authenticates with { $a0 } and has no secret field `{ $a1 }`
error-node-connectors-connector-catalog-s-not-edited-write-your = connector { $cid } is the catalog's and is not edited; write your own definition under another id
error-node-connectors-connector-definition-has-problem = the connector definition has { $a0 } problem{ $a1 }: { $words }{ $a2 }
error-node-connectors-connectors-oauth-port-not-port-number = connectors.oauth.port is not a port number
error-node-connectors-not-account-id = { $raw } is not an account id
error-node-connectors-port-busy-change-connectors-oauth-port = port { $port } is busy; change connectors.oauth.port ({ $e })
error-node-connectors-refused = { $detail }
error-node-conversation-bad-pubkey = bad pubkey { $h }: { $e }
error-node-conversation-channel-name-must-not-be-empty = channel name must not be empty
error-node-conversation-no-message-with-id = no message with that id
error-node-conversations-conversation-archived-take-back-out-continue = this conversation is archived; take it back out to continue it
error-node-conversations-id-needs-origin = `id` needs an `origin`
error-node-conversations-narrow-project-origin-not-both = narrow by `project` or by `origin`, not both
error-node-conversations-origin-id = origin { $kind } { $given ->
    [yes] takes no
   *[no] needs
  } an id
error-node-conversations-unknown-origin-one = unknown origin { $kind }: one of { $a0 }
error-node-decisions-not-decision-provider = `{ $provider }` is not a decision provider
error-node-decisions-provider-takes-no-api-key = the `{ $provider }` provider takes no API key
error-node-files-get-file-needs-path-file-under-root = GET /file needs ?path=<file under the root>
error-node-files-unknown-file-scope-use-one = unknown file scope { $raw }: use one of { $a0 }
error-node-goals-documents-must-name-least-one-file = documents must name at least one file
error-node-goals-no-captured-result-item = no captured result for this item
error-node-goals-not-work-item-id = not a work item id
error-node-goals-refused = { $detail }
error-node-goals-run-not-found = run { $rid } not found
error-node-goals-run-not-run-goal = run { $rid } is not a run of goal { $id }
error-node-goals-send-workflow-point-definition-record-not-both = send a workflow to point at, or a definition to record — not both
error-node-goals-statement-must-not-be-empty = statement must not be empty
error-node-hooks-backlog-full = { $listener } could not take the call now: its backlog is full
error-node-hooks-body-over-cap = a hook's body may be at most { $max } bytes
error-node-hooks-host-not-listening = { $host } is not listening; turn it on first
error-node-hooks-listening-off-on-this-machine = listening is off on this machine; turn events.enabled on first
error-node-hooks-not-hook-start = { $listener } is not a hook start that is armed
error-node-hooks-takes-no-public-calls = { $listener } takes no public calls
error-node-ide-apply-needs-files-from-preview-each-with = apply needs the files from the preview, each with its base_hash
error-node-ide-case-smart-sensitive-insensitive = case { $other }: smart, sensitive or insensitive
error-node-ide-consent-consented-git-operation-needs-workspace-token-authorization = a consented git operation needs the workspace token in the Authorization header
error-node-ide-file-too-large-to-render = { $path } is { $size } bytes; rendering stops at { $max }
error-node-ide-interactive-commit-needs-message = a commit needs a message
error-node-ide-interactive-pass-exactly-one-patch-non-empty-paths = pass exactly one of `patch` or a non-empty `paths`
error-node-ide-interactive-refused = { $detail }
error-node-ide-interactive-remote-needs-url = a remote needs a url
error-node-ide-kind-file-dir = kind { $other }: file or dir
error-node-ide-lsp-not-file-scope = { $raw } is not a file scope
error-node-ide-not-ref-scope = { $e }
error-node-ide-q-required = q is required
error-node-ide-refused = { $a0 }: { $e }
error-node-ide-serve-checkout-not-disk = the checkout is not on disk: { $e }
error-node-ide-serve-hidden-folder = { $folder } is hidden: a folder whose name starts with a dot is never served
error-node-ide-serve-folder-already-served = that folder is already served at { $a0 }
error-node-ide-serve-named-copy-has-no-folder = the named copy has no folder
error-node-ide-serve-no-loopback-port = no loopback port: { $e }
error-node-ide-serve-no-such-server = no such server
error-node-ide-serve-not-folder = { $a0 } is not a folder
error-node-ide-serve-not-folder-checkout = { $folder } is not a folder of the checkout: { $e }
error-node-ide-serve-refused = { $detail }
error-node-ide-serve-too-many = { $count } folders are served already, the most at once: stop one first
error-node-inbox-unknown-filter-use-one = unknown filter { $s }: use one of { $a0 }
error-node-inbox-unknown-source-use-one = unknown source { $s }: use one of { $a0 }
error-node-internal = something went wrong on this node; the log has it
error-node-listening-not-a-host = { $raw } is not a host: use workspace:<workflow id> or goal:<goal id>
error-node-listening-signal-not-found = signal not found: { $id }
error-node-listening-signal-payload-over-cap = the signal's payload is { $len } bytes, over the { $max }-byte cap
error-node-logs-no-crash-report = no crash report { $name }
error-node-logs-not-crash-report-s-name = { $name } is not a crash report's name
error-node-mobile-development-format-png-jpeg = format { $other }: png or jpeg
error-node-not-a-goal-id = { $value } is not a goal id
error-node-notes-changed-since-read = the note changed since you read it
error-node-notes-id = { $kind } id: { $a0 }
error-node-notes-id-needs-scope = an `id` needs a `scope`
error-node-notes-note-id = note id: { $e }
error-node-drawings-drawing-id = drawing id: { $e }
error-node-drawings-nothing-waits-under-id = no drawing request waits under that id
error-node-notes-note-names-nothing-drop-id = a { $kind } note names nothing — drop `id`
error-node-notes-scope-needs-id = scope={ $kind } needs an `id`
error-node-notes-unknown-scope-one = unknown scope { $kind }: one of { $a0 }
error-node-addons-file-not-found = no such file
error-node-addons-nothing-to-patch = say what to change: `enabled`, `granted`, or both
error-node-pets-pet-has-no-readable-sprite-sheet = that pet has no readable sprite sheet
error-node-pets-ships-with-platform-not-removed-put-away = { $a0 } ships with the platform and is not removed; put it away instead
error-node-projects-attaching-needs-goal = attaching needs a goal
error-node-projects-attaching-needs-goal-post-goals-projects-post = attaching needs a goal: POST /goals/{"{"}id{"}"}/projects, or POST /projects/{"{"}pid{"}"}/attach
error-node-projects-clone-needs-url = clone needs a url
error-node-projects-end-without-start = `end` without `start`
error-node-projects-goal-not-attached-project-attach-first = goal { $goal } is not attached to project { $a0 }: attach it first
error-node-projects-not-project-id = { $s } is not a project id
error-node-projects-not-workstream-id = { $s } is not a workstream id
error-node-projects-project-has-no-commits-yet-make-first = project { $a0 } has no commits yet — make the first commit before opening a workstream
error-node-projects-project-name-must-not-be-blank = project name must not be blank
error-node-projects-project-sets-no-run-command-set-one = the project sets no run command — set one under About › Settings › Workstream scripts
error-node-projects-publish-task-did-not-finish = publish task did not finish: { $e }
error-node-projects-publishing-workstream-neither-finished-nor-opened-gate = publishing workstream { $workstream } neither finished nor opened a gate
error-node-projects-pull-request-needs-title = a pull request needs a title
error-node-projects-source-cannot = cannot { $verb ->
    [adopt] adopt
    [import] import
    [import-addon] import an addon from
    [validate-addon] read an addon from
    [inspect] inspect
   *[install-pet] install a pet from
  } { $raw }: { $e }
error-node-projects-source-inside-workspace = refusing to { $verb ->
    [adopt] adopt
    [import] import
    [import-addon] import an addon from
    [validate-addon] read an addon from
    [inspect] inspect
   *[install-pet] install a pet from
  } { $path }: it is inside the Bisa workspace
error-node-projects-source-needs-absolute-path = { $verb ->
    [adopt] adopting
    [import] importing
    [import-addon] importing an addon
    [validate-addon] reading an addon
    [inspect] inspecting a folder
   *[install-pet] installing a pet
  } needs an absolute path; { $raw } is relative
error-node-projects-source-not-directory = cannot { $verb ->
    [adopt] adopt
    [import] import
    [import-addon] import an addon from
    [validate-addon] read an addon from
    [inspect] inspect
   *[install-pet] install a pet from
  } { $raw }: not a directory
error-node-projects-refused = { $detail }
error-node-projects-workstream-has-no-checkout = workstream { $wid } has no checkout at { $a0 }
error-node-pulse-before-seq-needs-before = before_seq needs before
error-node-pulse-unknown-concept-use-all-one = unknown concept { $s }: use all or one of { $a0 }
error-node-review-not-review-note-id = { $s } is not a review note id
error-node-review-not-workstream-id = { $w } is not a workstream id
error-node-runs-event-needs-its-start = an event needs the start that reads it: name the start step
error-node-runs-start-by-hand-reads-no-event = the start `{ $step }` begins by hand and reads no event; leave the event out, or name an event start
error-node-security-preview-reads-most-64-kib = a preview reads at most 64 KiB
error-node-security-tool-name-required = a tool name is required
error-node-settings-not-project-id = { $raw } is not a project id
error-node-settings-unknown-settings-scope-machine-workspace-project = unknown settings scope { $other }: machine, workspace or project
error-node-tags-refused = { $detail }
error-node-tags-unknown-tag-match-use-any-default-all = unknown tag match { $other }: use any (the default) or all
error-node-unauthorized = unauthorized
error-node-usage-unknown-usage-kind-use-one = unknown usage kind { $raw }: use one of { $a0 }
error-node-workflows-no-installed-workflow-named-install-template-first = no installed workflow is named { $raw }; install the template first
error-node-workflows-not-goal-id = { $goal } is not a goal id
error-node-workflows-not-workflow-id = { $s } is not a workflow id
error-node-workflows-unknown-scope-expected-library-all = unknown scope { $other }: expected library or all
error-node-workflows-workflow-not-found = workflow not found: { $id }
error-node-workflows-workflow-used-archive-point-them-elsewhere-first = workflow { $id } is used by { $a0 }; archive it, or point them elsewhere first
error-node-workflows-workflow-used-point-them-elsewhere-first = workflow { $id } is used by { $a0 }; point them elsewhere first

## The node, continued: pages and bodies a person reads.
error-node-attachments-no-sync-loop = this node is not running a sync loop, so there is nobody to ask
error-node-attachments-too-large = an attachment may be at most { $max } bytes
error-node-connectors-page-connected = Connected
error-node-connectors-page-connected-close-window = Connected. You can close this window and return to Bisa.
error-node-connectors-page-declined = The platform declined: { $why }. Start the connection again from Settings › Capabilities › Connectors.
error-node-connectors-page-not-connected = Not connected
error-node-connectors-page-stale-link = That link is stale or unknown. Start the connection again from Settings › Capabilities › Connectors.
error-node-connectors-page-unreachable = The platform could not be reached to finish the connection. Start it again from Settings › Capabilities › Connectors.
error-node-ide-changed-since-read = { $path } changed since you read it
error-node-ide-file-too-large = { $path } is { $size } bytes; the editor stops at { $limit }
error-node-ide-serve-checkout-not-folder = the checkout is not a folder
error-node-inbox-new-conversation = New conversation
error-node-inbox-question-could-not-render = Step { $step } — its question could not be rendered: { $e }
error-node-inbox-release-step-when-ready = Release step { $step } when you are ready.
error-node-projects-recorded-git-not-repository = recorded as a git project, but the folder is not a repository
error-node-attachments-peer-said = { $detail }
