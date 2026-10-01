//! The two halves of the embedded browser a node owns, with no window open:
//! a folder of a checkout **served** on a loopback port of this machine —
//! its own files and nothing else, however a URL spells the way out — and
//! the **bridge** an agent's browser tools speak through, which says at once
//! that nobody is home and hands a desktop's answer back when one is.
//!
//! Every page is read over loopback, the path sent as it is spelt: a client
//! that tidied `..` away before asking would prove nothing. The desktop is
//! played by the journey itself, through the two routes a desktop reads and
//! answers by; no page is rendered and nothing leaves this machine.

use super::sealed::Sealed;
use serde_json::{json, Value};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const COMMITTER: &str = "A Journey <journey@example.test>";

/// A project that is a repository, with a site in it and a file beside the
/// site: its id — its own tree's workstream's — and its tree.
fn a_checkout(ws: &Sealed, slug: &str) -> (String, PathBuf) {
    let made = ws.json(&["project", "new", slug, "--committer", COMMITTER]);
    let project = made["project"]["id"]
        .as_str()
        .expect("the project")
        .to_string();
    let tree = PathBuf::from(made["path"].as_str().expect("its tree"));
    for (name, text) in [
        ("site/index.html", "<h1>The shelf</h1>"),
        ("site/docs/page.html", "<p>How to hang it.</p>"),
        ("site/my page.html", "<p>A name with a space.</p>"),
        ("site/.hidden/inner.txt", "not for a page"),
        ("site/.dotfile", "not for a page either"),
        ("beside.txt", "the checkout's, not the site's"),
    ] {
        let path = tree.join(name);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
        std::fs::write(path, text).expect("the file");
    }
    (project, tree)
}

/// The port of `http://127.0.0.1:<port>/`.
fn port_of(url: &str) -> u16 {
    url.trim_start_matches("http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap_or_else(|_| panic!("a loopback URL with a port: {url}"))
}

/// One GET, the path as spelt: the status and the body, or nothing when
/// nobody listens there.
fn get(url: &str, path: &str) -> Option<(u16, String)> {
    let port = port_of(url);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a deadline");
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).ok()?;
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let status = answer.split_whitespace().nth(1)?.parse().ok()?;
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    Some((status, body))
}

fn status(url: &str, path: &str) -> u16 {
    get(url, path)
        .unwrap_or_else(|| panic!("no answer for {path}"))
        .0
}

fn serve(ws: &Sealed, checkout: &str, body: Value) -> (u16, Value) {
    ws.call(
        "POST",
        &format!("/workstreams/{checkout}/servers"),
        &[],
        Some(&body),
    )
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) {
    std::os::unix::fs::symlink(to, from).expect("a link");
}

#[test]
fn a_folder_is_served_with_its_own_files_and_nothing_else() {
    let mut ws = Sealed::bare();
    ws.start();
    let listening = ws.listen();
    let (shelf, tree) = a_checkout(&ws, "shelf");
    let (other, _) = a_checkout(&ws, "cupboard");
    #[cfg(unix)]
    {
        link(&tree.join("site/out.txt"), &tree.join("beside.txt"));
        link(&tree.join("site/in.html"), &tree.join("site/index.html"));
        link(&tree.join("site/cache"), &tree.join("site/.hidden"));
    }

    // --- served: a loopback port of its own -------------------------------------
    let (code, site) = serve(&ws, &shelf, json!({"folder": "site"}));
    assert_eq!(code, 200, "{site}");
    let url = site["url"].as_str().expect("its URL").to_string();
    assert_eq!(url, format!("http://127.0.0.1:{}/", site["port"]));
    assert_eq!(site["page"], url.as_str());
    assert_eq!(
        site["owner"],
        json!({"kind": "workstream", "workstream": shelf, "folder": "site"})
    );

    // Its own files, a directory by its index.
    assert_eq!(
        get(&url, "/"),
        Some((200, "<h1>The shelf</h1>".to_string()))
    );
    assert_eq!(
        get(&url, "/docs/page.html"),
        Some((200, "<p>How to hang it.</p>".to_string()))
    );
    assert_eq!(status(&url, "/my%20page.html"), 200);
    assert_eq!(status(&url, "/docs/"), 404, "a directory with no index");
    assert_eq!(status(&url, "/nothing.html"), 404);
    #[cfg(unix)]
    assert_eq!(
        status(&url, "/in.html"),
        200,
        "a link that stays is the folder's own"
    );

    // Nothing hidden, and nothing outside — however the URL spells it.
    for way_out in [
        "/.dotfile",
        "/%2Edotfile",
        "/%2edotfile",
        "/.hidden/inner.txt",
        "/%2Ehidden/inner.txt",
        "/../beside.txt",
        "/%2E%2E/beside.txt",
        "/..%2Fbeside.txt",
        "/docs/..%2F..%2Fbeside.txt",
        "/docs%5C..%5C..%5Cbeside.txt",
        "/docs/../../beside.txt",
        "/index.html%00.txt",
        "/%zz",
    ] {
        let answered = get(&url, way_out);
        let code = answered.as_ref().map(|(code, _)| *code);
        assert!(matches!(code, Some(400 | 404)), "{way_out}: {answered:?}");
        assert!(
            !answered.is_some_and(
                |(_, body)| body.contains("not for a page") || body.contains("not the site's")
            ),
            "{way_out} handed a file over"
        );
    }
    #[cfg(unix)]
    for leaving in ["/out.txt", "/cache/inner.txt"] {
        assert_eq!(
            status(&url, leaving),
            404,
            "{leaving}: where a link ends is what counts"
        );
    }

    // --- what a served page is, as a file of the checkout -----------------------
    let resolve = |path: &str| -> Value {
        let (code, answer) = ws.call(
            "GET",
            &format!(
                "/workstreams/{shelf}/servers/{}/resolve?path={path}",
                site["id"].as_str().unwrap_or_default()
            ),
            &[],
            None,
        );
        assert_eq!(code, 200, "{answer}");
        answer["path"].clone()
    };
    assert_eq!(resolve("/docs/page.html"), "site/docs/page.html");
    assert_eq!(resolve("/"), "site/index.html");
    assert_eq!(resolve("/.dotfile"), Value::Null);
    assert_eq!(resolve("/nothing.html"), Value::Null);

    // --- refused: twice, outside, no folder, nobody's checkout ------------------
    let (code, twice) = serve(&ws, &shelf, json!({"folder": "site"}));
    assert_eq!(code, 409, "{twice}");
    assert!(
        twice["error"]
            .as_str()
            .is_some_and(|said| said.contains(&url)),
        "it says where the folder already is: {twice}"
    );
    for (folder, why) in [
        ("..", "above the checkout"),
        ("../cupboard", "another checkout"),
        ("/etc", "a path of the machine's"),
        ("site/index.html", "a file"),
        ("nowhere", "nothing"),
        (".git", "hidden"),
    ] {
        let (code, refused) = serve(&ws, &shelf, json!({"folder": folder}));
        assert_eq!(code, 400, "{folder} ({why}): {refused}");
    }
    let (code, refused) = serve(&ws, &shelf, json!({"folder": "site", "port": 8080}));
    assert_eq!(code, 400, "a key the body does not know: {refused}");
    let (code, _) = serve(&ws, "01J8ZQ00000000000000000000", json!({}));
    assert_eq!(code, 404, "a checkout nobody has");

    // --- the whole checkout, on a port of its own --------------------------------
    let (code, whole) = serve(&ws, &shelf, json!({}));
    assert_eq!(code, 200, "{whole}");
    let whole_url = whole["url"].as_str().expect("its URL").to_string();
    assert_ne!(whole_url, url);
    assert_eq!(
        get(&whole_url, "/beside.txt"),
        Some((200, "the checkout's, not the site's".to_string()))
    );
    assert_eq!(status(&whole_url, "/site/"), 200);
    for hidden in [
        "/.git/config",
        "/.git/HEAD",
        "/%2Egit/config",
        "/site/.dotfile",
    ] {
        assert_eq!(status(&whole_url, hidden), 404, "{hidden}");
    }

    // --- listed, and stopped through its own checkout's door alone ---------------
    let (_, mine) = ws.call("GET", &format!("/workstreams/{shelf}/servers"), &[], None);
    assert_eq!(mine["servers"].as_array().map(Vec::len), Some(2), "{mine}");
    let (_, theirs) = ws.call("GET", &format!("/workstreams/{other}/servers"), &[], None);
    assert_eq!(theirs["servers"], json!([]));
    let (_, every) = ws.call("GET", "/servers", &[], None);
    assert_eq!(
        every["servers"].as_array().map(Vec::len),
        Some(2),
        "{every}"
    );
    let id = site["id"].as_str().expect("its id");
    let (code, _) = ws.call(
        "DELETE",
        &format!("/workstreams/{other}/servers/{id}"),
        &[],
        None,
    );
    assert_eq!(
        code, 404,
        "another checkout's door stops nothing of this one's"
    );
    assert_eq!(status(&url, "/"), 200);
    let (code, stopped) = ws.call(
        "DELETE",
        &format!("/workstreams/{shelf}/servers/{id}"),
        &[],
        None,
    );
    assert_eq!(code, 200, "{stopped}");
    ws.until("the port to be let go", || {
        get(&url, "/").is_none().then_some(())
    });
    let (code, _) = ws.call(
        "DELETE",
        &format!("/workstreams/{shelf}/servers/{id}"),
        &[],
        None,
    );
    assert_eq!(code, 404, "stopped twice is a server nobody has");
    // The folder can be served again, on a port of its own.
    let (code, again) = serve(&ws, &shelf, json!({"folder": "site/"}));
    assert_eq!(code, 200, "{again}");
    let again_url = again["url"].as_str().expect("its URL").to_string();
    assert_eq!(status(&again_url, "/"), 200);

    // Every start and every stop was said, naming the checkout.
    let said = ws.until("the servers' changes to be heard", || {
        let said: Vec<Value> = listening
            .heard()
            .frames
            .iter()
            .filter(|frame| frame["stream"] == "engine")
            .map(|frame| frame["payload"]["payload"].clone())
            .filter(|said| said["type"] == "server_changed")
            .collect();
        (said.len() >= 4).then_some(said)
    });
    assert!(
        said.iter().all(|s| s["workstream"] == shelf.as_str()),
        "{said:?}"
    );

    // --- the node goes: its servers with it, and none comes back by itself ------
    ws.stop();
    assert_eq!(
        get(&whole_url, "/"),
        None,
        "a server lives while its node does"
    );
    assert_eq!(get(&again_url, "/"), None);
    ws.start();
    let (_, every) = ws.call("GET", "/servers", &[], None);
    assert_eq!(every["servers"], json!([]), "the servers are no records");
    ws.stop();
}

/// What a scripted tool call was answered: whether it was refused, and the
/// words — a result's own, or the refusal's when the server answered with
/// one.
fn answered(ws: &Sealed, tool: &str) -> Vec<(bool, String)> {
    ws.recorded("tool")
        .iter()
        .filter(|fact| fact["name"] == tool)
        .map(|fact| {
            let result = &fact["result"];
            let words = match result["error"].as_str() {
                Some(refusal) => refusal.to_string(),
                None => result["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            (fact["failed"] == true, words)
        })
        .collect()
}

#[test]
fn a_browser_tool_is_told_at_once_that_nobody_is_home_and_answered_when_somebody_is() {
    let turn = |when: &str, tool: &str, arguments: Value, says: &str| {
        json!({
            "scope": "conversation", "when": when, "times": 1,
            "tools": [{ "name": tool, "arguments": arguments }],
            "say": [says],
        })
    };
    let mut ws = Sealed::with_script(&json!({ "turns": [
        turn("open the page", "browser_open", json!({"url": "http://127.0.0.1:9/"}), "I could not open it."),
        turn("serve the site", "browser_serve", json!({"folder": "site"}), "Served."),
        turn("look again", "browser_open", json!({"url": "http://127.0.0.1:9/shelf"}), "Opened."),
    ]}));
    ws.start();
    let (shelf, _tree) = a_checkout(&ws, "shelf");
    let talk = ws.json(&["conversation", "new", "project", &shelf]);
    let talk = talk["conversation"]["id"]
        .as_str()
        .expect("the conversation");
    let said = |words: &str| -> bool {
        ws.json(&["msgs", talk])["messages"]
            .as_array()
            .is_some_and(|all| all.iter().any(|m| m["content"] == words))
    };

    // --- nobody home: said at once, never after the bridge's long wait -----------
    let asked_at = Instant::now();
    ws.ok(&["conversation", "post", talk, "open the page, please"]);
    ws.until("the agent to give up", || {
        said("I could not open it.").then_some(())
    });
    assert!(
        asked_at.elapsed() < Duration::from_secs(30),
        "told at once, not after the minute a parked request waits: {:?}",
        asked_at.elapsed()
    );
    let opened = answered(&ws, "browser_open");
    assert_eq!(opened.len(), 1, "{opened:?}");
    assert!(opened[0].0, "a refusal: {opened:?}");
    assert!(
        opened[0]
            .1
            .contains("the embedded browser is not available")
            && opened[0].1.contains("none is open"),
        "{opened:?}"
    );
    let (_, parked) = ws.call("GET", "/browser/requests", &[], None);
    assert_eq!(parked["requests"], json!([]), "nothing was left waiting");

    // --- a session serves its own checkout, and the page is there ----------------
    ws.ok(&["conversation", "post", talk, "serve the site, please"]);
    ws.until("the folder to be served", || said("Served.").then_some(()));
    let served = answered(&ws, "browser_serve");
    assert!(!served[0].0, "{served:?}");
    let (_, up) = ws.call("GET", &format!("/workstreams/{shelf}/servers"), &[], None);
    let url = up["servers"][0]["url"]
        .as_str()
        .expect("the server")
        .to_string();
    assert!(
        served[0].1.contains(&url),
        "the tool answers the URL: {served:?}"
    );
    assert_eq!(up["servers"][0]["owner"]["folder"], "site");
    assert_eq!(
        get(&url, "/"),
        Some((200, "<h1>The shelf</h1>".to_string()))
    );

    // --- somebody home: the request waits for the desktop, and its answer is the
    // tool's. The journey is the desktop: it read the list — which is how the
    // engine knows one is home — and answers what it finds there.
    ws.ok(&["conversation", "post", talk, "look again, please"]);
    let request = ws.until("the request to wait for a desktop", || {
        let (_, parked) = ws.call("GET", "/browser/requests", &[], None);
        parked["requests"]
            .as_array()
            .and_then(|all| all.first().cloned())
    });
    assert_eq!(request["request"]["action"], "open", "{request}");
    assert_eq!(request["request"]["url"], "http://127.0.0.1:9/shelf");
    assert_eq!(
        request["scope"]["home"],
        json!({"scope": "workstream", "id": shelf}),
        "at home in the checkout the conversation is about: {request}"
    );
    assert_eq!(
        request["headless"], false,
        "a person is in the conversation"
    );
    let id = request["id"].as_str().expect("its id");
    let (code, refused) = ws.call(
        "POST",
        &format!("/browser/requests/{id}"),
        &[],
        Some(&json!({"ok": true, "tab": "b1", "selector": "#made-up"})),
    );
    assert_eq!(code, 400, "an answer with a key nobody knows: {refused}");
    let (code, taken) = ws.call(
        "POST",
        &format!("/browser/requests/{id}"),
        &[],
        Some(&json!({
            "ok": true, "tab": "b1", "url": "http://127.0.0.1:9/shelf", "title": "The shelf",
        })),
    );
    assert_eq!(code, 200, "{taken}");
    ws.until("the agent to hear the desktop's answer", || {
        said("Opened.").then_some(())
    });
    let opened = answered(&ws, "browser_open");
    assert_eq!(opened.len(), 2, "{opened:?}");
    assert!(!opened[1].0, "{opened:?}");
    assert!(
        opened[1].1.contains("b1") && opened[1].1.contains("The shelf"),
        "the tab and the page, as the desktop said them: {opened:?}"
    );
    let (code, _) = ws.call(
        "POST",
        &format!("/browser/requests/{id}"),
        &[],
        Some(&json!({"ok": true})),
    );
    assert_eq!(code, 404, "answered once, it waits no more");
    ws.stop();
}

/// Mobile development is off until this machine says otherwise, and off is
/// said before anything is looked for: the person's routes refuse with the
/// sentence, an agent's tools refuse with it, and no program of this
/// machine's is asked anything. The journey never turns the switch on — the
/// devices of the machine it runs on are not a test's to list — and never
/// reads the status, the one route that examines the machine either way.
#[test]
fn mobile_development_is_off_until_this_machine_says_otherwise() {
    let turn = |when: &str, tool: &str, arguments: Value, says: &str| {
        json!({
            "scope": "conversation", "when": when, "times": 1,
            "tools": [{ "name": tool, "arguments": arguments }],
            "say": [says],
        })
    };
    let mut ws = Sealed::with_script(&json!({ "turns": [
        turn("which devices", "mobile_development_devices", json!({}), "None that I may list."),
        turn("boot one", "mobile_development_boot", json!({"device": "a-device-nobody-has"}), "None that I may boot."),
        turn("the screen", "mobile_development_screenshot", json!({"device": "a-device-nobody-has"}), "None that I may read."),
    ]}));
    ws.start();
    let (shelf, _tree) = a_checkout(&ws, "shelf");
    let (code, set) = ws.call("GET", "/settings/resolved", &[], None);
    assert_eq!(code, 200, "{set}");
    let enabled = set["settings"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|row| row["key"] == "mobile_development.enabled")
        })
        .unwrap_or_else(|| panic!("the switch is a setting: {set}"));
    assert_eq!(enabled["value"], false, "off until somebody turns it on");

    for (method, route, body) in [
        ("GET", "/mobile-development/devices".to_string(), None),
        (
            "POST",
            "/mobile-development/devices/a-device-nobody-has/boot".to_string(),
            None,
        ),
        (
            "POST",
            "/mobile-development/devices/a-device-nobody-has/shutdown".to_string(),
            None,
        ),
        (
            "POST",
            "/mobile-development/devices/a-device-nobody-has/screenshot".to_string(),
            None,
        ),
        (
            "GET",
            "/mobile-development/devices/a-device-nobody-has/frame".to_string(),
            None,
        ),
        (
            "POST",
            "/mobile-development/simulators".to_string(),
            Some(json!({"name": "A", "devicetype": "B", "runtime": "C"})),
        ),
        (
            "GET",
            format!(
                "/workstreams/{shelf}/mobile-development/run-command?device=a-device-nobody-has"
            ),
            None,
        ),
    ] {
        let (code, refused) = ws.call(method, &route, &[], body.as_ref());
        assert_eq!(code, 409, "{method} {route}: {refused}");
        assert!(
            refused["error"]
                .as_str()
                .is_some_and(|said| said.contains("turned off")),
            "{method} {route}: {refused}"
        );
    }

    let talk = ws.json(&["conversation", "new", "project", &shelf]);
    let talk = talk["conversation"]["id"]
        .as_str()
        .expect("the conversation");
    for (words, reply, tool) in [
        (
            "which devices are there?",
            "None that I may list.",
            "mobile_development_devices",
        ),
        (
            "boot one, please",
            "None that I may boot.",
            "mobile_development_boot",
        ),
        (
            "read the screen, please",
            "None that I may read.",
            "mobile_development_screenshot",
        ),
    ] {
        ws.ok(&["conversation", "post", talk, words]);
        ws.until(&format!("the reply {reply:?}"), || {
            ws.json(&["msgs", talk])["messages"]
                .as_array()
                .is_some_and(|all| all.iter().any(|m| m["content"] == reply))
                .then_some(())
        });
        let told = answered(&ws, tool);
        assert_eq!(told.len(), 1, "{tool}: {told:?}");
        assert!(told[0].0, "{tool} is refused: {told:?}");
        assert!(
            told[0].1.contains("mobile development is turned off"),
            "{tool}: {told:?}"
        );
    }
    // And the prompt said nothing of mobile: the note rides only where it is on.
    assert!(
        ws.recorded("prompt").iter().all(|p| !p["text"]
            .as_str()
            .unwrap_or_default()
            .contains("mobile_development_devices")),
        "no session is told of tools that are off"
    );
    ws.stop();
}
