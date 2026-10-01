### Bisa — the desktop: goals (`desktop/src/views/_goals`).

goals-goal-act-popover-decision-waiting-needs-goal-s-own = A decision is waiting — it needs the goal's own surface.
goals-goal-act-popover-nothing-here-acts-one-control = Nothing here acts in one control.
goals-goal-act-popover-open-workflow-tab = Open the Workflow tab
goals-goal-card-act = Act
goals-goal-card-act-2 = Act on { $label }
goals-goal-card-delete = Delete…
goals-goal-card-designing = designing
goals-goal-card-goal = Goal { $row }
goals-goal-card-goal-names-run-node-cannot-read = The goal names a run this node cannot read; it is drawn without one. The diagnostic log names the file.
goals-goal-card-menu = Menu for { $label }
goals-goal-card-new-run = New run…
goals-goal-card-open = Open
goals-goal-card-queued = queued { $n }
goals-goal-card-restart = Restart
goals-goal-card-run-unreadable = run unreadable
goals-goal-card-status-closed = closed
goals-goal-card-status-done = done
goals-goal-card-status-draft = draft
goals-goal-card-status-failed = failed
goals-goal-card-status-running = running
goals-goal-card-status-waiting = waiting
goals-goal-card-stop = Stop
goals-goal-card-workflow = Workflow: { $workflow }
goals-goal-card-workflow-agent-designing-goal-s-workflow = The Workflow Agent is designing this goal's workflow
goals-goal-strip-agent-step = agent step
goals-goal-strip-agents-working = agents working
goals-goal-strip-being-designed = being designed
goals-goal-strip-child-goal = child goal
goals-goal-strip-connector-call = connector call
goals-goal-strip-kind-approval = approval
goals-goal-strip-kind-check = check
goals-goal-strip-kind-condition = condition
goals-goal-strip-kind-decision = decision
goals-goal-strip-kind-end = end
goals-goal-strip-kind-judgement = judgement
goals-goal-strip-kind-loop = loop
goals-goal-strip-kind-notification = notification
goals-goal-strip-kind-question = question
goals-goal-strip-kind-switch = switch
goals-goal-strip-kind-wait = wait
goals-goal-strip-loop-over-items = loop over items
goals-goal-strip-move = your move
goals-goal-strip-waiting-world = waiting on the world
goals-goals-filters-all = All
goals-goals-filters-archived = Archived
goals-goals-filters-every-workflow = Every workflow
goals-goals-filters-filter = Filter…
goals-goals-filters-filter-goals = Filter goals
goals-goals-filters-who-holds = Who holds it
goals-goals-filters-workflow = Workflow
goals-step-chip-strip-run = { $workflow_name } run

## By hand — a sentence with a plural, a slot or several pieces; the generator keeps this section.
goals-goal-card-more-step-steps = { $hidden } more { $hidden ->
    [one] step
   *[other] steps
  }
goals-goal-card-run-runs-waits-wait-behind-live = { $queued } { $queued ->
    [one] run
   *[other] runs
  } { $queued ->
    [one] waits
   *[other] wait
  } behind the live one
goals-goal-card-thing-things-waits-wait = { $owed } { $owed ->
    [one] thing
   *[other] things
  } { $owed ->
    [one] waits
   *[other] wait
  } on you
goals-goal-strip-failed = Failed at { $step }{ $flag ->
    [yes] {" "}— { $error }
   *[no] {""}
  }
goals-goal-card-assigned = Assigned: { $people }

## Events and gateways — a listening goal's chip, the new kinds' nouns (`goalCardModel.mjs`, `goalStripModel.mjs`).
goals-goal-card-listening = listening
goals-goal-card-paused = paused
goals-goal-strip-start-event = start event
goals-goal-strip-signal-raised = signal raised
goals-goal-strip-parallel-paths = parallel paths

## A run's chip strip with no workflow name (`views/_goals/StepChipStrip.tsx`) — words moved out of the markup.
goals-step-chip-strip-run-unnamed = run
