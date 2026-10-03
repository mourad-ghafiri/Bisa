### Bisa — the desktop: workflow (`desktop/src/views/_workflow/forms`).

workflow-agent-step-form-actionable-fresh-session-no-memory-conversation = Actionable by a fresh session with no memory of this conversation. Say what the step produces; its shape is the output schema below. { $TEMPLATE_HINT }
workflow-agent-step-form-assignee = Assignee
workflow-agent-step-form-comma-separated-fallback-order-blank-means = Comma-separated fallback order. Blank means the agent's own.
workflow-agent-step-form-exec = Read, write and run commands
workflow-agent-step-form-fixed-value = A fixed value
workflow-agent-step-form-from-input = From input { $i }
workflow-agent-step-form-goal-s-own-folder = The goal's own folder
workflow-agent-step-form-harness = Harness
workflow-agent-step-form-instructions = Instructions
workflow-agent-step-form-json-schema-result-must-satisfy-shown = JSON Schema the result must satisfy — shown to the session with its prompt; a later step reads a required field of it. Blank accepts any result, and no later step may then read a field of it. Committed when you leave the field.
workflow-agent-step-form-model = Model
workflow-agent-step-form-most-session-may-do = The most a session may do.
workflow-agent-step-form-output-schema = Output schema
workflow-agent-step-form-pin-never-substituted-unavailable-pin-fails = Runs on this exact model. If it is unavailable, the step fails; no other model stands in.
workflow-agent-step-form-project = Project
workflow-agent-step-form-read = Read
workflow-agent-step-form-tool-tier = What it may do
workflow-agent-step-form-where-work-happens-must-attached-goal = Where the work happens. Must be attached to the goal at run time.
workflow-agent-step-form-who-runs-blank-falls-back-goal = Who runs it. Blank falls back to the goal's own assignees.
workflow-agent-step-form-write = Read and write
workflow-approval-step-form-prompt = Prompt
workflow-approval-step-form-what-being-approved-declined-failure-step = What is being approved. Declined is a failure of the step, so put it before anything irreversible. { $TEMPLATE_HINT }
workflow-check-step-form-command = Command
workflow-check-step-form-json-schema = JSON Schema
workflow-check-step-form-judged = Judged by
workflow-check-step-form-nothing-runs-before-step-yet-judges = Nothing runs before this step yet; it judges the output of a step that does.
workflow-check-step-form-output = Output of
workflow-check-step-form-pick-step = Pick a step…
workflow-check-step-form-runs-goal-s-work-folder-node = Runs in the goal's work folder with the node's privileges. Exit 0 passes; the output is the evidence. Every substituted value is one shell word. { $TEMPLATE_HINT }
workflow-check-step-form-upstream-step-whose-result-schema-judges = An upstream step whose result the schema judges.
workflow-check-step-form-what-step-s-output-must-satisfy = What the step's output must satisfy. Committed when you leave the field.
workflow-condition-editor-add-condition = Add a condition
workflow-condition-editor-between = between
workflow-condition-editor-conditions-nest-deep-most = Conditions nest { $MAX_CONDITION_DEPTH } deep at most.
workflow-condition-editor-did-not-pass = did not pass
workflow-condition-editor-empty-group-problem = An empty group is a problem; add a condition.
workflow-condition-editor-from-hour = from hour
workflow-condition-editor-hour = to hour
workflow-condition-editor-label-condition = condition
workflow-condition-editor-label-contains = contains
workflow-condition-editor-label-input = input
workflow-condition-editor-label-outcome = outcome
workflow-condition-editor-label-step = step
workflow-condition-editor-label-value = value
workflow-condition-editor-o-clock-utc-range-past-midnight = o'clock UTC — a range past midnight wraps.
workflow-condition-editor-option-id = option id
workflow-condition-editor-passed = passed
workflow-condition-editor-path-dotted = path (dotted)
workflow-condition-editor-pick-input = Pick an input…
workflow-condition-editor-remove-condition = Remove condition
workflow-condition-editor-words = and
workflow-connector-step-form-account = Account
workflow-connector-step-form-account-machine = An account on this machine
workflow-connector-step-form-connector-installed-here-slug = A connector installed here, by slug.
workflow-connector-step-form-connector-s-default-account = The connector's default account
workflow-connector-step-form-connector-s-default-account-node-one = The connector's default account on this node, one by name, or an input the run is started with.
workflow-connector-step-form-file-path-inside-checkout = A path inside the run's checkout — where the run's work landed; the bytes are read when the step runs and never shown to the model.
workflow-connector-step-form-from-input-start = From an input at start
workflow-connector-step-form-input = input · { $i }
workflow-connector-step-form-json-schema-selected-answer-must-satisfy = JSON Schema the selected answer must satisfy. Blank accepts anything. Committed when you leave the field.
workflow-connector-step-form-machine-holds-no-account-yet-default = This machine holds no account for it yet; the default is used when one is added. An input of kind account lets the person starting the run pick one.
workflow-connector-step-form-not-parameter-operation = not a parameter of this operation
workflow-connector-step-form-nothing-installed-here-yet-settings-connectors = Nothing is installed here yet — Settings › Capabilities › Connectors.
workflow-connector-step-form-one-connector-s-operations = One of the connector's operations.
workflow-connector-step-form-one-template-per-parameter-required-one = One template per parameter; a required one cannot be left unset. { $TEMPLATE_HINT }
workflow-connector-step-form-operation = Operation
workflow-connector-step-form-operation-takes-no-parameters = This operation takes no parameters.
workflow-connector-step-form-output-selected = Output: <selected/> — a later step reads it as {"{"}steps.{ $step }.output.<field>{"}"}.
workflow-connector-step-form-output-whole-answer = Output: the whole answer — a later step reads it as {"{"}steps.{ $step }.output.<field>{"}"}.
workflow-connector-step-form-parameter-named = parameter { $name }
workflow-connector-step-form-parameters = Parameters
workflow-connector-step-form-pick-connector = Pick a connector…
workflow-connector-step-form-pick-operation = Pick an operation…
workflow-connector-step-form-remove = remove
workflow-connector-step-form-required = required
workflow-connector-step-operation-changes-something-platform-put-approval = This operation changes something on the platform: put an approval step before it, or switch it to run unattended.
workflow-connector-step-operation-changes-something-platform-runs-nobody = This operation changes something on the platform and runs with nobody's approval before it — your word.
workflow-connector-step-runs-unattended = Runs unattended
workflow-connector-step-say-write-runs-no-approval-human = Say the write runs with no approval or human step before it. Left off, the validator asks for a gate upstream.
workflow-decide-step-form-add-rule = Add a rule
workflow-decide-step-form-branch = → branch
workflow-decide-step-form-branch-when-no-rule-holds = The branch when no rule holds.
workflow-decide-step-form-first-holds-names-branch-draw-flow = The first that holds names the branch. Draw a flow per branch from the step's handles. A step with no rules always takes otherwise.
workflow-decide-step-form-otherwise = Otherwise
workflow-decide-step-form-remove-rule = Remove rule
workflow-decide-step-form-rule-n = rule { $n }
workflow-decide-step-form-rules = Rules
workflow-designer-draw-flows-from-bottom-handle-fail = draw flows from the bottom handle; an on-fail target is set in the inspector
workflow-designer-fail = on fail
workflow-designer-fit = Fit
workflow-designer-fit-whole-workflow = Fit the whole workflow
workflow-designer-panel-agent = Agent
workflow-designer-panel-properties = Properties
workflow-designer-panel-rail = Designer panel
workflow-designer-panel-resize = Resize the panel
workflow-designer-panel-runs = Runs
workflow-designer-redo = Redo
workflow-designer-redo-mod-shift-z = Redo (Mod+Shift+Z)
workflow-designer-session-conflict-somebody-saved-first = Conflict — somebody saved first
workflow-designer-session-not-saved-node-could-not-read = Not saved — the node could not read this design: { $failure }
workflow-designer-session-saved = Saved
workflow-designer-session-saving = Saving…
workflow-designer-session-unknown-error = unknown error
workflow-designer-session-unsaved-edits = Unsaved edits
workflow-designer-step-has-already-run = That step has already run.
workflow-designer-step-has-already-run-stays-ran = A step that has already run stays as it ran.
workflow-designer-step-has-already-run-stays-where = A step that has already run stays where it ran.
workflow-designer-tidy = Tidy
workflow-designer-tidy-lay-every-step-out-again = Tidy: lay every step out again, top to bottom
workflow-designer-undo = Undo
workflow-designer-undo-mod-z = Undo (Mod+Z)
workflow-for-each-step-form-items = Items
workflow-for-each-step-form-longer-list-fails-step-before-first = A longer list fails the step before its first item. { $DEFAULT_MAX_ITERATIONS } unless you say otherwise.
workflow-for-each-step-form-max-iterations = Max iterations
workflow-for-each-step-form-max-iterations-2 = max iterations
workflow-for-each-step-form-template-renders-json-array-upstream-output = A template that renders to a JSON array — an upstream output, {"{"}steps.list.output.items{"}"}. The body reads {"{"}steps.{ $step }.output.item{"}"}, .index and .count. { $TEMPLATE_HINT }
workflow-goal-workflow-tab-adopted-run-carries-own-copy = — adopted; the run carries its own copy.
workflow-goal-workflow-tab-being-designed = being designed
workflow-goal-workflow-tab-cancel = Cancel
workflow-goal-workflow-tab-choose-workflow = Choose workflow…
workflow-goal-workflow-tab-designing-workflow-save-then-start-when = Designing the workflow — save it, then start it when it reads right.
workflow-goal-workflow-tab-designed-for-goal = Designed for this goal
workflow-goal-workflow-tab-edit-steps = Edit the steps
workflow-goal-workflow-tab-editing-plan-every-step-yours-change = Editing the plan — every step is yours to change before it runs.
workflow-goal-workflow-tab-finished-run-history-read-only = A finished run is history; it is read-only.
workflow-goal-workflow-tab-fix-problems-first = Fix the problems first
workflow-goal-workflow-tab-new-steps-part-plan-will-start = New steps are part of the plan you will start.
workflow-goal-workflow-tab-not-adopted-yet-edit-adopt-choose = — not adopted yet. Edit and adopt it, or choose another workflow.
workflow-goal-workflow-tab-not-saved-yet = not saved yet
workflow-goal-workflow-tab-not-started = not started
workflow-goal-workflow-tab-problems = Problems
workflow-goal-workflow-tab-proposed-by-workflow-agent = Proposed by the Workflow Agent
workflow-goal-workflow-tab-queued-run-starts-when-live-run = A queued run: it starts when the live run finishes.
workflow-goal-workflow-tab-rev = rev { $revision }
workflow-goal-workflow-tab-review-every-step-move-adopt-start = — review every step in Your move, adopt it to start, ask for changes, or edit it here first.
workflow-goal-workflow-tab-save-changes = Save changes
workflow-goal-workflow-tab-saved-review-steps-then-start-run = Saved — review the steps, then start the run when you are ready.
workflow-goal-workflow-tab-steps = { $steps } steps
workflow-goal-workflow-tab-steps-2 = Steps
workflow-goal-workflow-tab-untitled-workflow = Untitled workflow
workflow-goal-workflow-tab-workflow-changed-elsewhere-drawing-kept-saving = This workflow changed elsewhere. Your drawing is kept; saving it will be refused until you take the newer one.
workflow-human-step-form-add-option = Add an option
workflow-human-step-form-ask-whom = Ask whom
workflow-human-step-form-blank-asks-goal-s-own-people = Blank asks the goal's own people.
workflow-human-step-form-id = id
workflow-human-step-form-label = label
workflow-human-step-form-leave-empty-free-text-question-ids = Leave empty for a free-text question. Ids come back in the answer; at most one recommended.
workflow-human-step-form-one-question-answerable-sentence = One question, answerable in a sentence. { $TEMPLATE_HINT }
workflow-human-step-form-option-label = option label
workflow-human-step-form-options = Options
workflow-human-step-form-several-may-picked = Several may be picked
workflow-if-step-form-holds-yes-flow-does-not-no = Holds → the yes flow; does not → the no flow. Combine tests with all, any, one and not.
workflow-if-step-form-when = When
workflow-input-defs-editor-account = an account of { $c }
workflow-input-defs-editor-account-connector = account connector
workflow-input-defs-editor-add-input = Add an input
workflow-input-defs-editor-choice-options = choice options
workflow-input-defs-editor-default = default
workflow-input-defs-editor-input-kind = input kind
workflow-input-defs-editor-input-label = input label
workflow-input-defs-editor-input-name = input name
workflow-input-defs-editor-label = label
workflow-input-defs-editor-name = name
workflow-input-defs-editor-no-connector-installed-here = no connector installed here
workflow-input-defs-editor-options-comma-separated = options, comma-separated
workflow-input-defs-editor-remove-input = Remove input
workflow-input-defs-editor-required = Required
workflow-input-defs-editor-steps-read-inputs-name-renaming-renames = Steps read it as {"{"}inputs.<name>{"}"}; renaming it renames every place that reads it.
workflow-inputs-form-no-account-on-machine = This machine holds no account of { $connector } yet — add one under Settings › Capabilities › Connectors.
workflow-inputs-form-pick-account = Pick an account…
workflow-inputs-form-pick-project = Pick a project…
workflow-inputs-form-project-attached-by-start = Picking one attaches it to the goal when the run starts: you say where the work is done.
workflow-inputs-form-project-run-works-there = The steps that read it work there. A run in the workspace attaches nothing.
workflow-inputs-form-workflow-takes-no-inputs = This workflow takes no inputs.
workflow-inspector-back-workflow = Back to the workflow
workflow-inspector-description = Description
workflow-inspector-how-files-library = How it files in the library.
workflow-inspector-inputs = Inputs
workflow-inspector-let-decision-making-agent-decide-runs-workflow = Let the Decision-Making Agent decide in runs of this workflow
workflow-inspector-name = Name
workflow-inspector-one-sentence-what = One sentence on what it is for.
workflow-inspector-remove-step = Remove step
workflow-inspector-step-has-started = This step has started; an amendment leaves it as it ran.
workflow-inspector-tags = Tags
workflow-inspector-what-run-started-steps-read-them = What a run is started with. Steps read them as {"{"}inputs.name{"}"}, or as an assignee or project reference. { $TEMPLATE_HINT }
workflow-json-field-not-json = Not JSON: { $problem }
workflow-judge-step-form-branch-when-not-sure-enough-gives = The branch when it is not sure enough, or gives no answer that holds to the decision contract. The step never fails for this.
workflow-judge-step-form-minimum-confidence = Minimum confidence
workflow-judge-step-form-option-n = option { $n }
workflow-judge-step-form-question-sentence-decision-making-agent-reads-against = The question, in a sentence the Decision-Making Agent reads against the state above.
workflow-judge-step-form-remove-option = Remove option
workflow-judge-step-form-rendered-text-when-step-entered-what = Rendered as text when the step is entered — what the Decision-Making Agent judges. { $TEMPLATE_HINT }
workflow-judge-step-form-state = State
workflow-judge-step-form-what-choosing-branch-means = what choosing this branch means
workflow-judge-step-form-what-each-branch-means-decision-making-agent = What each branch means, in the Decision-Making Agent's own words. A judgement chooses between at least two. Draw a flow per branch from the step's handles.
workflow-judge-step-form-which-branch-does-call = Which branch does this call for?
workflow-notify-step-form-agent-whose-message-left-blank-workflow = The agent whose message this is. Left blank, the Workflow Agent speaks.
workflow-notify-step-form-also-mention-from-inputs = Also mention from inputs
workflow-notify-step-form-assignee-input-run-started = An assignee input the run was started with.
workflow-notify-step-form-blank-posts-into-goal-s-own = Blank posts into the goal's own conversation; an input lets the person starting the run name the channel.
workflow-notify-step-form-channel = channel · { $channel }
workflow-notify-step-form-goal-s-conversation = The goal's conversation
workflow-notify-step-form-mention = Mention
workflow-notify-step-form-mentions-what-wake-agent-prose-reaches = Mentions are what wake an agent; prose reaches nobody. { $TEMPLATE_HINT }
workflow-notify-step-form-message = Message
workflow-notify-step-form-read-from-input-start = Read from an input at start
workflow-notify-step-form-speaks = Speaks as
workflow-notify-step-form-where = Where
workflow-notify-step-form-who-woken-none-wakes-nobody-notice = Who is woken. None wakes nobody — a notice, not a request.
workflow-palette-drag-onto-canvas-press-enter-add = { $explain } Drag onto the canvas, or press Enter to add.
workflow-palette-named-drag-onto-canvas = { $label }: { $explain } Drag onto the canvas, or press Enter to add.
workflow-palette-step-kinds = Step kinds
workflow-problems-list-no-problems-can-run = No problems — it can run.
workflow-problems-list-no-problems-last-design-read = No problems in the last design it read.
workflow-problems-list-workflow = workflow
workflow-run-history-run = Run { $index }
workflow-run-history-runs = Runs
workflow-run-history-view = View
workflow-run-history-withdraw = Withdraw
workflow-run-history-withdrawn = Withdrawn.
workflow-run-overlay-legend = Legend
workflow-run-overlay-run = Run { $index } · { $word }
workflow-run-overlay-run-2 = Run { $run }
workflow-run-overlay-runs = { $runs } runs
workflow-run-overlay-starts-when-live-run-finishes = — starts when the live run finishes
workflow-run-overlay-which = { $run } · { $revision }
workflow-run-overlay-which-run = Which run
workflow-run-view-cancelled = cancelled
workflow-run-view-diverted = diverted → { $by }
workflow-run-view-done = done
workflow-run-view-done-chose = done → { $branches }
workflow-run-view-failed = failed
workflow-run-view-failed-because = failed: { $error }
workflow-run-view-loop-place = { $branch } { $index }
workflow-run-view-pending = pending
workflow-run-view-pending-visited = pending (visited { $visits })
workflow-run-view-running = running
workflow-run-view-running-item = running (item { $work_item })
workflow-run-view-skipped = skipped
workflow-run-view-state-diverted = diverted
workflow-run-view-state-waiting = waiting
workflow-run-view-waiting = waiting on you
workflow-run-view-words = { $branch } { $index } of { $count }
workflow-run-dialog-refused-needs-goal = It runs on a goal only: { $steps } { $n ->
    [one] reads the goal it serves
   *[other] read the goal they serve
  }, and a run in the workspace has none.
workflow-run-dialog-refused-steps = { $first } and { $last }
workflow-run-workflow-dialog-no-inputs = It takes no inputs: it starts as it is.
workflow-run-workflow-dialog-optional = Optional.
workflow-run-workflow-dialog-queue-run = Queue run
workflow-run-workflow-dialog-run = Run “{ $workflow }”
workflow-run-workflow-dialog-run-2 = Run
workflow-run-workflow-dialog-run-has-started = The run has started.
workflow-run-workflow-dialog-runs-in-workspace = It runs in the workspace, with no goal — at once, beside any other run of it.
workflow-run-workflow-dialog-starting = Starting…
workflow-run-workflow-dialog-statement = Statement
workflow-runs-pane-no-runs-yet = Run… starts one in the workspace.
workflow-runs-pane-older-hidden = { $n ->
    [one] One older run is not shown.
   *[other] { $n } older runs are not shown.
  }
workflow-runs-pane-open = Open
workflow-runs-pane-restart = Restart
workflow-runs-pane-restarted = Restarted — a new run started with its inputs.
workflow-runs-pane-runs-on-goal-only = A step reads the goal it serves, so it runs on a goal only.
workflow-runs-pane-show-older = { $n ->
    [one] Show it
   *[other] Show { $n } more
  }
workflow-runs-pane-stop = Stop
workflow-runs-pane-stopped = Stopped.
workflow-runs-run-title = { $name } #{ $number }
workflow-runs-started-by-check = by check
workflow-runs-started-by-connector = by connector
workflow-runs-started-by-event = by an event
workflow-runs-started-by-hand = by you
workflow-runs-started-by-hook = by hook
workflow-runs-started-by-message = by message
workflow-runs-started-by-message-from = by message from { $detail }
workflow-runs-started-by-platform = by platform event
workflow-runs-started-by-project = by project change
workflow-runs-started-by-run = by run
workflow-runs-started-by-run-named = by run { $detail }
workflow-runs-started-by-schedule = by schedule
workflow-runs-started-by-signal = by signal
workflow-runs-started-by-signal-named = by signal { $detail }
workflow-runs-test-run = test run
workflow-runs-what-run = the run
workflow-runs-what-runs = the runs
workflow-spawn-step-form-also-from-inputs = Also from inputs
workflow-spawn-step-form-assignee-input-run-started-carries-child = An assignee input the run was started with carries the child too.
workflow-spawn-step-form-assignees = Assignees
workflow-spawn-step-form-blank-hands-child-workflow-agent-parent = Blank hands the child to the Workflow Agent, in the parent goal's mode.
workflow-spawn-step-form-child-goal-s-own-words = The child goal's own words. { $TEMPLATE_HINT }
workflow-spawn-step-form-guided-workflow-agent-proposes = Guided — the Workflow Agent proposes
workflow-spawn-step-form-off-step-done-moment-child-exists = Off: this step is done the moment the child exists.
workflow-spawn-step-form-wait-child-s-run-finish = Wait for the child's run to finish
workflow-spawn-step-form-who-carries-child = Who carries the child.
workflow-spawn-step-form-child-does-not-ask = the child's workflow does not ask for it
workflow-spawn-step-form-could-not-read-workflow = Could not read the workflow: { $why }
workflow-spawn-step-form-gives = What this step gives { $input }
workflow-spawn-step-form-its-run-needs-this = its run needs this
workflow-spawn-step-form-what-child-is-given = What the child is given
workflow-spawn-step-form-what-child-is-given-hint = Each input the child's workflow asks for, and what this step gives it, read as the input's kind: a number where a number is asked, a project's id where a project is. { $TEMPLATE_HINT }
workflow-spawn-step-form-workflow = Workflow
workflow-start-run-dialog-adopt-start = Adopt “{ $name }” and start it?
workflow-start-run-dialog-adopt-start-2 = Adopt and start
workflow-start-run-dialog-adopted-run-has-started = Adopted — the run has started.
workflow-start-run-dialog-queued-starts-when-live-run-finishes = Queued — it starts when the live run finishes.
workflow-start-run-dialog-queuing = Queuing…
workflow-start-run-dialog-run-live-one-starts-when-finishes = A run is live: this one starts when it finishes. One run at a time; the rest wait their turn.
workflow-start-run-dialog-run-starts-once-these-inputs = The run starts at once with these inputs.
workflow-start-run-dialog-start = Start “{ $name }”
workflow-start-run-dialog-start-run = Start run
workflow-start-run-dialog-workflow-agent-proposed-shape-adopting-records = The Workflow Agent proposed this shape. Adopting it records your decision and starts the run with these inputs.
workflow-step-actions-agent-step = An agent is on this step.
workflow-step-actions-answer = Your answer…
workflow-step-actions-answer-with = { $picked ->
    [0] Answer with the ticked options
    [one] Answer with this option
   *[other] Answer with these { $picked }
  }
workflow-step-actions-answered = Answered { $o }.
workflow-step-actions-answered-2 = Answered.
workflow-step-actions-anything-options-miss = Anything the options miss…
workflow-step-actions-approval-signed-decision-decide-gate-pinned = An approval is a signed decision — decide it on the gate pinned above the conversation.
workflow-step-actions-done-hand = Done by hand
workflow-step-actions-go-gate = Go to the gate
workflow-step-actions-i-m-not-sure = I'm not sure
workflow-step-actions-marked-done = Marked done.
workflow-step-actions-open-work-item = Open the work item
workflow-step-actions-release = Release
workflow-step-actions-released = Released.
workflow-step-actions-run-holds-here-until-release = The run holds here until you release it.
workflow-step-actions-said-not-sure = Said you are not sure.
workflow-step-actions-send-answer = Send answer
workflow-step-actions-step-yours = This step is yours
workflow-step-common-form-another-step-has-id = Another step has this id.
workflow-step-common-form-failed-attempts-re-run-before-failure = Failed attempts re-run before On failure applies.
workflow-step-common-form-failure = On failure
workflow-step-common-form-how-many-times-loop-may-enter = How many times a loop may enter this step. { $DEFAULT_MAX_VISITS } by default; at least one.
workflow-step-common-form-how-several-incoming-flows-meet-irrelevant = When several flows lead into this step, which of them it waits for. With one flow in, any choice reads the same.
workflow-step-common-form-id = Id
workflow-step-common-form-join = Join
workflow-step-common-form-max-visits = Max visits
workflow-step-common-form-max-visits-2 = max visits
workflow-step-common-form-named-flows-templates-steps-id-output = Named in flows and templates: {"{"}steps.<id>.output{"}"}.
workflow-step-common-form-once-retries-spent-route-another-step = Once the retries are spent. A route to another step is drawn from the card's left side; it is set here, not by dragging.
workflow-step-common-form-remediation-step-only-edge-taken-failure = The remediation step. Only this edge is taken on failure.
workflow-step-common-form-retries = Retries
workflow-step-common-form-route-failures = Route failures to
workflow-step-common-form-what-canvas-shows = What the canvas shows.
workflow-step-common-form-z-0-9-starts-letter-32 = Use a–z, 0–9, - and _, starting with a letter; 32 at most.
workflow-step-kinds-agent = Agent
workflow-step-kinds-agent-does-work-yields-result = An agent does the work and yields a result.
workflow-step-kinds-all-these-hold = all of these hold
workflow-step-kinds-any-these-holds = any of these holds
workflow-step-kinds-approval = Approval
workflow-step-kinds-call-one-operation-outside-platform-through = Call one operation of an outside platform through a connector installed here.
workflow-step-kinds-calls-connector-not-installed-here = calls a connector that is not installed here
workflow-step-kinds-calls-operation-connector-does-not-have = calls an operation the connector does not have
workflow-step-kinds-check = Check
workflow-step-kinds-check-approval-passed = a check or approval passed
workflow-step-kinds-choice-not-made-yet-pick-inspector = a choice not made yet — pick it in the inspector
workflow-step-kinds-command-schema-judges-result-not-passing = A command or a schema judges a result. Not passing is a failure.
workflow-step-kinds-condition-empty-nests-too-deep = a condition is empty or nests too deep
workflow-step-kinds-connector = Connector
workflow-step-kinds-cron-expression-cannot-scheduled = the cron expression cannot be scheduled
workflow-step-kinds-cron-schedule = a cron schedule
workflow-step-kinds-decide = Decide
workflow-step-kinds-decision-making-agent-reads-rendered-state-picks = The Decision-Making Agent reads a rendered state and picks one of its options; otherwise when it is not sure enough.
workflow-step-kinds-does-not-hold = this does not hold
workflow-step-kinds-each = For each
workflow-step-kinds-end = End
workflow-step-kinds-end-step-has-flows-out = an end step has flows out of it
workflow-step-kinds-every-flow-out-branching-step-carries = every flow out of a branching step carries a branch
workflow-step-kinds-exactly-one-arrives-second-fails-step = exactly one arrives; a second fails the step
workflow-step-kinds-exactly-one-these-holds = exactly one of these holds
workflow-step-kinds-fail-run = fail the run
workflow-step-kinds-flows-into-itself = flows into itself
workflow-step-kinds-flows-step-does-not-exist = flows to a step that does not exist
workflow-step-kinds-hour-utc-between = the hour (UTC) is between
workflow-step-kinds-human = Human
workflow-step-kinds-human-step-answered = a human step was answered with
workflow-step-kinds-if = If
workflow-step-kinds-input-declared-but-no-step-reads = an input is declared but no step reads it
workflow-step-kinds-input-equals = an input equals
workflow-step-kinds-input-not-kind-field-needs = the input is not of the kind this field needs
workflow-step-kinds-judge = Judge
workflow-step-kinds-labelled-flow-names-branch-no-rule = a labelled flow names a branch no rule chooses
workflow-step-kinds-loop-s-done-flow-leads-back = the loop's done flow leads back into the loop
workflow-step-kinds-max-iterations-must-least-one = max iterations must be at least one
workflow-step-kinds-max-visits-must-least-one = max visits must be at least one
workflow-step-kinds-more-than-one-step-has-nothing = more than one step has nothing flowing into it
workflow-step-kinds-names-agent-team-person-not-here = names an agent, team or person that is not here
workflow-step-kinds-names-harness-runtime-cannot-launch = names a harness this runtime cannot launch
workflow-step-kinds-names-input-workflow-does-not-declare = names an input the workflow does not declare
workflow-step-kinds-names-project-does-not-exist = names a project that does not exist
workflow-step-kinds-names-workflow-not-installed = names a workflow that is not installed
workflow-step-kinds-needs-name = needs a name
workflow-step-kinds-not-valid-question-human-step-s = not a valid question — a human step's options, or a judge's minimum confidence outside 0 to 1
workflow-step-kinds-nothing-choose-between-so-always-takes = nothing to choose between, so it always takes otherwise — a decide with no rules, a switch with no cases, or a judge with fewer than two options
workflow-step-kinds-nothing-loop-s-body-side-flows = nothing on the loop's body side flows back into it
workflow-step-kinds-nothing-reaches-step = nothing reaches this step
workflow-step-kinds-notify = Notify
workflow-step-kinds-number-seconds = a number of seconds
workflow-step-kinds-one-condition-yes-when-holds-no = One condition: yes when it holds, no when it does not.
workflow-step-kinds-one-item-time-down-each-flow = One item at a time down the each flow, then done; the body flows back.
workflow-step-kinds-only-branching-step-decide-if-switch = only a branching step (decide, if, switch, judge, for each, while) labels its flows, or a boundary event that diverts labels its own
workflow-step-kinds-parameter-unset-not-one-operation-takes = a parameter is unset, or is not one the operation takes
workflow-step-kinds-params-account-placeholder-belongs-connector-definition = a params or account placeholder belongs to a connector definition, not a step
workflow-step-kinds-person-answers-does-something-hand-marks = A person answers, or does something by hand and marks it done.
workflow-step-kinds-person-releasing = a person releasing it
workflow-step-kinds-pins-model-harness-does-not-list = pins a model the harness does not list
workflow-step-kinds-placeholder-root-platform-does-not-know = a placeholder root this platform does not know — to write a brace literally, double it
workflow-step-kinds-placeholders-inputs-x-steps-id-output = Placeholders: {"{"}inputs.x{"}"}, {"{"}steps.id.output.field{"}"}, {"{"}steps.id.answer{"}"}, {"{"}goal.statement{"}"}, {"{"}goal.title{"}"}; a literal brace is doubled: {"{"}{"{"} and {"}"}{"}"}.
workflow-step-kinds-post-into-conversation-agent-workflow-agent = Post into a conversation as an agent — the Workflow Agent unless one is named. Mentions wake the agents named.
workflow-step-kinds-posts-into-something-not-channel-goal = posts into something that is not a channel, goal or workstream
workflow-step-kinds-reads-field-step-s-output-schema = reads a field the step's output schema does not promise
workflow-step-kinds-reads-goal-run-workspace-has-none = reads the goal it serves, which a run in the workspace does not have
workflow-step-kinds-reads-output-from-step-yields-none = reads an output from a step that yields none
workflow-step-kinds-reads-step-not-sure-have-run = reads a step that is not sure to have run by then
workflow-step-kinds-refers-step-does-not-run-before = refers to a step that does not run before this one
workflow-step-kinds-rendered-value-against-cases-otherwise-when = A rendered value against its cases; otherwise when none matches.
workflow-step-kinds-round-loop-flow-while-condition-holds = Round the loop flow while the condition holds, then done.
workflow-step-kinds-route-another-step = route to another step
workflow-step-kinds-rule-chooses-branch-no-flow-carries = a rule chooses a branch no flow carries
workflow-step-kinds-runs-account-machine-does-not-have = runs as an account this machine does not have, or has no default account
workflow-step-kinds-schema-not-json-schema = the schema is not a JSON Schema
workflow-step-kinds-shell-command-exits-0 = a shell command exits 0
workflow-step-kinds-signed-yes-no-declined-failure-step = A signed yes or no. Declined is a failure of the step.
workflow-step-kinds-skip-continue = skip and continue
workflow-step-kinds-spawn = Spawn
workflow-step-kinds-spawns-workflow-spawns-one-back = spawns a workflow that spawns this one back
workflow-step-kinds-speaks-person-team-only-agent-speaks = speaks as a person or a team; only an agent speaks for a workflow
workflow-step-kinds-start-first-arrival = start on the first arrival
workflow-step-kinds-step-s-output-contains = a step's output contains
workflow-step-kinds-step-s-output-equals = a step's output equals
workflow-step-kinds-sub-goal-own-workflow-linked-one = A sub-goal with its own workflow, linked to this one.
workflow-step-kinds-switch = Switch
workflow-step-kinds-template-s-braces-do-not-balance = a template's braces do not balance — to write a brace literally, double it
workflow-step-kinds-two-flows-carry-same-branch = two flows carry the same branch
workflow-step-kinds-two-inputs-share-name = two inputs share this name
workflow-step-kinds-two-steps-share-id = two steps share this id
workflow-step-kinds-upstream-output-fits-json-schema = an upstream output fits a JSON schema
workflow-step-kinds-wait = Wait
workflow-step-kinds-wait-every-incoming-flow = wait for every incoming flow
workflow-step-kinds-while = While
workflow-step-kinds-workflow-has-no-steps = the workflow has no steps
workflow-step-kinds-writes-platform-no-approval-human-step = writes to a platform with no approval or human step before it and no word that it runs unattended
workflow-step-node-fail = on fail → { $step }
workflow-step-node-fail-skip = on fail skip
workflow-step-node-join = join { $step }
workflow-step-node-retries = retries { $retries }
workflow-step-node-visits = visits ≤ { $max_visits }
workflow-switch-step-form-add-case = Add a case
workflow-switch-step-form-branch-when-no-case-matches = The branch when no case matches.
workflow-switch-step-form-case-n = case { $n }
workflow-switch-step-form-case-value = case value
workflow-switch-step-form-cases = Cases
workflow-switch-step-form-first-case-whose-value-text-names = The first case whose value is the text names the branch. Draw a flow per branch from the step's handles.
workflow-switch-step-form-remove-case = Remove case
workflow-switch-step-form-rendered-text-when-step-entered-compared = Rendered as text when the step is entered and compared to each case's value exactly. { $TEMPLATE_HINT }
workflow-switch-step-form-words = On
workflow-template-card-brings = brings { $list }
workflow-template-card-installed = installed
workflow-template-card-not-installed = not installed yet
workflow-template-card-open = Open
workflow-template-card-use-template = Use template
workflow-template-gallery-already-installed-opening = { $slug } was already installed — opening it.
workflow-template-gallery-catalog-compiled-into-binary = The catalog is compiled into the binary.
workflow-template-gallery-installed-2 = Installed { $slug }, and with it: { $brought }.
workflow-template-gallery-installed-3 = Installed { $slug }.
workflow-template-gallery-installed-but-copy-could-not-found = { $slug } is installed, but its copy could not be found — reload the library.
workflow-template-gallery-no-templates-build = No templates in this build
workflow-wait-step-form-add-field = Add a field
workflow-wait-step-form-blank-utc-named-zone-follows-daylight = Blank is UTC; a named zone follows daylight saving.
workflow-wait-step-form-counted-engine-s-own-clock-survives = Counted on the engine's own clock; survives a restart. Fixed, or read from a number input.
workflow-wait-step-form-cron = Cron
workflow-wait-step-form-fields = Fields
workflow-wait-step-form-five-fields-0-9-1-5 = Five fields. 0 9 * * 1-5 is 9am on weekdays. Fixed, or read from a text input.
workflow-wait-step-form-remove-field = Remove field
workflow-wait-step-form-run-holds-here-until-person-releases = The run holds here until a person releases the step — from the goal's Workflow tab or the CLI.
workflow-wait-step-form-seconds = Seconds
workflow-wait-step-form-timezone = Timezone
workflow-wait-step-form-topic = Topic
workflow-wait-step-form-utc = UTC
workflow-wait-step-form-value-source = value source
workflow-wait-step-form-wait = Wait for
workflow-while-step-form-reaching-fails-step-unless-say-otherwise = Reaching it fails the step. { $DEFAULT_MAX_ITERATIONS } unless you say otherwise.
workflow-while-step-form-tested-every-entry-holds-loop-flow = Tested on every entry. Holds → the loop flow; the body's last step flows back here. Does not → done.
workflow-workflow-agent-pane-mention-workflow-agent-analyse-fix-finish = Mention @Workflow Agent to analyse, fix or finish this workflow — it reads its inputs, steps and runs and saves at the revision it read; the canvas beside you shows the change. Every conversation about it is saved here and yours to come back to.
workflow-workflow-card-archived = archived
workflow-workflow-card-delete = Delete…
workflow-workflow-card-from-template = from { $slug }
workflow-workflow-card-goals-workflows-use-it = Goals and workflows that use it
workflow-workflow-card-inputs = { $inputs } inputs
workflow-workflow-card-menu = Actions for { $w }
workflow-workflow-card-nothing-running = Nothing was running it.
workflow-workflow-card-open-project-ide = Open in the Project IDE
workflow-workflow-card-open = Open
workflow-workflow-card-problem-s = { $problems ->
    [one] 1 problem
   *[other] { $problems } problems
  }
workflow-workflow-card-projects-made-by-steps = { $projects ->
    [one] 1 project made by its steps:
   *[other] { $projects } projects made by its steps:
}
workflow-workflow-card-projects-steps-made = Projects its steps made
workflow-workflow-card-restart-every-run = Restart every run
workflow-workflow-card-restart-every-run-workflow = Restart every run of this workflow?
workflow-workflow-card-ready-to-run = ready to run
workflow-workflow-card-running-in-goals = { $n ->
    [one] running in one goal
   *[other] running in { $n } goals
}
workflow-workflow-card-runs-on-goal = runs on a goal
workflow-workflow-card-stop-every-run = Stop every run
workflow-workflow-card-stop-every-run-workflow = Stop every run of this workflow?
workflow-workflow-card-used = used by { $used_by }
workflow-workflow-card-yours = yours
workflow-workflow-form-account = One of the connector's accounts.
workflow-workflow-form-agent-id-team-id-human-64 = agent:<id>, team:<id> or human:<64 hex>.
workflow-workflow-form-number = A number.
workflow-workflow-form-one-options = One of the options.
workflow-workflow-form-project-id = A project id.
workflow-workflow-form-required = Required.
workflow-workflow-form-yes-no = Yes or no.
workflow-workflow-graph-both-ends-must-steps = both ends must be steps
workflow-workflow-graph-branch-already-flows-somewhere = branch { $branch } already flows somewhere
workflow-workflow-graph-branch-needs-name = a branch needs a name
workflow-workflow-graph-copy = { $src } (copy)
workflow-workflow-graph-end-step-has-nothing-after = an end step has nothing after it
workflow-workflow-graph-flow-already-exists = that flow already exists
workflow-workflow-graph-flow-out-step-carries-branch = a flow out of a { $kind } step carries a branch
workflow-workflow-graph-only-branching-step-decide-if-switch = only a branching step (decide, if, switch, judge, for each, while) labels its flows, or a boundary event that diverts labels its own
workflow-workflow-graph-only-decide-switch-judge-step-has = only a decide, switch or judge step has branches
workflow-workflow-graph-step-already-has-branch = this { $kind } step already has a branch { $next }
workflow-workflow-graph-step-cannot-flow-into-itself = a step cannot flow into itself
workflow-workflow-graph-step-has-no-branch = this { $kind } step has no branch { $branch }
workflow-workflow-picker-catalog-templates-installed-when-picked = Catalog templates (installed when picked)
workflow-workflow-picker-goal-s-designs = This goal's designs
workflow-workflow-picker-installed-but-could-not-find-afterwards = installed { $slug }, but could not find it afterwards
workflow-workflow-picker-installing = Installing…
workflow-workflow-picker-library = The library
workflow-workflow-picker-loading = Loading…
workflow-workflow-picker-no-workflow = No workflow
workflow-workflow-picker-option-another-goals-design = { $name } (another goal's design)
workflow-workflow-picker-option-archived = { $name } (archived)
workflow-workflow-picker-option-problems = { $name } ({ $n } { $n ->
    [one] problem
   *[other] problems
  })
workflow-workflow-picker-pick-workflow = Pick a workflow…
workflow-workflow-picker-workflow = workflow
workflow-workflow-verbs-run = Run…

## By hand — a sentence with a plural, a slot or several pieces; the generator keeps this section.
workflow-goal-workflow-tab-run-failed-history-read-only-workflow = This run failed at { $failure }{ $flag ->
    [yes] {" "}— { $error }
   *[no] {""}
  }. It is history and read-only: { $flag3 ->
    [yes] the Workflow Agent repairs it and the platform starts again
   *[no] restart it, or start a new { $flag2 ->
    [yes] run, or wait for the Workflow Agent's repair
   *[no] run
  }
  }.
workflow-step-actions-answered-one-option-options = Answered { $picked ->
    [one] one option
   *[other] { $picked } options
  }.
workflow-step-node-problem-problems = { $problems } { $problems ->
    [one] problem
   *[other] problems
  }
workflow-workflow-card-restarted-run-runs = Restarted { $runs } { $runs ->
    [one] run
   *[other] runs
  }.
workflow-workflow-card-stopped-run-runs = Stopped { $runs } { $runs ->
    [one] run
   *[other] runs
  }.
workflow-workflow-card-running-runs = running { $n ->
    [one] one run
   *[other] { $n } runs
  }
workflow-workflow-verbs-stop-every-run-words = { $n ->
    [one] The run of this workflow that is going is cancelled and its sessions end.
   *[other] The { $n } runs of this workflow that are going are cancelled and their sessions end.
  } A goal's run of it is its goal's, and goes on.
workflow-workflow-verbs-restart-every-run-words = { $n ->
    [one] The run of this workflow that is going is cancelled and a new run starts at once, with its inputs.
   *[other] The { $n } runs of this workflow that are going are cancelled and a new run starts at once in place of each, with its inputs.
  } A goal's run of it is its goal's, and goes on.
workflow-connector-step-form-account-option = { $label }{ $default ->
    [yes] {" "}(default)
   *[no] {""}
  }
workflow-connector-step-form-operation-option = { $id } — { $name }{ $writes ->
    [yes] {" "}(writes)
   *[no] {""}
  }
workflow-judge-step-form-minimum-confidence-hint = How sure the pick must be to be taken. Left blank, the node's own decisions.confidence.act applies.
workflow-problems-list-node-could-not-read-design = The node could not read this design: <detail/>
workflow-run-overlay-live-steps = live: <steps/>
workflow-designer-session-not-saved = Not saved: { $failure }
workflow-graph-branches-fixed = a { $kind } step's branches are fixed: { $branches }

## Events and gateways — the designer's vocabulary (`stepKinds.mjs`): the four families, the kinds start, emit and parallel, the closed sets the forms choose from, the problems.
workflow-step-kinds-events = Events
workflow-step-kinds-gateways = Gateways
workflow-step-kinds-loops = Loops
workflow-step-kinds-tasks = Tasks
workflow-step-kinds-start = Start
workflow-step-kinds-one-way-run-begins-hand-event = One way a run begins: by hand, or on an event — a schedule, a call, a message, a signal, a change.
workflow-step-kinds-hold-until-world-moves = Hold until the world moves: a delay, a moment, a schedule, a signal, a message, a change, a run's end — or a person releasing it.
workflow-step-kinds-emit-signal = Emit signal
workflow-step-kinds-raise-named-signal-workflows-start-wait = Raise a named signal that workflows start on, wait for, or hear on a live step.
workflow-step-kinds-end-path-finish-run-fail-it = End this path, finish the run, or fail it.
workflow-step-kinds-first-every-rule-holds-names-branches = The first rule that holds names the branch — or every rule that holds, all at once; otherwise the default.
workflow-step-kinds-parallel = Parallel
workflow-step-kinds-every-flow-out-at-once-paths-meet = Every flow out of it at once; where the paths meet, the step they flow into joins them.
workflow-step-kinds-moment-deadline = a moment — a deadline
workflow-step-kinds-named-signal = a named signal
workflow-step-kinds-message = a message
workflow-step-kinds-project-s-change = a project's change
workflow-step-kinds-run-s-end = a run's end
workflow-step-kinds-platform-event = a platform event
workflow-step-kinds-by-hand = By hand
workflow-step-kinds-on-schedule = On a schedule
workflow-step-kinds-when-called = When called
workflow-step-kinds-when-message-arrives = When a message arrives
workflow-step-kinds-when-signal-raised = When a signal is raised
workflow-step-kinds-when-project-changes = When a project changes
workflow-step-kinds-when-run-finishes = When a run finishes
workflow-step-kinds-when-platform-says = When the platform says
workflow-step-kinds-when-outside-platform-lists-something-new = When an outside platform lists something new
workflow-step-kinds-when-check-starts-failing = When a check starts failing
workflow-step-kinds-timeout = Timeout
workflow-step-kinds-reminder = Reminder
workflow-step-kinds-boundary-message = Message
workflow-step-kinds-boundary-signal = Signal
workflow-step-kinds-divert-path = Divert to a path
workflow-step-kinds-post = Post
workflow-step-kinds-emit = Emit
workflow-step-kinds-end-this-path = End this path
workflow-step-kinds-finish-run = Finish the run
workflow-step-kinds-fail-run-now = Fail the run
workflow-step-kinds-first-rule-holds = the first rule that holds
workflow-step-kinds-every-rule-holds = every rule that holds
workflow-step-kinds-one-at-time-later-wait = one at a time — the later ones wait their turn
workflow-step-kinds-skip-while-one-runs = skip it while one is still going
workflow-step-kinds-several-at-once = several at once, up to a number
workflow-step-kinds-from-you = you
workflow-step-kinds-from-any-agent = any agent
workflow-step-kinds-from-someone-named = someone named
workflow-step-kinds-commit = a commit
workflow-step-kinds-push-fetch = a push or a fetch
workflow-step-kinds-pull-request-s-change = a pull request's change
workflow-step-kinds-merge = a merge
workflow-step-kinds-change-files = a change of files
workflow-step-kinds-run-done = done
workflow-step-kinds-run-failed = failed
workflow-step-kinds-run-cancelled = cancelled
workflow-step-kinds-when-starts-failing = when it starts failing — once per outage
workflow-step-kinds-every-time-fails = every time it fails
workflow-step-kinds-every-time-passes = every time it passes
workflow-step-kinds-every-time-runs = every time it runs
workflow-step-kinds-flow-leads-into-start = a flow leads into a start; a start begins a run
workflow-step-kinds-more-than-one-start-by-hand = more than one start is by hand
workflow-step-kinds-start-by-hand-carries-mapping-guard = a start by hand carries an input mapping or a guard
workflow-step-kinds-event-read-outside-start-s-inputs = the event is read outside a start's inputs, or a start reads what is not its own
workflow-step-kinds-boundary-events-step-cannot-stopped = boundary events on a step that cannot be stopped while it is live
workflow-step-kinds-reminder-diverts-stops-step-first-tick = a reminder diverts — it would stop the step at its first tick
workflow-step-kinds-clock-cannot-run = a clock that cannot run — zero seconds, no fires, or both every and cron
workflow-step-kinds-signal-name-not-dotted-lowercase-words = a signal's name is not dotted lowercase words
workflow-step-kinds-poll-cannot-poll = a poll that cannot poll — a writing operation, a file, or no key
workflow-step-kinds-platform-event-never-emitted = a platform event this platform never emits
workflow-step-kinds-spawns-workflow-only-events-start = spawns a workflow only events start
workflow-workflow-graph-start-begins-run-nothing-flows-into = a start begins a run; nothing flows into it

## Start events — a start in a few words, its guard (`forms/startForm.mjs`) and its form (`forms/StartStepForm.tsx`).
workflow-start-form-guard-queue = One at a time: a later occurrence waits for the run before it.
workflow-start-form-guard-skip = Skipped while a run it started is still going — and it says so.
workflow-start-form-guard-parallel = Up to { $max } runs at once; the rest wait.
workflow-start-form-guard-debounced = { $overlap } One within { $secs } of the last run it started is dropped.
workflow-start-form-by-hand = by hand
workflow-start-form-every = every { $secs }
workflow-start-form-on-cron = on the schedule { $cron }
workflow-start-form-schedule-unset = on a schedule (not set yet)
workflow-start-form-when-called = when called
workflow-start-form-called-from-outside = when called from outside
workflow-start-form-message = a message
workflow-start-form-message-in = a message in { $scope }
workflow-start-form-signal = a signal
workflow-start-form-signal-named = the signal { $name }
workflow-start-form-project-change = { $change ->
    [commit] a commit
    [push] a push or a fetch
    [pull_request] a pull request's change
    [merge] a merge
   *[files] a change of files
  }
workflow-start-form-run-ends = { $outcome ->
    [done] a run that is done
    [failed] a run that fails
    [cancelled] a run that is cancelled
   *[any] a run that ends
  }
workflow-start-form-platform = a platform event
workflow-start-form-platform-topic = the platform event { $topic }
workflow-start-form-poll = each new item from an outside platform
workflow-start-form-poll-call = each new item from { $call }
workflow-start-form-check = { $fire_on ->
    [starts_failing] a check that starts failing
    [failing] a check that fails
    [passing] a check that passes
   *[always] a check's every result
  }
workflow-start-step-form-begins = Begins
workflow-start-step-form-begins-hint = What begins a run here. A workflow may carry several starts; a run enters the one it began at.
workflow-start-step-form-by-hand-hint = A person begins it — Run… in the library, a goal's start, the CLI — and gives its inputs then. A start by hand maps nothing and has no guard.
workflow-start-step-form-listening-inputs-hint = Placeholders: {"{"}inputs.x{"}"} — the inputs given when it was turned on; a literal brace is doubled: {"{"}{"{"} and {"}"}{"}"}.
workflow-start-step-form-could-not-arm = This start is not heard: { $why }
workflow-start-step-form-inputs-from-event = Inputs from the event
workflow-start-step-form-inputs-from-event-hint = One template per input, read off the occurrence — {"{"}event.payload.field{"}"}. The event is read here and nowhere else: a step reads the input. An input left blank takes what was given when it was turned on, else its default.
workflow-start-step-form-from-listening-inputs = given when it is turned on
workflow-start-step-form-left-default = its default
workflow-start-step-form-maps-onto = What maps onto { $input }
workflow-start-step-form-maps-undeclared-input = maps onto an input the workflow does not declare
workflow-start-step-form-guard = Guard
workflow-start-step-form-overlap = While a run it started is still going
workflow-start-step-form-at-most-at-once = How many at once
workflow-start-step-form-debounce = Drop repeats within
workflow-start-step-form-debounce-seconds = Seconds within which a repeat is dropped
workflow-start-step-form-seconds = seconds
workflow-start-step-form-local-call = Called on this machine, under the control-plane token: POST { $path }
workflow-start-step-form-local-call-once-saved = Its local call is shown once the design is saved.
workflow-start-step-form-public = May be called from outside
workflow-start-step-form-public-hint = Off, only this machine calls it. On, a caller outside posts to its public path with its secret — while Settings › Automation › Events allows public hooks.
workflow-start-step-form-public-call = Called from outside: POST { $path }
workflow-start-step-form-public-once-listening = Its public path and its secret are made when it begins listening; the secret is shown once, then.
workflow-start-step-form-secret-minted = A secret is set
workflow-start-step-form-no-secret-yet = No secret yet
workflow-start-step-form-rotate-title = Replace this hook's secret?
workflow-start-step-form-rotate-body = Callers using the current secret will be refused until they get the new one. The new secret is shown once.
workflow-start-step-form-rotate-confirm = Replace the secret
workflow-start-step-form-rotate = Rotate
workflow-start-step-form-poll-reads-only = One of the connector's reads: a poll never writes.
workflow-start-step-form-poll-params-hint = One template per parameter, over the inputs given when it was turned on. Every item its answer lists that an earlier poll did not begins a run; the first poll only learns what is there.
workflow-start-step-form-key = Key
workflow-start-step-form-key-hint = The field, inside one item, that tells one item from another — dotted.
workflow-start-step-form-check-command-hint = A shell command, run on the cadence below under the command guard's rules, as a check step's is. It reads the inputs given when it was turned on.
workflow-start-step-form-check-project-hint = Where it runs: in this project's tree. Left unset, in a scratch folder of its own.
workflow-start-step-form-own-scratch-folder = Its own scratch folder
workflow-start-step-form-fire-on = Begins a run

## The filters the events share — a start's, a wait's, a boundary event's (`forms/MessageFilterFields.tsx`, `ProjectFilterFields.tsx`, `RunFilterFields.tsx`, `SignalFilterFields.tsx`, `ScheduleFields.tsx`, `ExactFieldsEditor.tsx`).
workflow-message-filter-fields-where = Where
workflow-message-filter-fields-where-hint = The conversation it lands in — a channel, or one an input names. Anywhere when left so.
workflow-message-filter-fields-anywhere = Anywhere
workflow-message-filter-fields-from = From
workflow-message-filter-fields-from-hint = Who wrote it. A team counts every member's message; an announcement is never heard.
workflow-message-filter-fields-who = Who
workflow-message-filter-fields-mentions = Mentions
workflow-message-filter-fields-mentions-hint = Only a message that mentions this agent, team or person. Blank: any message.
workflow-message-filter-fields-contains = Contains
workflow-message-filter-fields-contains-hint = Text the message must contain, whatever its case. { $hint }
workflow-project-filter-fields-project-hint = The project whose change is heard — never a path on a disk. Fixed, or an input of kind project.
workflow-project-filter-fields-change = Change
workflow-project-filter-fields-branch = Branch
workflow-project-filter-fields-branch-hint = Only this branch; blank for any. A template.
workflow-project-filter-fields-files = Files
workflow-project-filter-fields-files-hint = Only files matching this — * and ? — blank for any. A template.
workflow-project-filter-fields-pull-requests-note = Heard for the pull requests the platform's workstreams opened or adopted.
workflow-run-filter-fields-workflow-hint = A run of this workflow — of any workflow when left so.
workflow-run-filter-fields-any-workflow = Any workflow
workflow-run-filter-fields-ending = Ending
workflow-run-filter-fields-any-end = however it ends
workflow-signal-filter-fields-name = Signal
workflow-signal-filter-fields-name-hint = Its name, dotted lowercase words — report.ready. { $hint }
workflow-signal-filter-fields-name-not-dotted-words = A signal's name is dotted lowercase words — report.ready.
workflow-signal-filter-fields-exact-matches = Exact matches on what it carries, by dotted path; all must hold. Values are templates.
workflow-signal-filter-fields-topic-hint = One of the engine's own events, by topic — goal.closed, step.changed. The advanced door, for what the other events do not name.
workflow-schedule-fields-cadence = Cadence
workflow-schedule-fields-every-seconds = every so many seconds
workflow-schedule-fields-on-cron = at each occurrence of a cron expression
workflow-schedule-fields-seconds-hint = Counted on the engine's own clock from the moment it begins listening. Fixed, or read from a number input.
workflow-exact-fields-needs-path = A field needs a path.
workflow-exact-fields-no-such-field = There is no field `{ $path }`.
workflow-exact-fields-path-taken = Another field already matches `{ $path }`.
workflow-exact-fields-path-too-long = A path is at most { $max } characters.
workflow-exact-fields-editor-path = field path
workflow-exact-fields-editor-value = field value

## Boundary events — the rules' words (`forms/boundaryModel.mjs`), the editor (`forms/BoundaryEventsEditor.tsx`) and the chips on the card (`StepNode.tsx`).
workflow-boundary-model-needs-name = a boundary event needs a name
workflow-boundary-model-name-too-long = a name is at most 64 characters
workflow-boundary-model-no-such-boundary = this step has no boundary event { $name }
workflow-boundary-model-name-taken = this step already uses the name { $name }
workflow-boundary-model-after = after { $secs }
workflow-boundary-model-every = every { $secs } · up to { $max }
workflow-boundary-model-message = message
workflow-boundary-model-message-containing = message “{ $text }”
workflow-boundary-model-signal = signal
workflow-boundary-model-signal-named = signal { $name }
workflow-boundary-model-diverts-to = Diverts to { $steps }.
workflow-boundary-model-diverts-nowhere-yet = Diverts nowhere yet: draw a flow from its handle — it carries “{ $name }”.
workflow-boundary-model-posts-beside = Posts beside the live step; the step goes on.
workflow-boundary-model-emits-beside = Raises its signal beside the live step; the step goes on.
workflow-boundary-events-editor-boundary-events = Boundary events
workflow-boundary-events-editor-hint = What may happen while this step is live — a timeout, a reminder, a message, a signal. Each diverts the step to a path of its own, drawn from its chip on the card, or acts beside it. A reminder never diverts.
workflow-boundary-events-editor-cannot-carry = This step cannot be stopped while it is live, so it carries no boundary events: remove these.
workflow-boundary-events-editor-name = Boundary event name
workflow-boundary-events-editor-remove = Remove boundary event
workflow-boundary-events-editor-listens-for = What it listens for
workflow-boundary-events-editor-does = What it does
workflow-boundary-events-editor-after = After
workflow-boundary-events-editor-after-hint = Seconds from the moment the step is entered. Fixed, or read from a number input.
workflow-boundary-events-editor-every = Every
workflow-boundary-events-editor-every-hint = Seconds between reminders, from the moment the step is entered. Fixed, or read from a number input.
workflow-boundary-events-editor-at-most = At most
workflow-boundary-events-editor-at-most-hint = How many times it fires while the step is live. { $n } unless you say otherwise.
workflow-boundary-events-editor-post-hint = Posted beside the live step — into the run's own conversation, as the Workflow Agent, unless the design names another. { $TEMPLATE_HINT }
workflow-boundary-events-editor-emit-hint = The signal it raises beside the live step — dotted lowercase words.
workflow-step-node-boundary-events = Boundary events
workflow-step-node-boundary-diverts = { $name } — { $words }: stops the step and takes its own path
workflow-step-node-boundary-acts = { $name } — { $words }: acts beside the step, which goes on

## Catch, throw and end events, and the gateways (`forms/WaitStepForm.tsx`, `EmitStepForm.tsx`, `EndStepForm.tsx`, `DecideStepForm.tsx`, `ParallelStepForm.tsx`).
workflow-wait-step-form-until = Until
workflow-wait-step-form-moment-hint = A moment: Unix seconds or an RFC 3339 timestamp — a deadline an input or an upstream step names. { $TEMPLATE_HINT }
workflow-wait-step-form-heard-is-output = What it heard is the step's output: {"{"}steps.{ $step }.output{"}"}.
workflow-emit-step-form-signal-hint = Its name, dotted lowercase words — report.ready. Heard by the starts, the waits and the boundary events that name it. { $TEMPLATE_HINT }
workflow-emit-step-form-payload = Payload
workflow-emit-step-form-payload-hint = What the signal carries, by dotted path — what a start maps onto its inputs and a filter matches. { $TEMPLATE_HINT }
workflow-emit-step-form-heard-by = Raised once, and never a failure because nobody listens. Its output names the signal: {"{"}steps.{ $step }.output.signal{"}"}.
workflow-end-step-form-ends = Ends
workflow-end-step-form-path-hint = This path ends here; the run is done once every path has.
workflow-end-step-form-done-hint = The run is done now: everything still live is cancelled.
workflow-end-step-form-failed-hint = The run fails now: this step fails, and everything still live is cancelled.
workflow-decide-step-form-choose = Takes
workflow-decide-step-form-every-holds-takes-branch = Every rule that holds takes its branch, all at once. Draw a flow per branch from the step's handles. When none holds, it takes otherwise.
workflow-parallel-step-form-takes-paths = { $n ->
    [0] It takes every flow out of it at once — draw one for each path.
    [one] It takes its one flow; draw another for each path to run beside it.
   *[other] It takes its { $n } flows at once.
  }
workflow-parallel-step-form-join-in-words = Where the paths meet again, the step they flow into joins them by its own join — every incoming flow, unless it says otherwise.

## Listening — On and Off: the switch's words (`listeningModel.mjs`, `ListeningSwitch.tsx`), turning On (`TurnOnDialog.tsx`, `HookSecretNote.tsx`), the verbs and the card's mark (`workflowVerbs.mjs`, `workflowCardModel.mjs`, `WorkflowCard.tsx`).
workflow-library-domain-general = general
workflow-listening-model-off = Off
workflow-listening-model-on = On — { $what }
workflow-listening-model-on-next = On — { $what } · next { $next }
workflow-listening-model-budget-not-a-ceiling = A ceiling is a number above zero — leave it blank for none.
workflow-listening-model-budget-tokens-whole = Tokens are counted whole.
workflow-listening-model-cannot-turn-on = Can't turn on: { $why }
workflow-listening-model-archived = it is archived
workflow-listening-model-goal-design = it is a goal's own design — its goal listens
workflow-listening-model-problems = { $n ->
    [one] 1 problem
   *[other] { $n } problems
  }
workflow-listening-model-reads-goal = a step reads the goal it serves, and a run in the workspace has none
workflow-listening-model-paused = Paused: { $why }
workflow-listening-model-paused-run-failed = a run it started failed
workflow-listening-model-paused-budget-spent = its budget is spent
workflow-listening-model-paused-unknown = it stopped hearing its events
workflow-listening-model-goal-listening = Listening
workflow-listening-model-goal-listening-for = Listening — { $what }
workflow-listening-model-goal-listening-next = Listening — { $what } · next { $next }
workflow-listening-switch-listen-again = Listen again
workflow-listening-switch-listening-again = Listening again.
workflow-workflow-verbs-test-run = Test run…
workflow-workflow-verbs-turn-on = Turn on…
workflow-workflow-verbs-turn-off = Turn off
workflow-workflow-card-on = On
workflow-workflow-card-paused = Paused
workflow-workflow-card-turned-off = “{ $workflow }” is off.
workflow-turn-on-dialog-title = Turn on “{ $workflow }”
workflow-turn-on-dialog-description = From now on it { $what }. Turning it on is no revision of the workflow.
workflow-turn-on-dialog-secrets = It is on. A hook that may be called from outside has its secret — shown once, here.
workflow-turn-on-dialog-needs = What its events do not supply — every run it starts takes these:
workflow-turn-on-dialog-no-needs = Its events supply everything a run needs: nothing to fill.
workflow-turn-on-dialog-budget = Each run may spend
workflow-turn-on-dialog-budget-hint = A ceiling for every run it starts. Left blank, the workspace's default applies — Settings › Automation › Budgets.
workflow-turn-on-dialog-dollars = dollars
workflow-turn-on-dialog-tokens = tokens
workflow-turn-on-dialog-minutes = minutes
workflow-turn-on-dialog-turn-on = Turn on
workflow-turn-on-dialog-turning-on = Turning on…
workflow-turn-on-dialog-done = Done
workflow-turn-on-dialog-on = “{ $workflow }” is on.
workflow-hook-secret-note-copy-now = Copy this secret now — it is shown once.
workflow-hook-secret-note-path = It guards POST { $path }.
workflow-hook-secret-note-copy = Copy the secret
workflow-hook-secret-note-copied = Secret copied.
workflow-hook-secret-note-not-copied = The secret could not be copied.
workflow-hook-secret-note-headers = A caller outside sends it as X-Bisa-Token, or signs the body with it as X-Hub-Signature-256.

## Run… — by hand, or a test as if an event happened (`RunWorkflowDialog.tsx`) — and a goal's start: listening, run now (`StartRunDialog.tsx`).
workflow-run-workflow-dialog-entry = Begins
workflow-run-workflow-dialog-by-hand = By hand
workflow-run-workflow-dialog-test-as-if = Test: as if “{ $start }” happened
workflow-run-workflow-dialog-test-description = A test run in the workspace: it begins at that start as if its event had happened — at once, beside any other run of it.
workflow-run-workflow-dialog-payload = What the event carries
workflow-run-workflow-dialog-payload-hint = A sample occurrence, as JSON — what the start's input mapping reads as {"{"}event.payload.…{"}"}. Edit it to try another.
workflow-run-workflow-dialog-payload-invalid = Not JSON: { $error }
workflow-run-workflow-dialog-from-event = { $n ->
    [one] 1 input is
   *[other] { $n } inputs are
  } read from the event.
workflow-run-workflow-dialog-test-run = Test run
workflow-run-workflow-dialog-test-started = The test run has started.
workflow-start-run-dialog-listen = Start listening with “{ $name }”
workflow-start-run-dialog-listen-2 = Start listening
workflow-start-run-dialog-listen-description = This workflow begins on events. The goal listens while it is open, and each occurrence starts a run of it with these inputs.
workflow-start-run-dialog-listening = The goal is listening.
workflow-start-run-dialog-adopt-listen = Adopt “{ $name }” and start listening?
workflow-start-run-dialog-adopt-listen-2 = Adopt and listen
workflow-start-run-dialog-adopt-listen-description = The Workflow Agent proposed this shape, and it begins on events. Adopting it records your decision; the goal then listens, and each occurrence starts a run of it with these inputs.
workflow-start-run-dialog-adopted-listening = Adopted — the goal is listening.
workflow-start-run-dialog-secrets-title = The goal listens with “{ $name }”
workflow-start-run-dialog-secrets = A hook that may be called from outside has its secret — shown once, here.
workflow-start-run-dialog-run-now = Run “{ $name }” now
workflow-start-run-dialog-run-now-2 = Run now
workflow-start-run-dialog-run-now-description = A run by hand, from the workflow's start by hand, with these inputs. The goal keeps listening.

## A hook's secret an adoption minted, shown once at the app's root (`HookSecretsDialog.tsx`).
workflow-hook-secrets-dialog-title = A hook's secret — copy it now

## An agent step's effort pin, beside its model pin (`forms/AgentStepForm.tsx`, `forms/agentStepModel.mjs`), and the problem of a pin no harness takes (`stepKinds.mjs`).
workflow-agent-step-form-effort = Effort
workflow-agent-step-model-effort-agent-decides = Inherit leaves it to the agent that runs the step.
workflow-agent-step-model-effort-auto = The Decision-Making Agent names the level for each task; the agent's own runs while it is off, unsure or does not answer.
workflow-step-kinds-pins-effort-none-harnesses-can-set = pins an effort none of its harnesses can set
workflow-step-kinds-spawn-gives-not-what-workflow-asks = gives its workflow an input it does not ask for, or leaves out one it requires
workflow-inspector-unknown-kind = This build does not know a “{ $kind }” step. It is kept as it is.
workflow-branch-name-branch = Branch
workflow-held-signals-held = { $n } held for you
workflow-held-signals-held-for-you = held for you to read
workflow-held-signals-let-through = Let it through
workflow-held-signals-let-through-done = Let through — its run begins as any other.
workflow-held-signals-list = Held for you to read
workflow-held-signals-note = A payload from outside that the content screen would not pass. Let it through and it begins its run; leave it and nothing happens.
workflow-held-signals-row = { $source }, heard by { $step }
workflow-held-signals-source-connector = an item its poll listed
workflow-held-signals-source-event = an event
workflow-held-signals-source-hook = a call to its hook
workflow-held-signals-source-signal = a signal raised from outside
workflow-held-signals-source-signal-named = the signal { $name }
workflow-judge-step-confidence-out-of-range = A confidence is a number from 0 to 1 — leave it blank for the workspace's own.
workflow-judge-step-needs-options = A judgement chooses between at least two options: add { $n } more.
workflow-judge-step-takes-otherwise = When it is not sure enough, or gives no answer that holds, the run takes { $otherwise }.

## A goal's design being edited — its problems (`views/_workflow/GoalWorkflowTab.tsx`) — words moved out of the markup.
workflow-goal-workflow-tab-problems-count = { $n } { $n ->
    [one] problem
   *[other] problems
  }

## A connector step naming what is not there (`views/_workflow/forms/ConnectorStepForm.tsx`) — words moved out of the markup.
workflow-connector-step-form-not-installed-here = { $connector } (not installed here)
workflow-connector-step-form-not-one-of-its-operations = { $operation } (not one of its operations)

## An account input naming a connector that is not there (`views/_workflow/forms/InputDefsEditor.tsx`) — words moved out of the markup.
workflow-input-defs-editor-not-installed-here = { $connector } (not installed here)

## A start event's key field (`views/_workflow/forms/StartStepForm.tsx`) — words moved out of the markup.
workflow-start-step-form-id = id

## A connector start's operation picker: a write the definition names, where only a read goes (`views/_workflow/forms/connectorStepModel.mjs`).
workflow-connector-step-form-writes-poll-only-reads = { $operation } (writes — a poll only reads)

## An input's kind in words, a refused id or name said where it was typed (`workflowForm.mjs`, `forms/InputDefsEditor.tsx`, `forms/StepCommonForm.tsx`).
workflow-workflow-form-kind-text = Text
workflow-workflow-form-kind-number = Number
workflow-workflow-form-kind-bool = Yes or no
workflow-workflow-form-kind-choice = Choice
workflow-workflow-form-kind-assignee = Assignee
workflow-workflow-form-kind-project = Project
workflow-workflow-form-kind-account = Connector account
workflow-input-defs-editor-another-input-has-name = Another input has this name.
workflow-workflow-form-kept = { $why } Kept “{ $kept }”.

## The inspector's Then — flows drawn from the keyboard — and its Flow and failure fold (`forms/ThenField.tsx`, `Inspector.tsx`).
workflow-then-field-then = Then
workflow-then-field-hint = The steps this one flows into — what a flow drawn from its bottom handle does.
workflow-then-field-branches-hint = Where each branch goes — what a flow drawn from the branch's handle does.
workflow-then-field-branch-goes-to = Where { $branch } goes
workflow-then-field-nowhere-yet = Nowhere yet
workflow-then-field-next-steps = Next steps
workflow-inspector-flow-and-failure = Flow and failure
workflow-designer-canvas = Workflow canvas

## A goal's drawing: saving and discarding it (`GoalWorkflowTab.tsx`).
workflow-goal-workflow-tab-checking-steps = Checking the steps…
workflow-goal-workflow-tab-discard-changes = Discard changes
workflow-goal-workflow-tab-discard-your-changes = Discard your changes to the steps?
workflow-goal-workflow-tab-discard-body = The steps go back to the saved workflow, and undo cannot bring the changes back.

## Stopping one run asks first (`StopRunDialog.tsx`), and a pane with no runs (`WorkflowRunsPane.tsx`).
workflow-stop-run-dialog-title = Stop this run?
workflow-stop-run-dialog-body = The run is cancelled and its sessions end. Restart begins a new run with the same inputs.
workflow-stop-run-dialog-confirm = Stop the run
workflow-runs-pane-no-runs-title = No runs yet

## A template being installed or opened (`TemplateCard.tsx`).
workflow-template-card-opening = Opening…
workflow-template-card-installing = Installing…
