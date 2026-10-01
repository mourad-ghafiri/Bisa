//! This Mac's network, read as facts for the footer's network word and for
//! Settings › Capabilities › Network (ide/01: a machine fact is the shell's
//! to read; ide/13): whether the internet is reachable and by which public
//! address, every interface that is up, whether a VPN is up and what it is,
//! the route and the resolvers the machine uses, and the proxy System
//! Settings names. Read, never written — nothing here changes a setting, and
//! the panel has no door that would.
//!
//! The OS's own unprivileged readers, each under `probe`'s budget and each
//! parsed by a pure function a fixture test holds:
//!
//! - `scutil --nc list` — the VPN services System Settings knows (IKEv2,
//!   IPsec, L2TP, a provider's Network Extension) and whether each is
//!   connected. Empty on a Mac whose tunnel is a daemon's own (Tailscale,
//!   WireGuard, OpenVPN), which is why it is one signal of three.
//! - `ifconfig -a` — every interface but the loopback, with its addresses:
//!   the tunnels (`utun*`, `ipsec*`, `ppp*`, `tun*`, `tap*`) are the VPN's,
//!   the rest are what the panel lists. macOS keeps a few `utun`s of its own
//!   that carry only a link-local `fe80::` address; a tunnel with a real
//!   address is a VPN's.
//! - `networksetup -listallhardwareports` — which device is the Wi-Fi and
//!   which an Ethernet port, by name.
//! - `netstat -rn -f inet` — the default route: which interface the
//!   machine's traffic leaves by, and each interface's routes and gateway —
//!   all of it, or a split.
//! - `scutil --dns` — the resolvers: the one the machine asks, and the ones
//!   bound to an interface, which is what *the VPN's DNS* means.
//! - `scutil --proxy` — what System Settings names, for the panel to show and
//!   to copy into the platform's own settings on request.
//!
//! And the one thing here that leaves the machine — the internet probe: a
//! TCP connect to two well-known addresses on 443 (no name has to resolve
//! for it), and one `GET` through `curl` to the echo service the person
//! named, which answers this machine's public address. The echo request
//! follows the proxy pair System Settings names, never a PAC. The internet
//! is up when either went through; the address is shown, never kept.
//!
//! The provider behind a tunnel — Tailscale, WireGuard, a corporate client —
//! is read off the process table the footer already keeps, from one table of
//! daemon names; a tunnel nobody names is *a tunnel*.

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

/// How long each reader may take before it is given up on.
const BUDGET: Duration = Duration::from_secs(2);

/// Two anycast resolvers on 443 — addresses, so no name has to resolve for
/// the connect to say whether there is a way out.
const ANYCASTS: [SocketAddr; 2] = [
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 1, 1, 1), 443)),
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(8, 8, 8, 8), 443)),
];

/// The most an echo service may answer; an address is a few dozen bytes.
const ECHO_MAX_BYTES: &str = "4096";

/// The interface families a VPN shows up as.
const TUNNEL_FAMILIES: [&str; 5] = ["utun", "ipsec", "ppp", "tun", "tap"];

/// The daemons behind a tunnel, by the name the process table shows, and the
/// word the panel says for each. One table; the desktop test reads it.
pub const PROVIDERS: [(&str, &str); 14] = [
    ("tailscaled", "Tailscale"),
    ("wireguard-go", "WireGuard"),
    ("WireGuard", "WireGuard"),
    ("openvpn", "OpenVPN"),
    ("vpnagentd", "Cisco Secure Client"),
    ("PanGPS", "GlobalProtect"),
    ("warp-svc", "Cloudflare WARP"),
    ("ZscalerTunnel", "Zscaler"),
    ("mullvad-daemon", "Mullvad"),
    ("nordvpnd", "NordVPN"),
    ("expressvpnd", "ExpressVPN"),
    ("protonvpn", "Proton VPN"),
    ("Viscosity", "Viscosity"),
    ("Tunnelblick", "Tunnelblick"),
];

/// A VPN service System Settings knows (`scutil --nc list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VpnService {
    pub name: String,
    /// The protocol word: `IKEv2`, `IPsec`, `L2TP`, a provider's own.
    pub kind: String,
    pub connected: bool,
}

/// One interface (`ifconfig -a`), the loopback left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Interface {
    pub name: String,
    /// `UP` and `RUNNING`, not `status: inactive`, with an address that is
    /// not link-local.
    pub up: bool,
    /// The addresses, IPv4 first, link-local ones left out.
    pub addresses: Vec<String>,
    /// The point-to-point peer, when the interface names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
}

/// What an interface is, for the panel's word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    Wifi,
    Ethernet,
    Tunnel,
    Other,
}

/// A hardware port (`networksetup -listallhardwareports`): the device and
/// the port's name — `en0` is *Wi-Fi*, `en7` a *USB 10/100/1000 LAN*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwarePort {
    pub device: String,
    pub port: String,
}

/// One interface's share of the routing table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceRoutes {
    pub interface: String,
    pub routes: usize,
    /// The gateway of the first `default` row naming the interface, scoped
    /// or not — `en0` keeps its gateway while a tunnel holds the default.
    pub gateway: Option<String>,
}

/// The routing table (`netstat -rn -f inet`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Routes {
    pub default: Option<DefaultRoute>,
    pub interfaces: Vec<InterfaceRoutes>,
}

/// One resolver (`scutil --dns`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolver {
    pub servers: Vec<String>,
    /// The domain the resolver answers for, when it is not the default one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    pub search: Vec<String>,
    /// The interface the resolver is bound to, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// Listed under *scoped queries* — asked only for that interface.
    pub scoped: bool,
}

/// The routing table's default (`netstat -rn -f inet`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefaultRoute {
    pub interface: String,
    pub gateway: String,
}

/// What System Settings names under Proxies (`scutil --proxy`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MacProxy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socks: Option<String>,
    /// A PAC file — a script the platform cannot follow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pac_url: Option<String>,
    pub auto_discovery: bool,
    pub exceptions: Vec<String>,
}

/// One interface that is up, with what the other readers say about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InterfaceFacts {
    pub name: String,
    pub kind: InterfaceKind,
    /// The hardware port's word — `Wi-Fi`, `Thunderbolt Bridge`, `USB
    /// 10/100/1000 LAN` — when System Settings names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// Whether the machine's default route leaves by it.
    pub default_route: bool,
    /// How many routes the table sends through it.
    pub routes: usize,
    /// The resolvers bound to it.
    pub dns: Vec<String>,
}

/// What each probe answered; `None` where it failed or was not asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Probe {
    /// A TCP connect to a well-known address on 443, in ms — the faster of
    /// the two.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp: Option<u64>,
    /// Whether the echo service's name resolved; `None` when no echo service
    /// is named, or it never answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<bool>,
    /// The echo request, in ms, when it answered an address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch: Option<u64>,
}

/// Whether the internet is reachable, and how it was asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct InternetFacts {
    /// The echo service answered an address, or a TCP connect went through.
    pub up: bool,
    pub probe: Probe,
    /// The address the echo service saw this machine as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_ip: Option<String>,
    /// Why the echo request gave no address, when it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One tunnel with what the other readers say about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TunnelFacts {
    pub interface: String,
    pub up: bool,
    /// The connected service's kind when one names the connection, else the
    /// interface family's word.
    pub protocol: String,
    /// The daemon behind it, from `PROVIDERS`, when one is running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    /// Whether the machine's default route leaves by this tunnel.
    pub default_route: bool,
    /// How many routes the table sends through it.
    pub routes: usize,
    /// The resolvers bound to it.
    pub dns: Vec<String>,
    pub search: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VpnFacts {
    /// A tunnel is up, or a service says connected.
    pub up: bool,
    pub tunnels: Vec<TunnelFacts>,
    pub services: Vec<VpnService>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DnsFacts {
    /// The servers the machine asks — the first unscoped resolver's.
    pub servers: Vec<String>,
    pub search: Vec<String>,
    /// The interface the machine's resolver is bound to, when it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
}

/// Everything the footer and the panel show about this Mac's network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkFacts {
    pub internet: InternetFacts,
    /// Every interface that is up, the tunnels among them.
    pub interfaces: Vec<InterfaceFacts>,
    pub vpn: VpnFacts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_route: Option<DefaultRoute>,
    pub dns: DnsFacts,
    pub proxy: MacProxy,
    /// How long the readers took, together.
    pub read_ms: u64,
}

// ---------------------------------------------------------------------------
// The parsers — pure, held by fixtures below.
// ---------------------------------------------------------------------------

/// `scutil --nc list`: one service per row after the header — the enabled
/// mark, the state in parentheses, a UUID, a type word, the quoted name, and
/// the bracketed kind.
pub fn parse_services(text: &str) -> Vec<VpnService> {
    text.lines()
        .filter_map(|line| {
            let open = line.find('(')?;
            let close = line[open..].find(')')? + open;
            let state = &line[open + 1..close];
            let name_start = line.find('"')?;
            let name_end = line[name_start + 1..].find('"')? + name_start + 1;
            let name = line[name_start + 1..name_end].to_string();
            let kind = line[name_end + 1..]
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string();
            let type_word = line[close + 1..name_start]
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();
            Some(VpnService {
                name,
                kind: service_kind(&kind, &type_word),
                connected: state.eq_ignore_ascii_case("Connected"),
            })
        })
        .collect()
}

/// The bracket's kind as a word: `IPSec`, `PPP:L2TP`, `VPN:com.apple.ike2`,
/// `VPN:<a provider's bundle id>`.
fn service_kind(bracket: &str, type_word: &str) -> String {
    let inner = bracket.split(':').next_back().unwrap_or(bracket);
    match inner {
        "com.apple.ike2" => "IKEv2".to_string(),
        "IPSec" | "IPsec" => "IPsec".to_string(),
        "L2TP" => "L2TP".to_string(),
        "PPTP" => "PPTP".to_string(),
        "" => type_word.to_string(),
        bundle => bundle
            .rsplit('.')
            .next()
            .filter(|w| !w.is_empty())
            .unwrap_or(bundle)
            .to_string(),
    }
}

/// `ifconfig -a`: every block but the loopback — the name line carries the
/// flags and the MTU, the indented lines its addresses and, on a port,
/// whether the link is active.
pub fn parse_interfaces(text: &str) -> Vec<Interface> {
    /// A block being read: the interface, and what decides `up` besides
    /// its addresses.
    struct Block {
        interface: Interface,
        running: bool,
        inactive: bool,
    }
    fn finished(b: Block) -> Interface {
        let mut i = b.interface;
        i.up = b.running && !b.inactive && !i.addresses.is_empty();
        i
    }
    let mut out: Vec<Interface> = Vec::new();
    let mut current: Option<Block> = None;
    for line in text.lines() {
        if !line.starts_with(|c: char| c.is_whitespace()) {
            if let Some(b) = current.take() {
                out.push(finished(b));
            }
            let Some((name, rest)) = line.split_once(':') else {
                continue;
            };
            if name == "lo0" || rest.contains("LOOPBACK") {
                continue;
            }
            let running = rest.contains('<') && rest.contains("UP") && rest.contains("RUNNING");
            let mtu = rest
                .split_whitespace()
                .skip_while(|w| *w != "mtu")
                .nth(1)
                .and_then(|m| m.parse().ok());
            current = Some(Block {
                interface: Interface {
                    name: name.to_string(),
                    up: false,
                    addresses: Vec::new(),
                    peer: None,
                    mtu,
                },
                running,
                inactive: false,
            });
            continue;
        }
        let Some(b) = current.as_mut() else {
            continue;
        };
        let mut words = line.split_whitespace();
        match words.next() {
            Some("inet") => {
                if let Some(addr) = words.next() {
                    b.interface.addresses.insert(0, addr.to_string());
                }
                let rest: Vec<&str> = words.collect();
                if let Some(at) = rest.iter().position(|w| *w == "-->") {
                    b.interface.peer = rest.get(at + 1).map(|p| p.to_string());
                }
            }
            Some("inet6") => {
                if let Some(addr) = words.next() {
                    let bare = addr.split('%').next().unwrap_or(addr);
                    if !bare.starts_with("fe80") {
                        b.interface.addresses.push(bare.to_string());
                    }
                }
            }
            Some("status:") => {
                // A port whose cable is out or whose radio is off still
                // reads UP and RUNNING; the status line says the link is not.
                b.inactive = words.next() == Some("inactive");
            }
            _ => {}
        }
    }
    if let Some(b) = current.take() {
        out.push(finished(b));
    }
    out
}

/// `networksetup -listallhardwareports`: `Hardware Port: X` opens a port,
/// the `Device: Y` after it closes it; the VLAN section at the end is not
/// a port.
pub fn parse_hardware_ports(text: &str) -> Vec<HardwarePort> {
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("VLAN Configurations") {
            break;
        }
        if let Some(port) = trimmed.strip_prefix("Hardware Port:") {
            pending = Some(port.trim().to_string());
        } else if let Some(device) = trimmed.strip_prefix("Device:") {
            if let Some(port) = pending.take() {
                out.push(HardwarePort {
                    device: device.trim().to_string(),
                    port,
                });
            }
        }
    }
    out
}

/// The kind and the label of an interface: a tunnel family is a tunnel
/// whatever the ports say; a port System Settings names is read by its
/// word; anything else is *other*.
pub fn kind_of(name: &str, ports: &[HardwarePort]) -> (InterfaceKind, Option<String>) {
    if is_tunnel_name(name) {
        return (InterfaceKind::Tunnel, None);
    }
    let Some(port) = ports.iter().find(|p| p.device == name) else {
        return (InterfaceKind::Other, None);
    };
    let word = port.port.as_str();
    let kind = if word.contains("Wi-Fi") || word.contains("AirPort") {
        InterfaceKind::Wifi
    } else if word.contains("Ethernet")
        || word.contains("LAN")
        || word
            .strip_prefix("Thunderbolt ")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
    {
        InterfaceKind::Ethernet
    } else {
        InterfaceKind::Other
    };
    (kind, Some(port.port.clone()))
}

fn is_tunnel_name(name: &str) -> bool {
    TUNNEL_FAMILIES.iter().any(|f| {
        name.starts_with(f)
            && name[f.len()..].chars().all(|c| c.is_ascii_digit())
            && name.len() > f.len()
    })
}

/// `netstat -rn -f inet`: the machine's default is the first `default` row
/// that is not interface-scoped (flag `I`); each interface counts the rows
/// naming it and keeps the gateway of its first `default` row, scoped or not.
pub fn parse_routes(text: &str) -> Routes {
    let mut routes = Routes::default();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() < 4 || words[0] == "Destination" {
            continue;
        }
        let (destination, gateway, flags, netif) = (words[0], words[1], words[2], words[3]);
        let is_default = destination == "default";
        if is_default && routes.default.is_none() && !flags.contains('I') {
            routes.default = Some(DefaultRoute {
                interface: netif.to_string(),
                gateway: gateway.to_string(),
            });
        }
        let entry = match routes.interfaces.iter_mut().find(|r| r.interface == netif) {
            Some(entry) => entry,
            None => {
                routes.interfaces.push(InterfaceRoutes {
                    interface: netif.to_string(),
                    routes: 0,
                    gateway: None,
                });
                routes.interfaces.last_mut().expect("just pushed")
            }
        };
        entry.routes += 1;
        if is_default && entry.gateway.is_none() {
            entry.gateway = Some(gateway.to_string());
        }
    }
    routes
}

/// `scutil --dns`: `resolver #n` blocks of `key : value` lines, the second
/// group under *for scoped queries*.
pub fn parse_resolvers(text: &str) -> Vec<Resolver> {
    let mut out = Vec::new();
    let mut scoped = false;
    let mut current: Option<Resolver> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("DNS configuration") {
            if let Some(r) = current.take() {
                out.push(r);
            }
            scoped = trimmed.contains("scoped");
            continue;
        }
        if trimmed.starts_with("resolver #") {
            if let Some(r) = current.take() {
                out.push(r);
            }
            current = Some(Resolver {
                servers: Vec::new(),
                domain: None,
                search: Vec::new(),
                interface: None,
                scoped,
            });
            continue;
        }
        let Some(r) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = trimmed.split_once(" : ") else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if key.starts_with("nameserver[") {
            r.servers.push(value.to_string());
        } else if key.starts_with("search domain[") {
            r.search.push(value.to_string());
        } else if key == "domain" {
            r.domain = Some(value.to_string());
        } else if key == "if_index" {
            // `22 (utun4)`
            r.interface = value
                .split('(')
                .nth(1)
                .map(|s| s.trim_end_matches(')').to_string());
        }
    }
    if let Some(r) = current.take() {
        out.push(r);
    }
    out
}

/// `scutil --proxy`: a `<dictionary>` of `Key : value` lines, the
/// exceptions as an `<array>` of `n : value` lines.
pub fn parse_mac_proxy(text: &str) -> MacProxy {
    let mut values: Vec<(String, String)> = Vec::new();
    let mut exceptions = Vec::new();
    let mut in_exceptions = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if in_exceptions {
            if trimmed.starts_with('}') {
                in_exceptions = false;
                continue;
            }
            if let Some((_, v)) = trimmed.split_once(" : ") {
                exceptions.push(v.trim().to_string());
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(" : ") else {
            continue;
        };
        if key.trim() == "ExceptionsList" {
            in_exceptions = value.contains("<array>");
            continue;
        }
        values.push((key.trim().to_string(), value.trim().to_string()));
    }
    let get = |k: &str| {
        values
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    let on = |k: &str| get(k) == Some("1");
    let host_port = |enable: &str, host: &str, port: &str| {
        if !on(enable) {
            return None;
        }
        let host = get(host)?;
        Some(match get(port) {
            Some(p) => format!("{host}:{p}"),
            None => host.to_string(),
        })
    };
    MacProxy {
        http: host_port("HTTPEnable", "HTTPProxy", "HTTPPort"),
        https: host_port("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        socks: host_port("SOCKSEnable", "SOCKSProxy", "SOCKSPort"),
        pac_url: if on("ProxyAutoConfigEnable") {
            get("ProxyAutoConfigURLString").map(str::to_string)
        } else {
            None
        },
        auto_discovery: on("ProxyAutoDiscoveryEnable"),
        exceptions,
    }
}

/// The provider word for a tunnel, from the daemons running.
pub fn provider_of(process_names: &[String]) -> Option<String> {
    PROVIDERS
        .iter()
        .find(|(daemon, _)| process_names.iter().any(|n| n == daemon))
        .map(|(_, word)| word.to_string())
}

fn family_word(interface: &str) -> &'static str {
    if interface.starts_with("ipsec") {
        "IPsec"
    } else if interface.starts_with("ppp") {
        "PPP"
    } else {
        "tunnel"
    }
}

/// The resolvers bound to one interface: its servers, deduplicated in
/// order, and its search domains.
fn bound_dns(resolvers: &[Resolver], name: &str) -> (Vec<String>, Vec<String>) {
    let bound: Vec<&Resolver> = resolvers
        .iter()
        .filter(|r| r.interface.as_deref() == Some(name))
        .collect();
    let servers =
        bound
            .iter()
            .flat_map(|r| r.servers.iter().cloned())
            .fold(Vec::new(), |mut acc, s| {
                if !acc.contains(&s) {
                    acc.push(s);
                }
                acc
            });
    let search = bound
        .iter()
        .flat_map(|r| r.search.iter().cloned())
        .collect();
    (servers, search)
}

// ---------------------------------------------------------------------------
// The internet probe.
// ---------------------------------------------------------------------------

/// What the echo request came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Echo {
    /// No echo service is named: the request was never made.
    NotAsked,
    /// The service answered an address, in this many ms.
    Answered(IpAddr, u64),
    /// It answered, but not an address — a captive portal's page, say.
    NotAnAddress,
    /// curl failed, with its exit code when it had one.
    Failed(Option<i32>),
    /// Nothing came back within the budget.
    NoAnswer,
}

/// curl's stdout with `-w "\n%{time_total}"`: the body on the lines before
/// the last, the seconds taken on the last. The body must be one address.
pub fn parse_echo(stdout: &str) -> Option<(IpAddr, u64)> {
    let (body, time) = stdout.trim_end().rsplit_once('\n')?;
    let ip: IpAddr = body.trim().parse().ok()?;
    let secs: f64 = time.trim().parse().ok()?;
    Some((ip, (secs * 1000.0).round() as u64))
}

/// curl's exit code, in words.
fn curl_word(code: Option<i32>) -> String {
    match code {
        Some(6) => "the echo service's name did not resolve".to_string(),
        Some(7) => "the echo service could not be connected to".to_string(),
        Some(22) => "the echo service answered an HTTP error".to_string(),
        Some(28) => "the echo request timed out".to_string(),
        Some(35) => "the TLS handshake with the echo service failed".to_string(),
        Some(c) => format!("the echo request failed (curl exited {c})"),
        None => "the echo request was ended by a signal".to_string(),
    }
}

/// The verdict: up when a connect went through or the echo answered an
/// address; the probe's facts kept whichever way. Pure, so a test holds
/// every branch without a wire.
pub fn internet(tcp: Option<u64>, echo: Echo) -> InternetFacts {
    let mut facts = InternetFacts {
        probe: Probe {
            tcp,
            ..Probe::default()
        },
        ..InternetFacts::default()
    };
    match echo {
        Echo::NotAsked => {}
        Echo::Answered(ip, ms) => {
            facts.probe.dns = Some(true);
            facts.probe.fetch = Some(ms);
            facts.public_ip = Some(ip.to_string());
        }
        Echo::NotAnAddress => {
            facts.probe.dns = Some(true);
            facts.error = Some(
                "the echo service answered something that is not an address — a captive portal?"
                    .to_string(),
            );
        }
        Echo::Failed(code) => {
            facts.probe.dns = Some(code != Some(6));
            facts.error = Some(curl_word(code));
        }
        Echo::NoAnswer => {
            facts.error = Some(format!(
                "the echo service did not answer within {} s",
                BUDGET.as_secs()
            ));
        }
    }
    facts.up = facts.probe.fetch.is_some() || tcp.is_some();
    facts
}

/// One TCP connect within the budget, in ms; `None` when it did not go
/// through.
#[cfg(target_os = "macos")]
fn tcp_probe(addr: SocketAddr, budget: Duration) -> Option<u64> {
    let started = std::time::Instant::now();
    std::net::TcpStream::connect_timeout(&addr, budget).ok()?;
    Some(started.elapsed().as_millis() as u64)
}

/// The echo request through curl, which ships with macOS: the body capped,
/// a failure an exit code (`-f`), the proxy pair System Settings names
/// followed — curl reads the environment's, not the system's — and never
/// a PAC.
#[cfg(target_os = "macos")]
fn read_echo(url: &str, proxy: &MacProxy) -> Echo {
    let secs = BUDGET.as_secs().to_string();
    let mut args = vec![
        "-s",
        "-f",
        "-m",
        secs.as_str(),
        "--max-filesize",
        ECHO_MAX_BYTES,
        "-A",
        "bisa-desktop",
        "-w",
        "\n%{time_total}",
    ];
    if let Some(p) = proxy.https.as_deref().or(proxy.http.as_deref()) {
        args.push("--proxy");
        args.push(p);
    }
    args.push(url);
    // A little over the budget: curl's own `-m` is what ends the request,
    // so the runner's kill is the last resort, not the usual end.
    let Some(out) = crate::probe::run("/usr/bin/curl", &args, BUDGET + Duration::from_millis(500))
    else {
        return Echo::NoAnswer;
    };
    if !out.status.success() {
        return Echo::Failed(out.status.code());
    }
    match parse_echo(&out.stdout) {
        Some((ip, ms)) => Echo::Answered(ip, ms),
        None => Echo::NotAnAddress,
    }
}

/// The internet, asked three ways at once: the two connects on their own
/// threads, the echo request on this one.
#[cfg(target_os = "macos")]
fn read_internet(url: Option<&str>, proxy: &MacProxy) -> InternetFacts {
    let url = url.map(str::trim).filter(|u| !u.is_empty());
    std::thread::scope(|s| {
        let connects: Vec<_> = ANYCASTS
            .iter()
            .map(|addr| s.spawn(move || tcp_probe(*addr, BUDGET)))
            .collect();
        let echo = match url {
            Some(u) => read_echo(u, proxy),
            None => Echo::NotAsked,
        };
        let tcp = connects
            .into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .min();
        internet(tcp, echo)
    })
}

/// One read of the machine, before it is joined: what each reader said,
/// and how long the reading took.
#[derive(Debug, Default)]
pub struct Readings {
    pub internet: InternetFacts,
    pub services: Vec<VpnService>,
    pub interfaces: Vec<Interface>,
    pub ports: Vec<HardwarePort>,
    pub routes: Routes,
    pub resolvers: Vec<Resolver>,
    pub proxy: MacProxy,
    /// The names of every process running now — a VPN provider's daemon is read off them.
    pub process_names: Vec<String>,
    pub read_ms: u64,
}

/// The facts, joined: every interface that is up with its kind, its share
/// of the routes and its resolvers; every tunnel with the service that
/// names the connection and the provider.
pub fn join(readings: Readings) -> NetworkFacts {
    let Readings {
        internet,
        services,
        interfaces,
        ports,
        routes,
        resolvers,
        proxy,
        process_names,
        read_ms,
    } = readings;
    let connected: Vec<&VpnService> = services.iter().filter(|s| s.connected).collect();
    let provider = provider_of(&process_names);
    let default_route = routes.default.clone();
    let carries_default = |name: &str| default_route.as_ref().is_some_and(|d| d.interface == name);
    let routes_of = |name: &str| routes.interfaces.iter().find(|r| r.interface == name);
    let up_interfaces: Vec<InterfaceFacts> = interfaces
        .iter()
        .filter(|i| i.up)
        .map(|i| {
            let (kind, label) = kind_of(&i.name, &ports);
            let (dns, _) = bound_dns(&resolvers, &i.name);
            InterfaceFacts {
                name: i.name.clone(),
                kind,
                label,
                addresses: i.addresses.clone(),
                mtu: i.mtu,
                gateway: routes_of(&i.name).and_then(|r| r.gateway.clone()),
                default_route: carries_default(&i.name),
                routes: routes_of(&i.name).map(|r| r.routes).unwrap_or(0),
                dns,
            }
        })
        .collect();
    let tunnels: Vec<TunnelFacts> = interfaces
        .into_iter()
        .filter(|i| is_tunnel_name(&i.name))
        .map(|t| {
            let (dns, search) = bound_dns(&resolvers, &t.name);
            let protocol = match connected.as_slice() {
                [one] if t.up => one.kind.clone(),
                _ => family_word(&t.name).to_string(),
            };
            TunnelFacts {
                default_route: carries_default(&t.name),
                routes: routes_of(&t.name).map(|r| r.routes).unwrap_or(0),
                dns,
                search,
                protocol,
                provider: if t.up { provider.clone() } else { None },
                interface: t.name,
                up: t.up,
                addresses: t.addresses,
                peer: t.peer,
                mtu: t.mtu,
            }
        })
        .collect();
    let up = tunnels.iter().any(|t| t.up) || !connected.is_empty();
    let machine = resolvers
        .iter()
        .find(|r| !r.scoped && r.domain.is_none() && !r.servers.is_empty());
    NetworkFacts {
        internet,
        interfaces: up_interfaces,
        vpn: VpnFacts {
            up,
            tunnels,
            services,
        },
        default_route,
        dns: DnsFacts {
            servers: machine.map(|r| r.servers.clone()).unwrap_or_default(),
            search: machine.map(|r| r.search.clone()).unwrap_or_default(),
            interface: machine.and_then(|r| r.interface.clone()),
        },
        proxy,
        read_ms,
    }
}

// ---------------------------------------------------------------------------
// The readers.
// ---------------------------------------------------------------------------

/// This Mac's network now; `None` where there are no readers. The proxy
/// first — the echo request needs it — then every other reader at once,
/// so the whole read takes about one budget, not six.
#[cfg(target_os = "macos")]
pub fn read(process_names: &[String], public_ip_url: Option<&str>) -> Option<NetworkFacts> {
    let started = std::time::Instant::now();
    let text = |program: &str, args: &[&str]| {
        crate::probe::run(program, args, BUDGET)
            .and_then(|o| o.success())
            .unwrap_or_default()
    };
    let proxy = parse_mac_proxy(&text("/usr/sbin/scutil", &["--proxy"]));
    let (internet, services, interfaces, ports, routes, resolvers) = std::thread::scope(|s| {
        let internet = s.spawn(|| read_internet(public_ip_url, &proxy));
        let services = s.spawn(|| parse_services(&text("/usr/sbin/scutil", &["--nc", "list"])));
        let interfaces = s.spawn(|| parse_interfaces(&text("/sbin/ifconfig", &["-a"])));
        let ports = s.spawn(|| {
            parse_hardware_ports(&text("/usr/sbin/networksetup", &["-listallhardwareports"]))
        });
        let routes = s.spawn(|| parse_routes(&text("/usr/sbin/netstat", &["-rn", "-f", "inet"])));
        let resolvers = s.spawn(|| parse_resolvers(&text("/usr/sbin/scutil", &["--dns"])));
        (
            internet.join().unwrap_or_default(),
            services.join().unwrap_or_default(),
            interfaces.join().unwrap_or_default(),
            ports.join().unwrap_or_default(),
            routes.join().unwrap_or_default(),
            resolvers.join().unwrap_or_default(),
        )
    });
    Some(join(Readings {
        internet,
        services,
        interfaces,
        ports,
        routes,
        resolvers,
        proxy,
        process_names: process_names.to_vec(),
        read_ms: started.elapsed().as_millis() as u64,
    }))
}

#[cfg(not(target_os = "macos"))]
pub fn read(_process_names: &[String], _public_ip_url: Option<&str>) -> Option<NetworkFacts> {
    None
}

/// This Mac's network facts, or `null` where the shell has no readers.
/// `public_ip_url` is the echo service the person named
/// (`network.public_ip_url`), handed over by the desktop — the shell reads no
/// setting; empty asks none.
#[tauri::command]
pub async fn network_facts(
    table: tauri::State<'_, std::sync::Arc<crate::stats::Table>>,
    public_ip_url: Option<String>,
) -> Result<Option<NetworkFacts>, String> {
    let table = std::sync::Arc::clone(&table);
    tauri::async_runtime::spawn_blocking(move || {
        read(&table.process_names(), public_ip_url.as_deref())
    })
    .await
    .map_err(|e| format!("the network read did not finish: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The shapes macOS 26 prints, with placeholder addresses. The connected
    // service row is reconstructed: this machine had none to observe.
    const NC_LIST: &str = "Available network connection services in the current set (*=enabled):\n\
*  (Connected)      1F2A3B4C-0000-4000-8000-000000000001 VPN               \"Office\"                  [VPN:com.apple.ike2]\n\
*  (Disconnected)   1F2A3B4C-0000-4000-8000-000000000002 IPSec             \"Old lab\"                 [IPSec]\n\
   (Disconnected)   1F2A3B4C-0000-4000-8000-000000000003 PPP               \"Legacy\"                  [PPP:L2TP]\n\
*  (Disconnected)   1F2A3B4C-0000-4000-8000-000000000004 VPN               \"Home\"                    [VPN:com.wireguard.macos]\n";

    const IFCONFIG: &str = "lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384\n\
\tinet 127.0.0.1 netmask 0xff000000\n\
utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1500\n\
\tinet6 fe80::1234:5678:9abc:def0%utun0 prefixlen 64 scopeid 0x12 \n\
\tnd6 options=201<PERFORMNUD,DAD>\n\
awdl0: flags=8843<UP,BROADCAST,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
\tinet6 fe80::1%awdl0 prefixlen 64 scopeid 0x10 \n\
utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1280\n\
\tinet 10.20.30.40 --> 10.20.30.40 netmask 0xffff0000\n\
\tinet6 fd7a:115c:a1e0::1 prefixlen 48 \n\
utun5: flags=8010<POINTOPOINT,MULTICAST> mtu 1500\n\
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
\tinet6 fe80::1c2b:3d4e:5f60:7182%en0 prefixlen 64 secured scopeid 0xe \n\
\tinet 192.168.1.20 netmask 0xffffff00 broadcast 192.168.1.255\n\
\tinet6 2001:db8::1 prefixlen 64 autoconf secured \n\
\tstatus: active\n\
en1: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
\tinet 10.0.0.5 netmask 0xffffff00 broadcast 10.0.0.255\n\
\tstatus: inactive\n\
bridge0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
\tConfiguration:\n\
\tstatus: inactive\n";

    const HARDWARE_PORTS: &str = "\n\
Hardware Port: Wi-Fi\n\
Device: en0\n\
Ethernet Address: 3c:22:fb:00:00:01\n\
\n\
Hardware Port: Thunderbolt Bridge\n\
Device: bridge0\n\
Ethernet Address: 36:2c:00:00:00:00\n\
\n\
Hardware Port: USB 10/100/1000 LAN\n\
Device: en7\n\
Ethernet Address: 00:e0:4c:00:00:02\n\
\n\
VLAN Configurations\n\
===================\n";

    const NETSTAT: &str = "Routing tables\n\nInternet:\n\
Destination        Gateway            Flags               Netif Expire\n\
default            10.20.30.1         UGScg               utun4       \n\
default            192.168.1.1        UGScIg                en0       \n\
10.20.30.40        10.20.30.40        UHr                 utun4       \n\
10.20/16           link#22            UCS                 utun4       \n\
127                127.0.0.1          UCS                   lo0       \n\
192.168.1          link#14            UCS                   en0      !\n";

    const DNS: &str = "DNS configuration\n\n\
resolver #1\n\
  nameserver[0] : 100.100.100.100\n\
  if_index : 22 (utun4)\n\
  flags    : Supplemental, Request A records, Request AAAA records\n\
  reach    : 0x00000002 (Reachable)\n\
  order    : 101600\n\n\
resolver #2\n\
  search domain[0] : corp.example\n\
  nameserver[0] : 192.168.1.1\n\
  nameserver[1] : 192.168.1.2\n\
  if_index : 14 (en0)\n\
  flags    : Request A records, Request AAAA records\n\
  reach    : 0x00020002 (Reachable,Directly Reachable Address)\n\n\
resolver #3\n\
  domain   : local\n\
  options  : mdns\n\
  timeout  : 5\n\
  flags    : Request A records, Request AAAA records\n\
  reach    : 0x00000000 (Not Reachable)\n\
  order    : 300000\n\n\
DNS configuration (for scoped queries)\n\n\
resolver #1\n\
  nameserver[0] : 192.168.1.1\n\
  if_index : 14 (en0)\n\
  flags    : Scoped, Request A records\n\
  reach    : 0x00020002 (Reachable,Directly Reachable Address)\n\n\
resolver #2\n\
  nameserver[0] : 100.100.100.100\n\
  if_index : 22 (utun4)\n\
  flags    : Scoped, Request A records, Request AAAA records\n\
  reach    : 0x00000002 (Reachable)\n";

    const PROXY_NONE: &str = "<dictionary> {\n}\n";
    const PROXY_SET: &str = "<dictionary> {\n\
  ExceptionsList : <array> {\n\
    0 : *.local\n\
    1 : 169.254/16\n\
    2 : .corp.example\n\
  }\n\
  FTPPassive : 1\n\
  HTTPEnable : 1\n\
  HTTPPort : 3128\n\
  HTTPProxy : proxy.corp.example\n\
  HTTPSEnable : 1\n\
  HTTPSPort : 3128\n\
  HTTPSProxy : proxy.corp.example\n\
  ProxyAutoDiscoveryEnable : 0\n\
  SOCKSEnable : 0\n\
}\n";
    const PROXY_PAC: &str = "<dictionary> {\n\
  ProxyAutoConfigEnable : 1\n\
  ProxyAutoConfigURLString : http://proxy.corp.example/proxy.pac\n\
}\n";

    #[test]
    fn services_are_read_with_their_state_and_kind() {
        let s = parse_services(NC_LIST);
        assert_eq!(s.len(), 4);
        assert_eq!(
            s[0],
            VpnService {
                name: "Office".into(),
                kind: "IKEv2".into(),
                connected: true
            }
        );
        assert_eq!(s[1].kind, "IPsec");
        assert!(!s[1].connected);
        assert_eq!(s[2].kind, "L2TP");
        assert_eq!(s[3].kind, "macos");
        assert!(parse_services(
            "Available network connection services in the current set (*=enabled):\n"
        )
        .is_empty());
    }

    #[test]
    fn interfaces_are_every_non_loopback_block_and_up_means_running_with_an_address() {
        let all = parse_interfaces(IFCONFIG);
        let names: Vec<&str> = all.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["utun0", "awdl0", "utun4", "utun5", "en0", "en1", "bridge0"],
            "every block but lo0"
        );
        let by = |name: &str| all.iter().find(|i| i.name == name).unwrap();
        assert!(!by("utun0").up, "macOS's own utun carries only fe80::");
        assert!(by("utun0").addresses.is_empty());
        assert!(!by("awdl0").up, "link-local only");
        let vpn = by("utun4");
        assert!(vpn.up);
        assert_eq!(vpn.addresses, vec!["10.20.30.40", "fd7a:115c:a1e0::1"]);
        assert_eq!(vpn.peer.as_deref(), Some("10.20.30.40"));
        assert_eq!(vpn.mtu, Some(1280));
        assert!(!by("utun5").up, "not RUNNING");
        let wifi = by("en0");
        assert!(wifi.up);
        assert_eq!(
            wifi.addresses,
            vec!["192.168.1.20", "2001:db8::1"],
            "IPv4 first, fe80:: left out"
        );
        assert_eq!(wifi.mtu, Some(1500));
        assert!(!by("en1").up, "status: inactive — the cable is out");
        assert!(!by("bridge0").up, "no address");
        assert!(
            is_tunnel_name("ipsec0")
                && is_tunnel_name("ppp0")
                && !is_tunnel_name("utun")
                && !is_tunnel_name("en0")
        );
    }

    #[test]
    fn hardware_ports_map_a_device_to_its_port_word() {
        let ports = parse_hardware_ports(HARDWARE_PORTS);
        assert_eq!(
            ports,
            vec![
                HardwarePort {
                    device: "en0".into(),
                    port: "Wi-Fi".into()
                },
                HardwarePort {
                    device: "bridge0".into(),
                    port: "Thunderbolt Bridge".into()
                },
                HardwarePort {
                    device: "en7".into(),
                    port: "USB 10/100/1000 LAN".into()
                },
            ],
            "the VLAN section is not a port"
        );
        assert_eq!(
            kind_of("en0", &ports),
            (InterfaceKind::Wifi, Some("Wi-Fi".to_string()))
        );
        assert_eq!(kind_of("en7", &ports).0, InterfaceKind::Ethernet);
        assert_eq!(kind_of("bridge0", &ports).0, InterfaceKind::Other);
        assert_eq!(
            kind_of(
                "en5",
                &[HardwarePort {
                    device: "en5".into(),
                    port: "Thunderbolt 1".into()
                }]
            )
            .0,
            InterfaceKind::Ethernet
        );
        assert_eq!(
            kind_of(
                "utun4",
                &[HardwarePort {
                    device: "utun4".into(),
                    port: "Wi-Fi".into()
                }]
            ),
            (InterfaceKind::Tunnel, None),
            "a tunnel family is a tunnel whatever the ports say"
        );
        assert_eq!(kind_of("en9", &ports), (InterfaceKind::Other, None));
    }

    #[test]
    fn the_default_route_is_the_first_unscoped_default_and_each_interface_has_its_routes_and_gateway(
    ) {
        let routes = parse_routes(NETSTAT);
        assert_eq!(
            routes.default,
            Some(DefaultRoute {
                interface: "utun4".into(),
                gateway: "10.20.30.1".into()
            })
        );
        let of = |name: &str| {
            routes
                .interfaces
                .iter()
                .find(|r| r.interface == name)
                .unwrap()
        };
        assert_eq!(of("utun4").routes, 3);
        assert_eq!(of("utun4").gateway.as_deref(), Some("10.20.30.1"));
        assert_eq!(of("en0").routes, 2);
        assert_eq!(
            of("en0").gateway.as_deref(),
            Some("192.168.1.1"),
            "a scoped default still names the port's gateway"
        );
        assert_eq!(of("lo0").gateway, None);
        let scoped_only = parse_routes("Routing tables\n\nInternet:\nDestination Gateway Flags Netif Expire\ndefault 192.168.1.1 UGScIg en0\n");
        assert_eq!(
            scoped_only.default, None,
            "an interface-scoped default is not the machine's"
        );
        assert_eq!(
            scoped_only.interfaces[0].gateway.as_deref(),
            Some("192.168.1.1")
        );
    }

    #[test]
    fn an_echo_answer_is_an_address_and_curls_time() {
        assert_eq!(
            parse_echo("203.0.113.7\n0.183\n"),
            Some(("203.0.113.7".parse().unwrap(), 183))
        );
        assert_eq!(
            parse_echo(" 2001:db8::7 \n0.05"),
            Some(("2001:db8::7".parse().unwrap(), 50))
        );
        assert_eq!(
            parse_echo("<html>login</html>\n0.2"),
            None,
            "a portal's page"
        );
        assert_eq!(parse_echo(""), None);
        assert_eq!(parse_echo("203.0.113.7"), None, "no time line");
    }

    #[test]
    fn the_internet_is_up_on_a_tcp_connect_or_an_echoed_address_and_down_when_both_fail() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let tcp_only = internet(Some(12), Echo::NotAsked);
        assert!(tcp_only.up);
        assert_eq!(
            tcp_only.probe,
            Probe {
                tcp: Some(12),
                dns: None,
                fetch: None
            }
        );
        assert_eq!(tcp_only.public_ip, None);
        assert_eq!(tcp_only.error, None);

        let echoed = internet(None, Echo::Answered(ip, 180));
        assert!(
            echoed.up,
            "a proxy-only network reaches HTTPS but not raw TCP"
        );
        assert_eq!(
            echoed.probe,
            Probe {
                tcp: None,
                dns: Some(true),
                fetch: Some(180)
            }
        );
        assert_eq!(echoed.public_ip.as_deref(), Some("203.0.113.7"));

        let no_name = internet(None, Echo::Failed(Some(6)));
        assert!(!no_name.up);
        assert_eq!(no_name.probe.dns, Some(false));
        assert!(no_name
            .error
            .as_deref()
            .unwrap()
            .contains("did not resolve"));

        let portal = internet(None, Echo::NotAnAddress);
        assert!(!portal.up, "a portal's page is not an address");
        assert_eq!(portal.probe.dns, Some(true));
        assert!(portal.error.as_deref().unwrap().contains("captive portal"));

        let silent = internet(None, Echo::NoAnswer);
        assert!(!silent.up);
        assert_eq!(silent.probe.dns, None);
        assert!(silent.error.as_deref().unwrap().contains("did not answer"));

        let portal_but_connected = internet(Some(9), Echo::NotAnAddress);
        assert!(portal_but_connected.up, "the connect carries it");
        assert!(
            portal_but_connected.error.is_some(),
            "the echo's failure is still said"
        );

        let nothing = internet(None, Echo::NotAsked);
        assert!(!nothing.up);
        assert_eq!(nothing.error, None, "nothing was asked, nothing failed");
        assert!(internet(None, Echo::Failed(Some(28)))
            .error
            .unwrap()
            .contains("timed out"));
        assert!(internet(None, Echo::Failed(None))
            .error
            .unwrap()
            .contains("signal"));
    }

    #[test]
    fn resolvers_carry_their_servers_domain_search_and_interface() {
        let r = parse_resolvers(DNS);
        assert_eq!(r.len(), 5);
        assert_eq!(r[0].servers, vec!["100.100.100.100"]);
        assert_eq!(r[0].interface.as_deref(), Some("utun4"));
        assert!(!r[0].scoped);
        assert_eq!(r[1].search, vec!["corp.example"]);
        assert_eq!(r[1].servers.len(), 2);
        assert_eq!(r[2].domain.as_deref(), Some("local"));
        assert!(r[3].scoped && r[4].scoped);
        assert_eq!(r[4].interface.as_deref(), Some("utun4"));
    }

    #[test]
    fn the_mac_proxy_reads_empty_set_and_pac() {
        assert_eq!(parse_mac_proxy(PROXY_NONE), MacProxy::default());
        let set = parse_mac_proxy(PROXY_SET);
        assert_eq!(set.http.as_deref(), Some("proxy.corp.example:3128"));
        assert_eq!(set.https.as_deref(), Some("proxy.corp.example:3128"));
        assert_eq!(set.socks, None);
        assert_eq!(
            set.exceptions,
            vec!["*.local", "169.254/16", ".corp.example"]
        );
        assert!(!set.auto_discovery);
        let pac = parse_mac_proxy(PROXY_PAC);
        assert_eq!(
            pac.pac_url.as_deref(),
            Some("http://proxy.corp.example/proxy.pac")
        );
        assert_eq!(
            pac.http, None,
            "a PAC file names no proxy the platform can copy"
        );
    }

    #[test]
    fn the_facts_join_a_tunnel_to_its_service_provider_routes_and_dns() {
        let facts = join(Readings {
            internet: internet(
                Some(23),
                Echo::Answered("203.0.113.7".parse().unwrap(), 140),
            ),
            services: parse_services(NC_LIST),
            interfaces: parse_interfaces(IFCONFIG),
            ports: parse_hardware_ports(HARDWARE_PORTS),
            routes: parse_routes(NETSTAT),
            resolvers: parse_resolvers(DNS),
            proxy: parse_mac_proxy(PROXY_SET),
            process_names: vec!["loginwindow".to_string(), "tailscaled".to_string()],
            read_ms: 7,
        });
        assert!(facts.internet.up);
        assert_eq!(facts.internet.public_ip.as_deref(), Some("203.0.113.7"));
        let names: Vec<&str> = facts.interfaces.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["utun4", "en0"],
            "the up ones, in ifconfig's order"
        );
        let wifi = facts.interfaces.iter().find(|i| i.name == "en0").unwrap();
        assert_eq!(wifi.kind, InterfaceKind::Wifi);
        assert_eq!(wifi.label.as_deref(), Some("Wi-Fi"));
        assert_eq!(wifi.gateway.as_deref(), Some("192.168.1.1"));
        assert!(!wifi.default_route, "the tunnel holds the default");
        assert_eq!(wifi.routes, 2);
        assert_eq!(wifi.dns, vec!["192.168.1.1", "192.168.1.2"]);
        let tunnel = facts.interfaces.iter().find(|i| i.name == "utun4").unwrap();
        assert_eq!(tunnel.kind, InterfaceKind::Tunnel);
        assert_eq!(tunnel.label, None);
        assert!(tunnel.default_route);
        assert_eq!(tunnel.routes, 3);
        assert!(facts.vpn.up);
        let t = facts
            .vpn
            .tunnels
            .iter()
            .find(|t| t.interface == "utun4")
            .unwrap();
        assert_eq!(
            t.protocol, "IKEv2",
            "the one connected service names the connection"
        );
        assert_eq!(t.provider.as_deref(), Some("Tailscale"));
        assert!(t.default_route);
        assert_eq!(t.routes, 3);
        assert_eq!(t.dns, vec!["100.100.100.100"]);
        let quiet = facts
            .vpn
            .tunnels
            .iter()
            .find(|t| t.interface == "utun0")
            .unwrap();
        assert_eq!(quiet.protocol, "tunnel");
        assert_eq!(
            quiet.provider, None,
            "a tunnel that is not up has no provider"
        );
        assert_eq!(
            facts.dns.servers,
            vec!["100.100.100.100"],
            "the machine asks the first unscoped resolver"
        );
        assert_eq!(facts.dns.interface.as_deref(), Some("utun4"));
        assert_eq!(facts.read_ms, 7);
        assert_eq!(
            facts.default_route.map(|d| d.interface),
            Some("utun4".to_string())
        );
    }

    #[test]
    fn no_tunnel_and_no_service_is_no_vpn() {
        let facts = join(Readings {
            interfaces: parse_interfaces(
                "en0: flags=8863<UP,BROADCAST,RUNNING> mtu 1500\n\tinet 192.168.1.2 netmask 0xffffff00\n",
            ),
            read_ms: 1,
            ..Readings::default()
        });
        assert!(!facts.vpn.up);
        assert!(facts.vpn.tunnels.is_empty());
        assert!(facts.dns.servers.is_empty());
        assert!(!facts.internet.up, "nothing was probed");
        assert_eq!(facts.interfaces.len(), 1);
        assert_eq!(
            facts.interfaces[0].kind,
            InterfaceKind::Other,
            "no ports read"
        );
        assert_eq!(facts.interfaces[0].gateway, None);
    }

    #[test]
    fn every_provider_has_a_daemon_and_a_word() {
        for (daemon, word) in PROVIDERS {
            assert!(!daemon.is_empty() && !word.is_empty());
        }
        assert_eq!(
            provider_of(&["openvpn".to_string()]).as_deref(),
            Some("OpenVPN")
        );
        assert_eq!(provider_of(&["Finder".to_string()]), None);
    }
}
