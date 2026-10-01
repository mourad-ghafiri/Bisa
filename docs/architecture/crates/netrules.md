# bisa-netrules

The rules three crates used to spell for themselves: what a **host pattern** is and when it names a
host, which authorities are **loopback**, and the **headers** a connector definition may never set.
A connector definition declares the hosts it may reach (`bisa-core` validates them), the client
refuses a resolved URL whose host none of them names (`bisa-connectors`), and the person's
`security.net.*` lists are judged by the same words (`bisa-security`). Three hand-written copies of
one grammar drifted in the small — one accepted an upper-case letter in a pattern, one let a
wildcard ignore the port — which is exactly the kind of disagreement a host check must not have.
This crate is the grammar, once; the three depend on it and spell nothing of their own. A leaf like
[`bisa-http`](http.md): no Bisa dependency, no I/O — one module, `lib.rs`, and its tests.

---

## The surface

| Item | Says |
|---|---|
| `is_host_pattern(pattern)` | `host[:port]` — letters, digits, dots and hyphens, no leading dot or hyphen, no `..`, a port of one to five digits — or the same behind `*.`; case is not the pattern's business |
| `host_matches(pattern, authority)` | an exact pattern matches itself only (`api.slack.com` is not `api.slack.com:443`); `*.suffix` matches every subdomain of `suffix` with the same port and never `suffix` itself; both sides lower-cased and trimmed |
| `is_loopback(authority)` | `127.0.0.1`, `::1` (bracketed or not) or `localhost`, in any case, with or without a port |
| `split_port(authority)` | the host and its port, when the authority names one |
| `RESERVED_HEADERS`, `is_reserved_header(name)` | `authorization`, `host`, `content-length`, `cookie`, `transfer-encoding` — the client's, whatever the case a definition writes them in |

## Tests

`tests/it/hosts.rs` — the cases the three copies each held, together: the pattern grammar in both
cases, exact and wildcard matching with ports, the loopback names, the reserved headers.
