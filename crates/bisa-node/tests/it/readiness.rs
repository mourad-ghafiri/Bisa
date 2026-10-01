//! `GET /readiness` (16 — The setup gate): the five checks over HTTP on a
//! node with no harness anywhere — nothing installed, nothing run.

use super::decisions::{boot_with, request, TOKEN};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn readiness_names_the_five_checks_in_order_and_is_not_ready_without_a_harness() {
    let (_dir, socket, stop) = boot_with(|_| {}).await;
    let (code, v) = request(&socket, "GET", "/readiness", None, TOKEN).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["ready"], json!(false));
    let ids: Vec<&str> = v["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "git",
            "harness",
            "decision_making_agent",
            "general_agent",
            "workflow_agent"
        ]
    );
    let harness = &v["checks"][1];
    assert_eq!(harness["state"], json!("missing"));
    assert_eq!(
        harness["door"],
        json!({"door": "settings", "tab": "harnesses"})
    );
    assert_eq!(
        harness["hint"]["url"],
        json!("https://code.claude.com/docs/en/setup")
    );
    assert!(harness["hint"]["commands"][0]["platform"].is_string());
    assert_eq!(v["checks"][2]["title"], json!("The Decision-Making Agent"));
    assert_eq!(
        v["checks"][2]["door"],
        json!({"door": "settings", "tab": "decision-making"})
    );
    assert_eq!(v["checks"][3]["door"], json!({"door": "agents"}));
    assert!(v["checked_at"].as_u64().unwrap() > 0);
    let _server_gone = stop.send(());
}
