//! Collaboration over HTTP (14-collaboration): invitations, people, the
//! roster's humans and held messages against the real store; the hosted
//! memberships and the wire through a fake pump lent to the node.

use crate::node::Node;
use bisa_collab::{Directory, HostCard, RelayCheck, RelayHealth, RelayStatusWord};
use bisa_core::{MemberRole, PrincipalId};
use bisa_guest::{Hosted, HostedMessage, HostedState};
use bisa_node::collab::{CollabDoors, CollabRefusal};
use bisa_node::dto::{HostedChannelRow, HostedPostBody, HostedPosted, SyncReport};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};

fn pid(hex: &str) -> PrincipalId {
    PrincipalId::new(hex.to_string()).unwrap()
}

/// A pump that remembers what it was asked and answers in kind.
struct FakeCollab {
    host: String,
    calls: Mutex<Vec<String>>,
}

impl FakeCollab {
    fn new() -> (Arc<Self>, String) {
        let host = nostr::key::Keys::generate().public_key().to_hex();
        (
            Arc::new(Self {
                host: host.clone(),
                calls: Mutex::new(Vec::new()),
            }),
            host,
        )
    }

    fn note(&self, what: impl Into<String>) {
        self.calls.lock().unwrap().push(what.into());
    }

    fn hosted(&self) -> Hosted {
        Hosted {
            host: HostCard {
                pubkey: pid(&self.host),
                name: "Acme".into(),
                relays: vec!["wss://relay.example".into()],
            },
            role: MemberRole::Guest,
            state: HostedState::Member,
            requested_at: 1,
            joined_at: Some(2),
            label: Some("Bob".into()),
        }
    }

    fn known(&self, host: &str) -> Result<(), CollabRefusal> {
        if host == self.host {
            Ok(())
        } else {
            Err(CollabRefusal::NotHosted)
        }
    }
}

impl CollabDoors for FakeCollab {
    fn sync(&self) -> BoxFuture<'_, SyncReport> {
        Box::pin(async {
            SyncReport {
                running: true,
                enabled: true,
                relays: vec![RelayHealth {
                    url: "wss://relay.example".into(),
                    status: RelayStatusWord::Connected,
                    connected: true,
                    attempts: 1,
                    success: 1,
                    success_rate: 1.0,
                    latency_ms: Some(12),
                    connected_at: Some(2),
                    bytes_sent: 10,
                    bytes_received: 20,
                    problem: None,
                }],
                connected_relays: 1,
                published: 3,
                ingested: 4,
                last_catchup: Some(5),
                people: 0,
                hosts: 1,
                iroh_node_id: None,
                iroh_peers_connected: 0,
            }
        })
    }
    fn check_relay(&self, url: String) -> BoxFuture<'_, RelayCheck> {
        self.note(format!("check {url}"));
        Box::pin(async move {
            RelayCheck {
                ok: url.starts_with("wss://"),
                url,
                latency_ms: Some(7),
                error: None,
            }
        })
    }
    fn reconnect(&self) -> BoxFuture<'_, ()> {
        self.note("reconnect");
        Box::pin(async {})
    }
    fn hosts(&self) -> BoxFuture<'_, Vec<Hosted>> {
        Box::pin(async { vec![self.hosted()] })
    }
    fn join(
        &self,
        code: String,
        label: Option<String>,
    ) -> BoxFuture<'_, Result<Hosted, CollabRefusal>> {
        self.note(format!("join {code} {label:?}"));
        Box::pin(async move {
            if code.contains("bad") {
                return Err(CollabRefusal::Refused("this code is not one".into()));
            }
            Ok(self.hosted())
        })
    }
    fn leave(&self, host: String) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        self.note(format!("leave {host}"));
        Box::pin(async move { self.known(&host) })
    }
    fn hosted_channels(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>> {
        Box::pin(async move {
            self.known(&host)?;
            Ok(vec![HostedChannelRow {
                channel: bisa_core::Channel::general(0),
                unread_count: 2,
                latest_at: Some(9),
            }])
        })
    }
    fn hosted_dms(
        &self,
        host: String,
    ) -> BoxFuture<'_, Result<Vec<HostedChannelRow>, CollabRefusal>> {
        Box::pin(async move {
            self.known(&host)?;
            Ok(vec![])
        })
    }
    fn hosted_members(&self, host: String) -> BoxFuture<'_, Result<Vec<Directory>, CollabRefusal>> {
        Box::pin(async move {
            self.known(&host)?;
            Ok(vec![Directory {
                pubkey: pid(&self.host),
                role: MemberRole::Owner,
                label: Some("Alice".into()),
                photo: None,
            }])
        })
    }
    fn hosted_messages(
        &self,
        host: String,
        scope: String,
        _before: Option<u64>,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HostedMessage>, CollabRefusal>> {
        self.note(format!("messages {scope} limit {limit}"));
        Box::pin(async move {
            self.known(&host)?;
            Ok(vec![HostedMessage {
                id: "m1".into(),
                scope,
                author: pid(&self.host),
                at: 9,
                text: "welcome".into(),
                reply_to: None,
                mentions: vec![],
                attachments: vec![],
                artifacts: vec![],
                reactions: vec![],
                retracted: false,
            }])
        })
    }
    fn hosted_post(
        &self,
        host: String,
        scope: String,
        post: HostedPostBody,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        self.note(format!("post {scope} {}", post.content));
        Box::pin(async move {
            self.known(&host)?;
            if scope == "design" {
                return Err(CollabRefusal::Refused(
                    "you do not reach design on this host".into(),
                ));
            }
            Ok(HostedPosted {
                id: "m2".into(),
                scope,
                redacted: 0,
            })
        })
    }
    fn hosted_react(
        &self,
        host: String,
        event: String,
        emoji: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        self.note(format!("react {event} {emoji}"));
        Box::pin(async move {
            self.known(&host)?;
            Ok(HostedPosted {
                id: "r1".into(),
                scope: "general".into(),
                redacted: 0,
            })
        })
    }
    fn hosted_retract(
        &self,
        host: String,
        event: String,
    ) -> BoxFuture<'_, Result<HostedPosted, CollabRefusal>> {
        self.note(format!("retract {event}"));
        Box::pin(async move {
            self.known(&host)?;
            Ok(HostedPosted {
                id: "x1".into(),
                scope: "general".into(),
                redacted: 0,
            })
        })
    }
    fn hosted_open_dm(
        &self,
        host: String,
        participants: Vec<String>,
    ) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        self.note(format!("dm {}", participants.join(",")));
        Box::pin(async move { self.known(&host) })
    }
    fn hosted_read(&self, host: String, scope: String) -> BoxFuture<'_, Result<(), CollabRefusal>> {
        self.note(format!("read {scope}"));
        Box::pin(async move { self.known(&host) })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn invitations_people_rosters_and_held_messages_over_http() {
    let node = Node::start().await;

    // The matrix is served, not restated.
    let v = node.get("/workspace/roles").await;
    let roles = v["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 4);
    assert!(roles
        .iter()
        .any(|r| r["role"] == json!("guest") && r["permissions"].as_array().unwrap().len() == 2));
    assert!(roles
        .iter()
        .any(|r| r["role"] == json!("owner") && r["permissions"].as_array().unwrap().len() == 10));
    assert_eq!(v["permissions"].as_array().unwrap().len(), 10);

    // A channel a guest can be put on.
    let v = node.post("/channels", json!({"name": "design"})).await;
    assert_eq!(v["channel"]["id"], json!("design"));

    // An invitation: the record, the link and the code — the secret in the
    // answer and nowhere else.
    let v = node
        .post(
            "/workspace/invites",
            json!({"role": "guest", "channels": ["design"], "label": "Bob"}),
        )
        .await;
    let invite_id = v["invite"]["id"].as_str().unwrap().to_string();
    assert_eq!(v["invite"]["state"]["state"], json!("pending"));
    assert_eq!(v["invite"]["role"], json!("guest"));
    assert_eq!(v["invite"]["channels"], json!(["design"]));
    let link = v["link"].as_str().unwrap();
    let code = v["code"].as_str().unwrap();
    assert!(link.starts_with("bisa://join/nprofile1"), "{link}");
    assert!(
        code.starts_with("nprofile1") && code.contains(':'),
        "{code}"
    );
    assert!(
        v["invite"].get("secret_hash").is_none(),
        "never a hash on the wire: {v}"
    );
    let parsed = bisa_collab::InviteCode::parse(link).unwrap();
    assert_eq!(bisa_collab::InviteCode::parse(code).unwrap(), parsed);
    let v = node.get("/workspace/invites").await;
    assert_eq!(v["invites"].as_array().unwrap().len(), 1);
    let (code_, _) = node
        .req("POST", "/workspace/invites", Some(json!({"role": "owner"})))
        .await;
    assert_eq!(code_, 400, "never an owner");
    let (code_, _) = node
        .req(
            "POST",
            "/workspace/invites",
            Some(json!({"channels": ["general"]})),
        )
        .await;
    assert_eq!(code_, 400, "general is everyone's already");
    let (code_, _) = node
        .req(
            "POST",
            &format!("/workspace/invites/{invite_id}/admit"),
            None,
        )
        .await;
    assert_eq!(code_, 400, "nothing waits on a pending invitation");
    let (code_, v) = node
        .req("DELETE", &format!("/workspace/invites/{invite_id}"), None)
        .await;
    assert_eq!(code_, 200, "{v}");
    assert_eq!(v["invite"]["state"]["state"], json!("revoked"));

    // A person by hand, on a roster, promoted, removed.
    let bob = nostr::key::Keys::generate().public_key().to_hex();
    let v = node
        .post(
            "/workspace/people",
            json!({"pubkey": bob, "role": "guest", "label": "Bob"}),
        )
        .await;
    assert_eq!(
        v["person"]["invited_by"],
        json!(node.ws.owner_principal().as_hex())
    );
    let (code_, v) = node
        .req("PATCH", "/channels/design", Some(json!({"humans": [bob]})))
        .await;
    assert_eq!(code_, 200, "{v}");
    assert_eq!(v["channel"]["roster"]["humans"], json!([bob]));
    let v = node.get("/workspace/people").await;
    let row = &v["people"].as_array().unwrap()[0];
    assert_eq!(row["role"], json!("guest"));
    assert_eq!(row["channels"], json!(["design"]));
    assert_eq!(row["permissions"], json!(["post_in_channels", "open_dms"]));
    let stranger = nostr::key::Keys::generate().public_key().to_hex();
    let (code_, _) = node
        .req(
            "PATCH",
            "/channels/design",
            Some(json!({"humans": [stranger]})),
        )
        .await;
    assert_eq!(code_, 400, "a stranger is not rostered");
    let v = node.get("/channels/design").await;
    assert!(
        v["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m.get("human").is_some()),
        "{v}"
    );
    let (code_, v) = node
        .req(
            "PUT",
            &format!("/workspace/people/{bob}/role"),
            Some(json!({"role": "member"})),
        )
        .await;
    assert_eq!(code_, 200, "{v}");
    let (code_, v) = node
        .req("DELETE", &format!("/workspace/people/{bob}"), None)
        .await;
    assert_eq!(code_, 200, "{v}");
    assert!(v["people"].as_array().unwrap().is_empty());
    let v = node.get("/channels/design").await;
    assert_eq!(
        v["channel"]["roster"]["humans"],
        json!([]),
        "unrostered on removal"
    );

    // Nothing is held; a release of nothing is refused by name.
    let v = node.get("/messages/held").await;
    assert!(v["held"].as_array().unwrap().is_empty());
    let (code_, _) = node.req("POST", "/messages/nothing/release", None).await;
    assert!(code_ == 400 || code_ == 404, "{code_}");

    // The inbox has no people row until something concerns one.
    let v = node.get("/inbox?source=people").await;
    assert!(v["rows"].as_array().unwrap().is_empty());

    node.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hosted_memberships_and_the_wire_go_through_the_doors() {
    let (fake, host) = FakeCollab::new();
    let node = Node::start_with_collab(fake.clone()).await;

    let v = node.get("/hosts").await;
    assert_eq!(v["hosts"].as_array().unwrap().len(), 1);
    assert_eq!(v["hosts"][0]["host"]["name"], json!("Acme"));
    assert_eq!(v["hosts"][0]["state"]["state"], json!("member"));

    let v = node
        .post(
            "/hosts/join",
            json!({"code": "nprofile1abc:ff", "label": "Bob"}),
        )
        .await;
    assert_eq!(v["host"]["role"], json!("guest"));
    let (code, v) = node
        .req("POST", "/hosts/join", Some(json!({"code": "bad"})))
        .await;
    assert_eq!(code, 400, "{v}");
    let (code, _) = node
        .req("POST", "/hosts/join", Some(json!({"code": "  "})))
        .await;
    assert_eq!(code, 400);

    let v = node.get(&format!("/hosts/{host}/channels")).await;
    assert_eq!(v["channels"][0]["channel"]["id"], json!("general"));
    assert_eq!(v["channels"][0]["unread_count"], json!(2));
    let v = node.get(&format!("/hosts/{host}/members")).await;
    assert_eq!(v["members"][0]["label"], json!("Alice"));
    let v = node
        .get(&format!("/hosts/{host}/channels/general/messages?limit=5"))
        .await;
    assert_eq!(v["messages"][0]["text"], json!("welcome"));
    let v = node
        .post(
            &format!("/hosts/{host}/channels/general/messages"),
            json!({"content": "hi"}),
        )
        .await;
    assert_eq!(v["id"], json!("m2"));
    let (code, v) = node
        .req(
            "POST",
            &format!("/hosts/{host}/channels/design/messages"),
            Some(json!({"content": "hi"})),
        )
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("do not reach"));
    let (code, _) = node
        .req(
            "POST",
            &format!("/hosts/{host}/channels/general/messages"),
            Some(json!({"content": "   "})),
        )
        .await;
    assert_eq!(code, 400);
    let v = node
        .post(
            &format!("/hosts/{host}/messages/m1/react"),
            json!({"emoji": "👍"}),
        )
        .await;
    assert_eq!(v["id"], json!("r1"));
    let v = node
        .post(&format!("/hosts/{host}/messages/m2/retract"), json!({}))
        .await;
    assert_eq!(v["id"], json!("x1"));
    let other = nostr::key::Keys::generate().public_key().to_hex();
    let v = node
        .post(&format!("/hosts/{host}/dms"), json!({"members": [other]}))
        .await;
    assert_eq!(v["asked"], json!(true));
    let v = node
        .post(&format!("/hosts/{host}/read"), json!({"scope": "general"}))
        .await;
    assert_eq!(v["ok"], json!(true));

    // A host this node is not on, and a key that is not one.
    let unknown = nostr::key::Keys::generate().public_key().to_hex();
    let (code, _) = node
        .req("GET", &format!("/hosts/{unknown}/channels"), None)
        .await;
    assert_eq!(code, 404);
    let (code, _) = node.req("GET", "/hosts/nope/channels", None).await;
    assert_eq!(code, 400);
    let (code, v) = node.req("DELETE", &format!("/hosts/{host}"), None).await;
    assert_eq!(code, 200, "{v}");

    // The wire.
    let v = node.get("/sync").await;
    assert_eq!(v["running"], json!(true));
    assert_eq!(v["enabled"], json!(true));
    assert_eq!(v["relays"][0]["status"], json!("connected"));
    assert_eq!(v["connected_relays"], json!(1));
    assert_eq!(v["hosts"], json!(1));
    let v = node
        .post("/sync/relays/check", json!({"url": "wss://relay.example"}))
        .await;
    assert_eq!(v["ok"], json!(true));
    let v = node.post("/sync/relays/reconnect", json!({})).await;
    assert_eq!(v["ok"], json!(true));

    let calls = fake.calls.lock().unwrap().clone();
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("join nprofile1abc:ff Some(\"Bob\")")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|c| c == "messages general limit 5"),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c == "post general hi"), "{calls:?}");
    assert!(calls.iter().any(|c| c == "react m1 👍"), "{calls:?}");
    assert!(
        calls.iter().any(|c| c == &format!("dm {other}")),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c == "read general"), "{calls:?}");
    assert!(
        calls.iter().any(|c| c == &format!("leave {host}")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|c| c == "check wss://relay.example"),
        "{calls:?}"
    );
    assert!(calls.iter().any(|c| c == "reconnect"), "{calls:?}");

    node.shutdown().await;
}

/// A node that runs no pump still answers the wire from the settings: the
/// switch off by default and every configured relay as `off`, in order —
/// and a relay written to the settings shows up there at once.
#[tokio::test(flavor = "multi_thread")]
async fn the_wire_without_a_pump_lists_the_configured_relays_as_off() {
    let node = Node::start().await;
    let v = node.get("/sync").await;
    assert_eq!(v["running"], json!(false));
    assert_eq!(v["enabled"], json!(false));
    let urls: Vec<&str> = v["relays"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        urls,
        vec![
            "wss://relay.nostr.com",
            "wss://relay.nostr.net",
            "wss://relay.damus.io",
            "wss://nos.lol"
        ]
    );
    let (code, v) = node
        .req(
            "PUT",
            "/settings/machine",
            Some(json!({"values": {"sync.relays": ["wss://relay.example"], "sync.enabled": true}})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    let v = node.get("/sync").await;
    assert_eq!(v["enabled"], json!(true), "the switch is the setting's");
    assert_eq!(v["relays"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["relays"][0]["status"],
        json!("off"),
        "no pump runs here, so nothing is contacted"
    );
    node.shutdown().await;
}

/// The owner's own profile over HTTP (14-collaboration): a label cleaned as a
/// joiner's is, a face that is a small picture this machine holds, each
/// cleared with `null`, and the refusals by name — a face over the cap, a
/// file that is not a picture, a hash not held.
#[tokio::test(flavor = "multi_thread")]
async fn the_owner_sets_a_name_and_a_face_and_a_face_that_is_not_one_is_refused() {
    let node = Node::start().await;
    let owner = node.ws.owner_principal().as_hex().to_string();
    let mut png = b"\x89PNG\r\n\x1a\nIHDR-tiny".to_vec();
    png.resize(900, 1);
    let face = node.ws.put_attachment(&png, "me.png", "image/png").unwrap();
    let (code, v) = node
        .req(
            "PUT",
            "/workspace/me",
            Some(json!({"label": "  Ada\u{7} Lovelace ", "photo": {"sha256": face.sha256, "name": "me.png", "mime": "image/png", "size": face.size}})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["me"]["pubkey"], json!(owner));
    assert_eq!(v["me"]["label"], json!("Ada Lovelace"), "cleaned");
    assert_eq!(v["me"]["photo"]["sha256"], json!(face.sha256));
    assert_eq!(v["me"]["role"], json!("owner"));
    let v = node.get("/workspace").await;
    assert_eq!(
        v["members"][0]["photo"]["sha256"],
        json!(face.sha256),
        "the owner's row carries the face"
    );

    // Kept when absent, cleared with null.
    let (code, v) = node.req("PUT", "/workspace/me", Some(json!({}))).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["me"]["label"], json!("Ada Lovelace"));
    let (code, v) = node
        .req(
            "PUT",
            "/workspace/me",
            Some(json!({"label": null, "photo": null})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["me"].get("label").is_none() || v["me"]["label"].is_null());
    assert!(v["me"].get("photo").is_none() || v["me"]["photo"].is_null());

    // A face over the cap, a file that is not a picture, a hash not held.
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(16 * 1024 + 1, 0);
    let big = node
        .ws
        .put_attachment(&big, "big.png", "image/png")
        .unwrap();
    let (code, v) = node
        .req("PUT", "/workspace/me", Some(json!({"photo": {"sha256": big.sha256, "name": "big.png", "mime": "image/png", "size": big.size}})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(v["error"].as_str().unwrap().contains("96 px square"), "{v}");
    let text = node
        .ws
        .put_attachment(b"<html>", "face.png", "image/png")
        .unwrap();
    let (code, v) = node
        .req("PUT", "/workspace/me", Some(json!({"photo": {"sha256": text.sha256, "name": "face.png", "mime": "image/png", "size": text.size}})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not a picture"),
        "{v}"
    );
    let (code, v) = node
        .req("PUT", "/workspace/me", Some(json!({"photo": {"sha256": "c".repeat(64), "name": "x.png", "mime": "image/png", "size": 1}})))
        .await;
    assert_eq!(code, 400, "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("not on this machine"),
        "{v}"
    );

    // An agent and a team take a picture the same way, and lose it with null.
    let v = node
        .post(
            "/agents",
            json!({"name": "Scout", "system_prompt": "look", "harness": "claude-code"}),
        )
        .await;
    let agent = v["agent"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req("PATCH", &format!("/agents/{agent}"), Some(json!({"photo": {"sha256": face.sha256, "name": "me.png", "mime": "image/png", "size": face.size}})))
        .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["agent"]["photo"]["sha256"], json!(face.sha256));
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/agents/{agent}"),
            Some(json!({"photo": null})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["agent"].get("photo").is_none() || v["agent"]["photo"].is_null());
    let v = node
        .post("/teams", json!({"name": "Crew", "members": [], "photo": {"sha256": face.sha256, "name": "me.png", "mime": "image/png", "size": face.size}}))
        .await;
    assert_eq!(v["team"]["photo"]["sha256"], json!(face.sha256), "{v}");
    let team = v["team"]["id"].as_str().unwrap().to_string();
    let (code, v) = node
        .req(
            "PATCH",
            &format!("/teams/{team}"),
            Some(json!({"photo": null})),
        )
        .await;
    assert_eq!(code, 200, "{v}");
    assert!(v["team"].get("photo").is_none() || v["team"]["photo"].is_null());
    let (code, _) = node
        .req("PATCH", &format!("/teams/{team}"), Some(json!({"photo": {"sha256": text.sha256, "name": "face.png", "mime": "image/png", "size": text.size}})))
        .await;
    assert_eq!(code, 400, "a team's picture is checked like a project's");

    node.shutdown().await;
}
