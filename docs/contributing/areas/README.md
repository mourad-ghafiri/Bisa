# Area guides

Pick the area your change belongs to. Each guide says where the code lives, what to read before you
touch it, the rules a change there must keep, how to test it, which recipes apply, what is part of the
public contract, and what review looks at first. The same areas appear in the issue forms and as
`area:` labels ([Triage](../triage.md#labels)).

## The platform
| Area | Guide | What it covers |
|---|---|---|
| Core | [core](core.md) | the domain: goals, workflows, runs, kinds, invariants — `bisa-core` |
| Workspace | [workspace](workspace.md) | the files that are the truth, the index, the catalog install — `bisa-store` |
| Node and HTTP API | [node](node.md) | the daemon's routes, auth and event stream — `bisa-node` |
| CLI | [cli](cli.md) | the `bisa` command line and the end-to-end journeys — `bisa-cli` |
| MCP tools | [mcp](mcp.md) | the tools every harness session is handed — `bisa-mcp` |
| Harnesses | [harnesses](harnesses.md) | Claude Code, Codex, OpenCode, Copilot, Grok, Gemini, pi, ACP, A2A, custom — `bisa-harness`, `bisa-adapters` |
| Collaboration | [collaboration](collaboration.md) | people on other nodes, GEP over Nostr — `bisa-collab`, `bisa-guest`, `bisa-net` |
| Security features | [security](security.md) | the Redactor, the Tool & Commands Guard, the Classifier, the publish gate — `bisa-security` |
| Decision-making | [decision-making](decision-making.md) | the Decision-Making Agent and the decision points — `bisa-decision` |
| Connectors | [connectors](connectors.md) | outside platforms as workflow steps — `bisa-connectors` |

## What a person uses
| Area | Guide | What it covers |
|---|---|---|
| Inbox and Pulse | [inbox-and-pulse](inbox-and-pulse.md) | everything that waits on a person; the live feed |
| Agents and teams | [agents-and-teams](agents-and-teams.md) | agents, teams, model plans, effort, skills, assignment |
| Projects | [projects](projects.md) | projects, workstreams, the publish gate, the Board |
| Project IDE | [ide](ide.md) | files, search, git, terminals, language servers, browser, Mobile Development, pull requests, agents in the IDE |
| Workflows | [workflows](workflows.md) | steps, start events, gateways, templates, the designer |
| Goals | [goals](goals.md) | goals, their modes, their runs |
| Channels and conversations | [channels](channels.md) | channels, direct messages, conversations, artifacts |
| Settings | [settings](settings.md) | the registry, scopes, the Settings screens |
| Notes and drawings | [notes-and-drawings](notes-and-drawings.md) | notes, Excalidraw drawings, their repositories |
| Addons | [addons](addons.md) | small windows of HTML, CSS and JavaScript, and their bridge |
| Desktop app | [desktop](desktop.md) | the Tauri shell and the UI: models, scenarios, themes, keymap, menu bar |
| Mobile app (planned) | [mobile-app](mobile-app.md) | the mobile client to come, and what exists for it today |

## Around the code
| Area | Guide | What it covers |
|---|---|---|
| Catalog | [catalog](catalog.md) | the agents, skills, teams, channels, templates, connectors, addons and pets that ship |
| Translation | [translation](translation.md) | the message catalogs, adding a language |
| Website | [website](website.md) | bisa.dev, built from `scripts/website/` |
| Docs | [docs](docs.md) | this documentation set and the checks that hold it |
| Build and release | [build-and-release](build-and-release.md) | the gate, CI, licences, the macOS bundle, releases |
