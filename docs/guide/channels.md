# Channels

A **channel** is a conversation with a name. A **standing** channel belongs to the whole workspace
and has a **roster** — the agents and teams that belong in the room. A **direct** channel has a
fixed **audience** — the principals it is encrypted to — and no roster. A goal has a thread of its own, and a
conversation with agents ([the desktop](the-desktop.md#conversations)) has an origin instead of a
name; all are rendered through the same surface. Architecture:
[05 — Channels](../architecture/05-channels.md).

## `general` is permanent

Every workspace has a `general` channel from the moment it opens. It is ensured at `Workspace::open`
from `library/core/general.toml`, its roster policy is `everyone` — every enabled agent and team is a
member without being listed — and it cannot be deleted (`DELETE /channels/general` is `409`;
`DeletableChannel::new` refuses to name it). Delete its file and it is restored on the next open.

## Rosters: a directory, not a subscription

A standing channel's roster names the agents and teams that belong there — and the people on other
nodes a guest reaches it through ([Collaboration](collaboration.md)). A team on a roster expands
to its enabled agents; the General Agent and the Workflow Agent are in every room and on no roster. What the roster buys:

- the channel header — who belongs here;
- the ordering of the `@`-picker in that channel;
- the **channel handle**: inside a channel, its own id is a mention token that expands to the
  roster's agents, with the General Agent and the Workflow Agent appended last.

**A rostered agent does not auto-reply.** Agents speak when they are addressed, or when they are in
a direct channel's audience. Five agents on `engineering` must never mean five harness sessions per
message. Membership is derived, never stored: `channel_members` is the roster ∩ enablement, so
disabling an agent or a team takes it out of every room at once, and the change is announced there
as a **membership event** — an ordinary message on the wire.

```sh
bisa channels list                                   # standing channels, with unread counts
bisa channels create "design" --topic "how it looks" --agent ux-designer --team design --human <pubkey>
bisa channels edit <channel> --topic "…" --agent … --team … --human … --tag …
bisa msg <scope> "what do we think?" [--mention <agent|team|pubkey|handle>]
bisa msgs <scope>                                    # read (marks read)
bisa read <scope> / bisa unread <scope>
```

An edit replaces the half of the roster it names — the agents, the teams, the people — and keeps
the halves it does not; a topic alone changes nothing else. While a node runs, what these verbs
write goes through it, so every open window follows; with none, into the record.

In the desktop, the **Channels** page lists every standing channel — the name, the topic, when it
last moved and who said what last (`GET /channels` carries each room's latest live post — a post
withdrawn, a join or a leave, a reaction are nobody's last words, in the list and as they land), the
roster's faces when nothing was said yet, the tags, the live unread count — `general` first, then in
creation order, with a search over name, topic and tags; the sidebar's section keeps the same order
([`the-desktop.md`](the-desktop.md#channels-and-messages)).

The audience and the kind are fixed at creation: changing an audience would change who past
messages were encrypted to. Eight standing channels are in the catalog
([`reference/catalog.md`](../reference/catalog.md#channels-8)); installing one installs its roster.

## Direct channels

A direct channel is a conversation with a restricted audience. Its content is encrypted pairwise to
its participants and to **nobody** else — there is no shared workspace key — on the relay path and
the direct path alike; history re-sent to a new member skips it. The channel's *definition* — that it exists, and
who is in it — still syncs workspace-wide; what was said does not.

```sh
bisa dm list
bisa dm send <pubkey> [<pubkey>…] --text "got a minute?"     # opens or reuses the conversation
```

An agent in a direct channel's audience answers without being mentioned, which is why messaging an
agent "just works": `bisa dm send <agent-pubkey> --text "…"`. The reply is signed by the agent. An agent
answers while an engine runs: with a node up — `bisa node`, or the desktop's — what you say from the
command line goes through it; with none, your words are kept and nobody is there to answer them.

The desktop's **Messages** page lists every direct channel by who is in it — never by its stored
name — with the last words said and when (`GET /dms` carries each one's latest live post), the one
that moved last first, and a search by name; the sidebar's *Direct messages* section keeps the same
order.

## Triage: the message that names nobody

An unaddressed message somebody *asked* — in a standing channel, a goal thread or a
conversation — wakes the General Agent, which answers or hands the question to the agent that should. A direct
channel never needs it. A workflow's `notify` step *announces* rather than asks — as the agent it
names, or the Workflow Agent, never as you — and wakes only the agents it mentions — none, by
default: every post records which it is, so a chatty webhook cannot
summon a harness session. The loop guard is bounded by shape: only a message from the General Agent
or the Workflow Agent may wake another agent — the other of the two included — and never itself.

## Messages

A message carries text, optional mentions, optional attachments and — from the IDE phases — optional
**context references** (a file, a selection, a diff hunk) with project-relative paths, so a peer with
the project attached can act on them. Hover a message for its verbs — **React**, **Reply**, **Copy**
(its text, as written) — and the ⋮ menu or a right-click for the full list: *Reply · Copy · Copy
link*, and *Retract* on your own; the same verbs in the same order in a channel, a direct message, a
goal's thread and the IDE's Agent panel. Replies indent one level; a retracted message collapses to
one line and offers nothing; reactions say who. Your message gets a 👀 from the agent that took it, signed by that agent, so
*who answered me* survives a reload and a sync.

**A conversation comes back where you were reading.** Leave a channel, a direct message or an
agent's thread scrolled up, and it opens on the message you were on — after a restart too — with
**Jump to newest** at its foot. It counts as read only when you reach its bottom, so what landed
while you were away is still marked unread. A thread you left at its bottom opens at its bottom.

**Files do not travel with the message.** A message carries the descriptor — name, type, size,
SHA-256 — and the bytes move on demand over a direct connection to whoever has them; 25 MB per
file. An agent's `post_message` may attach files from where it works — its own scratch folder, the
checkout its session runs in, the goal's scratch or a run in the workspace's — and nowhere else.

**An artifact is a file to look at.** What an agent made for you — a page, a chart, a sheet, a deck
— posts as an artifact and renders live under its message, with a viewer, *Save as…*, *Reveal in
Finder* and the conversation's gallery of versions; see [Artifacts](artifacts.md).

## The inbox

Every thing that has ever concerned you earns an inbox row and keeps it — a channel that named
you, a direct channel, a goal that asked, a conversation that moved, a workflow whose listener
could not start its run;
`bisa inbox` prints the same rows the desktop shows, with the offered options under each
question, each unread notice, and the right command under each row. The rows, their asks and their
notices are in [`the-desktop.md`](the-desktop.md#inbox).
