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

/// Every built-in's check builds with no parameter at all — what *Check*
/// sends — and its host is one the definition declares.
#[test]
fn every_check_builds_with_no_parameters_against_a_declared_host() {
    for (slug, _) in CATALOG.connectors {
        let d = def(slug);
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
        &Credential::ApiKey(Secret::new("lin_api_not_real")),
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
    for slug in ["gmail", "google-calendar", "google-drive", "youtube"] {
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
