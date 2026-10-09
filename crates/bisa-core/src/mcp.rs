//! MCP servers: a local registry entry, and the transport a harness connects
//! to. Local state with no GEP kind — a stdio transport names a command on one
//! disk, so what travels is the id.

use crate::id::McpId;
use crate::tags::Tags;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The one name an agent's own MCP list may never take: the engine injects the
/// platform's server into every session, and a second server answering to that
/// name would shadow the tools the platform runs on.
pub const RESERVED_MCP_NAME: &str = "bisa";

/// What a secret reads as on the way out: every `env` and `headers` value a
/// registry entry is answered with (`McpServerConfig::masked`) — keys stay,
/// values never leave the machine's truth file. A value sent back as exactly
/// this keeps the stored one (`unmasked_from`), so an edit that touches
/// nothing keeps everything.
pub const MASK: &str = "••••••";

/// How a harness reaches an MCP server — the three transports the protocol
/// has had (modelcontextprotocol.io/specification): a process on this
/// machine over stdio; **Streamable HTTP** (2025-03-26 onwards: one
/// endpoint, POST for requests and an optional GET event stream); and the
/// 2024-11-05 **HTTP+SSE** transport (GET opens the stream, its first event
/// names where to POST), deprecated by the spec and still what a minority
/// of servers speak. Reused by the harness crate verbatim, so nothing is
/// translated between the registry and a launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "transport")]
pub enum McpServerConfig {
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        /// Where the process starts; absent, the session's own directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Http {
        name: String,
        url: String,
        /// Sent with every request — a bearer token, an API key.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
    },
    Sse {
        name: String,
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
    },
}

impl McpServerConfig {
    pub fn name(&self) -> &str {
        match self {
            McpServerConfig::Stdio { name, .. }
            | McpServerConfig::Http { name, .. }
            | McpServerConfig::Sse { name, .. } => name,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            McpServerConfig::Stdio { .. } => "stdio",
            McpServerConfig::Http { .. } => "http",
            McpServerConfig::Sse { .. } => "sse",
        }
    }

    /// The URL a remote transport answers at; a process has none.
    pub fn url(&self) -> Option<&str> {
        match self {
            McpServerConfig::Stdio { .. } => None,
            McpServerConfig::Http { url, .. } | McpServerConfig::Sse { url, .. } => Some(url),
        }
    }

    /// The headers a remote transport sends; a process has none.
    pub fn headers(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            McpServerConfig::Stdio { .. } => None,
            McpServerConfig::Http { headers, .. } | McpServerConfig::Sse { headers, .. } => {
                Some(headers)
            }
        }
    }

    /// The same transport with the same name put on it — an edit renames
    /// through the transport, since the name lives there.
    pub fn renamed(self, name: String) -> Self {
        match self {
            McpServerConfig::Stdio {
                command,
                args,
                env,
                cwd,
                ..
            } => McpServerConfig::Stdio {
                name,
                command,
                args,
                env,
                cwd,
            },
            McpServerConfig::Http { url, headers, .. } => {
                McpServerConfig::Http { name, url, headers }
            }
            McpServerConfig::Sse { url, headers, .. } => {
                McpServerConfig::Sse { name, url, headers }
            }
        }
    }

    /// Every secret-bearing value — `env` on a process, `headers` on a
    /// remote — read as [`MASK`], keys kept: what leaves the machine over
    /// the wire and the terminal. The truth file keeps the values; a harness
    /// launch reads them there.
    pub fn masked(&self) -> Self {
        let mask = |m: &BTreeMap<String, String>| {
            m.keys()
                .map(|k| (k.clone(), MASK.to_string()))
                .collect::<BTreeMap<_, _>>()
        };
        match self {
            McpServerConfig::Stdio {
                name,
                command,
                args,
                env,
                cwd,
            } => McpServerConfig::Stdio {
                name: name.clone(),
                command: command.clone(),
                args: args.clone(),
                env: mask(env),
                cwd: cwd.clone(),
            },
            McpServerConfig::Http { name, url, headers } => McpServerConfig::Http {
                name: name.clone(),
                url: url.clone(),
                headers: mask(headers),
            },
            McpServerConfig::Sse { name, url, headers } => McpServerConfig::Sse {
                name: name.clone(),
                url: url.clone(),
                headers: mask(headers),
            },
        }
    }

    /// The incoming transport with every value that came back as [`MASK`]
    /// replaced by what `stored` holds under the same key — the edit's rule:
    /// a value not touched is a value kept. A masked key the stored
    /// transport never had is dropped: there is nothing it could mean.
    /// Kinds that differ take the incoming one whole, masks dropped.
    pub fn unmasked_from(stored: &Self, incoming: Self) -> Self {
        let fill = |from: Option<&BTreeMap<String, String>>, mut into: BTreeMap<String, String>| {
            into.retain(|k, v| v != MASK || from.is_some_and(|f| f.contains_key(k)));
            for (k, v) in into.iter_mut() {
                if v == MASK {
                    if let Some(kept) = from.and_then(|f| f.get(k)) {
                        *v = kept.clone();
                    }
                }
            }
            into
        };
        match (stored, incoming) {
            (
                McpServerConfig::Stdio { env: from, .. },
                McpServerConfig::Stdio {
                    name,
                    command,
                    args,
                    env,
                    cwd,
                },
            ) => McpServerConfig::Stdio {
                name,
                command,
                args,
                env: fill(Some(from), env),
                cwd,
            },
            (
                McpServerConfig::Http { headers: from, .. }
                | McpServerConfig::Sse { headers: from, .. },
                McpServerConfig::Http { name, url, headers },
            ) => McpServerConfig::Http {
                name,
                url,
                headers: fill(Some(from), headers),
            },
            (
                McpServerConfig::Http { headers: from, .. }
                | McpServerConfig::Sse { headers: from, .. },
                McpServerConfig::Sse { name, url, headers },
            ) => McpServerConfig::Sse {
                name,
                url,
                headers: fill(Some(from), headers),
            },
            (
                _,
                McpServerConfig::Stdio {
                    name,
                    command,
                    args,
                    env,
                    cwd,
                },
            ) => McpServerConfig::Stdio {
                name,
                command,
                args,
                env: fill(None, env),
                cwd,
            },
            (_, McpServerConfig::Http { name, url, headers }) => McpServerConfig::Http {
                name,
                url,
                headers: fill(None, headers),
            },
            (_, McpServerConfig::Sse { name, url, headers }) => McpServerConfig::Sse {
                name,
                url,
                headers: fill(None, headers),
            },
        }
    }

    /// The transport's own shape: a command to run, an absolute `cwd`, an
    /// absolute http(s) URL, header names that are tokens and header values
    /// with no control character (a `\r` in a value is a request-smuggling
    /// primitive, not a typo).
    pub fn validate(&self) -> Result<(), McpError> {
        match self {
            McpServerConfig::Stdio { command, cwd, .. } => {
                if command.trim().is_empty() {
                    return Err(McpError::EmptyCommand);
                }
                if let Some(dir) = cwd {
                    if !std::path::Path::new(dir).is_absolute() {
                        return Err(McpError::RelativeCwd(dir.clone()));
                    }
                }
                Ok(())
            }
            McpServerConfig::Http { url, headers, .. }
            | McpServerConfig::Sse { url, headers, .. } => {
                if !is_http_url(url) {
                    return Err(McpError::InvalidUrl(url.clone()));
                }
                for (k, v) in headers {
                    if !is_header_name(k) {
                        return Err(McpError::BadHeaderName(k.clone()));
                    }
                    if v.chars().any(|c| c.is_control()) {
                        return Err(McpError::BadHeaderValue(k.clone()));
                    }
                }
                Ok(())
            }
        }
    }
}

/// An absolute `http://` or `https://` URL with a host — what a remote
/// transport must be; a bare host, a path or another scheme is not.
fn is_http_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    !host.is_empty() && !host.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// An HTTP field name: one or more token characters (RFC 9110 §5.6.2).
fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

/// Where a server mounted on a session came from — the one fact that decides
/// whether a harness may call its tools without a judgement. The platform's
/// own server (`bisa`) is trusted by construction: the engine builds it and
/// judges every one of its tools at the intake. A server a person installed
/// on an agent is not: its tools reach the machine and the outside world, so
/// they go through the Tool & Commands Guard like a shell command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpProvenance {
    /// The engine injected it: the platform's own tools.
    Platform,
    /// An agent definition names it, from the workspace's MCP registry.
    Installed,
}

/// One MCP server as a session is handed it: the transport, and where it
/// came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpMount {
    pub config: McpServerConfig,
    pub provenance: McpProvenance,
}

impl McpMount {
    pub fn platform(config: McpServerConfig) -> Self {
        Self {
            config,
            provenance: McpProvenance::Platform,
        }
    }

    pub fn installed(config: McpServerConfig) -> Self {
        Self {
            config,
            provenance: McpProvenance::Installed,
        }
    }

    pub fn name(&self) -> &str {
        self.config.name()
    }

    /// Whether a harness may call this server's tools without a judgement.
    pub fn is_platform(&self) -> bool {
        self.provenance == McpProvenance::Platform
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    /// Slug, stable, and what agents reference.
    pub id: McpId,
    /// The name the harness sees. Distinct from `id`: two entries can front the
    /// same server name on different transports.
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Tags::is_empty")]
    pub tags: Tags,
    pub transport: McpServerConfig,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: u64,
}

fn default_true() -> bool {
    true
}

impl McpServer {
    /// The reserved name is refused here, once, at the door — and again at
    /// launch, so a hand-edited truth file cannot smuggle it through.
    pub fn validate(&self) -> Result<(), McpError> {
        if self.name.trim().is_empty() {
            return Err(McpError::EmptyName);
        }
        if self.name == RESERVED_MCP_NAME || self.transport.name() == RESERVED_MCP_NAME {
            return Err(McpError::ReservedName);
        }
        self.transport.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum McpError {
    #[error("an MCP server needs a name")]
    EmptyName,
    #[error(
        "`{RESERVED_MCP_NAME}` is reserved: it is the platform's own MCP server name and cannot be registered"
    )]
    ReservedName,
    #[error("a stdio server needs a command to run")]
    EmptyCommand,
    #[error("the working directory must be an absolute path, not `{0}`")]
    RelativeCwd(String),
    #[error("the URL must be absolute and start with http:// or https://, not `{0}`")]
    InvalidUrl(String),
    #[error("`{0}` is not a header name: letters, digits and !#$%&'*+-.^_`|~ only")]
    BadHeaderName(String),
    #[error("the header `{0}` carries a control character in its value")]
    BadHeaderValue(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str) -> McpServer {
        McpServer {
            id: McpId::new("postgres").unwrap(),
            name: name.into(),
            description: String::new(),
            tags: Tags::default(),
            transport: McpServerConfig::Stdio {
                name: name.into(),
                command: "npx".into(),
                args: vec!["-y".into(), "@some/pg-server".into()],
                env: BTreeMap::new(),
                cwd: None,
            },
            enabled: true,
            created_at: 0,
        }
    }

    #[test]
    fn the_reserved_name_is_refused() {
        assert!(server("postgres").validate().is_ok());
        assert_eq!(server("bisa").validate(), Err(McpError::ReservedName));
    }

    #[test]
    fn transport_roundtrip_is_tagged() {
        let s = server("postgres");
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["transport"]["transport"], "stdio");
        assert_eq!(serde_json::from_value::<McpServer>(json).unwrap(), s);
    }

    fn remote(kind: &str, url: &str, headers: &[(&str, &str)]) -> McpServerConfig {
        let headers: BTreeMap<String, String> = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if kind == "sse" {
            McpServerConfig::Sse {
                name: "docs".into(),
                url: url.into(),
                headers,
            }
        } else {
            McpServerConfig::Http {
                name: "docs".into(),
                url: url.into(),
                headers,
            }
        }
    }

    #[test]
    fn the_three_kinds_report_their_tag_and_a_remote_its_url_and_headers() {
        let stdio = server("pg").transport;
        assert_eq!(stdio.kind(), "stdio");
        assert_eq!(stdio.url(), None);
        assert!(stdio.headers().is_none());
        let http = remote(
            "http",
            "https://mcp.example.com/mcp",
            &[("Authorization", "Bearer x")],
        );
        assert_eq!(http.kind(), "http");
        assert_eq!(http.url(), Some("https://mcp.example.com/mcp"));
        assert_eq!(http.headers().unwrap().len(), 1);
        let sse = remote("sse", "https://mcp.example.com/sse", &[]);
        assert_eq!(sse.kind(), "sse");
        let json = serde_json::to_value(&sse).unwrap();
        assert_eq!(json["transport"], "sse");
        assert!(
            json.get("headers").is_none(),
            "an empty header map is not written"
        );
        let back: McpServerConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, sse);
    }

    #[test]
    fn a_url_must_be_absolute_http_or_https() {
        for bad in [
            "mcp.example.com/mcp",
            "ws://x/y",
            "file:///tmp/s",
            "http://",
            "https:// x",
        ] {
            assert_eq!(
                remote("http", bad, &[]).validate(),
                Err(McpError::InvalidUrl(bad.into())),
                "{bad}"
            );
        }
        for good in [
            "http://127.0.0.1:8000/mcp",
            "https://user@mcp.example.com/mcp?x=1",
        ] {
            assert_eq!(remote("http", good, &[]).validate(), Ok(()), "{good}");
        }
    }

    #[test]
    fn header_names_are_tokens_and_values_carry_no_control_character() {
        let ok = remote(
            "http",
            "https://x.test/mcp",
            &[("X-Api-Key", "k1"), ("Authorization", "Bearer t")],
        );
        assert_eq!(ok.validate(), Ok(()));
        assert_eq!(
            remote("http", "https://x.test/mcp", &[("X Api", "k")]).validate(),
            Err(McpError::BadHeaderName("X Api".into()))
        );
        assert_eq!(
            remote("sse", "https://x.test/sse", &[("X-Api", "k\r\nHost: evil")]).validate(),
            Err(McpError::BadHeaderValue("X-Api".into()))
        );
    }

    #[test]
    fn a_stdio_server_needs_a_command_and_an_absolute_cwd() {
        let mut s = server("pg");
        if let McpServerConfig::Stdio { command, .. } = &mut s.transport {
            *command = "  ".into();
        }
        assert_eq!(s.validate(), Err(McpError::EmptyCommand));
        let mut s = server("pg");
        if let McpServerConfig::Stdio { cwd, .. } = &mut s.transport {
            *cwd = Some("./srv".into());
        }
        assert_eq!(s.validate(), Err(McpError::RelativeCwd("./srv".into())));
        if let McpServerConfig::Stdio { cwd, .. } = &mut s.transport {
            *cwd = Some("/srv/mcp".into());
        }
        assert_eq!(s.validate(), Ok(()));
    }

    #[test]
    fn masking_hides_every_value_and_keeps_every_key_and_an_untouched_mask_keeps_the_stored_value()
    {
        let stored = remote(
            "http",
            "https://x.test/mcp",
            &[("Authorization", "Bearer real"), ("X-Tenant", "acme")],
        );
        let shown = stored.masked();
        assert_eq!(
            shown.headers().unwrap().values().collect::<Vec<_>>(),
            vec![MASK, MASK]
        );
        assert_eq!(shown.headers().unwrap().keys().count(), 2, "keys stay");
        // An edit that changes the URL and leaves the values masked keeps them.
        let edited = remote(
            "http",
            "https://y.test/mcp",
            &[
                ("Authorization", MASK),
                ("X-Tenant", MASK),
                ("X-New", "fresh"),
            ],
        );
        let kept = McpServerConfig::unmasked_from(&stored, edited);
        let h = kept.headers().unwrap();
        assert_eq!(h["Authorization"], "Bearer real");
        assert_eq!(h["X-Tenant"], "acme");
        assert_eq!(h["X-New"], "fresh");
        assert_eq!(kept.url(), Some("https://y.test/mcp"));
        // A masked key the stored transport never had means nothing and is dropped.
        let stray = remote("http", "https://y.test/mcp", &[("X-Ghost", MASK)]);
        assert!(McpServerConfig::unmasked_from(&stored, stray)
            .headers()
            .unwrap()
            .is_empty());
        // Kinds may change: a stdio edit over a stored http keeps nothing of the headers.
        let mut proc = server("pg").transport;
        if let McpServerConfig::Stdio { env, .. } = &mut proc {
            env.insert("TOKEN".into(), MASK.into());
            env.insert("PLAIN".into(), "1".into());
        }
        let out = McpServerConfig::unmasked_from(&stored, proc);
        if let McpServerConfig::Stdio { env, .. } = out {
            assert_eq!(env.get("TOKEN"), None, "no stored env to keep from");
            assert_eq!(env["PLAIN"], "1");
        } else {
            panic!("kind follows the incoming transport");
        }
        // stdio masking too.
        let mut proc = server("pg").transport;
        if let McpServerConfig::Stdio { env, .. } = &mut proc {
            env.insert("TOKEN".into(), "secret".into());
        }
        let m = proc.masked();
        if let McpServerConfig::Stdio { env, .. } = &m {
            assert_eq!(env["TOKEN"], MASK);
        }
        assert_eq!(MASK.chars().count(), 6);
    }

    #[test]
    fn renaming_keeps_everything_but_the_name() {
        let http = remote("http", "https://x.test/mcp", &[("A", "b")]).renamed("code".into());
        assert_eq!(http.name(), "code");
        assert_eq!(http.headers().unwrap()["A"], "b");
        let proc = server("pg").transport.renamed("pg2".into());
        assert_eq!(proc.name(), "pg2");
        assert_eq!(proc.kind(), "stdio");
    }

    // added by the coverage pass: mcp.rs

    #[test]
    fn a_remote_over_sse_is_renamed_unmasked_from_a_remote_and_taken_whole_from_a_process() {
        let sse = remote(
            "sse",
            "https://r.example/sse",
            &[("Authorization", "secret")],
        );
        let renamed = sse.clone().renamed("other".into());
        assert_eq!(renamed.name(), "other");
        assert!(matches!(renamed, McpServerConfig::Sse { .. }));
        let incoming = remote("sse", "https://r.example/sse", &[("Authorization", MASK)]);
        let kept = McpServerConfig::unmasked_from(&sse, incoming.clone());
        assert_eq!(
            kept.headers()
                .and_then(|h| h.get("Authorization"))
                .map(String::as_str),
            Some("secret")
        );
        let from_http = McpServerConfig::unmasked_from(
            &remote(
                "http",
                "https://r.example/mcp",
                &[("Authorization", "secret")],
            ),
            incoming.clone(),
        );
        assert_eq!(
            from_http
                .headers()
                .and_then(|h| h.get("Authorization"))
                .map(String::as_str),
            Some("secret")
        );
        let stdio = server("tool").transport;
        let from_process = McpServerConfig::unmasked_from(&stdio, incoming);
        assert!(
            matches!(&from_process, McpServerConfig::Sse { headers, .. } if !headers.contains_key("Authorization")),
            "a mask with nothing behind it is dropped"
        );
    }

    #[test]
    fn a_server_needs_a_name() {
        let mut s = server("tool");
        s.name = " ".into();
        assert_eq!(s.validate(), Err(McpError::EmptyName));
    }

    // added by the coverage pass: b5-mcp.rs
    #[test]
    fn an_sse_transport_masks_its_headers_and_an_edit_keeps_what_a_mask_stands_for() {
        let headers = BTreeMap::from([("Authorization".to_string(), "Bearer t".to_string())]);
        let sse = McpServerConfig::Sse {
            name: "s".into(),
            url: "https://x.example/sse".into(),
            headers: headers.clone(),
        };
        let masked = sse.masked();
        assert!(
            matches!(&masked, McpServerConfig::Sse { headers, .. } if headers["Authorization"] == MASK)
        );
        // The same kind: the mask reads the stored value back.
        assert_eq!(McpServerConfig::unmasked_from(&sse, masked.clone()), sse);
        // A stdio edit against a stdio record: the env is filled the same way.
        let stdio = McpServerConfig::Stdio {
            name: "p".into(),
            command: "npx".into(),
            args: vec![],
            env: BTreeMap::from([("TOKEN".to_string(), "secret".to_string())]),
            cwd: None,
        };
        assert_eq!(
            McpServerConfig::unmasked_from(&stdio, stdio.masked()),
            stdio
        );
        // A remote edit against a remote record of the other transport: the
        // headers carry over; against a stdio record a mask means nothing.
        let http = McpServerConfig::Http {
            name: "h".into(),
            url: "https://x.example/mcp".into(),
            headers: BTreeMap::from([("Authorization".to_string(), MASK.to_string())]),
        };
        assert!(matches!(
            McpServerConfig::unmasked_from(&sse, http.clone()),
            McpServerConfig::Http { headers, .. } if headers["Authorization"] == "Bearer t"
        ));
        assert!(matches!(
            McpServerConfig::unmasked_from(&stdio, http),
            McpServerConfig::Http { headers, .. } if headers.is_empty()
        ));
        assert!(matches!(
            McpServerConfig::unmasked_from(&stdio, masked),
            McpServerConfig::Sse { headers, .. } if headers.is_empty()
        ));
    }

    #[test]
    fn a_mount_knows_its_provenance_and_a_record_without_the_switch_is_enabled() {
        let config = server("pg").transport;
        let platform = McpMount::platform(config.clone());
        let installed = McpMount::installed(config);
        assert!(platform.is_platform() && !installed.is_platform());
        assert_eq!((platform.name(), installed.name()), ("pg", "pg"));
        assert_eq!(
            (platform.provenance, installed.provenance),
            (McpProvenance::Platform, McpProvenance::Installed)
        );
        let mut json = serde_json::to_value(server("pg")).unwrap();
        json.as_object_mut().unwrap().remove("enabled");
        assert!(serde_json::from_value::<McpServer>(json).unwrap().enabled);
    }
}
