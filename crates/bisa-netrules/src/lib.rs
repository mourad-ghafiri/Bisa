//! The rules three crates used to spell for themselves: what a **host
//! pattern** is and when it names a host, which authorities are **loopback**,
//! and the **headers** a connector definition may never set.
//!
//! A connector definition declares the hosts it may reach (`bisa-core`
//! validates them), the client refuses a resolved URL whose host none of
//! them names (`bisa-connectors`), and the person's `security.net.*` lists
//! are judged by the same words (`bisa-security`). Three hand-written copies
//! of one grammar drifted in the small — one accepted an upper-case letter in
//! a pattern, one let a wildcard ignore the port — which is exactly the kind
//! of disagreement a host check must not have. This crate is the grammar,
//! once; the three depend on it and spell nothing of their own.
//!
//! Pure: no I/O, no Bisa dependency, so the leaf crates stay leaves.

/// Headers a definition may not set: the client owns them — the credential,
/// the framing, the host — and a definition that sets one is either confused
/// or trying to smuggle a credential. Lower-case; compare lower-cased.
pub const RESERVED_HEADERS: [&str; 5] = [
    "authorization",
    "host",
    "content-length",
    "cookie",
    "transfer-encoding",
];

/// Whether `name` is a header a definition may not set, whatever its case.
pub fn is_reserved_header(name: &str) -> bool {
    RESERVED_HEADERS.contains(&name.trim().to_ascii_lowercase().as_str())
}

/// The host of an authority: a bracketed IPv6 address unbracketed, a port
/// split off.
fn host_of(authority: &str) -> &str {
    authority
        .strip_prefix('[')
        .and_then(|h| h.split_once(']').map(|(h, _)| h))
        .unwrap_or_else(|| {
            // A bare IPv6 address carries colons of its own and no port:
            // `::1` is a host, not the host `:` with the port `1`.
            if authority.matches(':').count() >= 2 {
                return authority;
            }
            authority
                .rsplit_once(':')
                .filter(|(_, p)| is_port(p))
                .map_or(authority, |(h, _)| h)
        })
}

/// This machine, by name or by address, with or without a port, in any
/// case: `localhost` (a trailing dot tolerated) and `*.localhost`; every
/// loopback address (`127.0.0.1` and the whole of `127/8`, `::1`), the
/// unspecified ones (`0.0.0.0`, `::`), and an IPv4-mapped IPv6 form of
/// either (`::ffff:127.0.0.1`) — read as addresses, never as strings, so an
/// alias does not slip past where a name would be refused. The names an
/// `http` URL or a self-signed certificate may use.
pub fn is_loopback(authority: &str) -> bool {
    let host = host_of(authority);
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(a)) => a.is_loopback() || a.is_unspecified(),
        Ok(std::net::IpAddr::V6(a)) => {
            a.is_loopback()
                || a.is_unspecified()
                || a.to_ipv4_mapped()
                    .is_some_and(|v| v.is_loopback() || v.is_unspecified())
        }
        Err(_) => false,
    }
}

/// A private or link-local address — a LAN (`10/8`, `172.16/12`,
/// `192.168/16`, `fc00::/7`), a link (`169.254/16`, where a cloud's metadata
/// service answers; `fe80::/10`), or an IPv4-mapped IPv6 form of one — read
/// as an address. A name that resolves to one is not read here: that is the
/// resolver's, at the moment of the call.
pub fn is_private_network(authority: &str) -> bool {
    match host_of(authority).parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(a)) => a.is_private() || a.is_link_local(),
        Ok(std::net::IpAddr::V6(a)) => {
            a.is_unique_local()
                || a.is_unicast_link_local()
                || a.to_ipv4_mapped()
                    .is_some_and(|v| v.is_private() || v.is_link_local())
        }
        Err(_) => false,
    }
}

/// `host[:port]` — letters, digits, dots and hyphens, an optional port of up
/// to five digits — or the same behind `*.`. Case is not the pattern's
/// business: a match compares lower-cased.
pub fn is_host_pattern(pattern: &str) -> bool {
    let bare = pattern.strip_prefix("*.").unwrap_or(pattern);
    let host = match bare.rsplit_once(':') {
        Some((h, p)) => {
            if !is_port(p) || p.len() > 5 {
                return false;
            }
            h
        }
        None => bare,
    };
    !host.is_empty()
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

/// Does `pattern` name `authority`? An exact `host[:port]` matches itself
/// only — `api.slack.com` is not `api.slack.com:443`; a `*.suffix` matches
/// every subdomain of `suffix` with the same port and never `suffix`
/// itself. Both sides are compared lower-cased.
pub fn host_matches(pattern: &str, authority: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let authority = authority.trim().to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(suffix) => {
            let (host, port) = split_port(&authority);
            let (suffix_host, suffix_port) = split_port(suffix);
            port == suffix_port
                && host.len() > suffix_host.len()
                && host.ends_with(suffix_host)
                && host[..host.len() - suffix_host.len()].ends_with('.')
        }
        None => pattern == authority,
    }
}

/// `host` and its port, when the authority names one.
pub fn split_port(authority: &str) -> (&str, Option<&str>) {
    match authority.rsplit_once(':') {
        Some((h, p)) if is_port(p) => (h, Some(p)),
        _ => (authority, None),
    }
}

fn is_port(p: &str) -> bool {
    !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())
}
