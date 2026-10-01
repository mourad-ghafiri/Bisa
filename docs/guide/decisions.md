# The Decision-Making Agent

The **Decision-Making Agent** is the platform's third core agent, beside the General Agent and
the Workflow Agent, and the one that judges: it is asked a typed question — is it so, which one, how
much — and answers with a calibrated number instead of a guess written into a rule. It holds no
conversation, does no work, and signs nothing; you never message it. Off by default. Where it is architecture is [15 — The Decision-Making Agent](../architecture/15-decision-making-agent.md).

## What it does for you

Eleven places in the platform ask a rule of their own when nobody says otherwise — which model leads a
launch, how hard it works, who of a pool takes a work item, whether a tool call is harmful, which agent answers an
unaddressed message, whether an auto goal's design reaches the goal, whether a browser tab an agent
opens needs you at it. Switch the Decision-Making Agent on and each of those points asks it first, sure
enough to act on, and falls back to its own rule the moment it is not sure or does not answer. You
never lose the rule — you gain a better first guess.

## Switching it on

**Globally.** Settings' `decisions.enabled` — machine, workspace or project — turns on every point
that has not been switched off in `decisions.points_off`.

**For one agent.** An agent's edit form (or `bisa agent edit <id> --decision-making true`) has *Let
the Decision-Making Agent decide for this agent* — on the General Agent and the Workflow Agent, the
third thing you may change besides the harness and the model plan. On, it counts toward the pick of any pool that agent
sits in, and toward whether an unaddressed message's triage asks the Decision-Making Agent.

**For one workflow.** A workflow's own switch turns the Decision-Making Agent on for every decision point a
run of it reaches — the pick of a pool a step's work goes to, whether an auto goal adopts the design
alone.

**By choosing it explicitly.** Four places name the Decision-Making Agent directly, and naming one *is*
switching it on there, whatever the switches above say:

- an agent's model plan set to the **`auto_route`** strategy;
- an effort set to **`auto`** — on an agent's plan, on one of its models, on a workflow's agent step,
  or in the `agents.effort` setting;
- the security classifier's **`decision_making_agent`** provider (Settings › Security › Classifier);
- a workflow's **`judge`** step.

## Choosing who decides

`decisions.provider` picks who answers:

| Provider | Best for | What it needs |
|---|---|---|
| `harness` (default) | trying the Decision-Making Agent with nothing to set up | a harness already installed on this machine — Claude Code with `claude-sonnet-5-5[1m]` at effort `high` out of the box (`decisions.harness.model`, `decisions.harness.effort`) |
| `agent` | a decision that benefits from an agent's own skills or context | any enabled agent, asked with no tools |
| `jev` | a calibrated answer whose confidence means what it says | an API key for TypeSafe AI's Jev |
| `rlcd` | any other model trained the same way, self-hosted or a partner's | an endpoint and a model name, and a key unless the endpoint takes none |

`harness` and `agent` ask a generative model held to the same shape; its probabilities are its own
estimate, not a calibrated model's — the status line says so (`calibrated: false`). `jev` and `rlcd`
are trained for calibrated decisions: a `confidence` of `0.9` from one of them is right about nine
times in ten.

A remote provider's key is never a setting — it lives only in this machine's own keystore, and is
never read back. In Settings › Decision Settings › Decision Making the key box hides what you type, the eye shows it, and the
box keeps it while the window is open; after a restart it shows *A key is stored* and `••••••` —
type to replace. The node never sends a key back:

```sh
bisa decisions key set jev --from @stdin
bisa decisions key set rlcd --from @path/to/key.txt
bisa decisions key clear jev
```

## Trying it

`POST /decisions/try` (`bisa decisions try`) puts one question to the provider as it is set up right
now — nothing is decided and nothing is recorded, so it is safe to try before you switch anything on:

```sh
bisa decisions try --state "Help! My payouts have been failing for 3 days." \
  --noul "Does this convey urgency?"

bisa decisions try --state '{"task":"rename a variable"}' \
  --choice "Which model suits the task?" \
  --option small="quick edits" --option large="design work"
```

## Reading the judgements

`bisa decisions status` says who answers, whether it can be asked right now, whether a key is
stored, and which of the eleven points the switches reach:

```
Decision-Making Agent · on for the workspace · answers as jev-latest (jev)
  can be asked
  API key: stored
  acts from 0.70 · calls a command safe from 0.90 · within 20s
  model.route        where it is selected
  model.effort       where it is selected
  security.tool      off
  assign.pick        on
  …
```

`bisa decisions list` (or `GET /decisions`) is every judgement this node has asked for, newest
first: the decision point, who answered, whether it was applied, unsure, or failed, and why.

## When it is unsure or down

An answer below the confidence bar (`decisions.confidence.act`, or `decisions.confidence.security` at
a security point) is **recorded and not acted on** — the point runs its own rule, the same as if the
Decision-Making Agent had never been asked. A provider that cannot be reached, has no key stored, or answers
past its deadline (`decisions.deadline_secs`) fails the same way. At the three security points —
whether a tool call, a message or content read from outside is harmful — nothing but a *sure* safe verdict is read as safe:
unsure, failed, or off all put the call to you, exactly as the classifier already does without it.

## Routing a model plan automatically

Set an agent's model strategy to `auto_route` and give each model a sentence for what it is the right
one for:

```sh
bisa agent edit my-agent --strategy auto-route \
  --model "claude-haiku-4-5: quick edits, typo fixes, small renames" \
  --model "claude-opus-5-5[1m]: design work, architecture, anything ambiguous"
```

Each launch, the Decision-Making Agent reads the task and picks the model whose sentence fits, preferring
the least capable model that will do the job well; a hard `model` pin on a step still wins outright,
and with no sure pick the plan's own order stands.

## Choosing the effort automatically

Set an effort to `auto` and the Decision-Making Agent names how hard the model works, task by task
([Agents and teams — Effort](agents-and-teams.md#effort)):

```sh
bisa agent edit my-agent --effort auto                        # the agent, whichever model runs
bisa agent edit my-agent --model "claude-opus-5-5[1m]@auto"   # one model of its plan
bisa settings set workspace agents.effort auto                # everyone who says nothing
```

Each launch it reads the task, the agent's name and description and the model about to run, and
picks one level among those that model takes, preferring the lowest that will do the job well. It
is asked once for a launch: when a model hits its wall and the next one takes over, the same level
goes with it, fitted to what that model takes. A model that takes one level, or none, is no
question and nobody is asked.

With no sure pick — the Decision-Making Agent off, unsure, or not answering — the next level down
the chain runs: the model's `auto` falls to the agent's level, the agent's to the setting's, and
`high` stands when nobody named one. A level somebody named is never put to the judge: a step
that says `effort: max` runs at `max`.

The Decision-Making Agent's own session is never asked how hard to work: it runs at
`decisions.harness.effort`, and the security classifier at `security.classifier.effort`.

## The `judge` step

A `judge` step reads a rendered `state` and asks one `choice` question with `instructions`, offering
a branch for each option and `otherwise` for anything else. Say a support-triage workflow wants to
route a captured ticket:

```
state:        {steps.summarize.output.report}
instructions: Is this ticket urgent enough to page someone right now?
options:
  page      — an outage, data loss, or a payment failure affecting customers now
  business_hours — a real bug with no immediate harm
otherwise:    a_person_looks
min_confidence: 0.8
```

The step's output is `{choice, confidence, judged}`; the flow labelled `page` or `business_hours` is
taken, and an unsure or failed judgement takes `otherwise` — the step never fails for want of an
answer. `otherwise` is a branch of its own, with a flow of its own: a judge that names one of its
options there is refused when you save (`duplicate_branch`), since a run could not say afterwards
whether the option was picked or nobody was sure. To send both the same way, draw both flows to the
same step. Naming a `judge` step switches the Decision-Making Agent on for it, whatever your workspace or
workflow switch says elsewhere.

## The security classifier

Set `security.classifier.provider` to `decision_making_agent` in Settings › Security's Classifier panel (or
`security.classifier.harness` / `security.classifier.model` when you want it on a harness rather than
an agent) and the classifier asks the Decision-Making Agent which of a named set of harms, if any, a tool
call or an outside message carries, instead of a generative model's one line. **It still never allows
what the rules did not**: a rule's `deny` still refuses, an `ask` still asks, and the classifier's own
verdict — from any provider — only ever turns a call the rules left undecided into an allow, a hold,
or a question for you.

## A workspace written before it had this name

The Decision-Making Agent was renamed before 0.1.0, the first public release, and nothing was
migrated. A development workspace written while it went by its earlier name reads as follows:

| What was stored | What happens |
|---|---|
| a workflow, or a run, with its own switch on | refused by name when it is read: the file names a field the platform no longer knows |
| an agent with its own switch on | opens with the switch off — switch it on again in its edit form |
| the security classifier set to it by its earlier word | reads as the default, the classifier agent — set it again in Settings › Security › Classifier |
| the judgements already recorded | read back unchanged |

A development workspace is started again with `scripts/reset-dev-workspace`.
