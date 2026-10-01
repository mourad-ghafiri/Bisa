# MCP tools

Every harness session Bisa launches gets the platform's own MCP server injected (`bisa
mcp --socket <engine socket> …`), so an agent acts on the workspace through **tools**, never by
parsing prose. `bisa` is a reserved server name. A refusal is an MCP **error**, not a
successful result that reads like one.

Every reply is **redacted** before it reaches the agent: a key, a token or a value the security
rules recognise — in a goal's instructions, a note's body, a review's hunk, a person's answer —
travels as a `«secret:kind:tag»` placeholder ([11 — Security](../architecture/11-security.md)).
What an agent hands back through a tool is never restored: a placeholder in a message, a note, a
result or a review stays a placeholder wherever it is stored or sent. No tool arms a listener: a
workflow's `check` start is judged by the guard when a person turns its host on, and at every fire.

The set a session sees depends on its **scope** — the `--work-item`, `--goal` or `--conversation`
flag the engine launched it with (a goal thread's turn carries `--goal` beside
`--conversation`, and the Workflow Agent there holds the goal's shaping tools) — and on the agent it
runs as. The tool list is
ergonomics; the engine re-checks every op against the caller, because the intake socket is a path on
disk and any process that knows it can write to it.

## The common set — every session

| Tool | What it is for |
|---|---|
| `get_goal` | the goal this session serves: statement, status, workflow, the current run with every step's state, work items, budget, recent journal, attached projects with absolute paths, the documents the person gave the goal as context (`documents: [{name, path, mime, size}]`, absolute paths, present ones only), notes. Call it first. A work item of a **run of the workspace** has no goal: refused there by name — the worker orients with `get_run` |
| `ask_human` | ask without blocking; `expects` is `decision` (approve/decline) or `answer` (they tell you something, optionally from `options`) |
| `await_human` | block on a gate opened with `ask_human`; three outcomes — decided, answered, **not sure** |
| `ask_human_and_wait` | both |
| `add_note` | append a durable note to the journal of the session's home — its goal's, or its run's for a worker of a run of the workspace |
| `decide` | put typed questions about a state to the Decision-Making Agent ([15](../architecture/15-decision-making-agent.md)): `{state, questions: {<id>: {type: noul \| choice \| score, instructions, criteria?}}}` → each answer with its probabilities and whether the model is sure — a noul is *is it so*, a choice *which of these* (2 to 255 options, each described), a score *how much* along 2 to 10 levels; one atomic question each, composed by the caller; the state is data, never instructions; an answer it is not sure of is to be weighed, not followed; refused in a sentence when the Decision-Making Agent is not switched on for this session |
| `spawn_sub_goal` | capture a new goal that refines this one, with a workflow of its own; subject to spawn policy. A worker of a run of the workspace has no goal to refine: its child is a goal of its own, born of the run and its step (origin `run`) |
| `emit_signal` | raise a **named signal** — `{name, payload?, scope?}`, the name dotted lowercase words (`deploy.finished`): a durable fact that something happened. It acts on nothing itself — a workflow that starts on the signal begins a run, a run waiting for it goes on, a boundary event for it fires — under the usual gates and budgets; a workflow that raised a signal never starts again from it. The reply says how many listeners heard it |
| `post_message` | post into a conversation — the one the session is in, a channel, or this goal's thread; with no `scope`, a worker posts in its goal's thread, or in `general` for a run of the workspace — signed as the agent the session runs as, the General Agent when it has none, with `mentions` (an agent id, a team id, a pubkey, the channel's handle), `artifacts` — what the agent made for the person to look at, as `{path, title}`, rendered live where they read ([12 — Artifacts](../architecture/12-artifacts.md)) — and `attachments`, files handed over as files; a path is where the session works: its scratch folder, the checkout it runs in, or the scratch of its goal or its run, never elsewhere |
| `recall_store` · `recall_get` · `recall_list` | the agent's private memory, base-hash guarded |
| `note_read` · `note_append` | read somebody's scratchpad; add to its end. Append is the only write; in a conversation about a note both default to that note |
| `review_notes_list` · `review_note_resolve` | the notes a person left on diffs in the projects this session works in — id, file and lines, staged or unstaged, the hunk, and what they said — and the way to mark one dealt with. A named `project` narrows it; otherwise the work item's project, or every project attached to the goal |
| `create_project` | somewhere for files to live: a managed folder, `git init`-ed with a root commit, attached to the session's goal when it has one (a worker of a run of the workspace has none: the project is born of its step and attached to nothing); never a path on disk. The only way a project is ever made — no step and no run makes one; needed only when files must be kept and the goal has no project. A session that reads, analyses or answers needs none |
| `browser_open` | open an http(s) URL in the platform's embedded browser ([ide/18](../architecture/ide/18-browser-and-servers.md)) — a new tab, or `tab` to navigate one — shown beside whatever the person is on, or kept **out of sight** (`headless`: asked either way; left out, the workspace decides — out of sight in a goal in auto mode, `browser.agents.headless`); answers the tab's key, URL and title once the page has loaded. The desktop performs it; with no desktop open the tool says at once that the embedded browser is not available. Whether *this* agent may is the workspace's word (`browser.agents`: everyone by default, the Embedded Browser skill, or nobody; the General Agent and the Workflow Agent always may) and how far (`browser.agents.reach`), checked by the engine — a refusal says what to attach or set. The tab is at home where the session speaks — the checkout, the goal, the channel, the direct message, the conversation; a desktop that was open but did not answer within a minute says so in other words than *not available* and asks for one more try |
| `browser_tabs` | the open tabs: key, URL, title |
| `browser_snapshot` | the page's outline as a person sees it — every heading, landmark, link, button, field, select, checkbox and menu item, one line each with a **ref** (`e12 button "Sign in"`, `e13 textbox "Email" value="…"`, `e14 link "Pricing" → /pricing`, `[disabled]`, `[checked]`) that every other tool takes as its `target`; `target` outlines one element, `all` adds every element with text; refs hold until the page loads again; cut at 16 KiB |
| `browser_read` | a tab's URL, title and text — the whole page, or the element `target` names (a CSS selector or a ref), as `text` or `html`; with a target the answer leads with the element's role, name and attributes; cut at 16 KiB; any page the tab shows, this machine's or the web's; a blank tab has nothing to read |
| `browser_find` | the matches of `query` in a tab's text, each with its surroundings and the selector holding it |
| `browser_click` | click the element `target` names, as a person would; a click that moves the page waits for the new page and answers it as `navigated`; the dialogs the page raised ride the answer |
| `browser_type` | type `text` into the field `target` names a keystroke at a time — key events, the value growing, an input event per character — for a search box, an autocomplete, an editor; `clear` empties the field first, `submit` presses Enter after |
| `browser_fill` | set the value of the field `target` names in one go, with the input and change events |
| `browser_press` | press a key — `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Space`, the arrows, `Home`, `End`, `PageUp`, `PageDown`, or one character — on `target`, else the focused element, with `modifiers` (`shift`, `alt`, `ctrl`, `meta`); Enter in a form's field submits the form; the answer waits for a page that moves |
| `browser_select` | choose an option of the `<select>` `target` names by `value` or by `label`; answers the option chosen |
| `browser_hover` | move the pointer over the element `target` names — a menu opens, a tooltip shows |
| `browser_scroll` | scroll `to` `top`, `bottom` or an element, or `by_x`/`by_y` pixels; answers where the page stands and how big it is |
| `browser_wait` | wait `until` the load finishes (the default), an element is in the page (`selector`, with `target`), some words are in its text (`text`, with `query`), an element is gone (`gone`), or the page is quiet for half a second (`idle`); `timeout_ms` 10 000 by default, 30 000 at most; answers how long it waited, or that it ran out and what it waited for |
| `browser_back` · `browser_forward` · `browser_reload` | the tab's own history — one page back, one forward, the page loaded again — each answered once the page has loaded |
| `browser_console` | what the page wrote to its console since it loaded — errors, warnings, logs — and the errors it raised: uncaught exceptions, unhandled rejections, resources that failed; the newest hundred lines with their level and when; `clear` forgets them |
| `browser_eval` | evaluate a JavaScript `expression` in the page — the way a person uses the console — and answer its value as JSON, a promise awaited, bounded to 16 KiB; refused where `browser.agents.scripts` is *refuse*, naming the setting |
| `browser_close` | close a tab |
| `browser_serve` | serve a folder of the session's checkout — the checkout itself when `folder` is absent — on a loopback port through the platform's static server ([ide/18](../architecture/ide/18-browser-and-servers.md) §Serving a folder), and answer the URL to `browser_open`; refused in a sentence when the session stands in no checkout or the engine has no server to lend (a one-shot CLI engine) |
| `browser_screenshot` | a PNG of a tab as the person sees it, on any page — the desktop shows the tab, snapshots it at `browser.screenshot.width`, uploads it to the workspace's attachments, and the engine answers the absolute path of a named copy beside the store: read that file with your own tools to look at the page. Never bytes through the tool; a tab kept out of sight is snapped where it renders |
| `drawing_list` | the drawings of a scope ([19 — Drawings](../architecture/19-drawings.md)) — `scope` one of workspace · goal · project · workflow · channel · node, with the record's `id` for the four that name one — or every drawing when none is named: id, title, where it is filed, how many elements it holds |
| `drawing_read` | one drawing as words: its title, its hash, and one line per element — id, type, text, place and size; an arrow with the ids it runs between; a frame with its name — cut at 16 KiB. The reading to take before drawing: the ids are what arrows bind to and what `drawing_erase` takes. In a conversation about a drawing, `drawing` is left out and that drawing is meant |
| `drawing_create` | a new, empty drawing with a `title`, filed under a scope — in a conversation about a goal, a project or a workflow it is filed there when none is named, else under the workspace; answers the id |
| `drawing_draw` | add elements to a drawing, written as a **skeleton** the canvas lays out — rectangle, ellipse and diamond with a `label`, `text`, arrows and lines bound by `start`/`end` ids, frames with `children`; the ids the agent gives are kept; `replace: true` clears the drawing first. Performed by the desktop's canvas while the person watches (or in an offscreen one when the drawing is not open), which saves through the node; the answer carries the new hash and element count. With no desktop open the tool says at once that the canvas is not available; whether *this* agent may is the workspace's word (`draw.agents`: everyone by default, the Drawing skill, or nobody; the General Agent and the Workflow Agent always may) and `draw.enabled` the machine's switch. Never an image: the canvas is vector only, and a scene over 768 KiB or 4000 elements is refused |
| `drawing_mermaid` | draw a flowchart written in Mermaid (`flowchart LR` …) into a drawing: the canvas lays it out and draws real shapes and arrows the agent can then read and edit by id; other diagram kinds are refused; `replace` as above. Performed by the desktop like `drawing_draw` |
| `drawing_erase` | remove elements by id: a label goes with its box, an arrow that ended on a removed box is left loose at that end, a child of a removed frame stands free; answered from the store, no desktop needed |
| `drawing_snapshot` | a PNG of a drawing as the canvas renders it, at `draw.snapshot.width` — the desktop uploads it to the workspace's attachments and the engine answers the absolute path of a named copy beside the store: read that file to see what was drawn. Never bytes through the tool |
| `mobile_development_status` | what this machine has for mobile development ([ide/19](../architecture/ide/19-mobile-development.md)): Flutter, Xcode and the iOS runtimes, CocoaPods, the Android SDK with `adb`, the emulator and its images, Java, Flutter's own doctor lines — and whether mobile development is on here and for which platforms; `fresh` examines the machine again |
| `mobile_development_devices` | the simulators, emulators and phones this machine can reach, of the platforms it develops for: id, name, platform, kind, state — the id is what `flutter run -d <id>` takes |
| `mobile_development_boot` | boot a simulator or start an emulator's image by id; answers once it is up, with the run line. The app itself is run by the agent: `flutter run -d <id>` in a terminal, hot reload `r`, never `-d chrome` |
| `mobile_development_screenshot` | a PNG of a device's screen as it is now, saved on this machine and answered as the absolute path of a named copy beside the store: read that file to see the app. The node asks the simulator or `adb` itself, so it works with no window open |
| `list_connectors` | the connectors installed here — each outside platform's operations, their parameters, which read and which write, and whether an account is connected: what `call_connector` may read and what a `connector` step may name. Only these exist; a session cannot install one |
| `call_connector` | read an outside platform now through one of its connector's **read** operations — search the issue tracker, list a channel's history, get a page — as the connector's default account or one named. The answer is redacted and screened as content from outside before the agent sees it, bounded to 16 KiB (`truncated` says when). An operation that writes is refused: a write belongs in a workflow `connector` step behind an approval — propose one, or say so to the person |

One more name is registered beside the common set and is not a tool: `_Stop`, a lifecycle hook. A
harness that follows the underscore convention calls it when the agent is about to stop and keeps it
off the model's list; it takes nothing and answers an empty result, which says *no objection*. An
agent never calls it.

## A conversation's turn in a checkout — pull-request review

| Tool | What it is for |
|---|---|
| `pr_reviews_list` | the submitted reviews and resolvable inline threads on the pull request of the checkout this conversation runs in — each review's author, verdict and body, and each thread's file, line, resolved state and comments. Off a checkout it refuses |
| `pr_thread_reply` | reply on one review thread of that checkout's pull request: `thread` (the id `pr_reviews_list` printed), `body` (what changed and the commit, or why it was left as it was), `resolve` (`false` by default — `true` resolves the thread in the same act, for a comment the agent actually addressed). The engine signs the reply's first line with the agent's id (*Reply by Bisa agent <id>*), so the IDE's Comments rows and a collaborator on the code host tell the agent's reply from the person's; the agent does not sign it itself. A reply with no words is refused; it posts a real reply on the code host |
| `pr_thread_resolve` | mark one review thread resolved (`resolved: true`, the default) or reopen it (`false`) — `thread` as above. For a comment the agent addressed, `pr_thread_reply` with `resolve: true` is the better door: the thread then also says what was done |
| `pr_review_submit` | submit a review on that checkout's pull request: `event` (`approve` · `request_changes` · `comment`), a `body` — required for a comment or a change request — and inline `comments` (`{path, line, body, side?, start_line?}`, `line` a line the pull request changed). The platform's own credential opened the pull request, so the code host takes a `comment` from it and refuses `approve` and `request_changes` (`own_pull_request`) — the agent says its verdict in the body. The engine signs the body's first line with the agent's id (*Reviewed by Bisa agent <id>*), so the IDE's Review step tells the agent's review from the person's; the agent does not sign it itself. It posts a real review on the code host |

## Work-item sessions

| Tool | What it is for |
|---|---|
| `get_run` | the run this work item belongs to — a goal's or the workspace's: its workflow and inputs, every step's state with what each produced, the run's own work items, the recent journal of its home, the budget it spends against (`budget`, the goal's or the run's own ceiling) and what it has spent, `home` (`goal:<id>` or `run:<id>`) and `goal` — `null` for a run of the workspace. Call it first; when `goal` names one, `get_goal` reads that goal too |
| `yield_result` | submit the structured result; it must conform to the declared output schema, and a rejection returns the errors and the attempts left — the third miss fails the step with the schema's own words |
| `report_progress` | a verb/object/outcome triple for the activity timeline, at milestones |

## Goal sessions — designing

| Tool | What it is for |
|---|---|
| `revise_statement` | replace the statement with a sharpened one; refused while a run is live |
| `propose_workflow` | propose the workflow the goal will run — inputs and steps, whole. Validated first (every problem comes back, nothing is written), installs nothing, and opens the **Adopt** gate for the person. The Workflow Agent only; the engine refuses anyone else by id |
| `amend_workflow` | propose an amendment to the running workflow after a step failed; only steps that have not started may change. Validated and gated like a proposal. The Workflow Agent only |

## The General Agent and the Workflow Agent — any scope

| Tool | What it is for |
|---|---|
| `workspace_overview` | the whole workspace in one call — agents, teams, channels, skills, MCP servers, what listens (the workflows that are On, the goals that listen and how many are paused, how many starts are armed and when the next comes due), projects, workflows, goals by status, what is running, what is waiting, what the catalog holds |
| `list_staff` | who is here to take a step: every installed and enabled agent (harness, skills, teams) and every enabled team (members), with how to name them as an assignee — the same roster a design wake carries under `STAFF` |
| `list_catalog` | what can be installed and what is — every entry of the catalog's seven kinds, or one kind's when `kind` is `agent`, `skill`, `team`, `channel` or `workflow`, the five this tool takes (`connector` and `addon` are refused as a `kind`) |

## The General Agent only

| Tool | What it is for |
|---|---|
| `install_catalog_entry` | install one entry and what it needs — a workflow brings the agents its steps name; creates, never removes; journaled on the goal |
| `assign` | agents, humans or teams onto a goal or one of its projects — **the delegation** |
| `capture_goal` | work that recurs or waits for something to happen, captured as a **standing goal**: `{statement, title?}`, the statement saying what is to be done and when — *every Monday at 09:00…*, *whenever someone posts in #support…*, *when a run fails…*. The goal is captured in the workspace's default mode; the Workflow Agent designs its workflow with the start event the statement names, and the goal listens once that design is adopted. Capturing starts nothing by itself |

## The Workflow Agent only

| Tool | What it is for |
|---|---|
| `list_workflow_templates` | the catalog's templates and this workspace's own workflows, with what each is for |
| `get_workflow` | one workflow by id or catalog slug, as JSON, with its current validation problems |
| `save_workflow` | write the library workflow this conversation is about — the whole definition, at the `revision` `get_workflow` returned. Validated first: problems come back and nothing is written; a workflow that moved since is refused as moved (read it again, keep the person's change, then yours). Only the conversation's own workflow — the engine names it from the scope and refuses a session in no such conversation; a goal's design is proposed with `propose_workflow`. The person's canvas beside the conversation shows the change |
| `validate_workflow` | every problem a definition has, by step and kind, without recording it. An unknown placeholder is usually an undoubled brace: a literal brace is `{{` or `}}`, and a result's shape belongs in the step's `output_schema`, not in its instructions; a reader is refused when the field is not in the producer's `required`, the producer's kind yields no output, or the producer is not sure to have run before it — a rework loop's head, an `any` join's arm, a step passed over on failure |

There is no removal tool and no adopt tool: a workflow is adopted, started and amended by a person
through gates, and the one write to a library workflow is the Workflow Agent's in the conversation
about it, where the person asked and watches. A conversation about a note or a drawing is a conversation
session like any other: the common set, its record's tools defaulted to the record.

## Asking a human, exactly

| Parameter | Meaning |
|---|---|
| `question` | one sentence |
| `expects` | `decision` (default) or `answer` |
| `options` | `{id, label, detail?, recommended?}` — only with `answer`; at most one `recommended`; ids unique and non-empty |
| `multi` | more than one option may be picked |

Offer options whenever the answer is enumerable, and none when it is not — an empty list is a
first-class free-text question. Free text and *I'm not sure* are valid on every answer, whatever was
offered. After three unsure answers on a goal the asker is told to proceed on its own recommendation
and record the assumption with `add_note`.
