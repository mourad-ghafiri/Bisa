# Collaboration — people on other nodes

Invite somebody, and they join your workspace from their own node — as a human only, at the role
you gave them, reaching what you allow. Join somebody else's, and their channels appear in your
sidebar under *Hosted by …*. Everything rides encrypted over ordinary Nostr relays, which see
ciphertext and a recipient and nothing else. The design is
[14 — Collaboration](../architecture/14-collaboration.md); this guide is how to use it.

## Quick path

```sh
# Alice, the host: the wire is off by default — check the relays, turn it on, invite
bisa relay check                                   # the four default relays, each tried once
bisa settings set machine sync.enabled true        # Settings › Workspace › Relays & sync › Sync over relays
bisa workspace invite --role guest --channel design --label bob
#   prints a link — bisa://join/… — and a code, once; share either out of band

# Bob, on his node: paste the link into Settings › Workspace › People › Join a workspace, or
bisa workspace join <code> --label bob

bisa workspace people        # Alice: the owner and Bob, a guest on #design
bisa workspace hosts         # Bob: "Hosted by Alice", guest
```

In the desktop: **Settings › Workspace › People › Invite someone** makes the link, the code and a QR
to scan;
**Join a workspace** takes what you were sent — or an invitation link opens the app there with the
code filled in.

## Roles

| Role | Reaches |
|---|---|
| **Guest** | only the channels you put them on, and direct messages. No agents. |
| **Member** | every standing channel, direct messages, your agents by mention, gates when governance names members |
| **Admin** | everything a member has, and making channels and inviting, promoting and removing people |
| **Owner** | you |

Change a role from Settings › Workspace › People (or `bisa workspace role <pubkey> member`); the
person hears
the change and their sidebar follows. Remove somebody and they stop receiving anything, come off
every roster and their held messages are dropped — what already reached them cannot be un-sent.

A guest is put on channels two ways: the invitation's `--channel` (repeatable), and a channel's
roster — the **People** picker under the agents on a channel's edit dialog, or `humans` on
`PATCH /channels/{id}`. An admin or a member reaches every standing channel whether listed or not.

## Invitations

An invitation is a single-use code that admits at the role and channels it names, good for
`collab.invite_ttl_hours` (a day by default). The secret is in the link and the code and nowhere
else: the host keeps a hash. Withdraw a pending one from Settings › Workspace › People or
`bisa workspace revoke <id>`. The same panel admits somebody **by key** — 64 hexadecimal
characters, nobody already here — and says under the field which of those a typed key fails.

`collab.join` decides what a valid code does: `admit` (the default) admits at once; `ask` puts the
claim in your Inbox — *Bob asks to join as a guest* — with **Admit** and **Refuse**. `collab.name`
is what guests see your workspace called.

## Relays

**Off by default.** A fresh node holds four public relays — `wss://relay.nostr.com`,
`wss://relay.nostr.net`, `wss://relay.damus.io`, `wss://nos.lol` — and contacts none of them
until **Sync over relays** (`sync.enabled`, machine scope) is turned on: the switch at the top of
Settings › Workspace › **Relays & sync**, or `bisa settings set machine sync.enabled true`. While it
is off an
invitation cannot be claimed and a workspace cannot be joined, and every surface says so.

The panel lists each relay with a dot and its state as the node measures it — *off*, connected or
not, latency, how many attempts answered, bytes moved — with **Check** on every row and **Check
all** (each relay tried once on a throwaway connection, on or off), **Check** before **Add**,
**Remove**, **Reset to the defaults** when the list differs from them, and **Reconnect**. The
list is the `sync.relays` setting at the machine scope (`bisa relay add|remove|list|check [url]` —
`check` with no URL tries every configured relay), so a change reaches the running node at once.
`sync.interval_secs` is the catch-up; `sync.publish_relay_list` says which relays this key reads,
so an invitation's hints stay findable.

The footer's node read-out lists the same relays with their state, **Check relays** tries them
all, and a door opens the panel. The panel, People's *relays are off* line and the read-out read
the wire again on one rule — the relays moved, or a `sync.*` setting did, the switch among them —
so none is left saying *off* after it went on. A check's answer is drawn only under the address
it was about: one that lands after the field moved on says nothing. The wire's counts — published, ingested, people hosted here,
workspaces joined, the last catch-up — are on the panel, in the read-out and at `GET /sync`.

## What a guest sees and does

A hosted section in the sidebar per workspace you joined: its channels and direct channels, with
unread counts. The sections are read again when a hosted workspace moves — a burst of its events
as one read, the newest answer the one that stands — and a host that could not answer is named
in the chrome until a read of it answers again. A hosted channel is the same conversation surface, read and written through the
host: text, mentions of the people there, and replies. No files, no artifacts, no agents of your
own — a message that names one of the host's agents by mention wakes it there, on the host, if
your role may. Reactions and retractions work; a retraction is of your own only.

**Your name and your face travel with you.** Both are set once, on your own node, in Settings ›
You › Identity; every workspace you are a member of hears them the moment you save — right after you
join, and on every change — and draws them beside your messages and in its people list. A member's
name and face reach your node the same way, and a host's face comes with the directory. A face is a
small square (the desktop scales it to 96 px, under 16 KiB) so it rides the same channel as the
words; the bytes are asked for once and kept on your machine.

Leave from Settings › Workspace › People › *Workspaces you are in*, or `bisa workspace leave
<host>`. The host is told; what you received stays on your machine, marked as left. The desktop
forgets the places you had in that workspace — its channels and direct messages — so no door
returns to a conversation you can no longer reach; the same when a host removes you. It forgets
only on memberships the node answered: a read of them that failed says nothing of who left, and
no place is given up for it.

## Safety

Three things read what crosses the wire:

- **The classifier** reads every message from outside before an agent hears it
  (`security.collaboration.classify`, on by default). A message it judges harmful — a prompt
  injection, a secret asked for, a link to run — is **held**: you see it with a caution, in the
  Inbox under People and at `GET /messages/held`, and it reaches no agent until you release it.
- **An agent woken by an outsider** puts every tool beyond reading to you first
  (`security.collaboration.agent_tools = ask`, the default); `as_owner` runs it as your own message
  would.
- **The redactor** runs over every message before it is signed — on your node for what leaves it,
  and on a guest's for what they send you — so a pasted token arrives as a placeholder.

Governance decides gates: *Only me · Me and the admins · Me, the admins and the members · A list*.
A guest never decides.

## Direct links

`sync.iroh.enabled` (on by default) binds the direct QUIC transport on this node with no third
party contacted (`sync.iroh.n0_relays` opts into n0's relay servers for NAT traversal). It carries
attachment bytes and facts between nodes that host each other's key, authenticated by a signed
hello; a peer entered by hand goes under `sync.iroh.peers` (`bisa relay peer-add <pubkey>
<node-id> <ip:port>`). A guest replica reads over relays: an attachment on a hosted message shows
its name and size, and its bytes stay on the host.

## Server mode

`bisa node` is the same binary serving the engine over a unix socket and, optionally, HTTP. An
organisation that wants an always-on host runs one on a server; it is the host of its own
workspace and nothing more.
