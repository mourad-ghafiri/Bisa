### Bisa — the engine's own sentences: a step's summary, a guard's reason, a
### run's stop, the Workflow Agent's notes to a thread.

## A step in one line (`Step::summary`, crates/bisa-core/src/workflow.rs): what a person judges a proposal's step by.

step-summary-start-manual = begins by hand
step-summary-start-every = begins every { $secs } seconds
step-summary-start-cron = begins on the schedule `{ $cron }`
step-summary-start-schedule-unset = begins on a schedule (not set yet)
step-summary-start-hook = begins when called
step-summary-start-hook-public = begins when called from outside
step-summary-start-message = begins when a message arrives
step-summary-start-message-in = begins when a message arrives in { $scope }
step-summary-start-signal = begins when the signal `{ $name }` is raised
step-summary-start-project =
    begins on { $change ->
        [commit] a commit
        [push] a push or a fetch
        [pull_request] a pull request's change
        [merge] a merge
       *[files] a change of files
    } in { $project }
step-summary-start-project-unchosen = begins when a project (not chosen yet) changes
step-summary-start-run =
    begins when { $outcome ->
        [done] a run is done
        [failed] a run fails
        [cancelled] a run is cancelled
       *[any] a run ends
    }
step-summary-start-run-of =
    begins when a run of { $workflow } { $outcome ->
        [done] is done
        [failed] fails
        [cancelled] is cancelled
       *[any] ends
    }
step-summary-start-platform = begins on the platform event `{ $topic }`
step-summary-start-connector = begins on each new item from { $call }
step-summary-start-connector-unchosen = begins on each new item from an outside platform (not chosen yet)
step-summary-start-check =
    begins when `{ $command }` { $fire_on ->
        [starts_failing] starts failing
        [failing] fails
        [passing] passes
       *[always] runs
    }
step-summary-parallel =
    { $n ->
        [1] takes its one path
       *[other] takes { $n } paths at once
    }
step-summary-emit = raises the signal `{ $signal }`
step-summary-agent = { $what }
step-summary-agent-assigned = { $what } — { $who }
step-summary-human = { $what }
step-summary-human-options = { $what } ({ $options })
step-summary-approval = approve: { $what }
step-summary-check-command = passes when `{ $command }` exits 0
step-summary-check-schema = checks the output of { $of } against a schema
step-summary-check-schema-unchosen = checks the output of a step (not chosen yet) against a schema
step-summary-decide =
    { $n ->
        [0] branches: { $otherwise } (otherwise)
       *[other] branches: { $branches } · { $otherwise } (otherwise)
    }
step-summary-if = if { $when }: { $yes } · { $no }
step-summary-switch =
    { $n ->
        [0] switches on { $on }: { $otherwise } (otherwise)
       *[other] switches on { $on }: { $values } · { $otherwise } (otherwise)
    }
step-summary-decide-every =
    { $n ->
        [0] every rule that holds: { $otherwise } (otherwise)
       *[other] every rule that holds: { $branches } · { $otherwise } (otherwise)
    }
step-summary-judge =
    { $n ->
        [0] judges { $what }: { $otherwise } (otherwise)
       *[other] judges { $what }: { $branches } · { $otherwise } (otherwise)
    }
step-summary-for-each = for each of { $items }, at most { $max } times
step-summary-while = while { $when }, at most { $max } times
step-summary-connector = calls { $call }
step-summary-connector-params = calls { $call } with { $params }
step-summary-connector-operation-unchosen = calls { $connector } (operation not chosen yet)
step-summary-connector-operation-unchosen-params = calls { $connector } (operation not chosen yet) with { $params }
step-summary-connector-unchosen = calls a connector (not chosen yet)
step-summary-connector-unchosen-params = calls a connector (not chosen yet) with { $params }
step-summary-wait-delay = waits { $secs } seconds
step-summary-wait-time = waits until { $at }
step-summary-wait-schedule = waits for the schedule `{ $cron }`
step-summary-wait-signal = waits for the signal `{ $name }`
step-summary-wait-message = waits for a message
step-summary-wait-message-in = waits for a message in { $scope }
step-summary-wait-project =
    waits for { $change ->
        [commit] a commit
        [push] a push or a fetch
        [pull_request] a pull request's change
        [merge] a merge
       *[files] a change of files
    } in { $project }
step-summary-wait-project-unchosen = waits for a project (not chosen yet) to change
step-summary-wait-run =
    waits until { $outcome ->
        [done] a run is done
        [failed] a run fails
        [cancelled] a run is cancelled
       *[any] a run ends
    }
step-summary-wait-run-of =
    waits until a run of { $workflow } { $outcome ->
        [done] is done
        [failed] fails
        [cancelled] is cancelled
       *[any] ends
    }
step-summary-wait-platform = waits for the platform event `{ $topic }`
step-summary-wait-release = waits for a person to release it
step-summary-notify = posts: { $what }
step-summary-notify-scope = posts to { $scope }: { $what }
step-summary-notify-as = posts: { $what } — as { $who }
step-summary-notify-scope-as = posts to { $scope }: { $what } — as { $who }
step-summary-spawn = spawns a child goal: { $what }
step-summary-spawn-no-wait = spawns a child goal: { $what } (does not wait for it)
step-summary-end-path = ends this path
step-summary-end-done = finishes the run
step-summary-end-failed = fails the run

## What the Workflow Agent says in a goal's thread (crates/bisa-engine/src/guided.rs) — a note the platform authors, carried on the post as `said`.

guided-say-design-off = Designing is off on this node, so nobody will design this goal's workflow. Design it on the Workflow tab, or pick one from the library or a template.
guided-say-cycle-failed = I could not finish this cycle. Ask for the design again.

## What the Workflow Agent says in a goal's thread when a proposal lands or a cycle ends.
## `$steps` is the proposal's own words — its description and numbered steps — as the card shows them.
guided-say-proposed-design-gated = I proposed **{ $name }**. Adopt it in *Your move*, ask me for changes, or edit it on the Workflow tab.{"\u000A"}{ $steps }
guided-say-proposed-design-gated-why = I proposed **{ $name }**, and it needs you: { $why }. Adopt it in *Your move*, or edit it first.{"\u000A"}{ $steps }
guided-say-proposed-design-adopted = I designed **{ $name }** and started it.{"\u000A"}{ $steps }
guided-say-proposed-design-listening = I designed **{ $name }** and it is listening: its first run starts when one of its events happens.{"\u000A"}{ $steps }
guided-say-proposed-design-drafted = I drafted **{ $name }** on the Workflow tab — edit it and start it when it reads right.{"\u000A"}{ $steps }
guided-say-proposed-repair-gated = I proposed a corrected workflow, **{ $name }**. Approve it in *Your move* to continue.{"\u000A"}{ $steps }
guided-say-proposed-repair-gated-why = I proposed a corrected workflow, **{ $name }**, and it needs you: { $why }.{"\u000A"}{ $steps }
guided-say-proposed-repair-adopted = I corrected the workflow, **{ $name }**, and it is running again.{"\u000A"}{ $steps }
guided-say-proposed-repair-listening = I corrected the workflow, **{ $name }**, and it is listening again.{"\u000A"}{ $steps }
guided-say-proposed-repair-drafted = I drafted a corrected workflow, **{ $name }**, on the Workflow tab.{"\u000A"}{ $steps }
guided-say-stalled = I couldn't finish { $phase ->
    [repair] repairing this goal's workflow
   *[design] designing this goal's workflow
  } ({ $detail }). Retry the design, or pick a workflow by hand.
guided-say-failed = I couldn't start { $phase ->
    [repair] repairing this goal's workflow
   *[design] designing this goal's workflow
  }: { $detail } Retry once it's fixed, or pick a workflow by hand.

## What an agent's turn says of itself in a conversation (crates/bisa-engine/src/conversation.rs) — a note the platform authors, carried on the post as `said`.

engine-conversation-reply-cut = The reply went on past what a conversation keeps of one turn ({ $kept }), and the rest was not kept.
