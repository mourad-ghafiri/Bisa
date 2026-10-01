//! The control-plane bearer token.
//!
//! Every route on both listeners requires `Authorization: Bearer <token>`,
//! or `?token=<token>` for the two things a browser cannot send a header
//! with — an `EventSource` and an `<img>`. The named exceptions
//! ([`is_exempt`]) are `/health` (a liveness probe carries no authority);
//! `/hooks/{host}/{step}` (a public hook, opened by its listener's own
//! secret — a hook called on this machine, `/workflows/{wfid}/hooks/{step}`
//! and `/goals/{id}/hooks/{step}`, takes the token like everything else);
//! the OAuth callback a browser lands on (opened by the one-time `state` of
//! the sign-in it ends); the A2A card and endpoint (their own protocol); an
//! installed addon's files ([`is_addon_file`]); and a terminal session's
//! four doors — `report`, `guard`, `exit`, `close` — which take the
//! session's own secret in the token's place ([`is_session_door`]).
//!
//! The token is 32 random bytes as 64 hex characters in `run/token`, created
//! `0600` and reused across restarts so the CLI keeps working; a
//! `NodeConfig::token` (the desktop hands its sidecar one through
//! `BISA_API_TOKEN`) is written there too, so every client on the
//! machine reads one place.
//!
//! Who this stops: a browser tab on `127.0.0.1`, a container with host
//! networking, a second user on the box. Who it does not: anything that can
//! already read the workspace directory — which is why the terminal never
//! became a node route.

use crate::Shared;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bisa_store::Paths;
use std::io::Write;

/// Read the workspace's token, or mint one. `configured` wins and is
/// recorded, so a token handed in from outside is the one the file says.
pub fn ensure_token(paths: &Paths, configured: Option<&str>) -> std::io::Result<String> {
    let path = paths.token_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if let Some(given) = configured {
        let given = given.trim().to_string();
        if given.is_empty() {
            // for the log: a startup fault
            return Err(std::io::Error::other("the configured API token is empty"));
        }
        write_private(&path, &given, false)?;
        return Ok(given);
    }
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let existing = existing.trim();
        if !existing.is_empty() {
            return Ok(existing.to_string());
        }
    }
    let minted = mint();
    match write_private(&path, &minted, true) {
        Ok(()) => Ok(minted),
        // Another process minted first: its token is the workspace's.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read_to_string(&path)?;
            let existing = existing.trim();
            if existing.is_empty() {
                return Err(std::io::Error::other("the token file is empty")); // for the log: a startup fault
            }
            Ok(existing.to_string())
        }
        Err(e) => Err(e),
    }
}

/// 32 random bytes, hex. The key generator is the one source of randomness
/// this crate already trusts for identities.
fn mint() -> String {
    let keys = nostr::key::Keys::generate();
    keys.secret_key().to_secret_hex()
}

/// Write the token at mode `0600`. A minted token is created exclusively
/// (`O_EXCL`), so two nodes racing to mint never both believe theirs is the
/// file's; a configured token replaces whatever is there, because it is the
/// one the operator named.
fn write_private(path: &std::path::Path, token: &str, exclusive: bool) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(token.as_bytes())?;
    file.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Routes that answer without the token, and why each is safe to.
pub fn is_exempt(path: &str) -> bool {
    path == "/health"
        || path == crate::connectors::OAUTH_CALLBACK_PATH
        || path.starts_with("/hooks/")
        || path == "/.well-known/agent-card.json"
        || path == "/a2a"
        || path.starts_with("/a2a/")
        || is_session_door(path)
        || is_addon_file(path)
}

/// `GET /addons/{id}/files/<path>` — the static files of an installed bundle,
/// for the sandboxed frame that runs them and holds no token by design (18 —
/// Addons). What that opens is a bundle a person installed and nothing else:
/// GET only, contained by `resolve_within`, typed from an allowlist, fenced by
/// a Content-Security-Policy, and a bare 404 for an addon that is not active.
pub fn is_addon_file(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/addons/") else {
        return false;
    };
    matches!(rest.split_once("/files/"), Some((id, file)) if !id.is_empty() && !id.contains('/') && !file.is_empty())
}

/// The doors of a session a person opened in a terminal, each presenting the
/// session's secret instead of the control-plane token: `report` and `guard`
/// for the hooks inside the terminal, `exit` and `close` for the PTY host
/// that runs it. The secret is a capability for one roster row and nothing
/// else; the handler checks it, and the token is never in a PTY.
///
/// One list, read here and nowhere else: a door the hooks call that is
/// missing from it is answered `401` before its handler sees the secret —
/// the guard's was, and no terminal harness was ever judged.
pub const SESSION_DOORS: [&str; 4] = ["report", "guard", "exit", "close"];

/// `POST /sessions/{id}/<door>` for one of the [`SESSION_DOORS`].
pub fn is_session_door(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/sessions/") else {
        return false;
    };
    matches!(rest.split_once('/'), Some((id, door)) if !id.is_empty() && SESSION_DOORS.contains(&door))
}

fn presented(req: &Request) -> Option<String> {
    presented_in(req.headers(), req.uri())
}

/// The token a request presents, from the header or the query — shared with
/// `ide::consent`, which re-reads it to mint a `HumanConsent`.
pub(crate) fn presented_in(
    headers: &axum::http::HeaderMap,
    uri: &axum::http::Uri,
) -> Option<String> {
    if headers.contains_key(header::AUTHORIZATION) {
        return presented_in_header(headers);
    }
    let query = uri.query()?;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("token=") {
            return Some(value.to_string());
        }
    }
    None
}

/// The token in the `Authorization` header alone — what a consented git
/// operation is minted from (`ide::consent`); the query is never enough.
pub(crate) fn presented_in_header(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    Some(token.trim().to_string())
}

/// Constant-time equality, so a wrong token costs the same whatever prefix
/// it got right.
pub(crate) fn same(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

pub(crate) async fn require_token(
    State(state): State<Shared>,
    req: Request,
    next: Next,
) -> Response {
    if is_exempt(req.uri().path()) {
        return next.run(req).await;
    }
    match presented(&req) {
        Some(token) if same(&token, &state.token) => next.run(req).await,
        _ => {
            // The same body every refusal has, in the request's language
            // (`i18n::localize` wraps this check), with the challenge header.
            let mut response = crate::ApiError::text(
                StatusCode::UNAUTHORIZED,
                bisa_core::text!("error-node-unauthorized"),
            )
            .into_response();
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                "Bearer".parse().expect("a header value"),
            );
            response
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_file_answers_open_and_every_other_addon_route_is_closed() {
        for open in [
            "/addons/clock/files/index.html",
            "/addons/acme.byte/files/img/a.png",
            "/addons/clock/files/bisa-addon.js",
        ] {
            assert!(is_exempt(open), "{open}");
        }
        for closed in [
            "/addons",
            "/addons/clock",
            "/addons/clock/fetch",
            "/addons/clock/files/",
            "/addons//files/x",
            "/addons/validate",
            "/addons/a/b/files/x",
        ] {
            assert!(!is_exempt(closed), "{closed}");
        }
    }

    #[test]
    fn a_consented_verb_reads_the_header_and_never_the_query() {
        let mut headers = axum::http::HeaderMap::new();
        let uri: axum::http::Uri = "/workstreams/w/git/checkout?token=abc".parse().unwrap();
        assert_eq!(
            presented_in(&headers, &uri).as_deref(),
            Some("abc"),
            "the middleware takes the query — an EventSource has no header"
        );
        assert_eq!(presented_in_header(&headers), None, "consent does not");
        headers.insert(header::AUTHORIZATION, "Bearer abc".parse().unwrap());
        assert_eq!(presented_in_header(&headers).as_deref(), Some("abc"));
        assert_eq!(presented_in(&headers, &uri).as_deref(), Some("abc"));
    }

    #[test]
    fn a_minted_token_is_64_hex_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let first = ensure_token(&paths, None).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        let second = ensure_token(&paths, None).unwrap();
        assert_eq!(
            first, second,
            "a restart keeps the token the CLI already read"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(paths.token_file())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn a_configured_token_is_recorded_for_the_cli() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let got = ensure_token(&paths, Some("from-the-desktop")).unwrap();
        assert_eq!(got, "from-the-desktop");
        assert_eq!(
            std::fs::read_to_string(paths.token_file()).unwrap(),
            "from-the-desktop"
        );
        assert!(ensure_token(&paths, Some("  ")).is_err());
    }

    #[test]
    fn the_exemptions_are_exactly_the_named_ones() {
        for open in [
            "/health",
            "/hooks/workspace:01J0000000000000000000000A/ticket",
            "/hooks/goal:01J0000000000000000000000A/ticket",
            "/.well-known/agent-card.json",
            "/a2a",
            "/connectors/oauth/callback",
            "/sessions/01J000000000000000000000SESS/report",
            "/sessions/01J000000000000000000000SESS/guard",
            "/sessions/01J000000000000000000000SESS/exit",
            "/sessions/01J000000000000000000000SESS/close",
        ] {
            assert!(is_exempt(open), "{open}");
        }
        for closed in [
            "/",
            "/goals",
            "/connectors",
            "/connectors/slack/accounts",
            "/healthz",
            "/hooks",
            // A hook called on this machine takes the token; so does all
            // else about listening.
            "/workflows/01J0000000000000000000000A/hooks/ticket",
            "/workflows/01J0000000000000000000000A/hooks/ticket/secret",
            "/goals/01J0000000000000000000000A/hooks/ticket",
            "/workflows/01J0000000000000000000000A/listening",
            "/listeners",
            "/signals",
            "/events",
            "/workspace",
            "/a2ax",
            // A session's other routes are the control plane's.
            "/sessions",
            "/sessions/terminal",
            "/sessions/01J000000000000000000000SESS",
            "/sessions/01J000000000000000000000SESS/abort",
            "/sessions/01J000000000000000000000SESS/guard/more",
            "/sessions//guard",
        ] {
            assert!(!is_exempt(closed), "{closed}");
        }
    }

    #[test]
    fn comparison_is_exact() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert!(!same("", "a"));
    }
}
