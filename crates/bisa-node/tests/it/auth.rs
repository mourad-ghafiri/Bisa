//! The bearer's exceptions, as one table against the route list: every
//! token-less route is one the `auth` module's doc names — a liveness probe,
//! a public hook with its listener's own secret, the OAuth callback a browser
//! lands on, the A2A card and endpoint (their own protocol), a session's own
//! doors under its own secret, an addon's files for a frame that holds no
//! token — and nothing else. A route added without a word here needs the
//! token: a hook called on this machine among them.

use bisa_node::auth::is_exempt;
use bisa_node::route_docs;

#[test]
fn the_token_less_routes_are_exactly_the_named_exceptions() {
    for path in [
        "/health",
        "/connectors/oauth/callback",
        "/hooks/workspace:01J0000000000000000000000A/ticket",
        "/hooks/goal:01J0000000000000000000000A/ticket",
        "/.well-known/agent-card.json",
        "/a2a",
        "/a2a/anything",
        "/sessions/01J000000000000000000000SESS/report",
        "/sessions/01J000000000000000000000SESS/guard",
        "/sessions/01J000000000000000000000SESS/exit",
        "/sessions/01J000000000000000000000SESS/close",
        "/addons/bisa-clock/files/index.html",
        "/addons/bisa-clock/files/js/app.js",
    ] {
        assert!(is_exempt(path), "{path} is a named exception");
    }
    for path in [
        "/healthz",
        "/hooks",
        "/workflows/01J0000000000000000000000A/hooks/ticket",
        "/workflows/01J0000000000000000000000A/hooks/ticket/secret",
        "/goals/01J0000000000000000000000A/hooks/ticket",
        "/goals/01J0000000000000000000000A/listening",
        "/listeners",
        "/signals",
        "/signals/01J0000000000000000000000A/release",
        "/a2ax",
        "/sessions/01J000000000000000000000SESS",
        "/sessions/01J000000000000000000000SESS/abort",
        "/sessions//report",
        "/addons/bisa-clock",
        "/addons/bisa-clock/files/",
        "/addons//files/index.html",
        "/addons/a/b/files/x",
        "/goals",
        "/events",
    ] {
        assert!(!is_exempt(path), "{path} takes the token");
    }
}

#[test]
fn every_documented_route_takes_the_token_unless_it_is_a_named_exception() {
    // The route tables are templates (`/goals/{id}`); a template is exempt
    // only when its literal prefix is one of the exceptions.
    let exempt_templates = [
        "/health",
        "/connectors/oauth/callback",
        "/hooks/{host}/{step}",
        "/.well-known/agent-card.json",
        "/a2a",
        "/sessions/{id}/report",
        "/sessions/{id}/guard",
        "/sessions/{id}/exit",
        "/sessions/{id}/close",
        "/addons/{id}/files/{*path}",
    ];
    let mut surprises = Vec::new();
    for r in route_docs::all() {
        // Fill the template with a plausible id so `is_exempt` judges a path.
        let filled = r
            .path
            .replace("{host}", "workspace:01J0000000000000000000000A")
            .replace("{step}", "ticket")
            .replace("{id}", "01J000000000000000000000ROWS")
            .replace("{*path}", "index.html");
        let exempt = is_exempt(&filled);
        let named = exempt_templates.contains(&r.path);
        if exempt != named {
            surprises.push(format!(
                "{} {} (exempt={exempt}, named={named})",
                r.method, r.path
            ));
        }
    }
    assert!(
        surprises.is_empty(),
        "routes whose token rule is not the documented one:\n{}",
        surprises.join("\n")
    );
}
