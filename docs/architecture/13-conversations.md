# 13 — Conversations

A **conversation** is a saved exchange a person starts with one or more agents: an id, an
**origin** — what it is about — a title, its messages, archived or not. There are many per origin;
a person starts one, comes back to it, searches for it, names it, puts it away. The agent's reply
**streams** into it as it is written — the words and the thinking that led to them — and lands as
one message with the thinking kept beside the words. The harness process that answers one of its turns is a
**session** of kind `conversation`, reached only through its conversation — never a row of the
checkout it happens to run in. Channels, direct channels and a goal's thread are what they were:
addressed by the thing they belong to, one per thing ([05 — Channels](05-channels.md)).

---

## Vocabulary

| Word | Means | Never |
|---|---|---|
| **conversation** | a saved exchange a person starts with agents — an id (`ConversationId`), an origin, a title, its messages, archived or not; many per origin; the agent's memory of it *is* the conversation | a channel, a direct channel, a goal's thread — those exist once per thing and are addressed by it |
| **origin** | what a conversation is about and where its turns run: `node` · `workspace` · `goal` · `workflow` · `project` · `workstream` · `drawing` · `note` (`ConversationOrigin`) | a **scope** — which message stream a message is in: `channel` · `goal` · `conversation` (`ScopeKind`) |
| **session** | one live run of a harness, in four kinds (`SessionKind`): `worker` (a step's work item) · `guided` (the Workflow Agent's design wake) · `conversation` (a turn of a conversation) · `terminal` (a harness a person opened in a terminal, reporting through its hooks) | "a chat", "a persona", "an interactive session" |
| **turn** | one wake of an agent in a conversation: the session that answers a message, live between messages, parked by the idle TTL | — |
| **thread** | a goal's conversation, where the Workflow Agent designs — and, asked for changes, proposes — where a `notify` speaks and gates are asked | a workstream's — a checkout has conversations, not a thread |
| **thinking** | the reasoning a harness streams before an agent's words (`ProgressEvent::ThinkingDelta`), shown as it arrives and kept beside the reply (`MessageBody::Post { thinking }`), folded above the words and shown or hidden at will | an instruction; the reply; a summary — the harness keeps and compacts its own context, the platform summarises nothing |
| the rail's rows | **work sessions and terminals**: `worker` sessions standing in the checkout, `terminal` sessions through the tab that claims them, shells | a conversation's or a note's turn |

`SessionKind` is the store's (`crates/bisa-store/src/index.rs`), re-exported by the engine's
registry, written to the session row and served on the roster as `kind`; the desktop reads the same
four words.

---

## The model

```rust
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ConversationOrigin {
    Node, Workspace,
    Goal { id: GoalId }, Workflow { id: WorkflowId },
    Project { id: ProjectId }, Workstream { id: WorkstreamId, project: ProjectId },
    Drawing { id: DrawingId }, Note { id: NoteId },
}
pub struct Conversation { id: ConversationId, origin: ConversationOrigin, title: Option<String>, created_at: u64, archived: bool, mode: ConversationMode, mode_before_plan: Option<ConversationMode> }
```

`crates/bisa-core/src/conversation.rs` is the rule: `ConversationOrigin::KINDS` is the eight words in
the order every picker offers them — the wire's `?origin=`, the index's `origin_kind` and the
desktop's `ORIGIN_KINDS`, which a test reads from the Rust source so the two cannot drift;
`kind_takes_id` says which kinds name a thing (`node` and `workspace` do not); `from_parts` builds
one from the wire; `project()` answers for a project or a workstream origin, `goal()` for a goal's;
`reaches_workflow_agent()` is false for a project, a workstream, a drawing or a note; `drawing()` and
`note()` answer for those two, the record the tools default to. `mode` is `manual` · `auto` ·
`plan` (`ConversationMode`, default `manual`) — meaningful only where `origin.is_checkout()`, read
and written by nothing elsewhere; `mode_before_plan` is `Some` only while `mode` is `plan`, the mode
`Conversation::set_mode` returns to when *Build this plan* leaves it (`mode_after_plan`, asked for by
the engine's `conversations::build_plan` — `POST /conversations/{id}/plan/build`) — both are
[ide/20 — Reviewing agent changes](ide/20-reviewing-agent-changes.md)'s. A title is a person's, trimmed,
non-empty when given and at most `MAX_CONVERSATION_TITLE_CHARS` (120); until one is given the
first line of the first post names the conversation (`conversationsModel.titleOf`), and *New
conversation* before that.

Beside the record the index keeps what a list draws (`ConversationRow`): when it last moved, how
much was said, the agents that took part (`conversation_agents` — the author of every agent post
and every agent mentioned). The node's `ConversationView` is the row.

**On disk** the record is `conversations/state/33415-<id>.json` — kind `KIND_CONVERSATION`, a
replaceable, addressable record with a workspace audience, synced through GEP like a channel's
([09 — GEP](09-protocol-gep.md)); its messages are kind-3407 events in `conversation/<id>.jsonl`,
the stream under the conversation's own id. The index (`conversations`, `conversation_agents`;
`SCHEMA_VERSION` 26) is rebuilt from both ([08 — Persistence](08-persistence.md)). Deleting a
goal, a workflow, a project or a workstream removes the conversations with that origin
(`remove_conversations_of`), their logs with them; deleting a project takes its workstreams'.

---

## Origins — placement, frame, reach

Where a turn runs and what the agent is told follow the origin, in one table:

| Origin | About | The turn runs in | The frame says | Workflow Agent | Default agent resolved at |
|---|---|---|---|---|---|
| `workstream` | one checkout of a project | that checkout — the tree for the primary, the worktree or copy otherwise | the project frame: the project and its goals, or that it is attached to none (`framing::project_frame`, [ide/09](ide/09-agents-in-the-ide.md)) | never | the project layer |
| `project` | a project as a whole | its primary checkout | the project frame | never | the project layer |
| `goal` | a goal | the agent's scratch (`agents/<id>/scratch/`) | *This conversation is about the goal …; you are not running a work item for it* — `get_goal` reads it, and a project made here attaches to it (I28a) | reachable | the workspace layer |
| `workflow` | a workflow in the library | the agent's scratch | *This conversation is about the workflow …: its inputs, its steps and its flows; read it with `get_workflow`, `validate_workflow` until clean, then `save_workflow` at the revision you read, and leave `workflow` out of every call* — the engine chooses this workflow for `save_workflow` and allows the write nowhere else (`intake::workflow_of`); the designer beside the conversation adopts the revision or offers its conflict banner | reachable | the workspace layer |
| `workspace` | the workspace as a whole | the agent's scratch | *This conversation is about the workspace as a whole* | reachable | the workspace layer |
| `node` | this machine's node — its harnesses, setup and settings | the agent's scratch | *This conversation is about this machine's node* | reachable | the workspace layer |
| `drawing` | a drawing on the canvas ([19](19-drawings.md)) | the agent's scratch | *This conversation is about the drawing …, open on the canvas beside the person; draw with the drawing tools and leave `drawing` out of every call* — the engine chooses this drawing for a drawing tool that names none | never — a picture has no workflow to shape | the workspace layer |
| `note` | a note in the notes overlay | the agent's scratch | *This conversation is about the note …, open beside the person; read it with `note_read` before answering, write into it only when asked — asked to change its text, rewrite it with `note_write` at the hash the read answered; asked to add, `note_append` adds under your name — and leave `note` out of every call; a change is written, never assumed: say what you did only after the tool answered* — the engine chooses this note for a note tool that names none; the reply is the conversation's, never the document's. A harness handed no MCP server (`HarnessCaps::MCP_SERVERS` absent — pi, Oh My Pi, a custom harness) reads instead that it has no platform tools here and cannot read or write the note, so it says so rather than describing a change it never made (`framing::origin_frame_for`); a drawing's frame says the same of the canvas | never — a scratchpad is somebody's own | the workspace layer |

The frame for the last six is `framing::origin_frame(origin, subject)`. The engine's
`conversation.rs` resolves a scope once into `ScopeFacts { conversation, goal, checkout }`, and
everything downstream reads the facts: `dispatch` is where a message is **heard** — once, by the
`message` starts, waits and boundary events that name where it was said, who said it or what it
says (`listen::ear::on_message`; never an announcement, a workflow's own post) — and it skips the Workflow Agent when the origin cannot
reach it (mentioned or named as the default — the general agent answers instead), `default_agent`
resolves `agents.default` at the layer the table names, `wake_attempt` places and frames the
session, and registers it as `SessionKind::Conversation` with its `conversation`, `workstream`,
`project` and `goal` set so the roster can say what it is a turn of. The desktop agrees with the
engine's reach in `addressModel.reachableIn` over the same kinds (the checkout kinds, `drawing` and `note`), so a setting
written from the CLI or synced in cannot route around the rule. The loop guard is unchanged: an
unaddressed message wakes a core agent, a core agent may wake one other, never itself
([05 — Channels § Triage](05-channels.md#triage-a-message-that-names-nobody)).

A turn of a conversation about a checkout runs under its own **mode**'s ceiling in that checkout —
manual's `Write`, auto's `Exec`, plan's `Read` — its permissions judged against that directory; an
edit lands pending and is kept or undone in the review, by hunk, by file or by turn, and the turn's
own asks are answered in the conversation itself, never the Inbox
([ide/20 — Reviewing agent changes](ide/20-reviewing-agent-changes.md)). A conversation
with a goal origin answers that goal through the intake (`get_goal`, `session_roots`) but is not the
goal's thread — the thread stays where the Workflow Agent designs and where a gate is asked.

---

## Lists, search, coming back

`GET /conversations` is the one list, filtered by `origin` (a kind) and `id` — or by `project`, every
conversation standing in a project, its own and every checkout's, never beside `origin` — by `agent`,
by `archived`, by `q` and paged by `before` and `limit` (`ConversationFilter`; the index's
`conversations.project` column, indexed with `archived` and the last activity): live conversations first,
each set by its last activity, newest first. `q` matches a title and the text of any message in it —
the message index's full-text table, the same one the Pulse searches, keyed by the conversation's id
— so a conversation is found by a word said in it weeks ago. `POST /conversations` starts one from an
origin and a title, `PATCH` renames (a `null` title takes the name away) or archives, `DELETE`
removes it, `GET|POST /conversations/{id}/messages` is its stream, and `GET
/conversations/{id}/live` is the turns in flight — below ([reference/http-api](../reference/http-api.md)). The engine's
door is `conversations.rs`; the CLI's is `bisa conversation list · new · show · post · rename ·
archive · unarchive · delete` ([reference/cli](../reference/cli.md)).

On the desktop:

- **One surface, every owner** (`views/_studio/useConversationSurface.ts` over
  `conversationSurfaceModel.mjs`, painted by `ConversationSurface.tsx`): a goal's Conversation tab
  (`GoalConversationPane`), the Workflow Designer's Agent pane (`WorkflowAgentPane.tsx`), the drawers
  beside a note or a drawing (`ConversationDrawer.tsx`) and the IDE's Agent pane and Agent mode
  (`useConversationPane`, which brings the thread's wiring to the same surface) each draw one bar —
  the owner's glyph, the thing's title (the picked conversation's while a thread shows) and the
  **Conversations** door with its count, one label (`ConversationsBar.tsx`: `ConversationsDoor` and
  `ConversationList`) — over one view at a time (`surfaceView`): the list, filling the surface — the
  rows wearing the origin in words (`originWords`: *the workspace*, *goal Dark mode*, *this checkout ·
  main*), the agents' marks and when each last moved, a search the **node** answers (`?q=`, what was
  said included — the IDE's too), an *Archived* switch, the owner's own surface as a first row where
  it has one (the goal's thread), and ***New conversation*** at the foot; the picked thread, whole
  (`ConversationThread.tsx`: the thread with its title renamed in place, its origin as a door to the
  thing, and its verbs — *Rename…*, *Archive* (or *Take it back out*), *Delete*; the IDE's
  `Conversation` with its chips, mode and ledger); the owner's own surface while nothing is picked,
  where it has one; a quiet line while a record is read; the error, with a retry, when the node
  cannot be reached — never mistaken for *no conversations*; or, only when there is none, the empty
  state (`NoConversation.tsx`) with the owner's sentence and one primary button that says what the
  foot says, ***New conversation*** — one label for one act. **The act** is one click, untitled,
  the thread standing the moment the node answers (`startConversation`; the record is read by id
  before the list has it, so nothing flashes); the thread's menu names it later.
  **The pick is the owner's, the list is a view.** What a surface is on is an id — the address's
  (`?conversation=`) or the owner's remembered one (`pickedId`) — whose record is read **by id**
  (`useConversation`) and held to the owner (`belongsTo`: the owner's own origin, or standing in the
  project for the IDE's project-wide list); the search, the switch, the page and a reload in flight
  never change it. A pick that is gone (the node's *not found*) or another owner's (a stale link) is
  dropped; a network error drops nothing; an archived pick is shown as it is — its thread says so and
  offers *Take it back out*. **No surface guesses**: with nothing picked it shows the owner's own
  surface, else the list while conversations exist, else the empty state. Where the pick lives is
  the owner's strategy (`conversationSelection.ts`): **route** — the goal's tab, `?conversation=` and
  `?conversations=1`, so a link lands on one and Back leaves it; **remembered** — the designer's pane,
  the route and a memory beside it, so every door back to a bare `#/workflows/<id>` comes back to the
  conversation left; **kept** — the drawers, the memory alone with the fold in state, so a drawer
  reopened lands on the conversation left there and writes nothing into the screen's address; **root**
  — the IDE, the memory, the fold remembered per root (`agentPaneViewStore.ts`, `bisa.ide.agents.view`)
  and a `?conversation=` link consumed on arrival with no history entry. One memory for every owner
  (`conversationPickStore.ts`, `bisa.conversations.pick`, per owner key — `workstream:<wid>`,
  `workflow:<id>`, `note:<id>` — thirty-two at most; a pick arriving by link is copied in, *back*
  forgets it). There is no screen that lists every conversation and no sidebar section: a conversation
  is reached where it is about.
- **A link** (`#/conversations/<id>`, `views/ConversationDoor.tsx`) is a door, not a page: it reads
  the conversation and replaces itself with its origin's address (`routeOf` — a goal's tab, a
  workflow's pane, the IDE's Agent panel for a project's or a workstream's). Only a conversation
  about the workspace or the node, which nothing owns, is drawn there, on a page of its own; the
  desktop starts none — the CLI does.
- **Quick open** (`⌘K`) lists live conversations by their name or first line, under
  *Conversations* ([ide/12](ide/12-search-and-quick-open.md)).
- **A goal's and a workflow's menus** offer *Conversations about this goal / workflow*: the goal's
  unfolds its tab's list, the workflow's opens the designer's Agent pane on its list
  (`routeOf` for a `workflow` row answers `?panel=agent&conversation=`).
- **The IDE's Agent pane** brings its own **source** to the surface: **the project's conversations** —
  the project's own and every checkout's, this one's and its siblings' alike, one read by `project`
  (`listQuery({project})`), each row wearing where it is about (*workstream feature/checkout of web*)
  — while *New conversation* is about the checkout; its pick is the root's (`workstream:<wid>`, the
  owner key and the root key letter for letter). The IDE never guesses which conversation a checkout
  is on; a **hand-off** from the editor, a page or a terminal does, and writes the pick
  (`conversationsStore.ensureConversation` over `conversationPaneModel.currentConversation`: the
  remembered pick while it is live in the project, else the newest live one whose turns run here —
  the checkout's own or the project's, `runsHere` — else the newest of the project, else a new one
  about the workstream; `handOffTarget` never lands in a sibling checkout's conversation, whose turn
  would run there). [ide/09 — Agents in the IDE](ide/09-agents-in-the-ide.md) has the pane.
- **The Inbox** gives a conversation a row of its own kind when it names you or moves unread
  (`InboxKind::Conversation`) — under the tab of what it is about (`InboxRow.source`: a goal's, a
  workflow's, a project's or a checkout's; the node's and the workspace's under *Messages*), the
  row saying its `origin` — its door *Open the conversation*; the Pulse files its messages
  under Channels; an `ConversationCreated` or `ConversationChanged` frame moves every list.
- **Read is the thread's own doing.** One local watermark per scope (`read_markers`, `POST /read`;
  a row is read when the watermark stands at or past its newest message, gate or notice), moved by
  the one component that shows a thread, `views/_studio/Chat.tsx`, through `useReadAsShown` over
  `readModel.mjs` — never by a route: when the scope has something to read (`readWanted`: unread
  messages, or an inbox row unread for a gate or a notice alone) and the person can see the newest
  message (`shownToReader`: the window visible, the thread at its bottom, the first page landed),
  the scope is marked read after `READ_AFTER_MS`; a change of scope, of unread or of what is shown
  restarts the beat. So the IDE's Agent pane, a goal's or a workflow's conversation, a channel, a
  direct message, the Inbox's own pane and a hosted channel all read the same way, and the store
  (`useWorkspaceData.markRead(scope, host)`) patches what the shell shows — the unread, the inbox
  row, a hosted entry — before the node's delta confirms it.
- **A thread comes back where it was being read** (`useThreadPlace.ts` over
  `threadPlaceModel.mjs`, in the desktop's `bisa.view.threads`, keyed by the chat's own key so a
  thread has one place whichever screen shows it). While the person scrolls, the place is the
  first message whose foot is below the viewport's top edge and the offset inside it — every
  message's wrapper carries `data-message` — and at the bottom it is nothing; nothing is kept on
  unmount, since a detached viewport reads zero. A mount with a place kept does not start at the
  bottom: `restoreStep` waits for the first page, scrolls to the message when the page has it,
  reads older pages — four at most — when it is above, and forgets a place it cannot reach, the
  thread then opening at its bottom. The place is put back again while what is above it grows,
  until the person's first wheel, click or key, or the grace; an ask that took the focus wins.
  **Reading is untouched**: a thread put back above new replies is not at its bottom, so it stays
  unread until the reader comes down — *Jump to newest* is the door. For the window's life a
  thread's messages are kept (`threadsStore.ts` over `threadCacheModel.mjs` — 24 threads, the
  newest 300 messages of each) and the newest page is **joined**, not swapped in (`joinNewest`:
  it replaces where it overlaps and keeps what is older; a full page with no overlap is a gap, and
  the thread starts over from the page), so a thread that comes back is drawn at once with what
  was loaded; after a restart the node is read.

---

## Every hand-off from the IDE goes to a conversation

A page annotation, an editor selection asked about or edited, a terminal's last lines, the
checkout's changes, a review asked of an agent: each is a message posted **into the conversation
the checkout is on** — and when there is none, one is started about the workstream first
(`conversationsStore.ensureConversation`; the toast says *Sent to Reviewer in a new conversation*).
A remembered pick that cannot be read starts a new one only on the node's *not found*; a node
that did not answer is an error, said — not a second conversation about one checkout.
A hand-off never names a session: a session is a run the platform made, not a place a person sends
things to (`conversationPaneModel.handOffTarget`). The agent's edit is followed to its end on that
agent's turn of that conversation (`agentEditsModel.settles`), and the review run's replies land in
the conversation the review was asked in (`useReviewRun`, `reviewStepModel`).

---

## Memory

A conversation is the agent's memory of it. A live session keeps the harness's own context between
turns; a **fresh** turn is given the transcript — the last `TRANSCRIPT_WINDOW` (50) messages,
oldest first (`transcript_tail(scope, TRANSCRIPT_WINDOW)`), each rendered with who said it. The
harness holds the rest as its own context and **compacts it itself**, the way Claude Code, Codex,
OpenCode and the rest already do: the platform writes no summary, keeps no counter, offers no
button and no setting. A message stream has two body kinds, `post` and `membership`, and nothing
else.

---

## The reply streams

While a turn runs, the harness's words and its reasoning reach the timeline as they are written.

**The harness says it.** `ProgressEvent::TextDelta { text }` is a piece of the reply;
`ProgressEvent::ThinkingDelta { text }` is a piece of the model's reasoning — never an instruction,
never posted as the reply. Claude Code is launched with `--include-partial-messages` and its
`stream_event` lines carry both, token-wise (`content_block_delta` — `text_delta`, `thinking_delta`);
the whole `assistant` message that follows is read for its `tool_use` blocks alone, so a word is
said once. Codex's `reasoning` item, OpenCode's `reasoning` part and ACP's `agent_thought_chunk`
are thinking; pi and a custom harness say text alone ([crates/adapters](crates/adapters.md)).

**The engine streams it.** The reply pump (`conversation.rs` `spawn_reply_pump`) appends the
answering agent's top-level deltas — a sub-agent's `Nested` ones are not the reply — to the turn in
flight, `LiveTurn { text, thinking, since, working }` per `(scope, agent)`, and emits
`EnginePayload::AgentStreamed { scope, agent, text, thinking, working }`: the deltas since the last
frame, gathered by a `Streamer` and flushed at most every `STREAM_FLUSH` (120 ms) — on the next delta
once the interval passed, or on the pump's timer when the interval passes with something gathered and
no further event (a harness thinking, a tool running, a turn that stops without saying so): a frame
is never more than an interval late — the moment the
delta's kind switches (`Streamer::switches`, flushed before the new kind is pushed — thinking to words is the moment a timeline folds the
thinking, and a frame late by an interval would show the fold a beat after the words), or when a
boundary event arrives — a tool, a sub-agent, a cost. A frame is many tokens; either part may be
empty. `working` is the tool the agent runs *now*, as it stands — `ToolStarted { name, args_summary }`
sets it (`Read src/app.ts`), `ToolEnded` clears it, a frame replaces it rather than appends, and a
`null` is news: the tool ended; a tool line that moved goes out even with nothing said. A
`TurnStarted`, or a follow-up delivered while the harness stayed silent about turn boundaries,
starts the turn over. Both kinds of delta pass the Redactor like the words did
([11 — Security](11-security.md)). `conversations::live_turns(inner, id)` is the turns in flight —
what `GET /conversations/{id}/live` answers (`{turns: [{agent, text, thinking, since, working}]}`,
empty when nothing runs) to a reader that joins mid-turn.

**The reply lands with its thinking.** At the turn's end `finish_turn` posts
`MessageBody::Post { text, context: [], artifacts: [], thinking }` signed by the agent — the
thinking trimmed, its tail kept within `MAX_THINKING_BYTES` (64 KiB; the end is what led to the
words), redacted like the words, `None` when the harness thought nothing aloud — removes the live
turn and emits `AgentReplied { scope, agent, posted, message }`, `message` the id the reply landed
as (`None` when nothing new was posted: a tool-only turn — a note rewritten with `note_write` and
nothing else said — a reply already given through `post_message`, or nothing could be posted), so a timeline can hold the
turn on screen until that very message is in its page. **A reply longer than one message holds**
(`MAX_TEXT_BYTES`, 256 KiB) is said whole, in as many messages as it takes and in order — cut where
a paragraph ends, else a line, else between two characters, a code fence open at the cut closed
and opened again (`bisa_core::split_text`) — the thinking beside the first, `message` the last.
What a turn keeps is bounded: `conversation::MAX_REPLY_BYTES` (eight messages' worth) of words,
and the thinking's tail; a turn that says more is cut there and the person is told so, in the
platform's own sentence (`engine-conversation-reply-cut`), after the words that were kept. The store keeps it in `messages.thinking` (null for a person's post) and
`MessageRow.thinking` carries it on the wire; a person's post never has one.

**The desktop draws it.** `liveTurnsStore.ts` (over `liveTurnModel.mjs`) adds `agent_streamed`
frames up to each agent's turn per scope — the tool line replaced, not appended — and settles the
turn on `agent_replied` with the message it names (`landedOf`, `settleTurn`: a reply longer than one
message lands as several and the frame names the **last**, so the live row gives way once the whole
reply is drawn; `posted: false` — nothing landed — clears it at once, and so does a turn that settles
where no timeline reads, which keeps the store to the turns in flight and the rows a page is about
to draw). Each message of the reply is its own read of its own row, in the order the frames came,
none cancelling another (`threadCacheModel.nudgeRead`, `useScopeMessages`); a
`Chat` mounted on a conversation primes itself once from the live route (`primeLiveTurns`), so a
reader who arrives mid-turn sees the whole of it. The store is read two ways so **a frame
re-renders one row and never the timeline**: `useLiveTurnAgents(scope)`, the agents with a row (a
stable array until the set changes), and `useLiveTurn(scope, agent)`, one turn (the same object
until it moves). A turn in flight stands at the foot of the timeline (`LiveTurnRow`: the agent,
the working dot, its thinking above its words, a caret at the end of what is still being written,
and — while a tool runs — one dim line, *running Read src/app.ts*; never a transcript,
[ide/09](ide/09-agents-in-the-ide.md)). **A post the platform authored is said in the reader's
language**: it carries `said` — the message of the catalog behind its English
([17](17-internationalisation.md)) — and the row draws `tx(said)` in place of its `content`, the
content alone when this catalog lacks the message (`timelineModel.contentOf`; the reply-to strip
reads the same). Its words and thinking are **paced**
(`useStreamPacer` over `streamPacerModel.mjs`): the engine's frame is many characters, and shown as
it lands a reply moves in jumps, so what is left of a frame's backlog is revealed over what is left
of the interval since it landed, on animation frames — a steady rate, caught up exactly as the next
frame is due, the whole at once past
`JUMP_CHARS`, when the turn is done, or under reduced motion (`shell/motion.ts`). They are
**parsed block by block** (`StreamedMarkdown` over `streamBlocksModel.settledBlocks`): a blank line
outside a fence settles every block above it, each settled block is one memoised `Markdown` parsed
once, and the tail alone is re-parsed as its words land — so a long reply costs no more per frame
than a short one. The timeline follows the foot by the content's `ResizeObserver`, not by
re-rendering. **Landing never blinks:** the settled row stands, frozen — no caret, no pacing — until
`retireLandedTurns` sees its message among the page's rows, and a conversation frame that names its
event reads that one message (`useScopeMessages`, `api.message`) rather than the page of sixty. A
landed reply keeps its thinking above its words (`ThinkingBlock`, the kit's disclosure — *Thinking
· 1.2k chars*; live, *Thinking… 4s · 800 chars*, and folded, one dim line of its tail moving —
`glimpse`). The footer's control — `ThinkingPicker`, a `ChoiceMenu` reading *Thinking: Auto ▾*,
drawn where there is thinking to show — sets the mode every block follows (`thinkingOpen`): **auto**
(the default) is open while the agent is only thinking and folds itself the moment its words begin,
a landed reply folded; **shown** opens every block; **hidden** folds every block. Remembered under
`bisa.chat.thinking` (`thinkingStore.ts`); one block's own press wins until the mode is changed
again. An agent working with nothing said yet is the footer's *is writing…* line; one that has
begun, or runs a tool, is its row.

---

## The rail draws work sessions and terminals

The project rail, the Workstreams panel, the footer's harness rows, the pulse line, the activity
fold, the resource overlay and the pet read **one rule** — `workstreamSessionsModel.isDrawn`: a
session is a row of the checkout it stands in when its kind is one of `WORK_KINDS` (`worker`,
`terminal`), and a `terminal` only through the tab that claims it. A conversation's turn, a note's
answer and a guided wake are never rows of a checkout, never counted in *2 working*, never the
pulse's subject; the engine's `ide/git.rs::running_in` counts the same way, so a workstream's
`running_agents` on the wire is its workers. A turn is shown, followed, stopped and answered **on
its conversation**: the Agent panel's session line (`conversationPaneModel.sessionsOf`,
`turnSummary`) lists the turns of the conversation the checkout is on, with their sub-agents,
harness, model and cost, the one *Stop* on the loudest stoppable one; the Agents screen's Running
tab names the conversation a `conversation` session is attached to. A harness a person opened in a
terminal is `kind: terminal` (`POST /sessions/terminal`) and stays where it was — the tab is the row
([ide/06](ide/06-terminals.md)).

**Presence carries every kind.** The roster (`GET /sessions`) serves every session with its `kind`
and, for a turn, its `conversation`; which kinds are drawn where is the desktop's rule and the
engine's status rule above, never presence's. The idle TTL parks a turn — the roster row and the
store row alike — an abort ends it, and deleting or archiving a conversation stops its turns first
(`conversations::stop_sessions`). A turn that is open has the workers' **wall clock**
(`default_wall_clock_secs`, measured from the turn's start; the idle TTL owns the gaps between
turns): past it the session is aborted and its row reads *failed: wall clock exceeded*, one note
on the conversation — a harness that never answers cannot hold a conversation's turn for ever.
The roster row says the turn's origin: its scope — the string `agent_thinking` spells — and the
person on another node whose message woke it.

---

## What a conversation is not

- **Not a channel.** A channel has a roster and a handle and exists once; a conversation has an
  origin and exists as many times as a person starts one. Nothing is triaged *between* them.
- **Not the goal's thread.** The Workflow Agent designs in the thread — and, asked for changes
  there, its turn is launched knowing the goal and proposes — and a gate is asked there; a
  conversation about a goal is a person talking with an agent *about* it, in the agent's scratch.
- **Not a session.** A session is evidence — a run the platform made, parked and disposed at will;
  the conversation is the record the session is a turn of. A hand-off is sent to a conversation.
- **Not a transcript viewer.** The session's transcript pane tails the harness's own file
  ([ide/09](ide/09-agents-in-the-ide.md#status--what-each-agent-is-doing)); the conversation's
  messages are the platform's, signed and synced — the reply and its thinking stream into them,
  the tool calls and the sub-agents do not.
- **Not a place a gate is answered.** A conversation about a workflow, the workspace or the node
  has no goal; an above-ceiling request or a question is refused with the reason and the agent
  asks in words ([feature status](../feature-status.md)). A conversation about a checkout has no
  goal either, and is the one exception: a call above its mode's ceiling is asked in the
  conversation itself ([ide/20](ide/20-reviewing-agent-changes.md)) — a question still has
  nowhere to go. A conversation about a goal carries the goal, and its gates go to the Inbox.

---

## Tests

| Invariant | Test |
|---|---|
| every origin kind round-trips its tag; `id()`, `project()` and `goal()` per variant; the Workflow Agent is reachable from node, workspace, goal and workflow and never from a project or a workstream; a blank title, one over 120 characters and a unicode one at 120; a reply keeps its thinking beside its words within `MAX_THINKING_BYTES`, none when blank, and no `summary` body parses; `ScopeKind::ALL` has three and `workstream` no longer parses | `crates/bisa-core/src/conversation.rs`, `crates/bisa-core/src/message.rs` (unit tests) |
| a conversation is created per origin and refused for an unknown goal, workflow, project or workstream; listed newest activity first, filtered by origin, id, agent, archived and a word said in it; renamed, archived, deleted with its log; a post bumps its counts and agents; a post's thinking round-trips and a person's post has none; the transcript tail is the newest messages oldest first; a project's deletion takes its workstreams' conversations; a rebuild reproduces the rows; a session row carries its kind and conversation | `crates/bisa-store/tests/it/conversations.rs` |
| the desktop has words and a glyph for every one of the wire's eight origin kinds (read from `types.gen.ts`); a pick that is gone is dropped and a note deleted under its editor is a `404` read as *gone* — nothing more is saved to it, a delete of it has happened; a reply in several messages is one read a row, in frame order, and the live row waits for the last; `posted: false` waits for none | `desktop/src/views/_studio/conversationsModel.test.mjs`, `desktop/src/notes/notesModel.test.mjs`, `desktop/src/views/_studio/liveTurnModel.test.mjs`, `desktop/src/views/_studio/threadCacheModel.test.mjs`, `desktop/src/scenarios/channelsAndMessages.test.mjs` |
| a post into a conversation about a workstream wakes the project's default agent in the checkout, one about a project in the primary, the others in the agent's scratch with their frame sentence; the Workflow Agent is woken by mention in a conversation about a workflow and never in one about a checkout; the roster row is `kind: conversation` naming its conversation while `running_agents` stays 0; a turn streams its words and its thinking as `AgentStreamed` frames that add up to the posted reply, which carries its thinking, and nothing is left in flight; a reader joining mid-turn reads the live turn and an unknown conversation is not found; the idle TTL parks the store row; `ConversationCreated` and `ConversationChanged` reach the bus | `crates/bisa-engine/tests/it/conversations.rs`, `crates/bisa-engine/tests/it/conversation.rs` |
| a reply longer than one message lands whole, in order, in as many messages as it takes, the thinking beside the first and the bus naming the last; a turn that never stops is kept to `MAX_REPLY_BYTES` and says it was cut | `crates/bisa-engine/tests/it/conversations.rs` (`a_reply_longer_than_one_message_lands_whole_in_as_many_as_it_takes`, `a_reply_without_an_end_is_kept_to_a_bound_and_says_it_was_cut`); `crates/bisa-core/src/message.rs` (`split_text`'s unit tests) |
| every route: created per origin over HTTP, an unknown origin 404, a bad title 400, the list's filters and search, a patch's tri-state title, delete 204 then 404, messages with chips, the live turns empty before a word and after the reply and 404 for an unknown conversation, no compaction counter on the wire, the retired workstream messages routes 404; `/sessions/terminal` | `crates/bisa-node/tests/it/conversations.rs`, `crates/bisa-node/tests/it/sessions.rs`, `crates/bisa-node/tests/it/ide.rs` |
| each of the eight origins — the node, the workspace, a goal, a workflow, a project, a workstream, a drawing, a note — is started, listed by its own origin and no other's, and read by its id whatever a page of the list holds; one that is gone is *not found*, and so is a conversation asked to be about a drawing or a note that is not here | `crates/bisa-node/tests/it/conversations.rs` — `each_origin_kind_is_started_listed_by_its_own_and_read_by_id_off_any_page`; the journey `crates/bisa-cli/tests/it/e2e/pulse_and_inbox.rs::a_conversation_sits_under_what_it_is_about_is_found_by_a_word_said_and_read_by_its_id` |
| the desktop's origin kinds are the Rust enum's; a row's name, order, route and words — a goal's opens on its tab, a workflow's in its pane, a checkout's in the IDE, one nothing owns on its own page; the surface — one door label and its hint per source, the empty sentence per owner, the pick as an id (the address's, else the remembered one, never a row), its record held to the owner (`belongsTo`), one view at a time (`surfaceView`), the query by origin or by project, one capped pick memory per owner; which sessions are a conversation's turns, what the bar says of them, and where a hand-off lands; the turns in flight — frames add up, a landed reply clears, a late reader is primed whole — and the thinking block's and the toggle's words; the rail rule — a conversation's turn is never a row, never counted, never the pulse's subject; the addressee follows the origin; an edit settles on the agent's turn of the conversation; a platform-authored post is said from its `said` message in this language and falls back to its content when the catalog lacks it | `desktop/src/views/_studio/conversationsModel.test.mjs`, `desktop/src/views/_studio/conversationSurfaceModel.test.mjs`, `desktop/src/views/_workbench/conversationPaneModel.test.mjs`, `desktop/src/views/_studio/liveTurnModel.test.mjs`, `desktop/src/views/_workbench/workstreamSessionsModel.test.mjs`, `desktop/src/views/_workbench/agentRailModel.test.mjs`, `desktop/src/views/_studio/addressModel.test.mjs`, `desktop/src/views/_workbench/agentEditsModel.test.mjs`, `desktop/src/views/_studio/timelineModel.test.mjs` |
| a person's afternoon: a checkout opens with none, the first hand-off starts one, the turn shows on the panel and not on the rail or in the footer, a second is started, the first picked up again, found by a word the node is asked for, archived and still on the pane's Archived switch, and the pick survives a restart; no screen lists every conversation — the sidebar, the router, the goal's page, the designer and the IDE's pane say so; the IDE's pane has one door and one view at a time and nothing about compaction, sessions or standing alone; a reply streams with its thinking and the thinking can be hidden and shown; the designer's Agent pane comes back to its conversation by any door, across a remount, a link's pick copied in and *Back* forgetting it | `desktop/src/scenarios/conversations.test.mjs` |
| one surface as a person meets it: a drawer opened fresh over three conversations shows them, never *none yet*; with none, the empty state whose button and the foot say one word; *New conversation* is *reading* then the thread, never the list; a search and the Archived switch leave the pick standing; a pick that is gone is dropped, another owner's is dropped, a network error drops nothing; the IDE's pane is the same surface with the project's list and a hand-off that still knows where to land; the goal's own thread stands while nothing is picked and is the list's first row; five hosts, one painter, one hook, one label, and nothing asks a name | `desktop/src/scenarios/conversationSurface.test.mjs`, `desktop/src/scenarios/conversationDrawer.test.mjs` |
