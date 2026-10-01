# Real-world scenarios

A hundred jobs people bring to the platform, each written as the **generic parts that do it** —
agents and teams from the catalog, a workflow template or a designed workflow with the events it
starts on, connectors, tools, gates — and what each still needs from a person. Nothing here is a feature of its
own: the platform ships no scenario, only the parts, and a scenario is a goal a person captures over
them. The page is a coverage check held to the code: every name in a code span below is a catalog
slug, a `connector.operation`, a step kind, a start event, a tool or a setting that exists, and a
test refuses the page when one does not (`crates/bisa-core/tests/it/docs.rs`).

**How to read a row.** *The ask* is the person's sentence. *The path* names the parts. *Today* is one
of three words: **runs** — the goal runs over shipped parts, unattended beyond the gates the person
chose; **with you** — a person's step is in it by design (an `approval` before an operation that
writes, a sign-in on the desktop's browser tab, a payment, a publish, a store submission); **custom
connector** — the platform's API is not in the catalog, and the person or an agent writes its
definition through the recipe ([Connectors § A custom connector](connectors.md#a-custom-connector)).
Where a row names a platform the catalog does not ship (a code host's REST API, a chat suite, a
payment provider, a paper index), that is what *custom connector* means — a TOML file, not a change
to the platform. The sections are lettered, and two letters are left out on purpose: a row numbered
under either would read as a label the documentation's own guard refuses.

Three constraints apply to every row. The embedded browser is the desktop's
([Operating § The browser is the desktop's](operating.md#the-browser-is-the-desktops)), so a goal
that browses needs the desktop open. A connector operation flagged *writes* has an `approval` step
before it in every workflow the Workflow Agent designs, so a standing goal that writes stops for a
person each time — unless the person marked that step *unattended*. Every run an event starts — on a goal that listens, or in the workspace for a
workflow that is On — spends against a ceiling: the goal's budget, or the one the workflow was
turned on with, else the workspace's default (`budget.default.max_usd_cents`,
`budget.default.max_tokens`, `budget.default.max_wall_clock_secs`).

A row that says *a `schedule` start*, *a `hook` start* or *a `check` start* names a `start` step of
the workflow — what begins a run is part of the workflow, heard once the workflow is turned On or
its goal listens ([Events and gateways](events.md)).

## A. Software delivery

| # | The ask | The path | Today |
|---|---|---|---|
| A1 | Make a landing page for my product and show it to me in a browser. | `developer` and `ux-designer` (team `design`); `create_project`; an `agent` step writes the page; a `check` step runs where the agent wrote; `browser_serve` the checkout and `browser_open` to the person. | runs |
| A2 | Here is the repo; finish what is left. | The repo attached as a project; `product-manager` reads what is open; `for_each` feature → `software-feature` (spec, build in a worktree, `code-reviewer`, `qa-engineer`, the publish gate). | runs |
| A3 | Migrate a service from one framework to another with no downtime. | `architect` with `decision-record`; `software-feature` per module; `devops-engineer` writes the deploy plan as `human` steps — a deploy is the person's. | with you |
| A4 | Fix a flaky test suite and keep it green for a month. | `bug-fix` on a goal that listens: a `check` start runs the test command every six hours and begins a run when it starts failing — once per outage, and one run at a time, so nothing piles up. | runs |
| A5 | Weekly dependency upgrades with a pull request. | A `schedule` start on Monday morning; `developer` runs the package manager's update and the tests; the publish gate; opening the pull request is a publish. | with you |
| A6 | A nightly security audit of my repositories. | `security-engineer` with `threat-modeling`; a nightly `schedule` start; `standing-health-check`; `notify` into `engineering`; findings as project notes. | runs |
| A7 | Reproduce and triage every new issue within an hour. | A public `hook` start the code host calls, signed; `developer` reproduces in a worktree; the comment goes through the code host's CLI, which the guard classifies, or a custom connector for the code host's API. | with you |
| A8 | Build a command-line tool and publish it on tag. | `software-feature`; `release-manager` with `release-checklist`; publishing the package is refused or asked by the guard — the person presses publish and a `human` step marks it done. | with you |
| A9 | Port a data pipeline to another language with parity tests. | `architect`, `developer`, `data-analyst`; `check` steps with a schema over the output compare results; a `while` loop with its bound. | runs |
| A10 | Onboard me to an unfamiliar codebase. | `technical-writer` and `architect` write an architecture note as project docs; `browser_serve` renders it. | runs |
| A11 | Hunt a performance regression after a release. | `sre` and `developer`; a `check` start on a benchmark script, beginning `incident-response` when it starts failing. | runs |
| A12 | Pair with me live on a bug in the IDE. | The Project IDE's conversation about the checkout, its agent mode, terminals and inline review. | runs |

## B. Mobile apps

| # | The ask | The path | Today |
|---|---|---|---|
| B1 | Build, publish and maintain an iOS app on a two-week cycle for a year. | A project; `mobile-developer` with `flutter-development` and `app-store-publishing`; `mobile-release` on a goal that listens, a `schedule` start every two weeks; `mobile_development_devices`, `mobile_development_boot`, `mobile_development_screenshot`; the store submission is asked by the guard each cycle, or goes through a custom connector for the store's API under the `jwt` scheme, behind an `approval`. | with you |
| B2 | The same app on Android. | The same path with `google-play-publishing`; the store's API is a custom connector under OAuth2. | with you |
| B3 | Fix a crash reported in the store console. | `browser_open` the console — the person signs in on the tab; `mobile-developer` reproduces on a simulator; `bug-fix`; `mobile-release`. | with you |
| B4 | Ship an MVP from a design link. | `ux-designer` reads the design in the browser; `mobile-developer` builds; the simulator's screen read back into the conversation. | runs |
| B5 | Localise the app into eight languages before each release. | A `for_each` over the locales inside `mobile-release`; a `check` on the translation files. | runs |
| B6 | Review the store reviews weekly with reply drafts. | A weekly `schedule` start; the stores' review APIs as custom connectors (one under `jwt`, one under OAuth2); `content-strategist` drafts; replies wait at an `approval`. | custom connector |
| B7 | Wipe and re-provision the test devices nightly. | A nightly `schedule` start; wiping a simulator is a guard *ask*, answered once per goal or run — each run of the workspace the schedule starts asks again, a goal that listens remembers. | with you |

## C. Web, e-commerce and client sites

| # | The ask | The path | Today |
|---|---|---|---|
| C1 | Run a client's WordPress store with daily security and SEO checks. | A project for the theme; the site's REST API as a custom connector under `basic` with an application password; a daily `schedule` start; `security-engineer` reads plugin versions, `performance-marketer` reads the pages with `browser_read`; `notify` to a client channel; fixes as `software-feature`; writes behind an `approval`. | custom connector |
| C2 | Set up a store and import its catalog. | The store's admin API as a custom connector; a CSV import is a `multipart` or `raw` body from a `file` parameter in the run's checkout. | custom connector |
| C3 | A weekly uptime and quality report for six client sites. | A weekly `schedule` start; `browser_open` and `browser_read` per site in a `for_each`; a `check` start on a request script for uptime. | runs |
| C4 | Move a static site from one host to another. | The build runs locally; the deploy is the person's — the cloud CLIs are classified by the guard. | with you |
| C5 | A demo funnel: landing page, form, a message in the channel. | A1 plus `slack.post_message`; a public `hook` start the form's backend calls, signed. | runs |
| C6 | Watch ten competitor pages daily and alert on change. | A daily `schedule` start; `browser_read` in a `for_each`; `recall_store` and `recall_get` hold the last reading; `notify` on a difference. | runs |
| C7 | Recover a hacked site. | `incident-response`; `security-engineer`; the hosting console in the browser with the person signed in; destructive commands refused by the guard. | with you |
| C8 | Translate a marketing site with a right-to-left review. | `content-strategist` and `developer`; `browser_open` for the person's review step. | runs |
| C9 | Audit the cookie banners across the portfolio. | `legal-advisor` reads the pages; a report. | runs |

## D. Open source and community

| # | The ask | The path | Today |
|---|---|---|---|
| D1 | Contribute to an open-source project for a month: issues, reviews, fixes. | The fork as a project; a daily `schedule` start; the code host's CLI lists what is open (a read the guard allows); `code-reviewer` writes reviews; posting one is a publish — asked. | with you |
| D2 | The same for a year, becoming a maintainer. | D1 as a goal that listens per repository, each under the default budget; every publish still asked. | with you |
| D3 | Answer questions on the project's forum daily. | The forum's API as a custom connector under `api_key`; a `connector` start polls new topics; answers wait at an `approval`. | custom connector |
| D4 | Triage new issues and label duplicates. | A public `hook` start; `evaluator` with `scoring-rubric`; labelling through the code host's CLI is asked. | with you |
| D5 | Cut a release with notes every two weeks. | `release-manager` with `release-checklist`; a `schedule` start every two weeks; the release is a publish. | with you |
| D6 | Keep the documentation's examples compiling. | A nightly `check` start on the build, beginning `bug-fix` when it starts failing. | runs |
| D7 | Review incoming pull requests within a day against a checklist. | A public `hook` start the code host calls on a new pull request — or, for the pull requests the platform's own workstreams opened, a `project` start on a pull request that changed state; `code-reviewer` with `code-review-checklist`; `pr_review_submit` is a publish — asked. | with you |

## F. Team collaboration and tickets

| # | The ask | The path | Today |
|---|---|---|---|
| F1 | Work my team's tickets, ask before pushing, answer the chat. | `jira.search` and `jira.get_issue`; a `connector` start polls the queue; `software-feature` per ticket in a worktree — the publish gate is *ask before pushing*; `jira.transition` and `jira.comment` behind an `approval`; the chat suite's API as a custom connector under OAuth2. | custom connector |
| F2 | A daily stand-up summary into the channel from the board. | A morning `schedule` start; a `weekly-review`-shaped workflow over the runs; `slack.post_message` behind an `approval`. | with you |
| F3 | Turn a specification page into tickets. | `confluence.get_page`; `jira.create_issue` in a `for_each`, each behind an `approval`. | with you |
| F4 | Keep two trackers in sync. | `linear.issues` and `linear.create_issue`; a public `hook` start on each side; the causal chain stops ping-pong; writes behind an `approval`. | with you |
| F5 | Facilitate sprint planning with estimates. | Team `engineering` and `facilitator`; `human` steps with options; `decision-record`. | runs |
| F6 | Keep the knowledge base current from merged work. | `notion.create_page`; a `project` start on a merge of the platform's own pull requests, or a public `hook` start the code host calls; behind an `approval`. | with you |
| F7 | A board for a marketing team, cards from a form. | `trello.create_card`; a public `hook` start the form calls; behind an `approval`. | with you |
| F8 | Review a colleague's branch when asked in the channel. | A `connector` start polls `slack.channel_history` — or, asked in one of the workspace's own channels, a `message` start that names the channel; `code-reviewer`; the reply behind an `approval`. | with you |
| F9 | Teammates on other machines approve my agents' work. | Hosted members with the governance roles; a decision is admitted from a hosted member's key; the gate is answered on the desktop. | with you |

## G. Content, social and media

| # | The ask | The path | Today |
|---|---|---|---|
| G1 | A faceless video channel, one video a day, learning from the numbers. | `content-pipeline` on a daily `schedule` start; `content-strategist` writes; rendering runs as commands; the upload is a `multipart` body with a `file` parameter on a custom connector for the platform's upload API; `youtube.video` and `youtube.channel` feed the next day through `recall_store`; the upload waits at an `approval`. | custom connector |
| G2 | A daily digest of my feed at eight. | A `schedule` start at eight; `x.search_recent` for topics — the home timeline is a custom operation on a copy of the definition; the digest as `notify`. | custom connector |
| G3 | Post a daily thread from my notes. | `x.post` behind an `approval`. | with you |
| G4 | Cross-post articles to three publishing platforms. | Each platform's API as a custom connector (`api_key`, `bearer` or OAuth2); posts behind an `approval`. | custom connector |
| G5 | A carousel three times a week. | `instagram.create_media` and `instagram.publish_media` from an image the person hosts; behind an `approval`. | with you |
| G6 | A weekly performance report. | `tiktok.video_list`; `data-analyst`; a report. | runs |
| G7 | Show notes and chapters from an audio file. | Transcription runs as a command; `technical-writer`. | runs |
| G8 | A newsletter every Friday, approved on my phone. | `content-pipeline`; `gmail.send_message` or a provider's API as a custom connector; the `approval` is answered on the desktop. | with you |
| G9 | Reply to the channel's comments daily in my voice. | The comments API as custom operations on a copy of the definition; a `connector` start polls; replies behind an `approval`. | custom connector |
| G10 | Cut one long video into ten shorts. | The cuts run as commands; the upload as G1. | custom connector |

## H. Research and knowledge

| # | The ask | The path | Today |
|---|---|---|---|
| H1 | A research paper with a novel contribution. | `research-report`; `researcher` with `evidence-and-citation` reads the indexes (custom connectors under `api_key` or `none`) and the browser; `data-analyst` runs the simulations in a project; `critic` with `critical-review` and `red-team`; `decision-record`; a `check` builds the manuscript. | runs |
| H2 | A weekly digest of new papers on a topic. | The index's JSON API as a custom connector, polled by a `connector` start for new entries; a weekly `schedule` start writes the digest; `notify`. | custom connector |
| H3 | A competitive analysis of five products. | Team `strategy`; `business-strategist` with `business-model-canvas`; the browser. | runs |
| H4 | Why does this module exist? | The history in the checkout; `technical-writer`. | runs |
| H5 | A personal knowledge base from my reading. | `obsidian.read_note` and `obsidian.search`; the vault's write endpoints as a `raw` body on a copy of the definition, behind an `approval`. | with you |
| H6 | Fact-check a draft with citations. | `critic` with `evidence-and-citation`; the browser. | runs |
| H7 | Reproduce a paper's results. | A project; `data-analyst`; `check` steps on the tolerances. | runs |
| H8 | A prior-art search. | `legal-advisor` and `researcher`; the patent offices' APIs as custom connectors; the browser. | runs |

## I. Hiring, HR and people operations

| # | The ask | The path | Today |
|---|---|---|---|
| I1 | Hire a developer end to end, through onboarding. | `hiring-loop`; `product-manager` writes the post; applications arrive through `gmail.list_messages` on a `connector` start; `evaluator` with `scoring-rubric`; a `human` shortlist; `google-calendar.create_event` behind an `approval`; the offer as a document; onboarding as `software-feature`-shaped tasks. | with you |
| I2 | Weekly one-on-one preparation from the tracker and the channel. | `jira.search` and `slack.channel_history` on a weekly `schedule` start. | runs |
| I3 | An onboarding buddy answering a new hire's questions. | A conversation in `operations`; the classifier reads outside messages. | runs |
| I4 | Performance review drafts from merged work. | The code host's history; `document-humanizer`. | runs |
| I5 | Contractor invoices against timesheets, monthly. | A monthly `schedule` start; `finance-analyst`; `google-drive.get_file`. | runs |
| I6 | Schedule interviews with candidates by mail. | `gmail.send_message` behind an `approval`; a `connector` start on `gmail.list_messages` catches the replies. | with you |
| I7 | Plan the team offsite. | `event-plan`. | runs |

## J. Personal life and errands

| # | The ask | The path | Today |
|---|---|---|---|
| J1 | Buy a house abroad as a foreigner. | `researcher` and `legal-advisor` on ownership forms and taxes; `finance-analyst` on the numbers; the browser reads listings and notaries; `decision-record`; viewings, transfers and signatures as `human` steps — never the agent's. | with you |
| J2 | Buy the best book on a subject this year. | `researcher` compares reviews in the browser; a `human` choice; `browser_open` the store; the payment is the person's step. | with you |
| J3 | Check my mail at six, answer the important, drop the rest. | A `connector` start on `gmail.list_messages`, polled at six; `gmail.get_message`; `decide` with `evaluator`; `gmail.send_message` behind an `approval`; archiving is a custom operation on a copy of the definition. | with you |
| J4 | Plan a two-week trip within budget. | `event-plan`; the browser; `google-calendar.create_event` behind an `approval`. | with you |
| J5 | Renew my passport. | A checklist of `human` steps; the browser for the forms; the person submits. | with you |
| J6 | A weekly meal plan and the grocery order. | The browser; the order's submit is the person's. | with you |
| J7 | Track my expenses from bank exports monthly. | `google-drive.get_file`; `finance-analyst`. | runs |
| J8 | Learn a language in twelve weeks with a weekly review. | A project; `weekly-review` turned On, its `schedule` start every week. | runs |
| J9 | Keep the family's shared calendar free of conflicts. | `google-calendar.list_events`; changes behind an `approval`. | with you |

## K. Operations, support and reliability

| # | The ask | The path | Today |
|---|---|---|---|
| K1 | Handle support tickets from a form and from mail. | `customer-support-triage` turned On, its `hook` start called by the form; `gmail.list_messages` on a `connector` start; replies behind an `approval`; the escalation gate. | with you |
| K2 | On-call incident response with a runbook. | `incident-response`, which begins when a run fails (its `run` start) or a `check` start on the service starts failing; `sre` with `incident-triage`; `notify`; remediation above the step's ceiling asks. | runs |
| K3 | Verify the backups daily. | `standing-health-check` turned On: its `check` start runs the verification daily and begins a run when it starts failing. | runs |
| K4 | Watch certificate expiry. | A `check` start on a script, daily; `notify` when it starts failing. | runs |
| K5 | A weekly cloud cost report. | The cloud CLIs are classified; a read-only billing export lands in `google-drive.list_files`; `finance-analyst`. | with you |
| K6 | A postmortem after each incident. | A `run` start on the incident workflow's runs that are done — or a `platform` start on a goal being closed; `technical-writer`. | runs |
| K7 | Rotate the secrets quarterly. | `human` steps; the redactor never shows a value. | with you |
| K8 | A service-level dashboard from the tracker. | `jira.search`; `data-analyst`; a page served with `browser_serve`. | runs |
| K9 | Watch a folder of incoming files and load them. | The folder adopted as a project; a `project` start on files that change there, its glob naming the ones that matter. | runs |

## L. Business, sales and finance

| # | The ask | The path | Today |
|---|---|---|---|
| L1 | Sell my application: pricing, positioning, launch, ads. | Team `go-to-market`; `product-launch`; `positioning-and-messaging` and `unit-economics`; the ad platforms in the browser or as custom connectors; spending is the person's step. | with you |
| L2 | A weekly pipeline review from the CRM. | The CRM's API as a custom connector under `bearer`; `sales-lead`. | custom connector |
| L3 | Invoice clients monthly through the payment provider. | The provider's API as a custom connector with a `form` body; behind an `approval`. | custom connector |
| L4 | A board deck every quarter. | `finance-analyst`, `business-analyst`, `document-humanizer`; the deck as a document in a project. | runs |
| L5 | Due diligence on a startup. | Team `venture`; `review-board`. | runs |
| L6 | Respond to inbound leads within ten minutes. | A public `hook` start the form calls; `sales-lead`; the reply behind an `approval`; a reminder on the `approval` step while it waits. | with you |
| L7 | Grant applications with deadlines. | A `wait` step on the deadline's moment; a timeout on the drafting step that diverts to a shorter path; `technical-writer`. | runs |

## N. Standing routines

| # | The ask | The path | Today |
|---|---|---|---|
| N1 | Answer the channel through a computer-use harness. | The platform automates no desktop of its own; a computer-use server is an installed MCP server on an agent, judged by the guard where the harness is judged (`security.mcp.observed` keeps it off the observed ones); `slack.channel_history` and `slack.post_message` cover the channel without it. | with you |
| N2 | A morning briefing at seven: calendar, mail, tickets, news. | A `schedule` start at seven; a `parallel` step reads the four at once — `google-calendar.list_events`, `gmail.list_messages`, `jira.search`; `notify` to a direct channel. | runs |
| N3 | A weekly review of every goal's spend and outcome. | `weekly-review` over the runs; the ceiling on every run an event starts makes the spend a fact. | runs |
| N4 | Nightly repository hygiene: a stale-branch report, no deletions. | A nightly `schedule` start; deletions refused by the guard. | runs |
| N5 | Plan the week's goals with me every Sunday. | `human` steps with options; `facilitator`. | runs |
| N6 | A goal that runs for a year without babysitting. | A goal that listens — captured by the General Agent with `capture_goal`, or started by hand: one run at a time, a failed run pausing it for its repair, restart recovery, stall detection and the default budget; every write still waits at an `approval`. | with you |

## What the tally says

Of the hundred, forty-five run over shipped parts, forty-two run with a person's step that is theirs
by design, and thirteen want a connector definition the catalog does not ship — a TOML file through
the recipe, never a change to the platform. What every *with you* row shares is the `approval` step
before an operation that writes, the gate a person chose or the platform's own rule put there. A
write rides unattended only where a person marked its `connector` step *unattended*, one step at a
time ([Connectors](connectors.md)); no policy lets every write of a workspace through.
