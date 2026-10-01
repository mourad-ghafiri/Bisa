# bisa-connectors

Outside platforms' HTTP APIs, called through one declarative shape. The domain owns the
**definition** — a connector's base URL, the hosts it may reach, its auth scheme, its operations
([03 — Workflows § Connectors](../03-workflows.md#connectors)) — and this crate owns everything
between that definition and the wire: binding a step's parameters by kind, rendering the
definition's `{account.…}` and `{params.…}` placeholders into a URL and a body encoded as its kind
says, reading the files a `file` parameter names through a port, applying the account's credential
through one signer per scheme, refusing a host the definition does not name, sending with one retry policy,
refreshing an OAuth2 token before it expires, and turning the answer into an `Outcome` the run
machine records. It depends on two leaves of ours and nothing above them — `bisa-http` for the
client set, `bisa-netrules` for the host grammar — and everything else it needs
from the platform arrives through traits the engine implements — so a test runs against a loopback
stub and nothing here ever reaches a real platform. A credential cannot be printed, every error is
scrubbed of the strings it exposed, no redirect is followed, nothing is written to disk.

---

## Where things live

| Module | Owns |
|---|---|
| `spec.rs` | the call-shaped mirror of a definition the engine builds once per call: `CallSpec { connector, operation, base_url, hosts, insecure_tls, auth, method, path, query, headers, body, params, select, writes, idempotency?: Idempotency { header }, idempotency_key?, page?: Paging { cursor_param, next_cursor, max_pages }, timeout }`, `Method`, `KeyPlace` (tag `in`), `AuthSpec` (tag `scheme`: `none`, `api_key { place, prefix }`, `bearer`, `basic`, `oauth2 { authorization_url, token_url, scopes, pkce, extra }`, `jwt { alg, claims, header, ttl_secs }`), `JwtAlg { Es256, Rs256 }`, `ParamKind { Text, Number, Bool, Json, File }`, `ParamSpec { name, kind, required }`, `CallBody` (tag `kind`: `json { value }`, `form { fields }`, `multipart { parts }`, `raw { content_type, from }`), `Part { name, source: PartSource::{File { file }, Text { text }}, filename?, content_type? }`; `CallSpec::param(name)` |
| `creds.rs` | `Secret` (a `Debug` and `Display` that never show it; one `expose`), `Field` (the nine wire words — `api_key`, `token`, `username`, `password`, `client_id`, `client_secret`, `access_token`, `refresh_token`, `private_key`), `AccountRef { connector, account }`, `Stored { fields, expires_at }`, `TokenSet`, `Credential` (with `Jwt(Secret)`, the key a token is signed with); the ports — `Credentials { load(&AccountRef) -> Stored, save_tokens(&AccountRef, &TokenSet) }` (the engine's is the workspace keystore), `Clock`, `Entropy` (`OsEntropy` over `getrandom`); `SystemClock` |
| `files.rs` | the port a `file` parameter's path is read through — `Files { read(path) -> Result<FileData, String> }` (the engine's reads the run's checkout; a test's is a map), `FileData { filename, content_type?, bytes }`, `NoFiles` (a call with no checkout — a poll, a check — refuses every file), `MAX_FILE_BYTES` (256 MiB; the client refuses past it) |
| `template.rs` | the crate's own scanner over the two connector roots, with the same `{{`/`}}` rule as the core's — `Values { account, params }`, `Encode { Authority, PathSegment, Text }`, `render`, `names_absent`, `typed_leaf` (a body leaf that is exactly one typed placeholder) |
| `body.rs` | one encoder per body kind behind `BodyEncoder { encode(&Sources) -> Encoded { content_type, bytes } }`, chosen by `encoder_for(&CallBody)`: `JsonEncoder` (leaves rendered, a typed leaf the typed value), `FormEncoder` (an absent optional field dropped, the rest form-encoded), `MultipartEncoder` (a `bisa-<24 hex>` boundary from `Entropy`; a text part rendered or dropped when absent, a file part's bytes with the part's or the file's name and type, `application/octet-stream` when neither says), `RawEncoder` (a `file` parameter's bytes, another kind's rendered text); `Sources { spec, values, files, entropy }`; `encode` |
| `request.rs` | `bind_params` (by kind; an unknown name, a missing required one or an uncoercible value is `BadParam`; a `file` is its path as text), `build_url` (account values first, then the operation's; a substituted value is never scanned again; path segments percent-encoded, an absent optional query pair dropped, the rest form-encoded), `build_headers` (`RESERVED_HEADERS` refused), `build_body` (through `body::encode`), `build_request(spec, values, files, entropy)` — the body's kind sets `Content-Type`, a definition's own header wins except under `multipart` |
| `auth/` | one signer per scheme behind `AuthSigner { credential_for(&Stored), apply(&Credential, &mut Request, &SignContext) }`, chosen by `signer_for(&AuthSpec)` (`auth/mod.rs`; also `credential_for`, `apply` — which adds the `bisa` user agent — and `SignContext { now, account }`): `auth/api_key.rs` (a prefixed header or a query pair), `auth/bearer.rs` (`write_bearer`, shared), `auth/basic.rs` (`Basic base64(user:pass)`), `auth/oauth2.rs` (the flow's access token as a bearer), `auth/jwt.rs` (`JwtSigner` — header `alg`/`typ` plus the definition's, rendered against the account; the claims plus `iat` and `exp = iat + ttl`; `parse_pem` reads `PRIVATE KEY` (PKCS#8) and `RSA PRIVATE KEY` (PKCS#1) and refuses SEC1, encrypted and foreign blocks by name without quoting them; ES256 over `aws-lc-rs` `EcdsaKeyPair`, RS256 over `RsaKeyPair`; the token and the key both scrubbed) — a missing field is `NotAuthenticated` naming it, a credential of the wrong shape `BadDefinition` |
| `hosts.rs` | `check(&Url, &[String], insecure_tls)` — `https`, or `http` on loopback only; the host exact or under a `*.suffix` (the grammar is [`bisa-netrules`](netrules.md)'s, shared with the core's validation and the security rules); `insecure_tls` on loopback only; `HostJudge { judge(host) }` — the engine's allow and deny lists; `AllowAll` for tests |
| `breaker.rs` | `Breaker` — one circuit per host: `OPEN_AFTER` (5) consecutive failures (a transport error, a timeout, a 5xx — never a refusal) open it for `COOLDOWN_SECS` (30); `admit(host, now)` refuses while open with `ConnectorError::Open { host, until_secs }` and lets one probe through when the pause ends; `record(host, ok, now)` — a success closes, a failed probe re-opens; `is_open` |
| `retry.rs` | `RetryPolicy { attempts 3, base 500 ms, cap 8 s, retry_after_cap 30 s }`, `decide(...) -> Next::{Done, RetryAfter}` — 429 and 502–504 always, 500 and a transport error for a read only, `Retry-After` as seconds or a date, jittered backoff from `Entropy` |
| `http.rs` | `Request` (a `Debug` that hides credentials, `shown_url`), `Response`, `TransportError { Connect, Timeout, Body }`, `HttpTransport { send(&Request, insecure_loopback) }`, `ReqwestTransport` — over the engine's `bisa_http::Clients` (`with_http`; `new()` is the process's shared set): the `strict` client — no redirects, no cookies, a 10 s connect, the proxy and the HTTP version the person chose, swapped under every call when they change — and a second client, built once from the loopback builder, that accepts an unsigned certificate used for loopback `insecure_tls` only |
| `outcome.rs` | `Outcome { status, body, selected, pages }`, `parse(&Response, select)`, `next_cursor(body, path)` (a non-empty string or a number as its text; nothing else names a next page) — a 1 MiB cap, 401/403 → `NotAuthenticated`, 404 → `NotFound`, 429 → `RateLimited`, other 4xx → `Refused`, 5xx → `Upstream`, `explanation(body)` for the reason, `json_path` for `select` |
| `oauth.rs` | `authorize_url` (a 64-hex state, a 43-char verifier, the S256 challenge, the scopes, the `extra` pairs — the consent page's host judged first), `exchange` (the token endpoint's host judged first (the form body, `token_auth = "basic"` for a client that wants its id and secret in the header, `token_type` bearer, saved through `Credentials`), `refresh`, `ensure_fresh` (within `REFRESH_MARGIN_SECS` of expiry, single-flight per account with a re-load under the lock), a 401 forcing one refresh and one resend |
| `error.rs` | `ConnectorError` — `BadDefinition`, `BadParam`, `Unresolved`, `HostRefused`, `NotAuthenticated`, `NotFound`, `Refused`, `RateLimited`, `Upstream`, `Unreachable` (a connection never made — safe to send again), `Transport` (lost after it was made), `Timeout`, `Open` (the host's circuit is open), `SelectMissing`, `TooLarge`, `OAuth`, `Store` — with `is_refusal` and `scrub(text, secrets)` (a secret and its base64 form alike) |
| `client.rs` | `Client::new(transport, creds, clock, entropy)` (the retry policy is the default one — a builder for another had no caller and went), one refresh lock per account taken through `refresh_lock` (a lock nobody holds is let go on the next take, so the table is the accounts refreshing now), `call_with(spec, account, account_params, params, files, judge) -> Result<Outcome>` — bind, read every `file` parameter through `files` (a refused path or one past the cap is the parameter's `BadParam`), load the credential once, then per page: render and encode (the cursor the page before named as the cursor parameter), check the host, ask the judge, sign, send through the host's circuit (`send_judged`) with retry, a 401 under OAuth2 forcing one refresh and one resend, parse; the pages' lists joined, `Outcome.pages` counted; a write's `idempotency_key` in the named header on every attempt; scrub; `call(…)` is the same under `NoFiles`; `breaker()` |
| `lib.rs` | the re-exports |

---

## Entry points

The engine builds one `Client` at boot (`engine/connectors.rs`: `ReqwestTransport`, the workspace
keystore as `Credentials`, `SystemClock`, `OsEntropy`) and, per `connector` step, maps the core's
`Connector` and `Operation` into a `CallSpec` with `call_spec(...)`, resolves the account, renders
the step's parameters against the run, and calls `client.call_with(...)` under `PolicyHostJudge`
with `RunFiles` over where the run's work landed; a poll and an account check call `client.call(...)`,
which has no files. The OAuth functions are called by the engine's `oauth_start` and
`oauth_complete`. Nothing else calls the crate.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A parameter is bound by its kind and an unknown, missing or uncoercible one is refused before any request; account values render before the operation's and a substituted value is never scanned again; a path segment is percent-encoded and a query value form-encoded; an absent optional query pair is dropped; a JSON body leaf that is one typed placeholder becomes the typed value; a reserved header is refused | `tests/it/request.rs` and `template.rs` unit tests |
| A body reaches the wire as its kind says — form-encoded fields with the absent dropped, multipart parts under the client's boundary with a file's bytes and name, one parameter's bytes under a raw type — and the kind's `Content-Type` stands, a definition's header winning except under `multipart` | `body.rs` unit tests, `tests/it/request.rs` |
| A `file` parameter is read through the `Files` port before any request; a path the port refuses, a call with no checkout (`NoFiles`) or a file past `MAX_FILE_BYTES` is the parameter's `BadParam` and nothing is sent | `tests/it/request.rs` |
| Every scheme puts the credential where it belongs and nowhere else — a prefixed header, a query pair, `Bearer`, `Basic base64(user:pass)`, a token signed per request — and a missing field is `NotAuthenticated` with no request sent | `tests/it/auth.rs` |
| A `jwt` token carries `alg`, `typ` and the definition's header fields, the claims rendered against the account, `iat` from the clock and `exp = iat + ttl`; the platform's public key verifies it (ES256 and RS256 over keys generated in the test); a SEC1 key, an RSA key under ES256 or no key is refused by name, the key never quoted, nothing sent | `tests/it/auth.rs`, `auth/jwt.rs` unit tests |
| A host the definition does not declare is refused before any request; `*.suffix` matches subdomains only; `http` and `insecure_tls` are loopback-only; the judge can deny a declared host | `tests/it/hosts.rs` |
| A 429 is retried after `Retry-After`, capped; a 500 is retried for a read and not for a write; three attempts then `Upstream` | `tests/it/retry.rs` (paused time, a scripted transport) |
| A keyed write carries the same key in its header on every attempt and none when the caller gave none; a paged read follows the cursor the answer names, joins the pages' lists, stops at the cap or where no next cursor is named, and a page that selects no list is a bad definition; five failures open a host's circuit — the sixth call is `Open` without a request — and one probe after the pause closes it on success | `tests/it/hardening.rs` |
| A `jwt` token's `iat` sits `IAT_LEEWAY_SECS` (30) behind the clock, `exp` is the clock plus the life | `tests/it/auth.rs` |
| A denied host refuses the OAuth consent URL and the token endpoint before anything is sent or kept | `tests/it/oauth.rs` |
| The authorize URL carries the state, the S256 challenge, the scopes and the extras; the exchange posts the form and saves the tokens; a refresh happens only within sixty seconds of expiry and once under contention; a 401 forces one refresh and a resend; `invalid_grant` is `NotAuthenticated` | `tests/it/oauth.rs` |
| No error text ever contains the secret — host refusal, a 401 body echoing the key, a transport error with the key in the URL — and a `Secret` or a `Request` never prints it | `tests/it/error.rs`, `creds.rs` and `http.rs` unit tests |
| A dotted `select` walks the answer and a missing path is named; a non-JSON body is text; the size cap holds | `tests/it/outcome.rs` |

---

## Errors

`ConnectorError` as above. The engine wraps it as `EngineError::Connector` and delegates
`is_refusal`; a step's failure carries the scrubbed sentence through the redactor; the node renders
`NotAuthenticated` as **401**, `NotFound` as **404**, a definition, parameter or OAuth refusal as
**400**, `HostRefused` and `Refused` as **409**, `RateLimited` as **429**, `Upstream` and
`Transport` as **502**, `Timeout` as **504**.

---

## Extension points

| To add… | Touch, in order |
|---|---|
| an auth scheme | `AuthSpec` in `spec.rs` → a signer in `auth/<scheme>.rs` implementing `AuthSigner`, its arm in `signer_for` → a `Field` in `creds.rs` if the account holds something new → the core's `AuthScheme` (`fields`, `required`, `word`, its `validate` rules) → the engine's `auth_spec` → the desktop's `SECRET_FIELDS_BY_SCHEME` and labels → `guide/connectors.md` |
| a body kind | `CallBody` in `spec.rs` → an encoder in `body.rs` implementing `BodyEncoder`, its arm in `encoder_for` → the core's `OperationBody` and its `validate` rules and `templates()` sites → the engine's `body_of` → `guide/connectors.md` |
| a parameter kind | `ParamKind` in `spec.rs` → `bind_params` in `request.rs` (and the port it reads through, as `File` reads `Files`) → the core's `ParamKind` → the engine's `kind_of` → the connector form's field |
| a retry rule | `decide` in `retry.rs` → `tests/it/retry.rs` |
| a way a call holds up (a field of the operation) | `CallSpec` in `spec.rs` → its use in `client.rs::call_inner` or `request.rs` → the core's `Operation` and its `validate` rules → the engine's `call_spec` → the CLI's `show` → `guide/connectors.md` and [`recipes.md §21`](../../contributing/recipes.md#21-add-a-connector) |
| a built-in connector | a TOML under `library/catalog/connectors/` ([recipe](../../contributing/recipes.md#21-add-a-connector)) — this crate changes nothing |

---

## Tests

`tests/it/` is one binary: `support/mod.rs` (a loopback axum stub with canned JSON and text
bodies, form parsing and recorded requests; `MemoryCreds`, `MemoryFiles`, `FixedClock`,
`CountingEntropy`, `ScriptedTransport`), then `request.rs`, `auth.rs`, `hosts.rs`, `retry.rs`,
`oauth.rs`, `error.rs`, `outcome.rs`, `hardening.rs`; unit tests beside `breaker.rs`, `creds.rs`, `template.rs`, `body.rs`,
`hosts.rs`, `retry.rs`, `outcome.rs`, `error.rs`, `auth/mod.rs`, `auth/jwt.rs` and `oauth.rs`. No
test reaches a platform, a keychain or a network beyond the loopback stub; the signing tests
generate their keys as they run.

---

## What this crate refuses to do

- depend on any crate of ours beyond the two leaves `bisa-http` and `bisa-netrules`;
- log, store or return a credential — `Secret` cannot be printed and every error is scrubbed;
- touch the filesystem — a token to keep goes back through `Credentials`, a file to send comes in
  through `Files`;
- reach a host the definition does not declare, or one the judge denies;
- follow a redirect, keep a cookie, or run a shell.
