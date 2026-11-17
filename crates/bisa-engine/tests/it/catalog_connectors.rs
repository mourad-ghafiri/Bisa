//! The built-in connectors on the wire, operation by operation, against what
//! each platform's own documentation says — the requests built but never
//! sent: Jira's enhanced search, Confluence's v2 pages, Meta's live Graph
//! version, X's hosts, Notion's version header, Obsidian's paths, the
//! expectations Slack, Linear and TikTok need, TikTok's spelling of OAuth,
//! Google's offline consent, and the consent and token hosts every OAuth
//! scheme declares by its own URLs. A spec that drifts from its platform
//! fails here, not on a person's machine.

use bisa_connectors::auth::{apply, SignContext};
use bisa_connectors::http::Request;
use bisa_connectors::request::{bind_params, build_request};
use bisa_connectors::template::Values;
use bisa_connectors::{authorize_url, AllowAll, AuthSpec, Credential, Expect, OsEntropy, Secret};
use bisa_core::{Connector, OperationId};
use bisa_engine::connectors::call_spec;
use bisa_security::net::{decide_host, HostPolicy, HostVerdict};
use bisa_store::catalog::parse_connector;
use bisa_store::CATALOG;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

fn def(slug: &str) -> Connector {
    let (_, toml) = CATALOG
        .connectors
        .iter()
        .find(|(s, _)| *s == slug)
        .unwrap_or_else(|| panic!("the catalog ships {slug}"));
    parse_connector(slug, toml)
        .unwrap()
        .as_connector(slug)
        .unwrap()
}

/// The account parameters a connector's base URL and queries read.
fn account_of(slug: &str) -> BTreeMap<String, Value> {
    match slug {
        "jira" | "confluence" => BTreeMap::from([("site".to_string(), json!("acme"))]),
        "trello" => BTreeMap::from([("key".to_string(), json!("app-key"))]),
        "sentry" => BTreeMap::from([
            ("region".to_string(), json!("de")),
            ("organization".to_string(), json!("acme")),
        ]),
        "pagerduty" => BTreeMap::from([("email".to_string(), json!("ops@acme.example"))]),
        "pipedrive" => BTreeMap::from([("company".to_string(), json!("acme"))]),
        "zendesk" => BTreeMap::from([("subdomain".to_string(), json!("acme"))]),
        "mailchimp" => BTreeMap::from([("server".to_string(), json!("us19"))]),
        "twilio" => BTreeMap::from([("account_sid".to_string(), json!("AC0123456789abcdef"))]),
        "box" => BTreeMap::from([("enterprise_id".to_string(), json!("123456"))]),
        "zoom" => BTreeMap::from([
            ("account_id".to_string(), json!("AbC123")),
            ("user".to_string(), json!("host@acme.example")),
        ]),
        _ => BTreeMap::new(),
    }
}

/// One operation's request, built as the engine builds it, never sent.
fn request(def: &Connector, op: &str, params: &[(&str, &str)]) -> Request {
    let op = def
        .operation(&OperationId::new(op).unwrap())
        .unwrap_or_else(|| panic!("{} has {op}", def.id));
    let spec = call_spec(def, op, Duration::from_secs(10));
    let given: BTreeMap<String, String> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let bound = bind_params(&spec.params, &given).unwrap();
    let account = account_of(def.id.as_str());
    let values = Values {
        account: &account,
        params: &bound,
    };
    build_request(&spec, &values, &BTreeMap::new(), &OsEntropy).unwrap()
}

fn query(req: &Request) -> BTreeMap<String, String> {
    req.url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn body(req: &Request) -> Value {
    serde_json::from_slice(req.body.as_deref().expect("a body")).unwrap()
}

fn expect_of(def: &Connector, op: &str) -> Option<Expect> {
    let op = def.operation(&OperationId::new(op).unwrap()).unwrap();
    call_spec(def, op, Duration::from_secs(1)).expect
}

fn authorize(def: &Connector) -> BTreeMap<String, String> {
    let op = &def.operations[0];
    let spec = call_spec(def, op, Duration::from_secs(1));
    let a = authorize_url(
        &spec.auth,
        "the-client",
        "http://127.0.0.1:4478/connectors/oauth/callback",
        &OsEntropy,
        &AllowAll,
    )
    .unwrap();
    let url = url::Url::parse(&a.url).unwrap();
    let mut q: BTreeMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    q.insert(
        "@host".into(),
        url.host_str().unwrap_or_default().to_string(),
    );
    q
}

/// The built-ins that name no check: Google's Sheets and Docs APIs have no
/// read that takes no spreadsheet or document id.
const CHECKLESS: [&str; 2] = ["google-docs", "google-sheets"];

/// A path as the platform reads it, percent-decoded.
fn decoded(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// Every built-in's check builds with no parameter at all — what *Check*
/// sends — and its host is one the definition declares.
#[test]
fn every_check_builds_with_no_parameters_against_a_declared_host() {
    for (slug, _) in CATALOG.connectors {
        let d = def(slug);
        if CHECKLESS.contains(slug) {
            assert_eq!(d.check, None, "{slug}");
            continue;
        }
        let check = d.check.clone().expect("a check");
        let req = request(&d, check.as_str(), &[]);
        // `host[:port]`, as the judge is given it.
        let host = bisa_connectors::hosts::host_of(&req.url).unwrap_or_default();
        let declared = d.declared_hosts();
        assert!(
            matches!(
                decide_host(&HostPolicy::default(), &declared, &host),
                HostVerdict::Allow { .. }
            ),
            "{slug}: the check's host {host} is not declared ({declared:?})"
        );
    }
}

#[test]
fn jira_searches_with_the_enhanced_endpoint_and_names_its_fields() {
    let d = def("jira");
    let req = request(
        &d,
        "search",
        &[
            ("jql", "project = ACME AND status = \"To Do\""),
            ("max", "5"),
            ("fields", "summary,status"),
        ],
    );
    assert_eq!(req.url.host_str(), Some("acme.atlassian.net"));
    assert_eq!(
        req.url.path(),
        "/rest/api/3/search/jql",
        "the endpoint Atlassian removed is not the one called"
    );
    let q = query(&req);
    assert_eq!(q["jql"], "project = ACME AND status = \"To Do\"");
    assert_eq!(q["maxResults"], "5");
    assert_eq!(q["fields"], "summary,status");
    // Left empty, `fields` is left out: Jira then answers ids alone, as its doc says.
    let bare = request(&d, "search", &[("jql", "assignee = currentUser()")]);
    assert!(!query(&bare).contains_key("fields"));
    assert!(!query(&bare).contains_key("maxResults"));
    let me = request(&d, "myself", &[]);
    assert_eq!(me.url.path(), "/rest/api/3/myself");
    let moved = request(&d, "transition", &[("key", "ACME-1"), ("transition", "31")]);
    assert_eq!(moved.url.path(), "/rest/api/3/issue/ACME-1/transitions");
    assert_eq!(body(&moved), json!({"transition": {"id": "31"}}));
}

#[test]
fn confluence_reads_and_creates_pages_with_v2_and_finds_a_space_by_its_key() {
    let d = def("confluence");
    let user = request(&d, "current_user", &[]);
    assert_eq!(user.url.path(), "/wiki/rest/api/user/current");
    let search = request(&d, "search", &[("cql", "type = page"), ("max", "3")]);
    assert_eq!(search.url.path(), "/wiki/rest/api/content/search");
    assert_eq!(query(&search)["cql"], "type = page");
    let spaces = request(&d, "spaces", &[("key", "ENG")]);
    assert_eq!(spaces.url.path(), "/wiki/api/v2/spaces");
    assert_eq!(query(&spaces)["keys"], "ENG");
    assert_eq!(query(&spaces)["limit"], "1");
    let page = request(&d, "get_page", &[("id", "123456789")]);
    assert_eq!(page.url.path(), "/wiki/api/v2/pages/123456789");
    assert_eq!(query(&page)["body-format"], "storage");
    let created = request(
        &d,
        "create_page",
        &[
            ("space_id", "42"),
            ("title", "Runbook"),
            ("html", "<p>hi</p>"),
        ],
    );
    assert_eq!(created.url.path(), "/wiki/api/v2/pages");
    assert_eq!(
        body(&created),
        json!({
            "spaceId": "42",
            "status": "current",
            "title": "Runbook",
            "body": {"representation": "storage", "value": "<p>hi</p>"}
        }),
        "no parent named, no parentId sent: the space's home takes it"
    );
    let under = request(
        &d,
        "create_page",
        &[
            ("space_id", "42"),
            ("title", "Runbook"),
            ("html", "<p>hi</p>"),
            ("parent_id", "7"),
        ],
    );
    assert_eq!(body(&under)["parentId"], json!("7"));
}

#[test]
fn meta_calls_a_live_graph_version_and_instagram_publishes_in_two_calls() {
    let pages = def("facebook-pages");
    assert_eq!(request(&pages, "me", &[]).url.path(), "/v26.0/me");
    let posts = request(&pages, "page_posts", &[("page", "1234")]);
    assert_eq!(posts.url.path(), "/v26.0/1234/posts");
    assert_eq!(
        query(&posts)["fields"],
        "id,message,created_time,permalink_url"
    );
    let published = request(
        &pages,
        "publish_post",
        &[("page", "1234"), ("message", "hi")],
    );
    assert_eq!(published.url.path(), "/v26.0/1234/feed");
    assert_eq!(body(&published), json!({"message": "hi"}));
    for op in pages
        .operations
        .iter()
        .chain(def("instagram").operations.iter())
    {
        assert!(
            op.path.starts_with("/v26.0/"),
            "{}: {} names a Graph version that is not the live one",
            op.id,
            op.path
        );
    }
    let ig = def("instagram");
    let staged = request(
        &ig,
        "create_media",
        &[
            ("user", "17841400"),
            ("image_url", "https://cdn.example/a.jpg"),
        ],
    );
    assert_eq!(staged.url.path(), "/v26.0/17841400/media");
    assert_eq!(
        body(&staged),
        json!({"image_url": "https://cdn.example/a.jpg"}),
        "no caption typed, no caption sent"
    );
    let shown = request(
        &ig,
        "publish_media",
        &[("user", "17841400"), ("creation_id", "99")],
    );
    assert_eq!(shown.url.path(), "/v26.0/17841400/media_publish");
    assert_eq!(body(&shown), json!({"creation_id": "99"}));
}

#[test]
fn x_lives_on_its_own_hosts() {
    let d = def("x");
    let me = request(&d, "me", &[]);
    assert_eq!(me.url.as_str(), "https://api.x.com/2/users/me");
    let search = request(
        &d,
        "search_recent",
        &[("query", "from:bisa"), ("max", "10")],
    );
    assert_eq!(search.url.path(), "/2/tweets/search/recent");
    assert_eq!(query(&search)["max_results"], "10");
    assert_eq!(
        query(&search)["tweet.fields"],
        "created_at,author_id,public_metrics"
    );
    let consent = authorize(&d);
    assert_eq!(consent["@host"], "x.com");
    assert_eq!(
        consent["scope"],
        "tweet.read tweet.write users.read offline.access"
    );
    assert_eq!(
        d.oauth_hosts(),
        vec!["x.com".to_string(), "api.x.com".to_string()]
    );
}

#[test]
fn notion_pins_its_version_and_queries_every_row_when_no_filter_is_given() {
    let d = def("notion");
    for op in &d.operations {
        assert_eq!(
            op.headers.get("Notion-Version").map(String::as_str),
            Some("2022-06-28"),
            "{}: every call names the version it was written against",
            op.id
        );
    }
    let all = request(
        &d,
        "query_database",
        &[
            ("database", "0123456789abcdef0123456789abcdef"),
            ("max", "10"),
        ],
    );
    assert_eq!(
        all.url.path(),
        "/v1/databases/0123456789abcdef0123456789abcdef/query"
    );
    assert_eq!(
        body(&all),
        json!({"page_size": 10}),
        "no filter typed, no filter sent — every row, as Notion documents"
    );
    let some = request(
        &d,
        "query_database",
        &[
            ("database", "0123456789abcdef0123456789abcdef"),
            ("max", "10"),
            (
                "filter",
                r#"{"property":"Status","select":{"equals":"Done"}}"#,
            ),
        ],
    );
    assert_eq!(
        body(&some)["filter"],
        json!({"property": "Status", "select": {"equals": "Done"}})
    );
    let everything = request(&d, "search", &[("max", "5")]);
    assert_eq!(body(&everything), json!({"page_size": 5}));
}

#[test]
fn obsidian_keeps_a_notes_slashes_lists_a_folder_under_a_trailing_slash_and_checks_the_key() {
    let d = def("obsidian");
    let note = request(&d, "read_note", &[("path", "Projects/Launch plan.md")]);
    assert_eq!(note.url.path(), "/vault/Projects/Launch%20plan.md");
    let folder = request(&d, "list_folder", &[("folder", "Projects/2026")]);
    assert_eq!(folder.url.path(), "/vault/Projects/2026/");
    let root = request(&d, "list_vault", &[]);
    assert_eq!(root.url.path(), "/vault/");
    assert_eq!(
        expect_of(&d, "status"),
        Some(Expect {
            path: "authenticated".into(),
            equals: Some(json!(true)),
            absent: false,
            reason: None,
        }),
        "the status page answers without a key; the flag is the check"
    );
    let search = request(&d, "search", &[("query", "launch")]);
    assert_eq!(search.url.path(), "/search/simple/");
    assert_eq!(query(&search)["query"], "launch");
}

#[test]
fn slack_expects_ok_on_every_call_and_linear_sends_its_key_bare_and_expects_no_errors() {
    let slack = def("slack");
    for op in &slack.operations {
        assert_eq!(
            expect_of(&slack, op.id.as_str()),
            Some(Expect {
                path: "ok".into(),
                equals: Some(json!(true)),
                absent: false,
                reason: Some("error".into()),
            }),
            "{}: Slack answers a failure as a 200 with ok:false",
            op.id
        );
    }
    let posted = request(
        &slack,
        "post_message",
        &[("channel", "#general"), ("text", "hi")],
    );
    assert_eq!(
        posted.url.as_str(),
        "https://slack.com/api/chat.postMessage"
    );
    assert_eq!(body(&posted), json!({"channel": "#general", "text": "hi"}));

    let linear = def("linear");
    for op in &linear.operations {
        assert_eq!(
            expect_of(&linear, op.id.as_str()),
            Some(Expect {
                path: "errors".into(),
                equals: None,
                absent: true,
                reason: Some("errors.0.message".into()),
            }),
            "{}: a GraphQL failure is a 200 with errors",
            op.id
        );
    }
    let mut viewer = request(&linear, "viewer", &[]);
    let spec = call_spec(&linear, &linear.operations[0], Duration::from_secs(1));
    assert!(matches!(spec.auth, AuthSpec::ApiKey { .. }));
    apply(
        &spec.auth,
        &Credential::ApiKey {
            key: Secret::new("lin_api_not_real"),
            extra: vec![],
        },
        &mut viewer,
        &SignContext {
            now: 0,
            account: &BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(
        viewer.header("authorization"),
        Some("lin_api_not_real"),
        "a personal key goes bare, the Bearer form being OAuth's"
    );
    assert_eq!(
        body(&viewer),
        json!({"query": "query { viewer { id name email } }"}),
        "the doubled braces are GraphQL's own"
    );
    let created = request(
        &linear,
        "create_issue",
        &[("team", "team-1"), ("title", "A title")],
    );
    assert_eq!(
        body(&created)["variables"],
        json!({"teamId": "team-1", "title": "A title"}),
        "no description typed, none sent"
    );
}

#[test]
fn tiktok_spells_oauth_its_own_way_and_expects_the_ok_code() {
    let d = def("tiktok");
    let consent = authorize(&d);
    assert_eq!(consent["@host"], "www.tiktok.com");
    assert_eq!(consent["client_key"], "the-client");
    assert!(!consent.contains_key("client_id"));
    assert_eq!(
        consent["scope"],
        "user.info.basic,user.info.stats,video.list"
    );
    assert_eq!(consent["code_challenge_method"], "S256");
    assert_eq!(consent["code_challenge"].len(), 64, "hex of the SHA-256");
    assert!(consent["code_challenge"]
        .bytes()
        .all(|b| b.is_ascii_hexdigit()));
    for op in &d.operations {
        assert_eq!(
            expect_of(&d, op.id.as_str()),
            Some(Expect {
                path: "error.code".into(),
                equals: Some(json!("ok")),
                absent: false,
                reason: Some("error.message".into()),
            }),
            "{}: every TikTok answer carries error.code",
            op.id
        );
    }
    let videos = request(&d, "video_list", &[("max", "5")]);
    assert_eq!(videos.url.path(), "/v2/video/list/");
    assert_eq!(body(&videos), json!({"max_count": 5}));
    let info = request(&d, "user_info", &[]);
    assert_eq!(
        query(&info)["fields"],
        "open_id,display_name,avatar_url,follower_count,video_count"
    );
}

#[test]
fn google_asks_for_offline_consent_with_pkce_and_every_oauth_scheme_declares_its_own_hosts() {
    for slug in [
        "gmail",
        "google-calendar",
        "google-docs",
        "google-drive",
        "google-sheets",
        "google-tasks",
        "youtube",
    ] {
        let d = def(slug);
        let consent = authorize(&d);
        assert_eq!(consent["@host"], "accounts.google.com", "{slug}");
        assert_eq!(consent["access_type"], "offline", "{slug}");
        assert_eq!(consent["prompt"], "consent", "{slug}");
        assert_eq!(consent["code_challenge_method"], "S256", "{slug}");
        assert_eq!(consent["client_id"], "the-client", "{slug}");
        assert!(
            consent["scope"].starts_with("https://www.googleapis.com/auth/"),
            "{slug}: {}",
            consent["scope"]
        );
        assert_eq!(
            d.oauth_hosts(),
            vec![
                "accounts.google.com".to_string(),
                "oauth2.googleapis.com".to_string()
            ],
            "{slug}"
        );
    }
    // Every OAuth scheme's consent page and token endpoint are judged as the
    // declared hosts are: allowed with no allow list, refused by the deny list.
    for (slug, _) in CATALOG.connectors {
        let d = def(slug);
        let declared = d.declared_hosts();
        for host in d.oauth_hosts() {
            assert!(
                matches!(
                    decide_host(&HostPolicy::default(), &declared, &host),
                    HostVerdict::Allow { .. }
                ),
                "{slug}: {host} should be the scheme's own"
            );
            let deny = HostPolicy {
                allow: vec![],
                deny: vec![host.clone()],
            };
            assert!(
                matches!(
                    decide_host(&deny, &declared, &host),
                    HostVerdict::Deny { .. }
                ),
                "{slug}: the deny list is read first"
            );
        }
    }
}

#[test]
fn trello_carries_its_app_key_on_every_query_and_gmail_sends_a_raw_message() {
    let trello = def("trello");
    for op in &trello.operations {
        let params: Vec<(&str, &str)> = op
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| (p.name.as_str(), "x"))
            .collect();
        let req = request(&trello, op.id.as_str(), &params);
        assert_eq!(query(&req)["key"], "app-key", "{}", op.id);
    }
    let card = request(&trello, "create_card", &[("list", "l1"), ("name", "Do it")]);
    assert_eq!(card.url.path(), "/1/cards");
    assert_eq!(query(&card)["idList"], "l1");
    assert!(!query(&card).contains_key("desc"));

    let gmail = def("gmail");
    let sent = request(&gmail, "send_message", &[("raw", "RnJvbTogbWU")]);
    assert_eq!(sent.url.path(), "/gmail/v1/users/me/messages/send");
    assert_eq!(body(&sent), json!({"raw": "RnJvbTogbWU"}));
    let listed = request(&gmail, "list_messages", &[("max", "20")]);
    assert_eq!(query(&listed)["maxResults"], "20");
    assert!(!query(&listed).contains_key("q"));
}

// --- engineering and operations: GitHub, GitLab, Bitbucket, Sentry, PagerDuty, Discord ---

/// The request with the account's credential on it, as a call sends it.
fn signed(def: &Connector, mut req: Request, credential: Credential) -> Request {
    let spec = call_spec(def, &def.operations[0], Duration::from_secs(1));
    apply(
        &spec.auth,
        &credential,
        &mut req,
        &SignContext {
            now: 0,
            account: &account_of(def.id.as_str()),
        },
    )
    .unwrap();
    req
}

fn key(value: &str) -> Credential {
    Credential::ApiKey {
        key: Secret::new(value),
        extra: vec![],
    }
}

/// Every operation of a connector, built with a placeholder for each
/// required parameter.
fn every_request(def: &Connector) -> Vec<(String, Request)> {
    def.operations
        .iter()
        .map(|op| {
            let params: Vec<(&str, &str)> = op
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| {
                    let value = match p.kind {
                        bisa_core::ParamKind::Number => "7",
                        bisa_core::ParamKind::Bool => "true",
                        _ => "x",
                    };
                    (p.name.as_str(), value)
                })
                .collect();
            (op.id.to_string(), request(def, op.id.as_str(), &params))
        })
        .collect()
}

/// What changes something says so, and nothing in the six deletes.
#[test]
fn the_engineering_connectors_mark_every_write_and_delete_nothing() {
    for slug in [
        "github",
        "gitlab",
        "bitbucket",
        "sentry",
        "pagerduty",
        "discord",
    ] {
        let d = def(slug);
        for op in &d.operations {
            assert_ne!(
                op.method,
                bisa_core::HttpMethod::Delete,
                "{slug}.{}: no built-in deletes",
                op.id
            );
            assert_eq!(
                op.writes,
                op.method != bisa_core::HttpMethod::Get,
                "{slug}.{}: a call that is not a GET is a write, and says so",
                op.id
            );
        }
    }
}

#[test]
fn github_pins_its_version_on_every_call_and_sends_a_bearer() {
    let d = def("github");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("api.github.com"), "{op}");
        assert_eq!(
            req.header("accept"),
            Some("application/vnd.github+json"),
            "{op}"
        );
        assert_eq!(
            req.header("x-github-api-version"),
            Some("2022-11-28"),
            "{op}"
        );
    }
    let me = signed(
        &d,
        request(&d, "user", &[]),
        Credential::Bearer(Secret::new("github_pat_not_real")),
    );
    assert_eq!(me.url.path(), "/user");
    assert_eq!(
        me.header("authorization"),
        Some("Bearer github_pat_not_real")
    );
    assert_eq!(
        me.header("user-agent"),
        Some("bisa"),
        "GitHub refuses a request with none"
    );

    let listed = request(
        &d,
        "list_issues",
        &[
            ("owner", "octocat"),
            ("repo", "hello-world"),
            ("state", "all"),
            ("max", "50"),
        ],
    );
    assert_eq!(listed.url.path(), "/repos/octocat/hello-world/issues");
    assert_eq!(query(&listed)["state"], "all");
    assert_eq!(query(&listed)["per_page"], "50");
    let bare = request(
        &d,
        "list_issues",
        &[("owner", "octocat"), ("repo", "hello-world")],
    );
    assert!(
        query(&bare).is_empty(),
        "GitHub's own defaults: open, thirty"
    );

    let one = request(
        &d,
        "get_issue",
        &[
            ("owner", "octocat"),
            ("repo", "hello-world"),
            ("number", "42"),
        ],
    );
    assert_eq!(one.url.path(), "/repos/octocat/hello-world/issues/42");
    let pulls = request(
        &d,
        "list_pulls",
        &[("owner", "octocat"), ("repo", "hello-world")],
    );
    assert_eq!(pulls.url.path(), "/repos/octocat/hello-world/pulls");

    let found = request(
        &d,
        "search_issues",
        &[("q", "repo:octocat/hello-world is:issue is:open")],
    );
    assert_eq!(found.url.path(), "/search/issues");
    assert_eq!(
        query(&found)["q"],
        "repo:octocat/hello-world is:issue is:open"
    );
    let search = d
        .operation(&OperationId::new("search_issues").unwrap())
        .unwrap();
    assert_eq!(search.output.select.as_deref(), Some("items"));

    let created = request(
        &d,
        "create_issue",
        &[
            ("owner", "octocat"),
            ("repo", "hello-world"),
            ("title", "It broke"),
        ],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/repos/octocat/hello-world/issues");
    assert_eq!(
        body(&created),
        json!({"title": "It broke"}),
        "no body typed, none sent"
    );
    let said = request(
        &d,
        "comment",
        &[
            ("owner", "octocat"),
            ("repo", "hello-world"),
            ("number", "42"),
            ("body", "On it."),
        ],
    );
    assert_eq!(
        said.url.path(),
        "/repos/octocat/hello-world/issues/42/comments"
    );
    assert_eq!(body(&said), json!({"body": "On it."}));
}

#[test]
fn gitlab_sends_its_token_in_its_own_header_and_encodes_a_projects_path() {
    let d = def("gitlab");
    let me = signed(&d, request(&d, "user", &[]), key("glpat-not-real"));
    assert_eq!(me.url.as_str(), "https://gitlab.com/api/v4/user");
    assert_eq!(me.header("private-token"), Some("glpat-not-real"));
    assert_eq!(me.header("authorization"), None);

    // A project named by its path: the slash is the path's, encoded.
    let listed = request(
        &d,
        "list_issues",
        &[
            ("project", "acme/web app"),
            ("state", "opened"),
            ("max", "50"),
        ],
    );
    assert_eq!(
        listed.url.path(),
        "/api/v4/projects/acme%2Fweb%20app/issues",
        "GitLab reads a namespaced path URL-encoded"
    );
    assert_eq!(query(&listed)["state"], "opened");
    assert_eq!(query(&listed)["per_page"], "50");
    let by_id = request(&d, "get_issue", &[("project", "42"), ("iid", "5")]);
    assert_eq!(by_id.url.path(), "/api/v4/projects/42/issues/5");

    let mine = request(&d, "projects", &[]);
    assert_eq!(query(&mine)["membership"], "true");
    assert_eq!(query(&mine)["simple"], "true");
    assert!(!query(&mine).contains_key("search"));

    let merges = request(
        &d,
        "list_merge_requests",
        &[("project", "42"), ("state", "merged")],
    );
    assert_eq!(merges.url.path(), "/api/v4/projects/42/merge_requests");
    assert_eq!(query(&merges)["state"], "merged");

    let created = request(
        &d,
        "create_issue",
        &[("project", "42"), ("title", "It broke")],
    );
    assert_eq!(created.url.path(), "/api/v4/projects/42/issues");
    assert_eq!(created.header("content-type"), Some("application/json"));
    assert_eq!(body(&created), json!({"title": "It broke"}));
    let noted = request(
        &d,
        "comment_issue",
        &[("project", "42"), ("iid", "5"), ("body", "On it.")],
    );
    assert_eq!(noted.url.path(), "/api/v4/projects/42/issues/5/notes");
    assert_eq!(body(&noted), json!({"body": "On it."}));
    let reviewed = request(
        &d,
        "comment_merge_request",
        &[("project", "42"), ("iid", "7"), ("body", "Looks right.")],
    );
    assert_eq!(
        reviewed.url.path(),
        "/api/v4/projects/42/merge_requests/7/notes"
    );
}

#[test]
fn bitbucket_signs_in_with_basic_and_reads_a_page_under_values() {
    let d = def("bitbucket");
    let me = signed(
        &d,
        request(&d, "user", &[]),
        Credential::Basic {
            username: "me@acme.example".into(),
            password: Secret::new("not-a-real-token"),
        },
    );
    assert_eq!(me.url.as_str(), "https://api.bitbucket.org/2.0/user");
    assert!(me
        .header("authorization")
        .is_some_and(|a| a.starts_with("Basic ")));

    let repos = request(
        &d,
        "repositories",
        &[("workspace", "acme"), ("role", "member")],
    );
    assert_eq!(repos.url.path(), "/2.0/repositories/acme");
    assert_eq!(query(&repos)["role"], "member");
    assert!(
        !query(&repos).contains_key("pagelen"),
        "the operation declares no page size"
    );
    let pulls = request(
        &d,
        "pull_requests",
        &[("workspace", "acme"), ("repo", "web")],
    );
    assert_eq!(pulls.url.path(), "/2.0/repositories/acme/web/pullrequests");
    assert!(
        query(&pulls).is_empty(),
        "open only, by Bitbucket's own default"
    );
    let merged = request(
        &d,
        "pull_requests",
        &[("workspace", "acme"), ("repo", "web"), ("state", "MERGED")],
    );
    assert_eq!(query(&merged)["state"], "MERGED");
    for op in ["repositories", "pull_requests"] {
        let op = d.operation(&OperationId::new(op).unwrap()).unwrap();
        assert_eq!(op.output.select.as_deref(), Some("values"), "{}", op.id);
    }
    let one = request(
        &d,
        "pull_request",
        &[("workspace", "acme"), ("repo", "web"), ("id", "12")],
    );
    assert_eq!(one.url.path(), "/2.0/repositories/acme/web/pullrequests/12");
    let said = request(
        &d,
        "comment_pull_request",
        &[
            ("workspace", "acme"),
            ("repo", "web"),
            ("id", "12"),
            ("text", "Looks right."),
        ],
    );
    assert_eq!(
        said.url.path(),
        "/2.0/repositories/acme/web/pullrequests/12/comments"
    );
    assert_eq!(body(&said), json!({"content": {"raw": "Looks right."}}));
    assert!(
        d.operations.iter().all(|op| !op.path.contains("/issues")),
        "the issue tracker is not in Atlassian's published description"
    );
}

#[test]
fn sentry_calls_the_accounts_region_and_carries_the_organization_in_every_path() {
    let d = def("sentry");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("de.sentry.io"), "{op}");
        assert!(
            req.url.path().starts_with("/api/0/organizations/acme/"),
            "{op}: {}",
            req.url.path()
        );
        assert!(
            req.url.path().ends_with('/'),
            "{op}: Sentry's paths end in a slash"
        );
    }
    let projects = signed(
        &d,
        request(&d, "projects", &[]),
        Credential::Bearer(Secret::new("sntryu_not_real")),
    );
    assert_eq!(projects.url.path(), "/api/0/organizations/acme/projects/");
    assert_eq!(
        projects.header("authorization"),
        Some("Bearer sntryu_not_real")
    );

    let unresolved = request(&d, "issues", &[]);
    assert!(
        !query(&unresolved).contains_key("query"),
        "no query sent: Sentry applies is:unresolved itself"
    );
    let errors = request(
        &d,
        "issues",
        &[
            ("query", "is:unresolved level:error"),
            ("period", "24h"),
            ("max", "25"),
        ],
    );
    assert_eq!(errors.url.path(), "/api/0/organizations/acme/issues/");
    assert_eq!(query(&errors)["query"], "is:unresolved level:error");
    assert_eq!(query(&errors)["statsPeriod"], "24h");
    assert_eq!(query(&errors)["limit"], "25");
    let events = request(&d, "issue_events", &[("issue", "123"), ("max", "10")]);
    assert_eq!(
        events.url.path(),
        "/api/0/organizations/acme/issues/123/events/"
    );
    assert_eq!(query(&events)["per_page"], "10");

    let resolved = request(
        &d,
        "update_issue",
        &[("issue", "123"), ("status", "resolved")],
    );
    assert_eq!(resolved.method.as_str(), "PUT");
    assert_eq!(resolved.url.path(), "/api/0/organizations/acme/issues/123/");
    assert_eq!(
        body(&resolved),
        json!({"status": "resolved"}),
        "only the attributes submitted are modified"
    );
    assert_eq!(d.hosts, vec!["*.sentry.io"]);
}

#[test]
fn pagerduty_sends_its_key_as_a_token_names_the_version_and_writes_as_the_accounts_user() {
    let d = def("pagerduty");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("api.pagerduty.com"), "{op}");
        assert_eq!(
            req.header("accept"),
            Some("application/vnd.pagerduty+json;version=2"),
            "{op}"
        );
        let writes = req.method.as_str() != "GET";
        assert_eq!(
            req.header("from"),
            writes.then_some("ops@acme.example"),
            "{op}: a write is made as the account's user, a read names nobody"
        );
    }
    let check = signed(&d, request(&d, "abilities", &[]), key("u+notARealKey"));
    assert_eq!(check.url.path(), "/abilities");
    assert_eq!(
        check.header("authorization"),
        Some("Token token=u+notARealKey")
    );

    let open = request(
        &d,
        "incidents",
        &[
            ("status", "triggered"),
            ("max", "50"),
            ("sort", "created_at:desc"),
        ],
    );
    let q = query(&open);
    assert_eq!(q["statuses[]"], "triggered");
    assert_eq!(q["limit"], "50");
    assert_eq!(q["sort_by"], "created_at:desc");
    assert!(
        !q.contains_key("since"),
        "the last month, by PagerDuty's own default"
    );
    let on_call = request(
        &d,
        "oncalls",
        &[("earliest", "true"), ("policy", "PABC123")],
    );
    assert_eq!(query(&on_call)["earliest"], "true");
    assert_eq!(query(&on_call)["escalation_policy_ids[]"], "PABC123");
    for (op, member) in [
        ("abilities", "abilities"),
        ("incidents", "incidents"),
        ("incident", "incident"),
        ("oncalls", "oncalls"),
        ("services", "services"),
        ("create_incident", "incident"),
        ("add_note", "note"),
    ] {
        let op = d.operation(&OperationId::new(op).unwrap()).unwrap();
        assert_eq!(op.output.select.as_deref(), Some(member), "{}", op.id);
    }

    let created = request(
        &d,
        "create_incident",
        &[
            ("title", "The site is down"),
            ("service", "PWIXJZS"),
            ("details", "502 from the edge."),
        ],
    );
    assert_eq!(created.url.path(), "/incidents");
    assert_eq!(
        body(&created),
        json!({"incident": {
            "type": "incident",
            "title": "The site is down",
            "service": {"id": "PWIXJZS", "type": "service_reference"},
            "body": {"type": "incident_body", "details": "502 from the edge."}
        }}),
        "no urgency typed, none sent: the service's own rule decides"
    );
    let noted = request(
        &d,
        "add_note",
        &[("id", "PT4KHLK"), ("content", "Rolled back.")],
    );
    assert_eq!(noted.url.path(), "/incidents/PT4KHLK/notes");
    assert_eq!(body(&noted), json!({"note": {"content": "Rolled back."}}));
}

#[test]
fn discord_signs_as_a_bot_names_itself_as_discord_asks_and_pins_v10() {
    let d = def("discord");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("discord.com"), "{op}");
        assert!(req.url.path().starts_with("/api/v10/"), "{op}");
        let agent = req.header("user-agent").unwrap_or_default();
        assert!(
            agent.starts_with("DiscordBot (https://") && agent.ends_with(')'),
            "{op}: `DiscordBot ($url, $versionNumber)`, got {agent:?}"
        );
    }
    let me = signed(&d, request(&d, "me", &[]), key("bot-token-not-real"));
    assert_eq!(me.url.path(), "/api/v10/users/@me");
    assert_eq!(me.header("authorization"), Some("Bot bot-token-not-real"));
    assert_eq!(
        me.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
            .count(),
        1,
        "the definition's user agent stands; the client adds none beside it"
    );

    let servers = request(&d, "guilds", &[("max", "50")]);
    assert_eq!(servers.url.path(), "/api/v10/users/@me/guilds");
    assert_eq!(query(&servers)["limit"], "50");
    let channels = request(&d, "channels", &[("guild", "81384788765712384")]);
    assert_eq!(
        channels.url.path(),
        "/api/v10/guilds/81384788765712384/channels"
    );
    let read = request(&d, "messages", &[("channel", "41771983423143937")]);
    assert_eq!(
        read.url.path(),
        "/api/v10/channels/41771983423143937/messages"
    );
    assert!(
        query(&read).is_empty(),
        "fifty, newest first, by Discord's own default"
    );
    let sent = request(
        &d,
        "send_message",
        &[
            ("channel", "41771983423143937"),
            ("content", "Deploy finished."),
        ],
    );
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(body(&sent), json!({"content": "Deploy finished."}));
}

// --- documents and mail: Google Sheets, Docs, Tasks; Outlook Mail, Calendar; OneDrive ---

const OFFICE: [&str; 6] = [
    "google-sheets",
    "google-docs",
    "google-tasks",
    "outlook-mail",
    "outlook-calendar",
    "onedrive",
];

/// What changes something says so, and nothing in the six deletes.
#[test]
fn the_office_connectors_mark_every_write_and_delete_nothing() {
    for slug in OFFICE {
        let d = def(slug);
        for op in &d.operations {
            assert_ne!(
                op.method,
                bisa_core::HttpMethod::Delete,
                "{slug}.{}: no built-in deletes",
                op.id
            );
            assert_eq!(
                op.writes,
                op.method != bisa_core::HttpMethod::Get,
                "{slug}.{}: a call that is not a GET is a write, and says so",
                op.id
            );
        }
    }
}

/// Sheets and Docs have no read without an id, so they name no check — and
/// they are the only built-ins that do not.
#[test]
fn only_sheets_and_docs_name_no_check() {
    let without: Vec<&str> = CATALOG
        .connectors
        .iter()
        .map(|(slug, _)| *slug)
        .filter(|slug| def(slug).check.is_none())
        .collect();
    assert_eq!(without, CHECKLESS);
}

#[test]
fn google_sheets_carries_a_range_in_the_path_and_names_how_a_write_is_read() {
    let d = def("google-sheets");
    assert_eq!(d.base_url, "https://sheets.googleapis.com");
    let meta = request(&d, "get_spreadsheet", &[("id", "1BxiMVs0XRA5")]);
    assert_eq!(meta.url.path(), "/v4/spreadsheets/1BxiMVs0XRA5");
    assert!(query(&meta).is_empty(), "no grid data unless asked");

    let read = request(
        &d,
        "read_values",
        &[("id", "1BxiMVs0XRA5"), ("range", "Sheet1!A1:D20")],
    );
    assert_eq!(read.method.as_str(), "GET");
    assert_eq!(
        decoded(read.url.path()),
        "/v4/spreadsheets/1BxiMVs0XRA5/values/Sheet1!A1:D20"
    );
    let spaced = request(
        &d,
        "read_values",
        &[("id", "1BxiMVs0XRA5"), ("range", "Q3 plan!A1/B2")],
    );
    assert_eq!(
        spaced.url.path().matches('/').count(),
        5,
        "a slash in a range stays inside its segment: {}",
        spaced.url.path()
    );
    assert_eq!(
        decoded(spaced.url.path()),
        "/v4/spreadsheets/1BxiMVs0XRA5/values/Q3 plan!A1/B2"
    );
    assert_eq!(
        def("google-sheets")
            .operation(&OperationId::new("read_values").unwrap())
            .unwrap()
            .output
            .select,
        None,
        "the whole ValueRange: Google does not say `values` is always there"
    );

    let append = request(
        &d,
        "append_values",
        &[
            ("id", "1BxiMVs0XRA5"),
            ("range", "Sheet1!A1"),
            ("values", r#"[["Ada", 36], ["Grace", 45]]"#),
        ],
    );
    assert_eq!(append.method.as_str(), "POST");
    assert_eq!(
        decoded(append.url.path()),
        "/v4/spreadsheets/1BxiMVs0XRA5/values/Sheet1!A1:append"
    );
    assert_eq!(
        query(&append),
        BTreeMap::from([("valueInputOption".to_string(), "USER_ENTERED".to_string())]),
        "a ValueInputOption must be named; USER_ENTERED unless the step says"
    );
    assert_eq!(
        body(&append),
        json!({"values": [["Ada", 36], ["Grace", 45]]})
    );

    let write = request(
        &d,
        "update_values",
        &[
            ("id", "1BxiMVs0XRA5"),
            ("range", "Sheet1!A2:B2"),
            ("values", r#"[["=1+1", ""]]"#),
            ("input", "RAW"),
        ],
    );
    assert_eq!(write.method.as_str(), "PUT");
    assert_eq!(
        decoded(write.url.path()),
        "/v4/spreadsheets/1BxiMVs0XRA5/values/Sheet1!A2:B2"
    );
    assert_eq!(query(&write)["valueInputOption"], "RAW");
    assert_eq!(body(&write), json!({"values": [["=1+1", ""]]}));

    let created = request(&d, "create_spreadsheet", &[("title", "Q3 pipeline")]);
    assert_eq!(created.url.path(), "/v4/spreadsheets");
    assert_eq!(
        body(&created),
        json!({"properties": {"title": "Q3 pipeline"}})
    );
}

#[test]
fn google_docs_creates_by_title_and_appends_at_the_end_of_the_body() {
    let d = def("google-docs");
    assert_eq!(d.base_url, "https://docs.googleapis.com");
    assert_eq!(
        d.operations
            .iter()
            .map(|op| op.id.to_string())
            .collect::<Vec<_>>(),
        ["get_document", "create_document", "append_text"],
        "the API's three methods, and no more"
    );
    let read = request(&d, "get_document", &[("id", "195j9eDD3ccgjQ")]);
    assert_eq!(read.url.path(), "/v1/documents/195j9eDD3ccgjQ");
    let created = request(&d, "create_document", &[("title", "Meeting notes")]);
    assert_eq!(created.url.path(), "/v1/documents");
    assert_eq!(body(&created), json!({"title": "Meeting notes"}));

    let appended = request(
        &d,
        "append_text",
        &[
            ("id", "195j9eDD3ccgjQ"),
            ("text", "Decided: ship.\nNext: tell the team."),
        ],
    );
    assert_eq!(appended.method.as_str(), "POST");
    assert_eq!(
        decoded(appended.url.path()),
        "/v1/documents/195j9eDD3ccgjQ:batchUpdate"
    );
    assert_eq!(
        body(&appended),
        json!({"requests": [{"insertText": {
            "text": "Decided: ship.\nNext: tell the team.",
            "endOfSegmentLocation": {"segmentId": ""}
        }}]}),
        "an empty segment id is the document's body"
    );
}

#[test]
fn google_tasks_reads_a_missing_list_as_empty_and_patches_only_what_was_set() {
    let d = def("google-tasks");
    assert_eq!(d.base_url, "https://tasks.googleapis.com");
    let lists = request(&d, "task_lists", &[]);
    assert_eq!(decoded(lists.url.path()), "/tasks/v1/users/@me/lists");
    assert!(query(&lists).is_empty());
    for op in ["task_lists", "list_tasks"] {
        let out = &d.operation(&OperationId::new(op).unwrap()).unwrap().output;
        assert_eq!(out.select.as_deref(), Some("items"), "{op}");
        assert!(out.or_empty, "{op}: an answer with no `items` is no tasks");
    }

    let tasks = request(&d, "list_tasks", &[("list", "MDk4")]);
    assert_eq!(tasks.url.path(), "/tasks/v1/lists/MDk4/tasks");
    assert!(query(&tasks).is_empty(), "Google's own defaults");
    let narrowed = request(
        &d,
        "list_tasks",
        &[
            ("list", "MDk4"),
            ("max", "100"),
            ("completed", "false"),
            ("hidden", "true"),
            ("due_from", "2026-09-07T00:00:00Z"),
        ],
    );
    assert_eq!(
        query(&narrowed),
        BTreeMap::from([
            ("maxResults".to_string(), "100".to_string()),
            ("showCompleted".to_string(), "false".to_string()),
            ("showHidden".to_string(), "true".to_string()),
            ("dueMin".to_string(), "2026-09-07T00:00:00Z".to_string()),
        ])
    );
    let one = request(&d, "get_task", &[("list", "MDk4"), ("task", "abc")]);
    assert_eq!(one.url.path(), "/tasks/v1/lists/MDk4/tasks/abc");

    let created = request(
        &d,
        "create_task",
        &[("list", "MDk4"), ("title", "Send the invoice")],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/tasks/v1/lists/MDk4/tasks");
    assert_eq!(body(&created), json!({"title": "Send the invoice"}));

    let done = request(
        &d,
        "update_task",
        &[("list", "MDk4"), ("task", "abc"), ("status", "completed")],
    );
    assert_eq!(done.method.as_str(), "PATCH");
    assert_eq!(done.url.path(), "/tasks/v1/lists/MDk4/tasks/abc");
    assert_eq!(
        body(&done),
        json!({"status": "completed"}),
        "patch semantics: what the step left empty is not sent"
    );
}

/// Microsoft's identity platform, as its pages describe a desktop app's
/// flow: the `common` tenant, PKCE, scopes joined by a space, a refresh
/// token only with `offline_access`, and one host for both ends.
#[test]
fn microsoft_signs_in_on_the_common_tenant_with_pkce_and_asks_for_a_refresh_token() {
    for (slug, permission) in [
        ("outlook-mail", "Mail.Send"),
        ("outlook-calendar", "Calendars.ReadWrite"),
        ("onedrive", "Files.ReadWrite"),
    ] {
        let d = def(slug);
        let consent = authorize(&d);
        assert_eq!(consent["@host"], "login.microsoftonline.com", "{slug}");
        assert_eq!(consent["response_type"], "code", "{slug}");
        assert_eq!(consent["code_challenge_method"], "S256", "{slug}");
        assert_eq!(consent["client_id"], "the-client", "{slug}");
        assert_eq!(
            consent["redirect_uri"], "http://127.0.0.1:4478/connectors/oauth/callback",
            "{slug}"
        );
        let scopes: Vec<&str> = consent["scope"].split(' ').collect();
        assert!(scopes.contains(&"offline_access"), "{slug}: {scopes:?}");
        assert!(scopes.contains(&permission), "{slug}: {scopes:?}");
        assert!(
            !consent.contains_key("access_type"),
            "{slug}: Google's word"
        );
        assert_eq!(
            d.oauth_hosts(),
            vec!["login.microsoftonline.com".to_string()],
            "{slug}"
        );
        let bisa_core::AuthScheme::OAuth2 {
            authorization_url,
            token_url,
            ..
        } = &d.auth
        else {
            panic!("{slug}: oauth2");
        };
        assert_eq!(
            authorization_url,
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
        );
        assert_eq!(
            token_url,
            "https://login.microsoftonline.com/common/oauth2/v2.0/token"
        );
        for (op, req) in every_request(&d) {
            assert_eq!(req.url.host_str(), Some("graph.microsoft.com"), "{op}");
            assert!(req.url.path().starts_with("/v1.0/me"), "{slug}.{op}");
        }
    }
}

#[test]
fn outlook_mail_pages_with_top_and_sends_one_plain_text_message() {
    let d = def("outlook-mail");
    assert_eq!(request(&d, "me", &[]).url.path(), "/v1.0/me");
    let all = request(&d, "list_messages", &[]);
    assert_eq!(all.url.path(), "/v1.0/me/messages");
    assert!(query(&all).is_empty(), "ten, by Graph's own default");
    let unread = request(
        &d,
        "list_messages",
        &[("filter", "isRead eq false"), ("max", "25")],
    );
    assert_eq!(
        query(&unread),
        BTreeMap::from([
            ("$filter".to_string(), "isRead eq false".to_string()),
            ("$top".to_string(), "25".to_string()),
        ]),
        "the `$` is always written"
    );
    let one = request(&d, "get_message", &[("id", "AAMkAGI2=")]);
    assert_eq!(decoded(one.url.path()), "/v1.0/me/messages/AAMkAGI2=");

    let sent = request(
        &d,
        "send_mail",
        &[
            ("to", "frannis@contoso.com"),
            ("subject", "Meet for lunch?"),
            ("body", "The new cafeteria is open."),
        ],
    );
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(sent.url.path(), "/v1.0/me/sendMail");
    assert_eq!(
        body(&sent),
        json!({"message": {
            "subject": "Meet for lunch?",
            "body": {"contentType": "Text", "content": "The new cafeteria is open."},
            "toRecipients": [{"emailAddress": {"address": "frannis@contoso.com"}}]
        }})
    );
}

#[test]
fn outlook_calendar_reads_a_view_between_two_moments_and_invites_nobody() {
    let d = def("outlook-calendar");
    assert_eq!(
        request(&d, "calendars", &[]).url.path(),
        "/v1.0/me/calendars"
    );
    let view = request(
        &d,
        "list_events",
        &[
            ("from", "2026-09-07T00:00:00-08:00"),
            ("to", "2026-09-14T00:00:00-08:00"),
        ],
    );
    assert_eq!(view.url.path(), "/v1.0/me/calendar/calendarView");
    assert_eq!(
        query(&view),
        BTreeMap::from([
            (
                "startDateTime".to_string(),
                "2026-09-07T00:00:00-08:00".to_string()
            ),
            (
                "endDateTime".to_string(),
                "2026-09-14T00:00:00-08:00".to_string()
            ),
        ])
    );
    let one = request(&d, "get_event", &[("id", "AAMkAGI1=")]);
    assert_eq!(decoded(one.url.path()), "/v1.0/me/events/AAMkAGI1=");

    let created = request(
        &d,
        "create_event",
        &[
            ("subject", "Let's go for lunch"),
            ("description", "Does noon work for you?"),
            ("start", "2017-04-15T12:00:00"),
            ("end", "2017-04-15T14:00:00"),
            ("timezone", "Pacific Standard Time"),
        ],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/v1.0/me/events");
    let sent = body(&created);
    assert_eq!(
        sent,
        json!({
            "subject": "Let's go for lunch",
            "body": {"contentType": "Text", "content": "Does noon work for you?"},
            "start": {"dateTime": "2017-04-15T12:00:00", "timeZone": "Pacific Standard Time"},
            "end": {"dateTime": "2017-04-15T14:00:00", "timeZone": "Pacific Standard Time"}
        })
    );
    assert!(
        sent.get("attendees").is_none(),
        "attendees would be sent invitations"
    );
}

#[test]
fn onedrive_searches_inside_the_path_and_never_replaces_a_folder() {
    let d = def("onedrive");
    assert_eq!(request(&d, "drive", &[]).url.path(), "/v1.0/me/drive");
    let top = request(&d, "list_root", &[("max", "50")]);
    assert_eq!(top.url.path(), "/v1.0/me/drive/root/children");
    assert_eq!(query(&top)["$top"], "50");
    let inside = request(&d, "list_children", &[("id", "01BYE5RZ6QN3")]);
    assert_eq!(
        inside.url.path(),
        "/v1.0/me/drive/items/01BYE5RZ6QN3/children"
    );
    assert!(query(&inside).is_empty(), "200, by Graph's own default");
    let item = request(&d, "get_item", &[("id", "01BYE5RZ6QN3")]);
    assert_eq!(item.url.path(), "/v1.0/me/drive/items/01BYE5RZ6QN3");

    let found = request(&d, "search", &[("q", "Contoso Project")]);
    assert_eq!(
        decoded(found.url.path()),
        "/v1.0/me/drive/root/search(q='Contoso Project')"
    );
    assert!(
        !found.url.path().contains(' '),
        "a space is percent-encoded: {}",
        found.url.path()
    );
    // Text that would close the quote or leave the segment stays text.
    let odd = request(&d, "search", &[("q", "a')/b")]);
    assert_eq!(
        odd.url.path().matches('/').count(),
        5,
        "one segment: {}",
        odd.url.path()
    );

    for (op, params, path) in [
        (
            "create_folder",
            vec![("parent", "01BYE5RZ6QN3"), ("name", "New Folder")],
            "/v1.0/me/drive/items/01BYE5RZ6QN3/children",
        ),
        (
            "create_root_folder",
            vec![("name", "New Folder")],
            "/v1.0/me/drive/root/children",
        ),
    ] {
        let made = request(&d, op, &params);
        assert_eq!(made.method.as_str(), "POST", "{op}");
        assert_eq!(made.url.path(), path, "{op}");
        assert_eq!(
            body(&made),
            json!({
                "name": "New Folder",
                "folder": {},
                "@microsoft.graph.conflictBehavior": "rename"
            }),
            "{op}: a name already taken gets a new one"
        );
    }
    assert!(
        d.operations
            .iter()
            .all(|op| op.method != bisa_core::HttpMethod::Put),
        "no upload: the reference does not say a PUT leaves a file that is there"
    );
}

// --- collaboration and CRM: Microsoft Teams, Zoom, HubSpot, Pipedrive, Calendly, Figma ---

const COLLABORATION: [&str; 6] = [
    "microsoft-teams",
    "zoom",
    "hubspot",
    "pipedrive",
    "calendly",
    "figma",
];

/// What changes something says so, and nothing in the six deletes.
#[test]
fn the_collaboration_connectors_mark_every_write_and_delete_nothing() {
    for slug in COLLABORATION {
        let d = def(slug);
        for op in &d.operations {
            assert_ne!(
                op.method,
                bisa_core::HttpMethod::Delete,
                "{slug}.{}: no built-in deletes",
                op.id
            );
            assert_eq!(
                op.writes,
                op.method != bisa_core::HttpMethod::Get,
                "{slug}.{}: a call that is not a GET is a write, and says so",
                op.id
            );
        }
    }
}

#[test]
fn teams_signs_in_work_accounts_only_and_posts_a_message_by_its_body() {
    let d = def("microsoft-teams");
    let consent = authorize(&d);
    assert_eq!(consent["@host"], "login.microsoftonline.com");
    assert_eq!(consent["code_challenge_method"], "S256");
    let scopes: Vec<&str> = consent["scope"].split(' ').collect();
    for scope in [
        "offline_access",
        "Team.ReadBasic.All",
        "Channel.ReadBasic.All",
        "ChannelMessage.Read.All",
        "ChannelMessage.Send",
        "Chat.Read",
    ] {
        assert!(scopes.contains(&scope), "{scope}: {scopes:?}");
    }
    let bisa_core::AuthScheme::OAuth2 {
        authorization_url,
        token_url,
        ..
    } = &d.auth
    else {
        panic!("oauth2");
    };
    assert_eq!(
        authorization_url, "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize",
        "a personal Microsoft account is not supported by any call here"
    );
    assert_eq!(
        token_url,
        "https://login.microsoftonline.com/organizations/oauth2/v2.0/token"
    );

    assert_eq!(
        request(&d, "joined_teams", &[]).url.path(),
        "/v1.0/me/joinedTeams"
    );
    let team = "fbe2bf47-16c8-47cf-b4a5-4b9b187c508b";
    let channel = "19:4a95f7d8db4c4e7fae857bcebe0623e6@thread.tacv2";
    let channels = request(&d, "channels", &[("team", team)]);
    assert_eq!(channels.url.path(), format!("/v1.0/teams/{team}/channels"));
    let read = request(
        &d,
        "channel_messages",
        &[("team", team), ("channel", channel), ("max", "50")],
    );
    assert_eq!(
        decoded(read.url.path()),
        format!("/v1.0/teams/{team}/channels/{channel}/messages")
    );
    assert_eq!(query(&read)["$top"], "50");

    let posted = request(
        &d,
        "send_channel_message",
        &[
            ("team", team),
            ("channel", channel),
            ("content", "Hello World"),
        ],
    );
    assert_eq!(posted.method.as_str(), "POST");
    assert_eq!(
        decoded(posted.url.path()),
        format!("/v1.0/teams/{team}/channels/{channel}/messages")
    );
    assert_eq!(body(&posted), json!({"body": {"content": "Hello World"}}));
    let replied = request(
        &d,
        "reply",
        &[
            ("team", team),
            ("channel", channel),
            ("message", "1616990032035"),
            ("content", "Hello World"),
        ],
    );
    assert_eq!(
        decoded(replied.url.path()),
        format!("/v1.0/teams/{team}/channels/{channel}/messages/1616990032035/replies")
    );
    assert_eq!(body(&replied), json!({"body": {"content": "Hello World"}}));

    assert_eq!(request(&d, "chats", &[]).url.path(), "/v1.0/me/chats");
    let chat = request(&d, "chat_messages", &[("chat", "19:abc@thread.v2")]);
    assert_eq!(
        decoded(chat.url.path()),
        "/v1.0/me/chats/19:abc@thread.v2/messages"
    );
    assert!(
        d.operations
            .iter()
            .filter(|op| op.writes)
            .all(|op| op.path.contains("/channels/")),
        "nothing is sent into a chat"
    );
}

#[test]
fn zoom_trades_the_apps_credentials_for_a_token_and_names_the_user_in_every_path() {
    let d = def("zoom");
    assert_eq!(
        serde_json::to_value(&d.auth).unwrap(),
        json!({
            "scheme": "client_credentials",
            "token_url": "https://zoom.us/oauth/token",
            "token_auth": "basic",
            "grant_type": "account_credentials",
            "params": {"account_id": "{account.account_id}"}
        }),
        "a Server-to-Server OAuth app: Basic, Zoom's own grant, the account's id"
    );
    assert_eq!(d.oauth_hosts(), vec!["zoom.us".to_string()]);

    let list = request(&d, "list_meetings", &[]);
    assert_eq!(list.url.host_str(), Some("api.zoom.us"));
    assert_eq!(
        decoded(list.url.path()),
        "/v2/users/host@acme.example/meetings",
        "never `me` with this kind of app"
    );
    assert!(
        query(&list).is_empty(),
        "scheduled, thirty, by Zoom's default"
    );
    let upcoming = request(&d, "list_meetings", &[("type", "upcoming"), ("max", "100")]);
    assert_eq!(
        query(&upcoming),
        BTreeMap::from([
            ("type".to_string(), "upcoming".to_string()),
            ("page_size".to_string(), "100".to_string()),
        ])
    );
    assert_eq!(
        request(&d, "get_meeting", &[("id", "85746065")]).url.path(),
        "/v2/meetings/85746065"
    );

    let made = request(
        &d,
        "create_meeting",
        &[("topic", "Weekly sync"), ("start", "2026-09-07T12:00:00Z")],
    );
    assert_eq!(made.method.as_str(), "POST");
    assert_eq!(
        decoded(made.url.path()),
        "/v2/users/host@acme.example/meetings"
    );
    assert_eq!(
        body(&made),
        json!({"topic": "Weekly sync", "start_time": "2026-09-07T12:00:00Z"}),
        "a scheduled meeting of sixty minutes, by Zoom's defaults"
    );
    let timed = request(
        &d,
        "create_meeting",
        &[
            ("topic", "Weekly sync"),
            ("start", "2026-09-07T12:00:00"),
            ("timezone", "Europe/Paris"),
            ("duration", "30"),
        ],
    );
    assert_eq!(
        body(&timed),
        json!({
            "topic": "Weekly sync",
            "start_time": "2026-09-07T12:00:00",
            "timezone": "Europe/Paris",
            "duration": 30
        })
    );
}

#[test]
fn hubspot_sends_a_bearer_and_creates_a_record_from_its_properties() {
    let d = def("hubspot");
    for (op, req) in every_request_with(&d, r#"{"x": "y"}"#) {
        assert_eq!(req.url.host_str(), Some("api.hubapi.com"), "{op}");
    }
    let check = signed(
        &d,
        request(&d, "account", &[]),
        Credential::Bearer(Secret::new("token-not-real")),
    );
    assert_eq!(check.url.path(), "/account-info/v3/details");
    assert_eq!(check.header("authorization"), Some("Bearer token-not-real"));

    let list = request(
        &d,
        "list_contacts",
        &[("max", "100"), ("properties", "email,firstname")],
    );
    assert_eq!(list.url.path(), "/crm/v3/objects/contacts");
    assert_eq!(
        query(&list),
        BTreeMap::from([
            ("limit".to_string(), "100".to_string()),
            ("properties".to_string(), "email,firstname".to_string()),
        ])
    );
    let by_email = request(
        &d,
        "get_contact",
        &[("id", "ada@example.com"), ("id_property", "email")],
    );
    assert_eq!(
        decoded(by_email.url.path()),
        "/crm/v3/objects/contacts/ada@example.com"
    );
    assert_eq!(query(&by_email)["idProperty"], "email");

    let created = request(
        &d,
        "create_contact",
        &[(
            "properties",
            r#"{"email": "ada@example.com", "firstname": "Ada"}"#,
        )],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/crm/v3/objects/contacts");
    assert_eq!(
        body(&created),
        json!({
            "properties": {"email": "ada@example.com", "firstname": "Ada"},
            "associations": []
        }),
        "the published description marks `associations` required; none is made"
    );
    let updated = request(
        &d,
        "update_contact",
        &[("id", "512"), ("properties", r#"{"phone": "+1 555 0100"}"#)],
    );
    assert_eq!(updated.method.as_str(), "PATCH");
    assert_eq!(updated.url.path(), "/crm/v3/objects/contacts/512");
    assert_eq!(
        body(&updated),
        json!({"properties": {"phone": "+1 555 0100"}})
    );

    assert_eq!(
        request(&d, "list_deals", &[]).url.path(),
        "/crm/v3/objects/deals"
    );
    assert_eq!(
        request(&d, "get_deal", &[("id", "77")]).url.path(),
        "/crm/v3/objects/deals/77"
    );
    let deal = request(
        &d,
        "create_deal",
        &[(
            "properties",
            r#"{"dealname": "Acme renewal", "dealstage": "appointmentscheduled"}"#,
        )],
    );
    assert_eq!(deal.url.path(), "/crm/v3/objects/deals");
    assert_eq!(
        body(&deal),
        json!({"properties": {"dealname": "Acme renewal", "dealstage": "appointmentscheduled"}})
    );
    assert_eq!(
        request(&d, "list_companies", &[]).url.path(),
        "/crm/v3/objects/companies"
    );
    let company = request(
        &d,
        "create_company",
        &[(
            "properties",
            r#"{"name": "Acme", "domain": "acme.example"}"#,
        )],
    );
    assert_eq!(company.url.path(), "/crm/v3/objects/companies");
    assert_eq!(
        body(&company),
        json!({"properties": {"name": "Acme", "domain": "acme.example"}})
    );
    assert!(
        d.operations.iter().all(|op| !op.path.ends_with("/search")),
        "no search: the description and the guide disagree on its body"
    );
}

/// Every operation built with a placeholder for each required parameter,
/// a JSON one given the value named.
fn every_request_with(def: &Connector, json_value: &str) -> Vec<(String, Request)> {
    def.operations
        .iter()
        .map(|op| {
            let params: Vec<(&str, &str)> = op
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| {
                    let value = match p.kind {
                        bisa_core::ParamKind::Number => "7",
                        bisa_core::ParamKind::Json => json_value,
                        _ => "xy",
                    };
                    (p.name.as_str(), value)
                })
                .collect();
            (op.id.to_string(), request(def, op.id.as_str(), &params))
        })
        .collect()
}

#[test]
fn pipedrive_calls_the_companys_own_domain_with_its_token_in_a_header() {
    let d = def("pipedrive");
    for (op, req) in every_request_with(&d, "[]") {
        assert_eq!(req.url.host_str(), Some("acme.pipedrive.com"), "{op}");
        assert!(
            req.url.path().starts_with("/api/v2/") || req.url.path().starts_with("/api/v1/"),
            "{op}: {}",
            req.url.path()
        );
        assert!(
            !req.url.as_str().contains("api_token"),
            "{op}: the token never rides in the address"
        );
    }
    let me = signed(&d, request(&d, "me", &[]), key("token-not-real"));
    assert_eq!(me.url.path(), "/api/v1/users/me");
    assert_eq!(me.header("x-api-token"), Some("token-not-real"));
    assert!(me.url.query().is_none());

    let deals = request(&d, "list_deals", &[("status", "open"), ("max", "500")]);
    assert_eq!(deals.url.path(), "/api/v2/deals");
    assert_eq!(
        query(&deals),
        BTreeMap::from([
            ("status".to_string(), "open".to_string()),
            ("limit".to_string(), "500".to_string()),
        ])
    );
    assert_eq!(
        request(&d, "get_deal", &[("id", "42")]).url.path(),
        "/api/v2/deals/42"
    );
    let added = request(
        &d,
        "create_deal",
        &[
            ("title", "Acme renewal"),
            ("value", "1200.5"),
            ("person", "7"),
        ],
    );
    assert_eq!(added.method.as_str(), "POST");
    assert_eq!(
        body(&added),
        json!({"title": "Acme renewal", "value": 1200.5, "person_id": 7}),
        "numbers are numbers: v2 takes JSON only"
    );
    let won = request(&d, "update_deal", &[("id", "42"), ("status", "won")]);
    assert_eq!(won.method.as_str(), "PATCH");
    assert_eq!(won.url.path(), "/api/v2/deals/42");
    assert_eq!(body(&won), json!({"status": "won"}));

    let person = request(
        &d,
        "create_person",
        &[
            ("name", "Ada Lovelace"),
            (
                "emails",
                r#"[{"value": "ada@example.com", "primary": true}]"#,
            ),
        ],
    );
    assert_eq!(person.url.path(), "/api/v2/persons");
    assert_eq!(
        body(&person),
        json!({
            "name": "Ada Lovelace",
            "emails": [{"value": "ada@example.com", "primary": true}]
        }),
        "emails are a list of objects in v2"
    );
    let org = request(&d, "create_organization", &[("name", "Acme")]);
    assert_eq!(org.url.path(), "/api/v2/organizations");
    assert_eq!(body(&org), json!({"name": "Acme"}));

    let found = request(&d, "search", &[("term", "acme"), ("types", "deal,person")]);
    assert_eq!(found.url.path(), "/api/v2/itemSearch");
    assert_eq!(
        query(&found),
        BTreeMap::from([
            ("term".to_string(), "acme".to_string()),
            ("item_types".to_string(), "deal,person".to_string()),
        ])
    );
    let note = request(
        &d,
        "add_note",
        &[("content", "<p>Called; renewing.</p>"), ("deal", "42")],
    );
    assert_eq!(note.url.path(), "/api/v1/notes", "notes are still v1");
    assert_eq!(
        body(&note),
        json!({"content": "<p>Called; renewing.</p>", "deal_id": 42})
    );
}

#[test]
fn calendly_names_resources_by_uri_and_makes_a_link_for_one_meeting() {
    let d = def("calendly");
    let me = signed(
        &d,
        request(&d, "me", &[]),
        Credential::Bearer(Secret::new("token-not-real")),
    );
    assert_eq!(me.url.as_str(), "https://api.calendly.com/users/me");
    assert_eq!(me.header("authorization"), Some("Bearer token-not-real"));

    let user = "https://api.calendly.com/users/AAAAAAAAAAAAAAAA";
    let types = request(&d, "event_types", &[("user", user), ("active", "true")]);
    assert_eq!(types.url.path(), "/event_types");
    assert_eq!(
        query(&types),
        BTreeMap::from([
            ("user".to_string(), user.to_string()),
            ("active".to_string(), "true".to_string()),
        ]),
        "a resource reference is its whole URI"
    );
    let events = request(
        &d,
        "scheduled_events",
        &[
            ("user", user),
            ("status", "active"),
            ("from", "2026-09-07T00:00:00.000000Z"),
            ("sort", "start_time:asc"),
            ("max", "50"),
        ],
    );
    assert_eq!(events.url.path(), "/scheduled_events");
    assert_eq!(
        query(&events),
        BTreeMap::from([
            ("user".to_string(), user.to_string()),
            ("status".to_string(), "active".to_string()),
            (
                "min_start_time".to_string(),
                "2026-09-07T00:00:00.000000Z".to_string()
            ),
            ("sort".to_string(), "start_time:asc".to_string()),
            ("count".to_string(), "50".to_string()),
        ])
    );
    assert_eq!(
        request(&d, "get_event", &[("id", "BBBB")]).url.path(),
        "/scheduled_events/BBBB"
    );
    assert_eq!(
        request(&d, "invitees", &[("id", "BBBB")]).url.path(),
        "/scheduled_events/BBBB/invitees"
    );

    let event_type = "https://api.calendly.com/event_types/CCCC";
    let link = request(&d, "create_scheduling_link", &[("event_type", event_type)]);
    assert_eq!(link.method.as_str(), "POST");
    assert_eq!(link.url.path(), "/scheduling_links");
    assert_eq!(
        body(&link),
        json!({"max_event_count": 1, "owner": event_type, "owner_type": "EventType"})
    );
}

#[test]
fn figma_sends_its_token_in_its_own_header_and_replies_by_a_comments_id() {
    let d = def("figma");
    let me = signed(&d, request(&d, "me", &[]), key("figd-not-real"));
    assert_eq!(me.url.as_str(), "https://api.figma.com/v1/me");
    assert_eq!(me.header("x-figma-token"), Some("figd-not-real"));
    assert_eq!(me.header("authorization"), None, "a bearer is OAuth's form");

    let key_ = "hC4f7bJnA1Kx";
    assert_eq!(
        request(&d, "file_meta", &[("file", key_)]).url.path(),
        format!("/v1/files/{key_}/meta")
    );
    let file = request(&d, "get_file", &[("file", key_), ("depth", "2")]);
    assert_eq!(file.url.path(), format!("/v1/files/{key_}"));
    assert_eq!(
        query(&file),
        BTreeMap::from([("depth".to_string(), "2".to_string())])
    );
    let nodes = request(&d, "file_nodes", &[("file", key_), ("ids", "1:2,1:3")]);
    assert_eq!(nodes.url.path(), format!("/v1/files/{key_}/nodes"));
    assert_eq!(query(&nodes)["ids"], "1:2,1:3");
    let images = request(
        &d,
        "render_images",
        &[
            ("file", key_),
            ("ids", "1:2"),
            ("format", "png"),
            ("scale", "2"),
        ],
    );
    assert_eq!(images.url.path(), format!("/v1/images/{key_}"));
    assert_eq!(
        query(&images),
        BTreeMap::from([
            ("ids".to_string(), "1:2".to_string()),
            ("format".to_string(), "png".to_string()),
            ("scale".to_string(), "2".to_string()),
        ])
    );
    let comments = request(&d, "comments", &[("file", key_), ("markdown", "true")]);
    assert_eq!(comments.url.path(), format!("/v1/files/{key_}/comments"));
    assert_eq!(query(&comments)["as_md"], "true");

    let posted = request(
        &d,
        "post_comment",
        &[("file", key_), ("message", "Tighten the spacing here.")],
    );
    assert_eq!(posted.method.as_str(), "POST");
    assert_eq!(posted.url.path(), format!("/v1/files/{key_}/comments"));
    assert_eq!(
        body(&posted),
        json!({"message": "Tighten the spacing here."})
    );
    let reply = request(
        &d,
        "post_comment",
        &[("file", key_), ("message", "Done."), ("reply_to", "1234")],
    );
    assert_eq!(
        body(&reply),
        json!({"message": "Done.", "comment_id": "1234"})
    );
}

// --- work tracking: Airtable, Asana, ClickUp, monday.com, Todoist, Intercom ---

const WORK: [&str; 6] = [
    "airtable", "asana", "clickup", "monday", "todoist", "intercom",
];

fn bearer(value: &str) -> Credential {
    Credential::Bearer(Secret::new(value))
}

/// What changes something says so, and nothing in the six deletes. A POST
/// may be a read where the platform's query is one — a GraphQL query, a
/// search — and those are named here, one by one.
#[test]
fn the_work_connectors_mark_every_write_and_delete_nothing() {
    let reads_by_post = [
        ("monday", "me"),
        ("monday", "boards"),
        ("monday", "items"),
        ("monday", "get_item"),
        ("intercom", "search_contacts"),
    ];
    for slug in WORK {
        let d = def(slug);
        for op in &d.operations {
            assert_ne!(
                op.method,
                bisa_core::HttpMethod::Delete,
                "{slug}.{}: no built-in deletes",
                op.id
            );
            let read = op.method == bisa_core::HttpMethod::Get
                || reads_by_post.contains(&(slug, op.id.as_str()));
            assert_eq!(
                op.writes, !read,
                "{slug}.{}: what is not a read is a write, and says so",
                op.id
            );
        }
    }
}

#[test]
fn airtable_names_a_table_in_the_path_and_updates_with_a_patch() {
    let d = def("airtable");
    let who = signed(&d, request(&d, "whoami", &[]), bearer("pat-not-real"));
    assert_eq!(who.url.as_str(), "https://api.airtable.com/v0/meta/whoami");
    assert_eq!(who.header("authorization"), Some("Bearer pat-not-real"));
    assert_eq!(request(&d, "list_bases", &[]).url.path(), "/v0/meta/bases");
    assert_eq!(
        request(&d, "tables", &[("base", "appLkNDICXNqxSDhG")])
            .url
            .path(),
        "/v0/meta/bases/appLkNDICXNqxSDhG/tables"
    );

    let list = request(
        &d,
        "list_records",
        &[
            ("base", "appLkNDICXNqxSDhG"),
            ("table", "Open tasks"),
            ("max", "50"),
            ("formula", "{Status} = 'Open'"),
        ],
    );
    assert_eq!(
        decoded(list.url.path()),
        "/v0/appLkNDICXNqxSDhG/Open tasks",
        "a table by its name"
    );
    assert!(!list.url.path().contains(' '), "{}", list.url.path());
    assert_eq!(
        query(&list),
        BTreeMap::from([
            ("maxRecords".to_string(), "50".to_string()),
            (
                "filterByFormula".to_string(),
                "{Status} = 'Open'".to_string()
            ),
        ])
    );
    assert_eq!(
        request(
            &d,
            "get_record",
            &[
                ("base", "appLkNDICXNqxSDhG"),
                ("table", "tblA1"),
                ("record", "rec560UJdUtocSouk")
            ]
        )
        .url
        .path(),
        "/v0/appLkNDICXNqxSDhG/tblA1/rec560UJdUtocSouk"
    );

    let created = request(
        &d,
        "create_record",
        &[
            ("base", "appLkNDICXNqxSDhG"),
            ("table", "tblA1"),
            ("fields", r#"{"Name": "Ada", "Status": "Open"}"#),
        ],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/v0/appLkNDICXNqxSDhG/tblA1");
    assert_eq!(
        body(&created),
        json!({"fields": {"Name": "Ada", "Status": "Open"}}),
        "one record at the top level"
    );
    let updated = request(
        &d,
        "update_record",
        &[
            ("base", "appLkNDICXNqxSDhG"),
            ("table", "tblA1"),
            ("record", "rec560UJdUtocSouk"),
            ("fields", r#"{"Status": "Done"}"#),
            ("typecast", "true"),
        ],
    );
    assert_eq!(
        updated.method.as_str(),
        "PATCH",
        "a PUT would clear every field not named"
    );
    assert_eq!(
        body(&updated),
        json!({"fields": {"Status": "Done"}, "typecast": true})
    );
}

#[test]
fn asana_wraps_a_body_in_data_and_creates_a_task_in_a_project() {
    let d = def("asana");
    let me = signed(&d, request(&d, "me", &[]), bearer("pat-not-real"));
    assert_eq!(me.url.as_str(), "https://app.asana.com/api/1.0/users/me");
    assert_eq!(me.header("authorization"), Some("Bearer pat-not-real"));
    for op in &d.operations {
        assert_eq!(
            op.output.select.as_deref(),
            Some("data"),
            "{}: an answer comes under `data`",
            op.id
        );
    }
    let projects = request(&d, "projects", &[("workspace", "12345"), ("max", "100")]);
    assert_eq!(projects.url.path(), "/api/1.0/projects");
    assert_eq!(
        query(&projects),
        BTreeMap::from([
            ("workspace".to_string(), "12345".to_string()),
            ("limit".to_string(), "100".to_string()),
        ])
    );
    let open = request(
        &d,
        "project_tasks",
        &[("project", "67890"), ("completed_since", "now")],
    );
    assert_eq!(open.url.path(), "/api/1.0/projects/67890/tasks");
    assert_eq!(query(&open)["completed_since"], "now");
    assert_eq!(
        request(&d, "get_task", &[("task", "111")]).url.path(),
        "/api/1.0/tasks/111"
    );

    let created = request(
        &d,
        "create_task",
        &[
            ("project", "67890"),
            ("name", "Send the invoice"),
            ("due", "2026-09-07"),
        ],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/api/1.0/tasks");
    assert_eq!(
        body(&created),
        json!({"data": {
            "name": "Send the invoice",
            "projects": ["67890"],
            "due_on": "2026-09-07"
        }}),
        "the project names the workspace"
    );
    let done = request(&d, "update_task", &[("task", "111"), ("completed", "true")]);
    assert_eq!(done.method.as_str(), "PUT");
    assert_eq!(done.url.path(), "/api/1.0/tasks/111");
    assert_eq!(
        body(&done),
        json!({"data": {"completed": true}}),
        "only the fields in the data block are updated"
    );
    let said = request(&d, "comment", &[("task", "111"), ("text", "Sent.")]);
    assert_eq!(said.url.path(), "/api/1.0/tasks/111/stories");
    assert_eq!(body(&said), json!({"data": {"text": "Sent."}}));
}

#[test]
fn clickup_sends_its_token_bare_and_walks_from_a_workspace_to_a_task() {
    let d = def("clickup");
    let user = signed(&d, request(&d, "user", &[]), key("pk_not_real"));
    assert_eq!(user.url.as_str(), "https://api.clickup.com/api/v2/user");
    assert_eq!(
        user.header("authorization"),
        Some("pk_not_real"),
        "no Bearer: that form is OAuth's"
    );
    for (op, params, path) in [
        ("workspaces", vec![], "/api/v2/team"),
        ("spaces", vec![("workspace", "9")], "/api/v2/team/9/space"),
        ("folders", vec![("space", "8")], "/api/v2/space/8/folder"),
        ("lists", vec![("folder", "7")], "/api/v2/folder/7/list"),
        (
            "folderless_lists",
            vec![("space", "8")],
            "/api/v2/space/8/list",
        ),
        ("tasks", vec![("list", "6")], "/api/v2/list/6/task"),
        ("get_task", vec![("task", "abc1")], "/api/v2/task/abc1"),
        (
            "comments",
            vec![("task", "abc1")],
            "/api/v2/task/abc1/comment",
        ),
    ] {
        let req = request(&d, op, &params);
        assert_eq!(req.method.as_str(), "GET", "{op}");
        assert_eq!(req.url.path(), path, "{op}");
        assert!(query(&req).is_empty(), "{op}");
    }
    let closed = request(
        &d,
        "tasks",
        &[("list", "6"), ("page", "2"), ("closed", "true")],
    );
    assert_eq!(
        query(&closed),
        BTreeMap::from([
            ("page".to_string(), "2".to_string()),
            ("include_closed".to_string(), "true".to_string()),
        ]),
        "a hundred a page: the page is named, not its size"
    );

    let created = request(
        &d,
        "create_task",
        &[
            ("list", "6"),
            ("name", "Send the invoice"),
            ("priority", "2"),
            ("due", "1788782400000"),
        ],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/api/v2/list/6/task");
    assert_eq!(
        body(&created),
        json!({"name": "Send the invoice", "priority": 2, "due_date": 1788782400000u64})
    );
    let moved = request(&d, "update_task", &[("task", "abc1"), ("status", "done")]);
    assert_eq!(moved.method.as_str(), "PUT");
    assert_eq!(moved.url.path(), "/api/v2/task/abc1");
    assert_eq!(body(&moved), json!({"status": "done"}));
    let said = request(&d, "comment", &[("task", "abc1"), ("text", "Sent.")]);
    assert_eq!(said.url.path(), "/api/v2/task/abc1/comment");
    assert_eq!(
        body(&said),
        json!({"comment_text": "Sent.", "notify_all": false}),
        "both are required"
    );
}

#[test]
fn monday_posts_graphql_to_one_address_names_its_version_and_expects_no_errors() {
    let d = def("monday");
    for (op, req) in every_request_with(&d, "{}") {
        assert_eq!(req.method.as_str(), "POST", "{op}");
        assert_eq!(req.url.as_str(), "https://api.monday.com/v2", "{op}");
        assert_eq!(req.header("api-version"), Some("2026-10"), "{op}");
        let sent = body(&req);
        let text = sent["query"].as_str().unwrap();
        assert!(
            !text.contains("{{") && !text.contains("}}"),
            "{op}: the doubled braces are GraphQL's single ones on the wire: {text}"
        );
        assert_eq!(
            expect_of(&d, &op),
            Some(Expect {
                path: "errors".into(),
                equals: None,
                absent: true,
                reason: Some("errors.0.message".into()),
            }),
            "{op}: an application error is a 200 with `errors`"
        );
    }
    let me = signed(&d, request(&d, "me", &[]), key("token-not-real"));
    assert_eq!(me.header("authorization"), Some("token-not-real"));
    assert_eq!(
        body(&me),
        json!({"query": "query { me { id name email } }"})
    );

    let boards = request(&d, "boards", &[]);
    assert_eq!(
        body(&boards)["query"],
        "query($limit: Int) { boards(limit: $limit) { id name state workspace_id } }"
    );
    let items = request(&d, "items", &[("board", "1234567890")]);
    assert_eq!(
        body(&items)["variables"],
        json!({"board": ["1234567890"], "limit": 25}),
        "`limit` is `Int!`: 25 unless the step says"
    );
    assert_eq!(
        body(&items)["query"],
        "query($board: [ID!], $limit: Int!) { boards(ids: $board) { items_page(limit: $limit) \
         { cursor items { id name column_values { id text value } } } } }"
    );
    let one = request(&d, "get_item", &[("item", "9876543210")]);
    assert_eq!(body(&one)["variables"], json!({"ids": ["9876543210"]}));

    let created = request(
        &d,
        "create_item",
        &[
            ("board", "1234567890"),
            ("name", "new item"),
            ("values", r#"{"date":"2023-05-25"}"#),
        ],
    );
    assert_eq!(
        body(&created)["variables"],
        json!({
            "board": "1234567890",
            "name": "new item",
            "values": "{\"date\":\"2023-05-25\"}"
        }),
        "column values travel as a JSON-formatted string"
    );
    assert_eq!(
        body(&created)["query"],
        "mutation($board: ID!, $name: String!, $group: String, $values: JSON) { \
         create_item(board_id: $board, item_name: $name, group_id: $group, column_values: \
         $values) { id } }"
    );
    let set = request(
        &d,
        "set_column",
        &[
            ("board", "1234567890"),
            ("item", "9876543210"),
            ("column", "status"),
            ("value", "Working on it"),
        ],
    );
    assert_eq!(
        body(&set)["variables"],
        json!({
            "board": "1234567890",
            "item": "9876543210",
            "column": "status",
            "value": "Working on it"
        })
    );
    let update = request(
        &d,
        "create_update",
        &[("item", "9876543210"), ("body", "Shipped.")],
    );
    assert_eq!(
        body(&update)["variables"],
        json!({"item": "9876543210", "body": "Shipped."})
    );
}

#[test]
fn todoist_calls_the_v1_api_and_updates_and_closes_with_a_post() {
    let d = def("todoist");
    let user = signed(&d, request(&d, "user", &[]), bearer("token-not-real"));
    assert_eq!(user.url.as_str(), "https://api.todoist.com/api/v1/user");
    assert_eq!(user.header("authorization"), Some("Bearer token-not-real"));
    assert_eq!(request(&d, "projects", &[]).url.path(), "/api/v1/projects");
    let tasks = request(
        &d,
        "tasks",
        &[("project", "6Jf8VQXxpwv56VQ7"), ("max", "200")],
    );
    assert_eq!(tasks.url.path(), "/api/v1/tasks");
    assert_eq!(
        query(&tasks),
        BTreeMap::from([
            ("project_id".to_string(), "6Jf8VQXxpwv56VQ7".to_string()),
            ("limit".to_string(), "200".to_string()),
        ])
    );
    let due = request(&d, "filter_tasks", &[("query", "today | overdue")]);
    assert_eq!(due.url.path(), "/api/v1/tasks/filter");
    assert_eq!(
        query(&due),
        BTreeMap::from([("query".to_string(), "today | overdue".to_string())])
    );
    assert_eq!(
        request(&d, "get_task", &[("task", "abc")]).url.path(),
        "/api/v1/tasks/abc"
    );

    let added = request(
        &d,
        "create_task",
        &[
            ("content", "Send the invoice"),
            ("priority", "4"),
            ("due", "tomorrow at 9"),
        ],
    );
    assert_eq!(added.method.as_str(), "POST");
    assert_eq!(added.url.path(), "/api/v1/tasks");
    assert_eq!(
        body(&added),
        json!({"content": "Send the invoice", "priority": 4, "due_string": "tomorrow at 9"})
    );
    let changed = request(&d, "update_task", &[("task", "abc"), ("due", "no date")]);
    assert_eq!(changed.method.as_str(), "POST", "an update is a POST here");
    assert_eq!(changed.url.path(), "/api/v1/tasks/abc");
    assert_eq!(body(&changed), json!({"due_string": "no date"}));
    let closed = request(&d, "close_task", &[("task", "abc")]);
    assert_eq!(closed.method.as_str(), "POST");
    assert_eq!(closed.url.path(), "/api/v1/tasks/abc/close");
    assert!(closed.body.is_none(), "a close carries nothing");
    let said = request(&d, "add_comment", &[("task", "abc"), ("content", "Sent.")]);
    assert_eq!(said.url.path(), "/api/v1/comments");
    assert_eq!(body(&said), json!({"task_id": "abc", "content": "Sent."}));
}

#[test]
fn intercom_names_its_version_on_every_call_and_searches_by_one_condition() {
    let d = def("intercom");
    for (op, req) in every_request_with(&d, "{}") {
        assert_eq!(req.url.host_str(), Some("api.intercom.io"), "{op}");
        assert_eq!(req.header("intercom-version"), Some("2.18"), "{op}");
    }
    let me = signed(&d, request(&d, "me", &[]), bearer("token-not-real"));
    assert_eq!(me.url.as_str(), "https://api.intercom.io/me");
    assert_eq!(me.header("authorization"), Some("Bearer token-not-real"));

    let list = request(&d, "list_contacts", &[("max", "150")]);
    assert_eq!(list.url.path(), "/contacts");
    assert_eq!(query(&list)["per_page"], "150");
    assert_eq!(
        request(&d, "get_contact", &[("contact", "6762f0cd")])
            .url
            .path(),
        "/contacts/6762f0cd"
    );
    let found = request(
        &d,
        "search_contacts",
        &[("field", "email"), ("value", "ada@example.com")],
    );
    assert_eq!(found.method.as_str(), "POST");
    assert_eq!(found.url.path(), "/contacts/search");
    assert_eq!(
        body(&found),
        json!({"query": {"field": "email", "operator": "=", "value": "ada@example.com"}}),
        "`=` unless the step says"
    );

    let created = request(
        &d,
        "create_contact",
        &[("email", "ada@example.com"), ("name", "Ada")],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/contacts");
    assert_eq!(
        body(&created),
        json!({"email": "ada@example.com", "name": "Ada"})
    );
    let updated = request(
        &d,
        "update_contact",
        &[("contact", "6762f0cd"), ("phone", "+1 555 0100")],
    );
    assert_eq!(updated.method.as_str(), "PUT");
    assert_eq!(updated.url.path(), "/contacts/6762f0cd");
    assert_eq!(body(&updated), json!({"phone": "+1 555 0100"}));
    let note = request(
        &d,
        "add_note",
        &[("contact", "6762f0cd"), ("body", "Asked about invoices.")],
    );
    assert_eq!(note.url.path(), "/contacts/6762f0cd/notes");
    assert_eq!(body(&note), json!({"body": "Asked about invoices."}));

    assert_eq!(
        request(&d, "list_conversations", &[]).url.path(),
        "/conversations"
    );
    let one = request(&d, "get_conversation", &[("conversation", "123")]);
    assert_eq!(one.url.path(), "/conversations/123");
    assert_eq!(query(&one)["display_as"], "plaintext");
    assert!(
        d.operations.iter().all(|op| !op.path.ends_with("/reply")),
        "no reply: the admin's form of it could not be read in full"
    );
}

// --- commerce and messaging: Stripe, Zendesk, Mailchimp, SendGrid, Twilio, Box ---

const COMMERCE: [&str; 6] = [
    "stripe",
    "zendesk",
    "mailchimp",
    "sendgrid",
    "twilio",
    "box",
];

/// A form body's pairs, decoded.
fn form(req: &Request) -> BTreeMap<String, String> {
    url::form_urlencoded::parse(req.body.as_deref().expect("a body"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn basic(username: &str, password: &str) -> Credential {
    Credential::Basic {
        username: username.to_string(),
        password: Secret::new(password),
    }
}

/// What changes something says so, and nothing in the six deletes.
#[test]
fn the_commerce_connectors_mark_every_write_and_delete_nothing() {
    for slug in COMMERCE {
        let d = def(slug);
        for op in &d.operations {
            assert_ne!(
                op.method,
                bisa_core::HttpMethod::Delete,
                "{slug}.{}: no built-in deletes",
                op.id
            );
            assert_eq!(
                op.writes,
                op.method != bisa_core::HttpMethod::Get,
                "{slug}.{}: a call that is not a GET is a write, and says so",
                op.id
            );
        }
    }
}

#[test]
fn stripe_moves_no_money_writes_in_a_form_and_keys_its_two_writes() {
    let d = def("stripe");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("api.stripe.com"), "{op}");
        assert_eq!(
            req.header("stripe-version"),
            Some("2026-10-28.endive"),
            "{op}"
        );
    }
    let writes: Vec<&str> = d
        .operations
        .iter()
        .filter(|op| op.writes)
        .map(|op| op.id.as_str())
        .collect();
    assert_eq!(
        writes,
        ["create_customer", "update_customer"],
        "a customer record, and nothing that charges, refunds or pays out"
    );
    for op in d.operations.iter().filter(|op| op.writes) {
        assert!(
            op.path.starts_with("/v1/customers"),
            "{}: {}",
            op.id,
            op.path
        );
        let spec = call_spec(&d, op, Duration::from_secs(1));
        assert_eq!(
            spec.idempotency.map(|i| i.header),
            Some("Idempotency-Key".to_string()),
            "{}: every v1 POST accepts one",
            op.id
        );
    }

    let balance = signed(&d, request(&d, "balance", &[]), bearer("rk_test_not_real"));
    assert_eq!(balance.url.as_str(), "https://api.stripe.com/v1/balance");
    assert_eq!(
        balance.header("authorization"),
        Some("Bearer rk_test_not_real")
    );
    let list = request(
        &d,
        "customers",
        &[("email", "jenny@example.com"), ("max", "100")],
    );
    assert_eq!(list.url.path(), "/v1/customers");
    assert_eq!(
        query(&list),
        BTreeMap::from([
            ("email".to_string(), "jenny@example.com".to_string()),
            ("limit".to_string(), "100".to_string()),
        ])
    );
    let found = request(
        &d,
        "search_customers",
        &[("query", "name:'Jane Doe' AND metadata['foo']:'bar'")],
    );
    assert_eq!(found.url.path(), "/v1/customers/search");
    assert_eq!(
        query(&found)["query"],
        "name:'Jane Doe' AND metadata['foo']:'bar'"
    );

    let created = request(
        &d,
        "create_customer",
        &[("name", "Jenny Rosen"), ("email", "jennyrosen@example.com")],
    );
    assert_eq!(created.method.as_str(), "POST");
    assert_eq!(created.url.path(), "/v1/customers");
    assert_eq!(
        created.header("content-type"),
        Some("application/x-www-form-urlencoded"),
        "form-encoded request bodies"
    );
    assert_eq!(
        form(&created),
        BTreeMap::from([
            ("name".to_string(), "Jenny Rosen".to_string()),
            ("email".to_string(), "jennyrosen@example.com".to_string()),
        ])
    );
    let updated = request(
        &d,
        "update_customer",
        &[("id", "cus_NffrFeUfNV2Hib"), ("phone", "+15558675310")],
    );
    assert_eq!(updated.method.as_str(), "POST");
    assert_eq!(updated.url.path(), "/v1/customers/cus_NffrFeUfNV2Hib");
    assert_eq!(
        form(&updated),
        BTreeMap::from([("phone".to_string(), "+15558675310".to_string())]),
        "parameters not provided are left unchanged"
    );
    for (op, params, path) in [
        ("customer", vec![("id", "cus_1")], "/v1/customers/cus_1"),
        ("invoices", vec![("status", "open")], "/v1/invoices"),
        ("invoice", vec![("id", "in_1")], "/v1/invoices/in_1"),
        ("charges", vec![("customer", "cus_1")], "/v1/charges"),
        ("charge", vec![("id", "ch_1")], "/v1/charges/ch_1"),
        (
            "subscriptions",
            vec![("status", "all")],
            "/v1/subscriptions",
        ),
        ("products", vec![("active", "true")], "/v1/products"),
    ] {
        let req = request(&d, op, &params);
        assert_eq!(req.method.as_str(), "GET", "{op}");
        assert_eq!(req.url.path(), path, "{op}");
    }
}

#[test]
fn zendesk_signs_in_with_the_token_form_and_comments_by_updating_the_ticket() {
    let d = def("zendesk");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("acme.zendesk.com"), "{op}");
        assert!(req.url.path().starts_with("/api/v2/"), "{op}");
        assert_eq!(req.header("accept"), Some("application/json"), "{op}");
    }
    let me = signed(
        &d,
        request(&d, "me", &[]),
        basic("jdoe@example.com/token", "token-not-real"),
    );
    assert_eq!(me.url.path(), "/api/v2/users/me");
    assert_eq!(
        me.header("authorization"),
        // base64("jdoe@example.com/token:token-not-real")
        Some("Basic amRvZUBleGFtcGxlLmNvbS90b2tlbjp0b2tlbi1ub3QtcmVhbA=="),
        "`{{email_address}}/token:{{api_token}}`"
    );

    let list = request(&d, "tickets", &[]);
    assert_eq!(list.url.path(), "/api/v2/tickets");
    assert_eq!(
        query(&list),
        BTreeMap::from([("page[size]".to_string(), "100".to_string())]),
        "a page size is what turns cursor pagination on"
    );
    assert_eq!(
        request(&d, "ticket", &[("id", "35436")]).url.path(),
        "/api/v2/tickets/35436"
    );
    assert_eq!(
        request(&d, "comments", &[("id", "35436")]).url.path(),
        "/api/v2/tickets/35436/comments"
    );
    let found = request(&d, "search", &[("query", "type:ticket status:open")]);
    assert_eq!(found.url.path(), "/api/v2/search");
    assert_eq!(query(&found)["query"], "type:ticket status:open");

    let opened = request(
        &d,
        "create_ticket",
        &[
            ("subject", "My printer is on fire!"),
            ("body", "The smoke is very colorful."),
        ],
    );
    assert_eq!(opened.method.as_str(), "POST");
    assert_eq!(opened.url.path(), "/api/v2/tickets");
    assert_eq!(
        body(&opened),
        json!({"ticket": {
            "subject": "My printer is on fire!",
            "comment": {"body": "The smoke is very colorful."}
        }})
    );
    let note = request(
        &d,
        "add_comment",
        &[("id", "35436"), ("body", "Called the customer.")],
    );
    assert_eq!(note.method.as_str(), "PUT", "there is no comment endpoint");
    assert_eq!(note.url.path(), "/api/v2/tickets/35436");
    assert_eq!(
        body(&note),
        json!({"ticket": {"comment": {"body": "Called the customer.", "public": false}}}),
        "an internal note unless the step makes it public"
    );
    let solved = request(
        &d,
        "update_ticket",
        &[("id", "35436"), ("status", "solved")],
    );
    assert_eq!(solved.method.as_str(), "PUT");
    assert_eq!(body(&solved), json!({"ticket": {"status": "solved"}}));
}

#[test]
fn mailchimp_calls_the_accounts_server_and_sends_no_campaign() {
    let d = def("mailchimp");
    for (op, req) in every_request_with(&d, "{}") {
        assert_eq!(req.url.host_str(), Some("us19.api.mailchimp.com"), "{op}");
        assert!(req.url.path().starts_with("/3.0/"), "{op}");
    }
    assert!(
        d.operations
            .iter()
            .filter(|op| op.writes)
            .all(|op| op.path.contains("/members")),
        "members are written; a campaign is only read"
    );
    let ping = signed(&d, request(&d, "ping", &[]), bearer("key-not-real-us19"));
    assert_eq!(ping.url.as_str(), "https://us19.api.mailchimp.com/3.0/ping");
    assert_eq!(
        ping.header("authorization"),
        Some("Bearer key-not-real-us19")
    );

    let lists = request(&d, "lists", &[("max", "1000")]);
    assert_eq!(lists.url.path(), "/3.0/lists");
    assert_eq!(query(&lists)["count"], "1000");
    let members = request(
        &d,
        "members",
        &[("list", "a1b2c3"), ("status", "subscribed")],
    );
    assert_eq!(members.url.path(), "/3.0/lists/a1b2c3/members");
    assert_eq!(query(&members)["status"], "subscribed");
    let one = request(
        &d,
        "member",
        &[("list", "a1b2c3"), ("email", "ada@example.com")],
    );
    assert_eq!(
        decoded(one.url.path()),
        "/3.0/lists/a1b2c3/members/ada@example.com",
        "the address is taken where the hash is"
    );

    let added = request(
        &d,
        "add_member",
        &[
            ("list", "a1b2c3"),
            ("email", "ada@example.com"),
            ("status", "pending"),
            ("merge_fields", r#"{"FNAME": "Ada"}"#),
        ],
    );
    assert_eq!(added.method.as_str(), "POST");
    assert_eq!(added.url.path(), "/3.0/lists/a1b2c3/members");
    assert_eq!(
        body(&added),
        json!({
            "email_address": "ada@example.com",
            "status": "pending",
            "merge_fields": {"FNAME": "Ada"}
        })
    );
    let changed = request(
        &d,
        "update_member",
        &[
            ("list", "a1b2c3"),
            ("email", "ada@example.com"),
            ("status", "unsubscribed"),
        ],
    );
    assert_eq!(changed.method.as_str(), "PATCH");
    assert_eq!(body(&changed), json!({"status": "unsubscribed"}));
    let note = request(
        &d,
        "add_note",
        &[
            ("list", "a1b2c3"),
            ("email", "ada@example.com"),
            ("note", "Met at the conference."),
        ],
    );
    assert_eq!(
        decoded(note.url.path()),
        "/3.0/lists/a1b2c3/members/ada@example.com/notes"
    );
    assert_eq!(body(&note), json!({"note": "Met at the conference."}));
    assert_eq!(
        request(&d, "campaigns", &[("status", "sent")]).url.path(),
        "/3.0/campaigns"
    );
    assert_eq!(
        request(&d, "campaign_report", &[("campaign", "42694e9e57")])
            .url
            .path(),
        "/3.0/reports/42694e9e57"
    );
}

#[test]
fn sendgrid_sends_one_message_to_one_address_and_queues_one_contact() {
    let d = def("sendgrid");
    let scopes = signed(&d, request(&d, "scopes", &[]), bearer("SG.not-real"));
    assert_eq!(scopes.url.as_str(), "https://api.sendgrid.com/v3/scopes");
    assert_eq!(scopes.header("authorization"), Some("Bearer SG.not-real"));

    let sent = request(
        &d,
        "send_mail",
        &[
            ("to", "ada@example.com"),
            ("from", "ops@acme.example"),
            ("subject", "Deploy finished"),
            ("body", "All green."),
        ],
    );
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(sent.url.path(), "/v3/mail/send");
    assert_eq!(
        body(&sent),
        json!({
            "personalizations": [{"to": [{"email": "ada@example.com"}]}],
            "from": {"email": "ops@acme.example"},
            "subject": "Deploy finished",
            "content": [{"type": "text/plain", "value": "All green."}]
        }),
        "one personalization, one recipient"
    );

    let stats = request(&d, "stats", &[("from", "2026-09-01"), ("by", "week")]);
    assert_eq!(stats.url.path(), "/v3/stats");
    assert_eq!(
        query(&stats),
        BTreeMap::from([
            ("start_date".to_string(), "2026-09-01".to_string()),
            ("aggregated_by".to_string(), "week".to_string()),
        ])
    );
    let bounces = request(&d, "bounces", &[("max", "500")]);
    assert_eq!(bounces.url.path(), "/v3/suppression/bounces");
    assert_eq!(query(&bounces)["limit"], "500");
    let templates = request(&d, "templates", &[("generations", "dynamic")]);
    assert_eq!(templates.url.path(), "/v3/templates");
    assert_eq!(
        query(&templates),
        BTreeMap::from([
            ("generations".to_string(), "dynamic".to_string()),
            ("page_size".to_string(), "50".to_string()),
        ]),
        "a page size is required: fifty unless the step says"
    );
    assert_eq!(request(&d, "lists", &[]).url.path(), "/v3/marketing/lists");

    let contact = request(
        &d,
        "upsert_contact",
        &[("email", "ada@example.com"), ("first_name", "Ada")],
    );
    assert_eq!(contact.method.as_str(), "PUT");
    assert_eq!(contact.url.path(), "/v3/marketing/contacts");
    assert_eq!(
        body(&contact),
        json!({"contacts": [{"email": "ada@example.com", "first_name": "Ada"}]})
    );
}

#[test]
fn twilio_names_the_account_in_every_path_and_sends_a_message_as_a_form() {
    let d = def("twilio");
    for (op, req) in every_request(&d) {
        assert_eq!(req.url.host_str(), Some("api.twilio.com"), "{op}");
        assert!(
            req.url
                .path()
                .starts_with("/2010-04-01/Accounts/AC0123456789abcdef/"),
            "{op}: {}",
            req.url.path()
        );
        assert!(
            req.url.path().ends_with(".json"),
            "{op}: the 2010 API answers XML otherwise"
        );
    }
    assert_eq!(
        d.check.as_ref().map(|c| c.as_str()),
        Some("messages"),
        "a standard API key cannot fetch the account"
    );
    let list = signed(
        &d,
        request(&d, "messages", &[("max", "20")]),
        basic("SK0123", "secret-not-real"),
    );
    assert_eq!(
        list.url.path(),
        "/2010-04-01/Accounts/AC0123456789abcdef/Messages.json"
    );
    assert_eq!(query(&list)["PageSize"], "20");
    assert_eq!(
        list.header("authorization"),
        // base64("SK0123:secret-not-real")
        Some("Basic U0swMTIzOnNlY3JldC1ub3QtcmVhbA==")
    );
    assert_eq!(
        request(&d, "message", &[("sid", "SMaaaa")]).url.path(),
        "/2010-04-01/Accounts/AC0123456789abcdef/Messages/SMaaaa.json"
    );

    let sent = request(
        &d,
        "send_message",
        &[
            ("to", "+15558675310"),
            ("from", "+15017122661"),
            ("body", "Deploy finished."),
        ],
    );
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(
        sent.url.path(),
        "/2010-04-01/Accounts/AC0123456789abcdef/Messages.json"
    );
    assert_eq!(
        sent.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        form(&sent),
        BTreeMap::from([
            ("To".to_string(), "+15558675310".to_string()),
            ("From".to_string(), "+15017122661".to_string()),
            ("Body".to_string(), "Deploy finished.".to_string()),
        ]),
        "a plus sign survives the form"
    );
    let by_service = request(
        &d,
        "send_message",
        &[
            ("to", "+15558675310"),
            ("service", "MG0123"),
            ("body", "Hi"),
        ],
    );
    assert_eq!(form(&by_service)["MessagingServiceSid"], "MG0123");
    assert!(!form(&by_service).contains_key("From"));

    let calls = request(&d, "calls", &[("status", "completed")]);
    assert_eq!(
        calls.url.path(),
        "/2010-04-01/Accounts/AC0123456789abcdef/Calls.json"
    );
    assert_eq!(query(&calls)["Status"], "completed");
    assert_eq!(
        request(&d, "phone_numbers", &[]).url.path(),
        "/2010-04-01/Accounts/AC0123456789abcdef/IncomingPhoneNumbers.json"
    );
}

#[test]
fn box_signs_in_as_the_apps_service_account_and_starts_at_the_root_folder() {
    let d = def("box");
    assert_eq!(
        serde_json::to_value(&d.auth).unwrap(),
        json!({
            "scheme": "client_credentials",
            "token_url": "https://api.box.com/oauth2/token",
            "params": {
                "box_subject_type": "enterprise",
                "box_subject_id": "{account.enterprise_id}"
            }
        }),
        "the client's id and secret in the form, the RFC's grant, Box's two subject pairs"
    );
    assert_eq!(d.oauth_hosts(), vec!["api.box.com".to_string()]);
    assert_eq!(
        request(&d, "me", &[]).url.as_str(),
        "https://api.box.com/2.0/users/me"
    );

    let root = request(&d, "folder_items", &[]);
    assert_eq!(
        root.url.path(),
        "/2.0/folders/0/items",
        "the root is always 0"
    );
    assert!(query(&root).is_empty());
    let inside = request(&d, "folder_items", &[("folder", "12345"), ("max", "1000")]);
    assert_eq!(inside.url.path(), "/2.0/folders/12345/items");
    assert_eq!(query(&inside)["limit"], "1000");
    assert_eq!(
        request(&d, "folder", &[("folder", "12345")]).url.path(),
        "/2.0/folders/12345"
    );
    assert_eq!(
        request(&d, "file", &[("file", "426436")]).url.path(),
        "/2.0/files/426436"
    );
    let found = request(&d, "search", &[("query", "sales report"), ("type", "file")]);
    assert_eq!(found.url.path(), "/2.0/search");
    assert_eq!(
        query(&found),
        BTreeMap::from([
            ("query".to_string(), "sales report".to_string()),
            ("type".to_string(), "file".to_string()),
        ])
    );

    let made = request(&d, "create_folder", &[("name", "New Folder")]);
    assert_eq!(made.method.as_str(), "POST");
    assert_eq!(made.url.path(), "/2.0/folders");
    assert_eq!(
        body(&made),
        json!({"name": "New Folder", "parent": {"id": "0"}})
    );
    assert_eq!(
        request(&d, "comments", &[("file", "426436")]).url.path(),
        "/2.0/files/426436/comments"
    );
    let said = request(
        &d,
        "comment",
        &[("file", "426436"), ("message", "Review completed!")],
    );
    assert_eq!(said.url.path(), "/2.0/comments");
    assert_eq!(
        body(&said),
        json!({"message": "Review completed!", "item": {"type": "file", "id": "426436"}})
    );
}
