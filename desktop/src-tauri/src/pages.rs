//! Pages an agent wrote, served to this webview alone (12 — Artifacts;
//! 11 — Security § Pages an agent wrote).
//!
//! A page artifact and the page the IDE annotates used to be `srcdoc`
//! frames. A document loaded from `about:srcdoc` inherits its parent's
//! Content-Security-Policy (CSP3 § 7.8, as `data:` and `blob:` documents
//! do), so the moment the main window carries a policy of its own —
//! `script-src 'self'`, no inline script — every page an agent wrote would
//! go blank. The webview therefore **stages** a page here (`page_stage`) and
//! loads it as `bisa-page://page/<id>/`: a document under its own scheme
//! inherits nothing, carries its own policy as a response header (the same
//! words `pageDocument.mjs` puts in its head), keeps an opaque origin under
//! `sandbox="allow-scripts"`, and has no IPC — the shell's commands answer
//! the app's own page, and a custom-scheme document is not it. Nothing is
//! written to disk and the node still serves no page: the bytes live in
//! memory, keyed by sixteen random bytes nobody can guess, held until the
//! frame that asked releases them (`page_release`) — a page may reload
//! itself meanwhile — and at most [`MAX_STAGED`] at a time, the oldest
//! dropped, since a page nobody released is a frame that is gone. The
//! scheme answers the main window alone: a child webview (an embedded tab)
//! asking for a staged page gets a 403, whatever it knows.

use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, State, UriSchemeContext};

/// The scheme, as `tauri.conf.json`'s `frame-src` and `navigation.rs` name it.
pub const SCHEME: &str = "bisa-page";
/// How many pages stay staged at once; the oldest goes when one more comes.
pub const MAX_STAGED: usize = 64;
/// The most a staged page may weigh: a page is a document, not a dataset.
pub const MAX_PAGE_BYTES: usize = 32 * 1024 * 1024;
/// The host every staged page stands under: `bisa-page://page/<id>/`.
const HOST: &str = "page";

struct Staged {
    id: String,
    html: Vec<u8>,
    csp: String,
}

/// The staged pages, newest last.
#[derive(Default)]
pub struct PageStore(Mutex<VecDeque<Staged>>);

impl PageStore {
    /// Stage `html` under a fresh name; the URL the frame loads.
    pub fn stage(&self, html: String, csp: String) -> Result<String, String> {
        if html.len() > MAX_PAGE_BYTES {
            return Err(format!(
                "the page is {} bytes; at most {MAX_PAGE_BYTES} can be staged",
                html.len()
            ));
        }
        let id =
            crate::random::random_hex(16).map_err(|e| format!("could not name the page: {e}"))?;
        let mut pages = self.0.lock().unwrap_or_else(|e| e.into_inner());
        while pages.len() >= MAX_STAGED {
            pages.pop_front();
        }
        pages.push_back(Staged {
            id: id.clone(),
            html: html.into_bytes(),
            csp,
        });
        Ok(url_of(&id))
    }

    /// Let a staged page go; `true` when it was there.
    pub fn release(&self, id: &str) -> bool {
        let mut pages = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = pages.len();
        pages.retain(|p| p.id != id);
        pages.len() != before
    }

    /// The answer to one request under the scheme: the page named by the
    /// URL's first path segment, with its policy; anything else a 404.
    pub fn serve(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let uri = request.uri();
        let path = uri.path();
        // `bisa-page://page/<id>/` — the id is the one segment; a page's
        // relative reference (`/x.png`) names nothing staged.
        let id = match (
            uri.host(),
            path.trim_start_matches('/')
                .split('/')
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            (Some(HOST), [id, rest @ ..]) if rest.iter().all(|s| s.is_empty()) => id.to_string(),
            _ => return not_found(),
        };
        let pages = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(page) = pages.iter().find(|p| p.id == id) else {
            return not_found();
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CONTENT_SECURITY_POLICY, page.csp.as_str())
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::REFERRER_POLICY, "no-referrer")
            .body(page.html.clone())
            .unwrap_or_else(|_| not_found())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

fn url_of(id: &str) -> String {
    format!("{SCHEME}://{HOST}/{id}/")
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(b"no such page".to_vec())
        .expect("a literal response")
}

fn forbidden() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(b"not this webview's".to_vec())
        .expect("a literal response")
}

/// The scheme's handler, registered in `main.rs`: the main window's
/// requests answered from the store, every other webview's refused.
// LCOV_EXCL_START reached only through the webview's own loads; `serve` and `for_webview` are tested
pub fn protocol<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let store = ctx.app_handle().state::<PageStore>();
    for_webview(&store, ctx.webview_label(), &request)
}
// LCOV_EXCL_STOP

/// `serve` for the main window; a 403 for any other webview.
fn for_webview(store: &PageStore, label: &str, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    if label != crate::tray::MAIN_WINDOW {
        return forbidden();
    }
    store.serve(request)
}

// LCOV_EXCL_START a Tauri command over the running app's state; out by decision — `stage` and `release` are tested
/// Stage a page; the URL a sandboxed frame loads it from.
#[tauri::command]
pub fn page_stage(
    store: State<'_, PageStore>,
    html: String,
    csp: String,
) -> Result<String, String> {
    store.stage(html, csp)
}

/// Let a staged page go, by the id in its URL.
#[tauri::command]
pub fn page_release(store: State<'_, PageStore>, id: String) -> Result<bool, String> {
    Ok(store.release(&id))
}
// LCOV_EXCL_STOP

#[cfg(test)]
mod tests {
    use super::*;

    fn get(url: &str) -> Request<Vec<u8>> {
        Request::builder().uri(url).body(Vec::new()).unwrap()
    }

    fn id_of(url: &str) -> &str {
        url.trim_end_matches('/').rsplit('/').next().unwrap()
    }

    #[test]
    fn a_staged_page_is_served_by_its_id_with_its_policy_until_released() {
        let store = PageStore::default();
        let csp = "default-src 'none'; script-src 'unsafe-inline'";
        let url = store.stage("<p>hi</p>".into(), csp.into()).unwrap();
        assert!(
            url.starts_with("bisa-page://page/") && url.ends_with('/'),
            "{url}"
        );
        assert_eq!(id_of(&url).len(), 32, "sixteen random bytes as hex");

        let answer = store.serve(&get(&url));
        assert_eq!(answer.status(), StatusCode::OK);
        assert_eq!(answer.body(), b"<p>hi</p>");
        let h = |name: header::HeaderName| {
            answer
                .headers()
                .get(name)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(h(header::CONTENT_TYPE), "text/html; charset=utf-8");
        assert_eq!(h(header::CONTENT_SECURITY_POLICY), csp);
        assert_eq!(h(header::CACHE_CONTROL), "no-store");
        assert_eq!(h(header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        assert_eq!(h(header::REFERRER_POLICY), "no-referrer");

        // Served again — a page may reload itself — until released.
        assert_eq!(store.serve(&get(&url)).status(), StatusCode::OK);
        assert!(store.release(id_of(&url)));
        assert!(!store.release(id_of(&url)), "released once");
        assert_eq!(store.serve(&get(&url)).status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn an_unknown_id_a_relative_reference_and_another_host_are_404s() {
        let store = PageStore::default();
        let url = store
            .stage("<p>hi</p>".into(), "default-src 'none'".into())
            .unwrap();
        let id = id_of(&url).to_string();
        for bad in [
            "bisa-page://page/0123456789abcdef0123456789abcdef/".to_string(),
            format!("bisa-page://page/{id}/x.png"),
            format!("bisa-page://page/{id}/a/"),
            format!("bisa-page://other/{id}/"),
            "bisa-page://page/".to_string(),
            "bisa-page://page".to_string(),
        ] {
            let answer = store.serve(&get(&bad));
            assert_eq!(answer.status(), StatusCode::NOT_FOUND, "{bad}");
            assert_eq!(
                answer.headers().get(header::CACHE_CONTROL).unwrap(),
                "no-store"
            );
        }
        // Without its trailing slash the page is still the page.
        assert_eq!(
            store.serve(&get(url.trim_end_matches('/'))).status(),
            StatusCode::OK
        );
    }

    #[test]
    fn the_store_keeps_no_more_than_the_cap_and_refuses_a_page_past_the_weight() {
        let store = PageStore::default();
        let first = store.stage("1".into(), String::new()).unwrap();
        for _ in 0..MAX_STAGED {
            store.stage("n".into(), String::new()).unwrap();
        }
        assert_eq!(store.len(), MAX_STAGED);
        assert_eq!(
            store.serve(&get(&first)).status(),
            StatusCode::NOT_FOUND,
            "the oldest went"
        );
        let heavy = "x".repeat(MAX_PAGE_BYTES + 1);
        let refused = store.stage(heavy, String::new()).unwrap_err();
        assert!(refused.contains("at most"), "{refused}");
        assert_eq!(store.len(), MAX_STAGED, "a refused page is not staged");
    }

    #[test]
    fn only_the_main_window_is_answered() {
        let store = PageStore::default();
        let url = store.stage("<p>hi</p>".into(), String::new()).unwrap();
        assert_eq!(
            for_webview(&store, crate::tray::MAIN_WINDOW, &get(&url)).status(),
            StatusCode::OK
        );
        let tab = for_webview(&store, "tab-1", &get(&url));
        assert_eq!(tab.status(), StatusCode::FORBIDDEN);
        assert_eq!(tab.body(), b"not this webview's");
    }
}
