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
connector's scheme needs, typed once into write-only fields. Every account row wears the
**health** its last check found — a chip saying *connected (200)*, *refused (401)* or
*unreachable*, with *checked 2 min ago* in its tooltip and the platform's own sentence under a
failing row; the status is the platform's own (a 403 reads *refused (403)*), and an account that
holds no secret for its scheme is *refused* with no status and *the account holds no token;
connect it first*, since there is no way in to try. *Check* makes one request to the platform as
that account, within twenty seconds;
*Check all* does it for every account of every connector that names a check, a few at a time. The
health is the node's — kept for the engine's lifetime, read back by every window through the bus
— and is forgotten when the account's secrets are set again, a parameter is edited, it is
connected again or forgotten, or its definition changes: what a check said was about the way in
the account had then. The first account is the
default; *Make default* moves the mark; *Forget…* removes the account and every secret it held. The
rows say which fields are set and where they live — `file` under `identity/`, or `keyring` when
`BISA_KEYSTORE=keyring` — and never a value.

**CLI.**

```sh
bisa connector accounts jira
bisa connector account add jira --label work --param site=acme --secret username=@user.txt --secret password=@stdin
bisa connector account check jira <account> --timeout-secs 10
```

A secret is best read from stdin (`@stdin`, once per command) or a file (`@path`), which keeps it
out of the shell's history; a value typed on the command line (`field=VALUE`) is taken too, and the
shell keeps it. Beside a running node, `bisa connector accounts` shows each account's health as a
glyph — `✓`, `✗`, or `·` for nothing asked yet — and `--json` carries it; `account check` waits
`--timeout-secs` (1 to 60; 20 unsaid) for the platform and says *connected*, *refused* or
*unreachable* with the status and the reason.

**OAuth2** (Google, X, TikTok). Register your own OAuth client at the platform — nothing ships
with one — with the redirect URI `http://127.0.0.1:4478/connectors/oauth/callback` (the port is
`connectors.oauth.port`, a machine setting), enter its client id and secret on the account, then
press *Connect*: the browser opens the platform's consent page, the node listens on that loopback
port only while the connection is pending, the page says the window can be closed, and the tokens
land in the keystore. The panel reads the landing as a change to the account's row — a token now
held, tokens issued anew, the health the reconnect forgot — so *Connect again* on a connected
account lands the same way; a browser that cannot open leaves the consent link on the row to
copy; the paste box says how long a pasted code is still taken (a connection lives ten minutes)
and when it expired. Tokens refresh on their own before they expire. The consent page and the
token endpoint are hosts the scheme declares by its own URLs — nothing to add to
`security.net.allow_hosts`; the deny list is still read first. A platform that cannot send a
browser back to loopback shows a *paste the code* field instead (`bisa connector connect
<connector> <account>` prints the URL and waits for the code). A long-lived token you already hold
can always be entered as the `access_token` field directly.

## The built-ins

| Connector | Auth | What to create at the platform | Operations | Reference |
|---|---|---|---|---|
| `gmail` | OAuth2 | a Google Cloud OAuth client (desktop app) with the Gmail API on, scopes `gmail.readonly` and `gmail.send` | `profile` (check), `list_messages`, `get_message`, `send_message` — the message as a base64url RFC 2822 string | [OAuth for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app), [Gmail API](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/list) |
| `google-drive` | OAuth2 | the same client, the Drive API on, scope `drive` | `about` (check), `list_files`, `get_file`, `create_file` | [Drive API](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list) |
| `google-calendar` | OAuth2 | the same client, the Calendar API on, scope `calendar` | `calendar_list` (check), `list_events`, `create_event` (`calendar` is `primary` for your own) | [Calendar API](https://developers.google.com/workspace/calendar/api/v3/reference/events/list) |
| `youtube` | OAuth2 | the same client, the YouTube Data API on, scope `youtube.readonly` | `my_channel` (check), `search`, `channel`, `video` | [YouTube Data API](https://developers.google.com/youtube/v3/docs/search/list) |
| `slack` | bearer | a Slack app with a bot token (`xoxb-…`) and the scopes its operations need (`chat:write`, `channels:read`, `channels:history`); an app outside the Slack Marketplace reads history under a stricter limit | `auth_test` (check), `post_message`, `list_channels`, `channel_history` — every answer must say `ok: true` | [Web API](https://docs.slack.dev/apis/web-api/), [conversations.history](https://docs.slack.dev/reference/methods/conversations.history) |
| `x` | OAuth2 (PKCE) | an X developer app with OAuth 2.0 user context, scopes `tweet.read tweet.write users.read offline.access`; the API on `api.x.com`, the consent page on `x.com` | `me` (check), `post`, `search_recent` | [OAuth 2.0](https://docs.x.com/resources/fundamentals/authentication/oauth-2-0/authorization-code), [users/me](https://docs.x.com/x-api/users/user-lookup-me) |
| `facebook-pages` | bearer | a long-lived page access token from a Meta app; the Graph API at v26.0 | `me` (check), `page_posts`, `publish_post` | [Pages API](https://developers.facebook.com/docs/pages-api/posts), [versioning](https://developers.facebook.com/docs/graph-api/guides/versioning) |
| `instagram` | bearer | a long-lived token for an Instagram professional account through a Meta app — the Instagram API *with Facebook Login*, permissions `instagram_basic` and `instagram_content_publish`; the Graph API at v26.0 | `me` (check), `media`, `create_media`, `publish_media` | [content publishing](https://developers.facebook.com/docs/instagram-platform/content-publishing) |
| `tiktok` | OAuth2 (PKCE) | a TikTok developer app with `user.info.basic`, `user.info.stats` and `video.list`; TikTok's own spelling — `client_key`, comma-joined scopes, a hex challenge — is on the scheme | `user_info` (check), `video_list` — every answer must say `error.code: ok` | [Login Kit for desktop](https://developers.tiktok.com/doc/login-kit-desktop), [user info](https://developers.tiktok.com/doc/tiktok-api-v2-get-user-info) |
| `jira` | basic | an Atlassian API token; the account's `site` is the part before `.atlassian.net`; the username is your email | `myself` (check), `search` (the enhanced JQL search, `/rest/api/3/search/jql` — a bounded query; ids alone unless `fields` says what to return), `get_issue`, `create_issue`, `transition`, `comment` | [issue search](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/) |
| `confluence` | basic | the same token and `site` | `current_user` (check), `search` (CQL, v1), `spaces` (a key's numeric id, v2), `get_page`, `create_page` (v2: a space id, a parent page or the space's home) | [REST v1](https://developer.atlassian.com/cloud/confluence/rest/v1/api-group-content/), [REST v2](https://developer.atlassian.com/cloud/confluence/rest/v2/intro/) |
| `notion` | bearer | an internal integration's secret, shared with the pages it may read; pinned to `Notion-Version: 2022-06-28`, which Notion keeps serving — a database that gained a second data source needs the newer API | `me` (check), `search`, `get_page`, `create_page`, `query_database` (every row when no filter is given) | [versioning](https://developers.notion.com/reference/versioning), [search](https://developers.notion.com/reference/post-search) |
| `obsidian` | bearer | the Local REST API community plugin's key; the vault is reached at `https://127.0.0.1:27124` with its self-signed certificate accepted (loopback only); the check reads the plugin's `authenticated` flag, since its status page answers without a key | `status` (check), `list_vault` (the root), `list_folder` (a folder, its path a `path` parameter), `read_note` (a `path`), `search` | [Local REST API](https://github.com/coddingtonbear/obsidian-local-rest-api) |
| `linear` | API key in the `Authorization` header, bare | a personal API key (the `Bearer` form is OAuth's) | `viewer` (check), `issues`, `create_issue` — a GraphQL answer must carry no `errors` | [GraphQL API](https://linear.app/developers/graphql) |
| `trello` | API key in the query | your Trello app key (public, an account parameter) and a member token (the secret) | `me` (check), `boards`, `lists`, `create_card` | [authorization](https://developer.atlassian.com/cloud/trello/guides/rest-api/authorization/) |
| `github` | bearer | a personal access token with access to the repositories — read and write on their issues to open one or comment; pinned to `X-GitHub-Api-Version: 2022-11-28` | `user` (check), `list_issues` (pull requests are among them, each with a `pull_request` key), `get_issue`, `list_pulls`, `search_issues`, `create_issue`, `comment` | [REST API](https://docs.github.com/en/rest/using-the-rest-api/getting-started-with-the-rest-api?apiVersion=2022-11-28), [issues](https://docs.github.com/en/rest/issues/issues?apiVersion=2022-11-28) |
| `gitlab` | API key in the `PRIVATE-TOKEN` header | an access token on gitlab.com: `read_api` to read, `api` to write (a self-managed instance is a custom connector with its own host) | `user` (check), `projects`, `list_issues`, `get_issue`, `list_merge_requests`, `create_issue`, `comment_issue`, `comment_merge_request` — a project by its id or its path, `group/project` | [REST API](https://docs.gitlab.com/api/rest/), [authentication](https://docs.gitlab.com/api/rest/authentication/) |
| `bitbucket` | basic | your Atlassian account email and a Bitbucket API token scoped to repositories and pull requests | `user` (check), `repositories`, `pull_requests` (the open ones unless a state is named), `pull_request`, `comment_pull_request` — no issue tracker: its operations are not in Atlassian's published description | [API description](https://api.bitbucket.org/swagger.json), [API tokens](https://support.atlassian.com/bitbucket-cloud/docs/using-api-tokens/) |
| `sentry` | bearer | an auth token with `org:read` and `event:read`, and `event:write` to update; the account names the `region` (`us` or `de`) and the `organization` slug | `projects` (check), `issues` (the unresolved ones unless a query is given), `issue`, `issue_events`, `update_issue` (resolve, reopen, ignore, assign) | [API](https://docs.sentry.io/api/), [issues](https://docs.sentry.io/api/events/list-an-organizations-issues/) |
| `pagerduty` | API key in the `Authorization` header, as `Token token=<key>` | a REST API key; the account names the `email` of a user the writes are made as (PagerDuty's `From`) | `abilities` (check), `incidents` (the last month unless dates are given), `incident`, `oncalls`, `services`, `create_incident` (it notifies whoever is on call), `add_note` | [versioning](https://docs.pagerduty.com/developer/versioning), [create an incident](https://docs.pagerduty.com/developer/api/reference/rest/incidents/create-incident) |
| `discord` | API key in the `Authorization` header, as `Bot <token>` | a bot's token; the bot in the server and allowed to see the channel (reading message text needs Discord's privileged Message Content access configured for the app) | `me` (check), `guilds`, `channels`, `messages`, `send_message` | [API reference](https://docs.discord.com/developers/reference), [messages](https://docs.discord.com/developers/resources/message) |

`bisa connector show <slug>` prints every operation with its parameters and what each selects
from the answer; [`reference/catalog.md`](../reference/catalog.md#connectors-21) lists them too.
Each file under `library/catalog/connectors/` opens with `# Reference:` lines naming the platform
documentation it was checked against, and carries a `revision` the platform reads at the engine's
start: an installed copy below the bundle's revision is refreshed — its accounts, secrets and
steps kept, a credential moved to the field a changed scheme reads — so a fix to a built-in
reaches you without a reinstall, and `ConnectorsChanged` says so in the feed.

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
body = { kind = "json", value = { title = "{params.title}", body = "{params.body}" } }
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
body = { kind = "json", value = { amount = "{params.amount}" } }
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

An operation may also say **what a good answer looks like**, for a platform that answers a failure
with a 200 — Slack's `{"ok": false, "error": "invalid_auth"}`, a GraphQL answer with `errors`, a
status page that says `authenticated: false`:

```toml
[connector.operations.output]
select = "channels"
expect = { path = "ok", equals = true, reason = "error" }      # Slack: `ok` must be true
# expect = { path = "errors", absent = true, reason = "errors.0.message" }   # GraphQL: no errors
```

`or_empty = true` beside `select` reads a list the platform leaves out of an empty answer as an
empty list, where a missing field is otherwise a failure; say it only where the platform's
documentation says the field can be absent.

`expect` names a dotted path and one rule — `equals = <value>` or `absent = true` (nothing, `null`
or an empty list) — and, optionally, the dotted path of the platform's own sentence. It is read
before `select`; a miss is a refusal in the platform's words, never a stop, and a check that
misses it says *refused*. A parameter of kind **`path`** is a slash-separated path on the platform
— a note in a vault, a file in a tree: in a URL path its slashes stay and each segment is encoded
on its own, where a `text` parameter is one segment. An **optional parameter left empty is left
out** of a JSON body — the member or item that is exactly its placeholder is not sent, the way an
empty query pair or form field is — so a `description` nobody typed reaches no platform as `""`;
a leaf mixing text with an empty parameter is refused instead. An OAuth2 scheme may spell a
platform's **dialect**: `client_id_param = "client_key"` (the client's id under that name, on the
consent page and in the token form), `scope_join = "comma"` (scopes joined by a comma rather than
RFC 6749's space) and `code_challenge = "hex"` (the PKCE challenge as the hex of the SHA-256, under
the same `S256` word); each is the RFC's own when unsaid.

The desktop's *Add a connector* takes the same definition as JSON and validates it on the node as
you type. The rules a definition must pass: an `https` base URL (or `http` on loopback) whose host
is in `hosts`; hosts spelled `host[:port]` or `*.suffix`; every `{params.<name>}` declared on that
operation and every `{account.<name>}` on the connector; no reserved header (`Authorization` is the
scheme's to set); a `check` operation with no required parameter that writes nothing. A parameter's
`kind` is `text`, `number`, `bool`, `json`, `path` or `file`; a parameter may carry a `default` — `default = 100`
on a number, `true` on a bool, an object on `json`, a value of its own kind or the definition is refused
— sent when the step leaves it unset (a required one with a default need not be set; an emptied
field reads as unset), shown in the designer as the field's placeholder and printed by `bisa
connector show`; an account's parameter takes none; a JSON body leaf that is exactly one typed
placeholder becomes the typed value, and one naming an absent optional parameter is left out. A connector something uses — an account, a workflow step —
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

### More than one key, and a client's own login

**More than one key.** Some platforms ask for two keys on every request — an API key and an
application key, say. `extra_keys` on an `api_key` scheme names each further one: the account's
secret field that holds it (`secret`), the platform's own name for it (`label`, what the account's
form asks for), where it goes (`place`) and an optional `prefix`. Every key is required, each is
set like the first and scrubbed like it from whatever the platform says back, and an account
holding one key of two is *not connected* — nothing is sent with half a credential. A second key
is kept under `app_key`, or any of `token`, `username`, `password`, `client_id`, `client_secret`;
never under `api_key` (the first key's) or a field a connection writes. Two keys cannot share a
header or a query name. A definition without `extra_keys` is exactly what it was.

```toml
[connector]
name = "Metrics"
description = "A monitoring platform that asks for two keys."
base_url = "https://api.metrics.example"
hosts = ["api.metrics.example"]

[connector.auth]
scheme = "api_key"
place = { in = "header", name = "X-Api-Key" }

[[connector.auth.extra_keys]]
secret = "app_key"
label = "Application key"
place = { in = "header", name = "X-Application-Key" }
```

```sh
bisa connector account add metrics --label work --secret api_key=@api.txt --secret app_key=@app.txt
```

**A client's own login** (`client_credentials`, RFC 6749 §4.4). For a platform that gives a
server-side app a client id and secret and no person to consent: the account holds the two, and
the platform trades them for an access token at `token_url`. There is no *Connect* and no browser
— the token is asked for when the first call needs one, kept in the keystore with its expiry,
used until a minute before it lapses, and asked for again then or when the platform answers 401
(once; a second 401 is the platform's answer). Calls at the same moment ask for one token between
them. `scopes` and `scope_join` (`space` · `comma`) spell the `scope` pair; `token_auth` says
where the client's id and secret travel (`form`, the default, or `basic` for an `Authorization:
Basic` header); `grant_type` is `client_credentials` unless the platform names its own; `params`
adds pairs to the token request, each a template over the account's parameters. The token
endpoint is a host the scheme declares by its URL, the deny list still read first. A platform
that refuses the client reads *the platform refused the client's id and secret; set them again*.
Setting the client's id or secret again, or editing the account's parameters, lets go of the
token kept for the old ones.

```toml
[connector]
name = "Meetings"
description = "A meetings platform reached as a server-side app."
base_url = "https://api.meetings.example"
hosts = ["api.meetings.example"]

[connector.auth]
scheme = "client_credentials"
token_url = "https://auth.meetings.example/oauth/token"
scopes = ["meeting:read"]
token_auth = "basic"
params = { account_id = "{account.account_id}" }

[[connector.params]]
name = "account_id"
label = "Account id"
required = true
```

```sh
bisa connector account add meetings --label ops --param account_id=… --secret client_id=@id.txt --secret client_secret=@stdin
```

`bisa connector show <connector>` says on its `secrets` line which fields an account must hold.

**What no scheme expresses yet.** A secret that is part of the URL's path, a login that trades a
username and password for a session token, a request signed over its own bytes (AWS Signature
Version 4, an HMAC of the body), and OAuth 1.0a. A platform that needs one of these cannot be
written as a connector today.

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
  attempt, every wait, every page, and the token refresh a call needs on its way.
- **Retries, by what is safe.** A read is tried three times on a 429, a 500–504, a lost connection
  or a timeout, backing off from half a second to eight with jitter and honouring `Retry-After` up
  to thirty seconds. A **write** is retried only on a 429 or a 503 — an answer that says the
  request was not taken — and, when it carries a key, on a 502 or a 504 too; never on a 500, a
  timeout or a lost connection, where the platform may have done the thing. A `put`, a `patch` or
  a `delete` is held to the write rules whatever its definition calls it.
- **A write is keyed or stopped.** An operation with an `idempotency` header sends the same key on
  every attempt of the same visit of the same step of the same run — a retry, the step's own
  `retries`, a restart — so the platform does it once; each iteration of a loop, and each time a
  run comes back to the step, is a write of its own with a key of its own. A write the platform
  **took** whose answer could not be used — the selected field is missing, the answer is too
  large or does not fit the step's output — is stopped too, and says *the platform took the
  write*: sending it again would do it twice. A write **without** a key that fails ambiguously (a timeout, a
  connection lost after it was made, a 5xx after sending) is **stopped**: the step fails whatever
  `retries` it had, `on_fail` decides, and the reason says *the platform may have received it —
  check there before running this step again*. A restart that finds such a write running stops it
  the same way instead of sending it again. A connection that was never made, a paused host or a
  refusal is retried as before: nothing reached the platform.
- **A circuit per host.** Five failures in a row (a timeout, a lost connection, a 5xx) pause every
  call to that host for thirty seconds; a call in the pause fails at once with *calls to it are
  paused*, one probe goes through when the pause ends, and its success closes the circuit. A
  refusal — a 4xx, a bad parameter, a denied host — never opens it; a probe whose call is dropped
  before it answers — a stopped run, a deadline above the call — is let go, and the next call after
  the pause is the probe.
- **A redirect is a refusal.** A connector follows no redirect: a 3xx names a definition's wrong
  URL, in the platform's own `Location`, and is never a reason to stop a write. A dated
  `Retry-After` is read against the clock.
- **A check has a budget.** *Check* waits twenty seconds for the platform (1 to 60 at the caller's
  word), the wait for a permit included, and says *unreachable* when it runs out; a check already
  running for the account is joined, not doubled.
- **A poll loses nothing.** A start that polls remembers an item only once it wrote the
  occurrence down: one that met a full backlog or a spent minute's budget is found again at the
  next poll, in order. An item over 64 KiB fires with its key and `oversize_bytes` in place of
  the item. Polls run four at a time on a ticker of their own, so one slow platform holds neither
  another poll nor a schedule.
- **Never a proxy on the way to this machine.** A call to a loopback address — however it is
  spelled — goes direct, whatever proxy the environment or Settings › Network names.
- **A cap per connector.** `connectors.concurrency` (4 by default, a machine setting) is how many
  calls one connector has in flight at once; the next waits for a permit. A `for_each` fanning out
  twenty calls does not hammer the platform, and the platform's own rate limit is still honoured
  call by call.
- **Pages.** A read whose operation `page`s is followed across its cursor under the one deadline.
- **A stopped run stops its call.** Withdrawing or stopping a run — a goal's, or one in the
  workspace — aborts a `connector` step's call in flight; nothing waits on a platform for a step
  nobody reads any more.

## Agents

**Who may call** is the workspace's word, `connectors.agents` — a project may say its own:
*everyone* (the default), *assigned* — agents that carry the **Connectors** skill, and the
platform's own agents always — or *nobody*. A refusal is a sentence, said before a definition, an
account or a secret is read. A workflow's `connector` step is not an agent's call: the setting
does not touch it.

An agent — any session, not only the Workflow Agent — **lists** the connectors installed here
(`list_connectors`: each platform's operations, which read and which write, whether an account is
connected; never a secret) and **reads** through one with `call_connector`: the connector, a read
operation, an account or the default, the parameters as text. The call is the same call a step
makes — the host policy, the deadline (the operation's plus the engine's overhead, the wait for a
permit included, so a read behind a full pool is refused in the operation's time), the cap, the
circuit — and its answer comes back **redacted**
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
token endpoint too, which are hosts the scheme declares by its own URLs: the flow reaches them as a
call reaches the operations' hosts, with no row in `hosts` and nothing to allow by hand. A refusal is a guard decision in the Pulse and the journal, with the
method and host as its subject. The credential is applied to the one request that leaves this
machine and appears nowhere else: not in the step's output, not in an error, not in a prompt — and
an answer that echoes a secret back, or mints one, goes through the redactor before it becomes a
step's output, a signal or an agent's reading.
Details: [11 — Security § Outbound hosts](../architecture/11-security.md#outbound-hosts).
