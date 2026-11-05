//! The grammar, once — the cases the three copies each held, together.

use bisa_netrules::{
    host_matches, is_host_pattern, is_loopback, is_reserved_header, split_port, RESERVED_HEADERS,
};

#[test]
fn a_pattern_is_a_host_or_a_wildcard_suffix_in_any_case() {
    for ok in [
        "api.slack.com",
        "API.Slack.com",
        "127.0.0.1:27124",
        "*.atlassian.net",
        "*.example.com:8443",
        "localhost",
        "my-host.example",
    ] {
        assert!(is_host_pattern(ok), "{ok}");
    }
    for bad in [
        "",
        "*.",
        ".example.com",
        "example.com.",
        "example..com",
        "-host.example",
        "host:port",
        "host:123456",
        "a/b",
        "*example.com",
        "host:",
        "user@host",
        "*",
    ] {
        assert!(!is_host_pattern(bad), "{bad}");
    }
}

#[test]
fn exact_patterns_match_themselves_and_wildcards_match_subdomains_only() {
    assert!(host_matches("api.slack.com", "API.Slack.com"));
    assert!(
        !host_matches("api.slack.com", "api.slack.com:443"),
        "a port is part of the authority"
    );
    assert!(host_matches("*.atlassian.net", "acme.atlassian.net"));
    assert!(host_matches("*.atlassian.net", "a.b.atlassian.net"));
    assert!(
        !host_matches("*.atlassian.net", "atlassian.net"),
        "never the suffix itself"
    );
    assert!(!host_matches("*.atlassian.net", "evilatlassian.net"));
    assert!(host_matches("*.example.com:8443", "x.example.com:8443"));
    assert!(
        !host_matches("*.example.com:8443", "x.example.com"),
        "a wildcard's port is matched too"
    );
    assert!(!host_matches("*.example.com", "x.example.com:8443"));
    assert!(
        host_matches(" api.slack.com ", "api.slack.com"),
        "a list entry may carry spaces"
    );
}

#[test]
fn loopback_is_this_machine_by_name_or_by_address() {
    for yes in [
        "127.0.0.1",
        "127.0.0.1:27124",
        "[::1]",
        "[::1]:9",
        "::1",
        "localhost",
        "LOCALHOST:80",
        // By address, not by string: the whole of 127/8, the unspecified
        // addresses, an IPv4 mapped into IPv6.
        "127.0.0.2",
        "127.255.255.255:80",
        "0.0.0.0",
        "0.0.0.0:8080",
        "[::]",
        "[::ffff:127.0.0.1]:8080",
        "::ffff:127.0.0.1",
        // A trailing dot, a subdomain of localhost.
        "localhost.",
        "app.localhost",
        "app.localhost:3000",
    ] {
        assert!(is_loopback(yes), "{yes}");
    }
    for no in [
        "127.0.0.1.evil.example",
        "localhost.example",
        "localhost.evil.test",
        "example.com",
        "10.0.0.1",
        "192.168.1.1",
        "",
    ] {
        assert!(!is_loopback(no), "{no}");
    }
}

#[test]
fn private_network_is_the_lan_and_link_local() {
    use bisa_netrules::is_private_network;
    for yes in [
        "10.0.0.1",
        "10.255.255.255:443",
        "172.16.0.1",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "[fc00::1]",
        "[fd12::1]:8080",
        "[fe80::1]",
        "[::ffff:10.0.0.1]",
    ] {
        assert!(is_private_network(yes), "{yes}");
    }
    for no in [
        "172.32.0.1",
        "11.0.0.1",
        "127.0.0.1",
        "8.8.8.8",
        "example.com",
        "localhost",
        "",
    ] {
        assert!(!is_private_network(no), "{no}");
    }
}

#[test]
fn a_port_is_split_off_only_when_it_is_one() {
    assert_eq!(split_port("a.b:8443"), ("a.b", Some("8443")));
    assert_eq!(split_port("a.b"), ("a.b", None));
    assert_eq!(split_port("a.b:"), ("a.b:", None));
    assert_eq!(split_port("a.b:x"), ("a.b:x", None));
}

#[test]
fn the_reserved_headers_are_the_clients_whatever_the_case() {
    assert_eq!(RESERVED_HEADERS.len(), 5);
    for h in [
        "Authorization",
        "HOST",
        "content-length",
        " Cookie ",
        "Transfer-Encoding",
    ] {
        assert!(is_reserved_header(h), "{h}");
    }
    assert!(!is_reserved_header("x-api-key"));
    assert!(!is_reserved_header("accept"));
}
