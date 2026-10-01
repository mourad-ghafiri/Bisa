//! The documented surface of the node, as data.
//!
//! Every module that mounts routes keeps a `ROUTES` table beside its
//! `routes()` fn; this module gathers them, in the order the reference reads
//! best, for two consumers: `--bin api-docs`, which renders
//! `docs/reference/http-api.md`, and the route test, which drives every
//! documented route through the running node and fails on one that is not
//! mounted. A route with a table entry and no `.route(...)` is a 404 in the
//! test; a route mounted without an entry is invisible here, and the reverse
//! cannot be asserted without router introspection — the reference says so.

/// One documented route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDoc {
    /// The HTTP method, upper-case.
    pub method: &'static str,
    /// The axum path, placeholders in braces (`/goals/{id}`).
    pub path: &'static str,
    /// One sentence: what it does, and the body or query it takes.
    pub summary: &'static str,
}

/// A section of the reference: a heading and the routes under it.
pub struct RouteSection {
    pub title: &'static str,
    pub routes: &'static [RouteDoc],
}

/// The routes mounted by `lib.rs` itself.
pub const ROUTES: &[RouteDoc] = &[
    RouteDoc {
        method: "GET",
        path: "/health",
        summary:
            "`{ok, version}`. Answers without the token — a liveness probe carries no authority.",
    },
    RouteDoc {
        method: "GET",
        path: "/events",
        summary:
            "Server-sent events: `{stream: \"engine\" | \"conversation\" | \"inbox\", payload}` frames — and `{stream: \"system\", payload: {kind: \"lagged\", dropped}}` when this client read too slowly and missed events: re-read what you show.",
    },
    RouteDoc {
        method: "GET",
        path: "/node",
        summary: "What this node is: `{version, pid, started_at, socket, listen?, data_dir, logs_dir, paused, live_sessions}` — the process, where it listens, where the workspace lives, whether the engine is paused and how many sessions are live.",
    },
    RouteDoc {
        method: "GET",
        path: "/workspace",
        summary: "This node's identity, data and logs directories, and its members (the owner first). Relays are the `sync.relays` setting; the wire is `GET /sync`.",
    },
];

/// Every documented route, grouped for the reference.
pub fn sections() -> Vec<RouteSection> {
    #[allow(unused_mut)] // the `a2a` feature pushes one more
    let mut out = vec![
        RouteSection {
            title: "Node",
            routes: ROUTES,
        },
        RouteSection {
            title: "Goals",
            routes: crate::goals::ROUTES,
        },
        RouteSection {
            title: "Workflows",
            routes: crate::workflows::ROUTES,
        },
        RouteSection {
            title: "Runs",
            routes: crate::runs::ROUTES,
        },
        RouteSection {
            title: "Work items",
            routes: crate::work_items::ROUTES,
        },
        RouteSection {
            title: "Projects and workstreams",
            routes: crate::projects::ROUTES,
        },
        RouteSection {
            title: "Channels and direct channels",
            routes: crate::conversation::ROUTES,
        },
        RouteSection {
            title: "Conversations",
            routes: crate::conversations::ROUTES,
        },
        RouteSection {
            title: "Reviewing an agent's changes",
            routes: crate::changes::ROUTES,
        },
        RouteSection {
            title: "Inbox",
            routes: crate::inbox::ROUTES,
        },
        RouteSection {
            title: "Collaboration: people, invitations, hosts and the wire",
            routes: crate::collab::ROUTES,
        },
        RouteSection {
            title: "Pulse",
            routes: crate::pulse::ROUTES,
        },
        RouteSection {
            title: "Agents, teams and sessions",
            routes: crate::agents_api::ROUTES,
        },
        RouteSection {
            title: "Skills",
            routes: crate::skills::ROUTES,
        },
        RouteSection {
            title: "MCP servers",
            routes: crate::mcp::ROUTES,
        },
        RouteSection {
            title: "Catalog",
            routes: crate::catalog::ROUTES,
        },
        RouteSection {
            title: "Notes",
            routes: crate::notes::ROUTES,
        },
        RouteSection {
            title: "Drawings",
            routes: crate::drawings::ROUTES,
        },
        RouteSection {
            title: "Files",
            routes: crate::files::ROUTES,
        },
        RouteSection {
            title: "The IDE: files and the watcher",
            routes: crate::ide::ROUTES,
        },
        RouteSection {
            title: "The IDE: review notes",
            routes: crate::review::ROUTES,
        },
        RouteSection {
            title: "The IDE: branches, tags, remotes and the consented tier",
            routes: crate::ide::interactive::ROUTES,
        },
        RouteSection {
            title: "The IDE: pull requests and the code host",
            routes: crate::codehost::ROUTES,
        },
        RouteSection {
            title: "Connectors",
            routes: crate::connectors::ROUTES,
        },
        RouteSection {
            title: "The IDE: language servers",
            routes: crate::ide::lsp::ROUTES,
        },
        RouteSection {
            title: "The IDE: served folders and the browser",
            routes: crate::ide::serve::ROUTES,
        },
        RouteSection {
            title: "The IDE: the browser bridge",
            routes: crate::browser::ROUTES,
        },
        RouteSection {
            title: "Mobile Development: the toolchain, the devices and the captures",
            routes: crate::mobile_development::ROUTES,
        },
        RouteSection {
            title: "Attachments",
            routes: crate::attachments::ROUTES,
        },
        RouteSection {
            title: "Listening, listeners and signals",
            routes: crate::listening::ROUTES,
        },
        RouteSection {
            title: "Public hooks",
            routes: crate::hooks::ROUTES,
        },
        RouteSection {
            title: "Tags",
            routes: crate::tags::ROUTES,
        },
        RouteSection {
            title: "Usage",
            routes: crate::usage::ROUTES,
        },
        RouteSection {
            title: "Pets",
            routes: crate::pets::ROUTES,
        },
        RouteSection {
            title: "Addons",
            routes: crate::addons::ROUTES,
        },
        RouteSection {
            title: "Workspace and administration",
            routes: crate::admin::ROUTES,
        },
        RouteSection {
            title: "Settings",
            routes: crate::settings::ROUTES,
        },
        RouteSection {
            title: "Logging",
            routes: crate::logs::ROUTES,
        },
        RouteSection {
            title: "Network",
            routes: crate::network::ROUTES,
        },
        RouteSection {
            title: "Security",
            routes: crate::security::ROUTES,
        },
        RouteSection {
            title: "Decisions",
            routes: crate::decisions::ROUTES,
        },
        RouteSection {
            title: "Readiness",
            routes: crate::readiness::ROUTES,
        },
        RouteSection {
            title: "Git config and profiles",
            routes: crate::git_config::ROUTES,
        },
        RouteSection {
            title: "SSH for git hosts",
            routes: crate::ssh::ROUTES,
        },
    ];
    #[cfg(feature = "a2a")]
    out.push(RouteSection {
        title: "Agent-to-agent (feature `a2a`)",
        routes: crate::a2a::ROUTES,
    });
    out
}

/// Every documented route, flat.
pub fn all() -> Vec<RouteDoc> {
    sections()
        .iter()
        .flat_map(|s| s.routes.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_route_is_documented_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for r in all() {
            assert!(
                seen.insert((r.method, r.path)),
                "{} {} is documented twice",
                r.method,
                r.path
            );
        }
    }

    #[test]
    fn every_summary_is_one_sentence_that_ends() {
        for r in all() {
            assert!(
                !r.summary.is_empty(),
                "{} {} has no summary",
                r.method,
                r.path
            );
            assert!(
                r.summary.ends_with('.') || r.summary.ends_with('`'),
                "{} {}: summaries end with a full stop",
                r.method,
                r.path
            );
        }
    }
}
