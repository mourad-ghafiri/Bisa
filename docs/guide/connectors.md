# Connectors

A **connector** lets a workflow step call an outside platform — send a mail, post to a channel,
search an issue tracker, read a page — through the platform's own API. Fifteen are built in; a
custom one is a small TOML file. A connector is a *definition* and syncs with the workspace; the
**account** you add for it — your login — is this machine's alone, its secret fields in the
keystore and never in a record, a snapshot, a prompt or a route's answer. The model is
[03 — Workflows § Connectors](../architecture/03-workflows.md#connectors); the step is in
[`workflows.md`](workflows.md#connectors).

## Adding an account

**Desktop.** Settings › Capabilities › Connectors lists every connector installed here — the catalog's and yours —
with its accounts (only one of yours offers *Edit* and *Delete*; the catalog's are the platform's to refresh). *Add account* asks for a label and the connector's parameters (Jira and
Confluence want your Atlassian `site`; Trello its public app `key`), then the secret fields the
connector's scheme needs, typed once into write-only fields. *Check* makes one request to the
platform as that account and says *connected*, *refused* or *unreachable* — an answer that stands
until the account's secrets are set again, it is connected again or forgotten, and is then taken down. The first account is the
default; *Make default* moves the mark; *Forget…* removes the account and every secret it held. The
rows say which fields are set and where they live — `file` under `identity/`, or `keyring` when
`BISA_KEYSTORE=keyring` — and never a value.

**CLI.**

```sh
bisa connector accounts jira
bisa connector account add jira --label work --param site=acme --secret username=@user.txt --secret password=@stdin
bisa connector account check jira <account>
```

A secret is best read from stdin (`@stdin`, once per command) or a file (`@path`), which keeps it
out of the shell's history; a value typed on the command line (`field=VALUE`) is taken too, and the
shell keeps it.

**OAuth2** (Google, X, TikTok). Register your own OAuth client at the platform — nothing ships
with one — with the redirect URI `http://127.0.0.1:4478/connectors/oauth/callback` (the port is
`connectors.oauth.port`, a machine setting), enter its client id and secret on the account, then
press *Connect*: the browser opens the platform's consent page, the node listens on that loopback
port only while the connection is pending, the page says the window can be closed, and the tokens
land in the keystore. Tokens refresh on their own before they expire. A platform that cannot send a
browser back to loopback shows a *paste the code* field instead (`bisa connector connect
<connector> <account>` prints the URL and waits for the code). A long-lived token you already hold
can always be entered as the `access_token` field directly.

## The built-ins

| Connector | Auth | What to create at the platform | Operations |
|---|---|---|---|
| `gmail` | OAuth2 | a Google Cloud OAuth client (desktop app) with the Gmail API on, scopes `gmail.readonly` and `gmail.send` | `profile` (check), `list_messages`, `get_message`, `send_message` — the message as a base64url RFC 2822 string |
| `google-drive` | OAuth2 | the same client, the Drive API on, scope `drive` | `about` (check), `list_files`, `get_file`, `create_file` |
| `google-calendar` | OAuth2 | the same client, the Calendar API on, scope `calendar` | `calendar_list` (check), `list_events`, `create_event` (`calendar` is `primary` for your own) |
| `youtube` | OAuth2 | the same client, the YouTube Data API on, scope `youtube.readonly` | `my_channel` (check), `search`, `channel`, `video` |
| `slack` | bearer | a Slack app with a bot token (`xoxb-…`) and the scopes its operations need (`chat:write`, `channels:read`, `channels:history`) | `auth_test` (check), `post_message`, `list_channels`, `channel_history` |
| `x` | OAuth2 (PKCE) | an X developer app with OAuth 2.0 user context, scopes `tweet.read tweet.write users.read offline.access` | `me` (check), `post`, `search_recent` |
| `facebook-pages` | bearer | a long-lived page access token from a Meta app | `me` (check), `page_posts`, `publish_post` |
| `instagram` | bearer | a long-lived token for an Instagram professional account through a Meta app | `me` (check), `media`, `create_media`, `publish_media` |
| `tiktok` | OAuth2 (PKCE) | a TikTok developer app with `user.info.basic` and `video.list` | `user_info` (check), `video_list` |
| `jira` | basic | an Atlassian API token; the account's `site` is the part before `.atlassian.net`; the username is your email | `myself` (check), `search` (JQL), `get_issue`, `create_issue`, `transition`, `comment` |
| `confluence` | basic | the same token and `site` | `current_user` (check), `search` (CQL), `get_page`, `create_page` |
| `notion` | bearer | an internal integration's secret, shared with the pages it may read | `me` (check), `search`, `get_page`, `create_page`, `query_database` |
| `obsidian` | bearer | the Local REST API community plugin's key; the vault is reached at `https://127.0.0.1:27124` with its self-signed certificate accepted (loopback only) | `status` (check), `list_vault`, `read_note`, `search` |
| `linear` | bearer | a personal API key | `viewer` (check), `issues`, `create_issue` |
| `trello` | API key in the query | your Trello app key (public, an account parameter) and a member token (the secret) | `me` (check), `boards`, `lists`, `create_card` |

`bisa connector show <slug>` prints every operation with its parameters and what each selects
from the answer; [`reference/catalog.md`](../reference/catalog.md#connectors-15) lists them too.

## A custom connector

The same shape as a built-in, in a file:

```toml
[connector]
name = "Status page"
description = "Our status page's API: read incidents, open one."
tags = ["ops"]
base_url = "https://status.example.com"
hosts = ["status.example.com"]
auth = { scheme = "api_key", place = { in = "header", name = "X-Api-Key" } }
check = "incidents"

[[connector.operations]]
id = "incidents"
name = "List incidents"
description = "The open incidents, newest first."
method = "get"
path = "/api/v1/incidents"
query = { status = "open" }
output = { select = "incidents" }

[[connector.operations]]
id = "open_incident"
name = "Open an incident"
description = "Opens an incident with a title and a body."
method = "post"
path = "/api/v1/incidents"
body = { title = "{params.title}", body = "{params.body}" }
writes = true

[[connector.operations.params]]
name = "title"
label = "Title"
required = true
doc = "One line."

[[connector.operations.params]]
name = "body"
label = "Body"
doc = "Markdown."
```

```sh
bisa connector new --from status-page.toml
```

An operation may also say **how it holds up** — three optional fields, each the platform's own
documentation names:

```toml
[[connector.operations]]
id = "export"
name = "Export the report"
description = "A slow, large answer: its own deadline."
method = "get"
path = "/api/v1/export"
timeout_secs = 300                                   # 1 to 600; the machine's connector timeout otherwise

[[connector.operations]]
id = "charge"
name = "Charge the card"
description = "A write the platform can tell a resend from a second request."
method = "post"
path = "/api/v1/charges"
body = { amount = "{params.amount}" }
writes = true
idempotency = { header = "Idempotency-Key" }         # a writing operation only

[[connector.operations]]
id = "incidents_all"
name = "Every incident"
description = "Followed across its pages under the one deadline."
method = "get"
path = "/api/v1/incidents"
query = { cursor = "{params.cursor}" }
output = { select = "incidents" }
page = { cursor_param = "cursor", next_cursor = "meta.next", max_pages = 10 }   # a read that selects a list

[[connector.operations.params]]
name = "cursor"
label = "Cursor"
doc = "Where the page before ended; empty for the first."
```

`cursor_param` is an optional text parameter of the operation; `next_cursor` is the dotted path in
the answer that names the next page; `max_pages` is 1 to 20. The pages' lists are joined into one
answer, and a `connector` start polling such an operation sees every item, not the first page's.
None of the built-ins pages: their reads take a `max`-shaped parameter and stay one page by design.

The desktop's *Add a connector* takes the same definition as JSON and validates it on the node as
you type. The rules a definition must pass: an `https` base URL (or `http` on loopback) whose host
is in `hosts`; hosts spelled `host[:port]` or `*.suffix`; every `{params.<name>}` declared on that
operation and every `{account.<name>}` on the connector; no reserved header (`Authorization` is the
scheme's to set); a `check` operation with no required parameter that writes nothing. A parameter's
`kind` is `text`, `number`, `bool`, `json` or `file`; a JSON body leaf that is exactly one typed
placeholder becomes the typed value. A connector something uses — an account, a workflow step —
cannot be removed until that goes first.

### Bodies, files and signed tokens

A body says its `kind`, and the client encodes it: `json` (an object or array of templates, the
common case), `form` (`application/x-www-form-urlencoded` fields — token endpoints and the older
APIs), `multipart` (`multipart/form-data` — an upload: text parts beside file parts) and `raw` (one
parameter's bytes under a `content_type` — XML, a binary put). A `file` parameter is a path inside
the run's checkout — where the run's work landed — read when the step runs; the model never sees the
bytes, only a multipart part or a raw body may carry them, and a file is read up to 256 MiB. The
body's kind sets the `Content-Type`; a header the definition writes wins, except for `multipart`,
whose boundary only the client knows.

```toml
[[connector.operations]]
id = "upload"
name = "Upload a video"
description = "Sends the video with its title and description."
method = "post"
path = "/upload/v1/videos"
body = { kind = "multipart", parts = [
  { name = "metadata", text = "{params.metadata}", content_type = "application/json" },
  { name = "media", file = "video" },
] }
writes = true

[[connector.operations.params]]
name = "metadata"
label = "Metadata"
kind = "json"
required = true
doc = "The title, the description, the privacy — as the platform's JSON."

[[connector.operations.params]]
name = "video"
label = "Video file"
kind = "file"
required = true
doc = "A path inside the checkout, `renders/today.mp4`."
```

The `jwt` scheme is for the APIs that hand out a private key instead of a token — a developer API's
`.p8`, a service account's JSON key: the platform signs a fresh token at each request and sends it
as `Authorization: Bearer`. The account holds the key as the `private_key` secret (PEM: `BEGIN
PRIVATE KEY` for either algorithm, `BEGIN RSA PRIVATE KEY` for RSA; a `BEGIN EC PRIVATE KEY` is
refused with the conversion to make); `alg` is `ES256` or `RS256`; `claims` and `header` are
templates over the account's parameters, so an issuer id or a key id is a value the person fills once;
`iat` and `exp` (`iat + ttl_secs`, 300 s when unsaid, at most a day) are the clock's.

```toml
[connector]
name = "Developer API"
description = "A platform's developer API, signed with the team's key."
base_url = "https://api.example.com"
hosts = ["api.example.com"]

[connector.auth]
scheme = "jwt"
alg = "ES256"
claims = { iss = "{account.issuer_id}", aud = "developer-api-v1" }
header = { kid = "{account.key_id}" }
ttl_secs = 1200

[[connector.params]]
name = "issuer_id"
label = "Issuer id"
required = true

[[connector.params]]
name = "key_id"
label = "Key id"
required = true
```

```sh
bisa connector account add developer-api --label team --param issuer_id=… --param key_id=… --secret private_key=@AuthKey.p8
```

## Polling a platform

A connector step reads a platform when a run reaches it; a workflow's **`connector` start** reads
one on a schedule and begins a run for every item it has not seen
([Events and gateways](events.md#connector)): the connector, one of its read operations, an
account, the parameters as text, a cadence and the `key` that tells items apart. The event's
payload is `{ id, item }`, read by the start's input mapping. A writing operation is refused when
the workflow is saved; the first poll learns what is there and begins nothing; an item is held for
the content screen before it can begin anything, as everything from outside is.

```toml
[[workflow.steps]]
id = "new-mail"
name = "New mail arrives"
kind = "start"
on = { event = "connector", connector = "gmail", operation = "list_messages", key = "id", every = 900, params = { q = "is:unread newer_than:1d" } }
inputs = { message_id = "{event.payload.id}" }
guard = { overlap = "queue" }
then = ["triage"]
```

Turn the workflow On — in the designer's header, or `bisa workflow on <id>` — and the poll begins.

## When a call fails

Every call — a step's, a start's poll, an account check, an agent's read — goes through one
door, and holds up the same way:

- **A deadline.** The operation's `timeout_secs`, else the engine's own `connector_timeout_secs`
  (60 s — a field of the engine's configuration, not a setting), covers the whole call: every
  attempt, every wait, every page.
- **Retries, by what is safe.** A read is tried three times on a 429, a 500–504, a lost connection
  or a timeout, backing off from half a second to eight with jitter and honouring `Retry-After` up
  to thirty seconds. A **write** is retried only on a 429 or a 502–504 — an answer that says the
  request was not taken — never on a 500, a timeout or a lost connection, where the platform may
  have done the thing.
- **A write is keyed or stopped.** An operation with an `idempotency` header sends the same key on
  every attempt of the same step of the same run — a retry, the step's own `retries`, a restart —
  so the platform does it once. A write **without** one that fails ambiguously (a timeout, a
  connection lost after it was made, a 5xx after sending) is **stopped**: the step fails whatever
  `retries` it had, `on_fail` decides, and the reason says *the platform may have received it —
  check there before running this step again*. A restart that finds such a write running stops it
  the same way instead of sending it again. A connection that was never made, a paused host or a
  refusal is retried as before: nothing reached the platform.
- **A circuit per host.** Five failures in a row (a timeout, a lost connection, a 5xx) pause every
  call to that host for thirty seconds; a call in the pause fails at once with *calls to it are
  paused*, one probe goes through when the pause ends, and its success closes the circuit. A
  refusal — a 4xx, a bad parameter, a denied host — never opens it.
- **A cap per connector.** `connectors.concurrency` (4 by default, a machine setting) is how many
  calls one connector has in flight at once; the next waits for a permit. A `for_each` fanning out
  twenty calls does not hammer the platform, and the platform's own rate limit is still honoured
  call by call.
- **Pages.** A read whose operation `page`s is followed across its cursor under the one deadline.
- **A stopped run stops its call.** Withdrawing or stopping a run — a goal's, or one in the
  workspace — aborts a `connector` step's call in flight; nothing waits on a platform for a step
  nobody reads any more.

## Agents

An agent — any session, not only the Workflow Agent — **lists** the connectors installed here
(`list_connectors`: each platform's operations, which read and which write, whether an account is
connected; never a secret) and **reads** through one with `call_connector`: the connector, a read
operation, an account or the default, the parameters as text. The call is the same call a step
makes — the host policy, the deadline, the cap, the circuit — and its answer comes back **redacted**
and then **screened as content from outside** the way a page or a code-host review is
([11 — Security](../architecture/11-security.md)), bounded to 16 KiB (`truncated` says when). An
operation that **writes** is refused to the tool by name: a write is a workflow `connector` step
behind an `approval` or `human` step — or one the person marked **unattended** — never something a
session does on its own word. The validator holds the workflow to that: a writing step with no gate
upstream and no `unattended: true` is refused (`ungated_write`), and the designer's step form shows
the switch beside the writes warning.

## What a call may reach

A call reaches only the hosts its connector declares. Two settings sit on top: `security.net.deny_hosts`
forbids a host whatever a definition says, and `security.net.allow_hosts` allows one beyond a
connector's own; deny wins — for the operation's host, and for an OAuth2 scheme's consent page and
token endpoint too. A refusal is a guard decision in the Pulse and the journal, with the
method and host as its subject. The credential is applied to the one request that leaves this
machine and appears nowhere else: not in the step's output, not in an error, not in a prompt — and
an answer that echoes a secret back, or mints one, goes through the redactor before it becomes a
step's output, a signal or an agent's reading.
Details: [11 — Security § Outbound hosts](../architecture/11-security.md#outbound-hosts).
