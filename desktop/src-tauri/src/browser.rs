//! The embedded browser (ide/18): one native child webview per browser tab,
//! laid over the main window where the tab's body is — the IDE's centre, or
//! the Browser pane — moved and hidden as the webview says. The webview
//! says where the tab's body is and which tab is showing; this side owns
//! the webviews and nothing else — no page is read here, no script decided
//! here. The one thing read is pixels: a screenshot of a tab is WebKit's
//! own snapshot of that webview, never the screen, so no grant is asked.
//!
//! What a page may reach: the init script the webview hands `browser_open`
//! runs in every page the tab loads, and every page has the same one door
//! out — a script message handler named `bisa` on the tab's user content
//! controller (`door::PageDoor`), whose words are re-emitted to the main
//! window as `browser:message`. No page has IPC: the tabs' webviews stand
//! in no capability, so nothing of the app is reachable from a page,
//! whatever its origin; what a page says is data the main window bounds.
//! A navigation to anything but http(s) or `about:blank` is refused.
//!
//! Two things every page says itself, because the webview reads them: its
//! title (`browser:titled`), and a window it asks for — a link with a
//! target, `window.open` — which is refused here and re-said to the main
//! window (`browser:new-window`), where it becomes a tab of ours. Every
//! load's start and finish (`browser:navigated`) carries whether the tab
//! can go back or forward, read from the webview's own history.

//! Where a tab's webview goes is said in the main page's own CSS pixels
//! (`Bounds`) and drawn **anchored on the main webview** (`anchor::Anchor`):
//! on macOS every tab stands in the browser layer (`layer`), a view the
//! content view's size just above the main `WKWebView`, which cuts a hole
//! wherever one of the main page's floating overlays sits (`clear`,
//! `browser_clear`) so the overlay shows and takes its clicks over a live
//! page. The page's `(0, 0)` need not be that view's top-left — a content inset
//! puts the page lower than the frame, a zoom makes a CSS pixel more than a
//! point — so the child is placed relative to the main webview's frame and
//! the page's viewport, read together on the main thread, and its frame is
//! read back through the same map. The anchor is logged once per change.

use crate::sync::Locked;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, State, Url, Webview,
    WebviewUrl,
};
#[cfg(not(target_os = "macos"))]
use tauri::{Position, Rect, Size};

/// The event the main window hears when a tab's page moves.
pub const NAVIGATED: &str = "browser:navigated";
/// The event the main window hears when a page says something.
pub const MESSAGE: &str = "browser:message";
/// The event the main window hears when a page's title is known.
pub const TITLED: &str = "browser:titled";
/// The event the main window hears when a page asked for a window.
pub const NEW_WINDOW: &str = "browser:new-window";
/// The page a blank tab shows: nothing of anyone's.
pub const BLANK: &str = "about:blank";
/// The name the page's script posts to: `window.webkit.messageHandlers.bisa`.
pub const DOOR: &str = "bisa";
/// The most a page may say in one message; a longer one is dropped whole.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// A tab's webview label: `browser-<key>` — never the window's own
/// (`tray::MAIN_WINDOW`), so the window stays findable by its label.
pub(crate) fn label_of(key: &str) -> String {
    format!("browser-{key}")
}

/// The open tabs, by key — each with the anchor its last placement used,
/// so a changed anchor is logged once and a repeat is not.
#[derive(Default)]
pub struct BrowserRegistry {
    views: Mutex<HashMap<String, Option<anchor::Anchor>>>,
    /// How many pieces the browser layer cut last (`browser_clear`).
    holes: Mutex<Option<usize>>,
}

impl BrowserRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, key: &str) {
        self.views.locked().insert(key.to_string(), None);
    }

    /// Note the anchor a tab was placed with; true when it differs from the
    /// last one noted for that tab — the one time it is worth a log line.
    fn note_anchor(&self, key: &str, anchor: anchor::Anchor) -> bool {
        let mut views = self.views.locked();
        match views.get_mut(key) {
            Some(slot) if *slot != Some(anchor) => {
                *slot = Some(anchor);
                true
            }
            _ => false,
        }
    }

    /// Note how many pieces the browser layer cut; true when that differs
    /// from the last count — the one time it is worth a log line.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn note_holes(&self, count: usize) -> bool {
        let mut last = self.holes.locked();
        let changed = *last != Some(count);
        *last = Some(count);
        changed
    }

    fn remove(&self, key: &str) -> bool {
        self.views.locked().remove(key).is_some()
    }

    fn keys(&self) -> Vec<String> {
        self.views.locked().keys().cloned().collect()
    }

    /// Close every tab — the app is going (`RunEvent::Exit`); a close only hides the window.
    pub fn close_all<R: Runtime>(&self, app: &AppHandle<R>) {
        for key in self.keys() {
            if let Some(view) = app.get_webview(&label_of(&key)) {
                if let Err(e) = view.close() {
                    // The layer is going whatever happens; a view that would
                    // not close is said, not swallowed.
                    tracing::warn!(target: "bisa_desktop", key = %key, "a browser view did not close: {e}");
                }
            }
            self.remove(&key);
        }
    }
}

/// A tab's box in the main window, in CSS pixels of the main webview.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Bounds {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// The main page's viewport in its own CSS pixels — what `window.innerWidth`
/// and `window.innerHeight` say — sent with every placement, so a content
/// inset or a zoom of the main webview is read off the page itself.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[cfg(not(target_os = "macos"))]
impl Bounds {
    /// The box as one logical rectangle, never thinner than a pixel.
    pub fn rect(&self) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(self.left, self.top)),
            size: Size::Logical(LogicalSize::new(self.width.max(1.0), self.height.max(1.0))),
        }
    }
}

/// The box a tab's webview was drawn in, read back after a placement — what
/// the main window compares with what it asked, in the same CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub shown: bool,
}

impl Placed {
    /// A webview read back, in logical pixels at the window's scale.
    #[cfg(not(target_os = "macos"))]
    pub fn of(rect: &Rect, scale: f64) -> Self {
        let position = rect.position.to_logical::<f64>(scale);
        let size = rect.size.to_logical::<f64>(scale);
        Self {
            left: position.x,
            top: position.y,
            width: size.width,
            height: size.height,
            shown: true,
        }
    }

    /// A webview put out of sight: no box to speak of.
    pub fn hidden() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            shown: false,
        }
    }
}

/// What the main window hears on a navigation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigated {
    pub key: String,
    pub url: String,
    /// `started` or `finished`.
    pub event: &'static str,
    /// The webview's own history: a page to go back to, one to go forward to.
    pub can_back: bool,
    pub can_forward: bool,
}

/// What a page said, as the main window hears it.
#[derive(Clone, Debug, Serialize)]
pub struct Said {
    pub key: String,
    pub message: serde_json::Value,
}

/// A page's title, as the webview reads it.
#[derive(Clone, Debug, Serialize)]
pub struct Titled {
    pub key: String,
    pub title: String,
}

/// A window a page asked for: the tab that asked and the page it wanted.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Opened {
    pub key: String,
    pub url: String,
}

/// A window a page asks for is a tab here when it is a page — http(s) —
/// and nothing otherwise: a scripted `window.open()` on `about:blank` or a
/// `javascript:` URL has no page to show.
pub(crate) fn opened(key: &str, url: &Url) -> Option<Opened> {
    matches!(url.scheme(), "http" | "https").then(|| Opened {
        key: key.to_string(),
        url: url.to_string(),
    })
}

/// Whether a tab may show this URL: an http(s) page, or the blank page.
pub(crate) fn allowed(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") || url.as_str() == BLANK
}

/// An http(s) URL or the blank page, or the reason it is neither.
pub(crate) fn admit(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url.trim()).map_err(|_| format!("{url:?} is not a URL"))?;
    if allowed(&parsed) {
        Ok(parsed)
    } else {
        Err(format!(
            "only http and https pages open here, not {}",
            parsed.scheme()
        ))
    }
}

/// What a page posted, as a message the main window may hear: one JSON
/// object under the size cap; anything else is nothing.
pub(crate) fn message_of(text: &str) -> Option<serde_json::Value> {
    if text.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .filter(|v| v.is_object())
}

fn view_of<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Webview<R>, String> {
    app.get_webview(&label_of(key))
        .ok_or_else(|| format!("no browser tab {key}"))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Say a tab's page moved, with what the webview knows of its history —
/// read on the main thread, where WebKit answers. A webview already gone
/// has nothing left to say.
fn say_navigated<R: Runtime>(
    app: AppHandle<R>,
    view: &Webview<R>,
    key: String,
    url: String,
    event: &'static str,
) {
    // A window that is gone hears nothing: the tab is closing with it.
    let _window_gone = view.with_webview(move |platform| {
        let (can_back, can_forward) = page::history(&platform);
        // The main window may be gone; nothing is owed to a window that is.
        let _window_gone = app.emit_to(
            "main",
            NAVIGATED,
            Navigated {
                key,
                url,
                event,
                can_back,
                can_forward,
            },
        );
    });
}

/// Open a tab: a child webview on `url`, the init script aboard, the
/// page's door installed, hidden until its first placement. The key is the
/// webview's — a tab already open under it is a refusal, not a second
/// webview.
#[tauri::command]
pub fn browser_open<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, BrowserRegistry>,
    key: String,
    url: String,
    script: String,
) -> Result<(), String> {
    let url = admit(&url)?;
    if app.get_webview(&label_of(&key)).is_some() {
        return Err(format!("browser tab {key} is already open"));
    }
    let window = app
        .get_window("main")
        .ok_or_else(|| "no main window".to_string())?;
    let loading = app.clone();
    let loading_key = key.clone();
    let titled = app.clone();
    let titled_key = key.clone();
    let asking = app.clone();
    let asking_key = key.clone();
    let builder = WebviewBuilder::new(label_of(&key), WebviewUrl::External(url))
        .initialization_script(&script)
        .on_navigation(allowed)
        .on_document_title_changed(move |_webview, title| {
            // The main window may be gone; nothing is owed to a window that is.
            let _window_gone = titled.emit_to(
                "main",
                TITLED,
                Titled {
                    key: titled_key.clone(),
                    title,
                },
            );
        })
        .on_new_window(move |url, _features| {
            if let Some(said) = opened(&asking_key, &url) {
                // The main window may be gone; nothing is owed to a window that is.
                let _window_gone = asking.emit_to("main", NEW_WINDOW, said);
            }
            NewWindowResponse::Deny
        })
        .on_page_load(move |webview, payload| {
            let event = match payload.event() {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };
            say_navigated(
                loading.clone(),
                &webview,
                loading_key.clone(),
                payload.url().to_string(),
                event,
            );
        });
    // Out of the way and one pixel until the main window says where the
    // tab's body is; hidden, so not even that pixel shows.
    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(1.0, 1.0),
        )
        .map_err(err)?;
    webview.hide().map_err(err)?;
    // Into the browser layer at once, still hidden, so an overlay of the main
    // page is never under it (`layer`).
    #[cfg(target_os = "macos")]
    if let Some(main) = app.get_webview("main") {
        if let Err(why) = place::adopt(&main, &webview) {
            tracing::warn!(target: "bisa_desktop", key, %why, "a tab stayed out of the browser layer; its first placement adopts it");
        }
    }
    let saying = app.clone();
    let saying_key = key.clone();
    door::install(
        &webview,
        Box::new(move |text| {
            if let Some(message) = message_of(&text) {
                // The main window may be gone; nothing is owed to a window that is.
                let _window_gone = saying.emit_to(
                    "main",
                    MESSAGE,
                    Said {
                        key: saying_key.clone(),
                        message,
                    },
                );
            }
        }),
    );
    registry.insert(&key);
    Ok(())
}

/// Move a tab's page to `url`.
#[tauri::command]
pub fn browser_navigate<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    url: String,
) -> Result<(), String> {
    let url = admit(&url)?;
    view_of(&app, &key)?.navigate(url).map_err(err)
}

/// One page back, in the webview's own history.
#[tauri::command]
pub fn browser_back<R: Runtime>(app: AppHandle<R>, key: String) -> Result<(), String> {
    page::back(&view_of(&app, &key)?)
}

/// One page forward.
#[tauri::command]
pub fn browser_forward<R: Runtime>(app: AppHandle<R>, key: String) -> Result<(), String> {
    page::forward(&view_of(&app, &key)?)
}

/// Load the page again.
#[tauri::command]
pub fn browser_reload<R: Runtime>(app: AppHandle<R>, key: String) -> Result<(), String> {
    page::reload(&view_of(&app, &key)?)
}

/// Stop a page on its way. The webview reports a load's start and its
/// finish and never a stop, so the finish is said here, on the page the
/// tab is on — else the tab would show loading for good.
#[tauri::command]
pub fn browser_stop<R: Runtime>(app: AppHandle<R>, key: String) -> Result<(), String> {
    let view = view_of(&app, &key)?;
    page::stop(&view)?;
    let url = view.url().map_err(err)?;
    say_navigated(app, &view, key, url.to_string(), "finished");
    Ok(())
}

/// How long a placement may take on the main thread before it is given up on.
pub const PLACEMENT_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Where the main page is in the window's content view, and how big one of
/// its CSS pixels is — what every tab's box is placed relative to.
///
/// AppKit's frames have their origin at the bottom-left of the superview;
/// the page's box has its origin at the top-left of the page. The two agree
/// only when the main webview fills the content view from its top and the
/// page fills the main webview: neither is a given. A top content inset —
/// the page's viewport shorter than the webview's frame — puts the page
/// lower; a zoom makes a CSS pixel more than a point; a main webview that is
/// shorter or lower than the content view moves the page with it. The
/// anchor is read from the frames and the viewport each time a tab is
/// placed, never assumed. Pure, so a test needs no window.
pub mod anchor {
    use super::{Bounds, Placed, Viewport};

    /// A view's frame in its superview: AppKit's points, origin at the bottom-left.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Frame {
        pub x: f64,
        pub y: f64,
        pub width: f64,
        pub height: f64,
    }

    /// Where the page's `(0, 0)` is, from the content view's left and top,
    /// and how many points one CSS pixel of the page is.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Anchor {
        pub parent_height: f64,
        /// The page's left edge, from the parent's left.
        pub left: f64,
        /// The page's top edge, from the parent's top — the inset included.
        pub top: f64,
        pub zoom: f64,
        /// Points between the main webview's top and the page's top.
        pub inset: f64,
    }

    impl Anchor {
        /// `zoom` is the main webview's width over the page's; `inset` is
        /// what of the webview's height the page does not fill, taken to be
        /// at its top; `top` is the webview's top from the parent's top, the
        /// inset added.
        pub fn of(parent_height: f64, main: Frame, viewport: Viewport) -> Self {
            let zoom = if main.width > 0.0 && viewport.width > 0.0 {
                main.width / viewport.width
            } else {
                1.0
            };
            let inset = (main.height - viewport.height * zoom).max(0.0);
            Self {
                parent_height,
                left: main.x,
                top: (parent_height - (main.y + main.height)) + inset,
                zoom,
                inset,
            }
        }

        /// The child's frame in the parent for a box in page CSS pixels —
        /// never thinner than a pixel.
        pub fn frame_for(&self, bounds: &Bounds) -> Frame {
            let width = bounds.width.max(1.0) * self.zoom;
            let height = bounds.height.max(1.0) * self.zoom;
            Frame {
                x: self.left + bounds.left * self.zoom,
                y: self.parent_height - self.top - bounds.top * self.zoom - height,
                width,
                height,
            }
        }

        /// A frame read back, as the box it is in page CSS pixels — the
        /// inverse of `frame_for`, so a child drawn elsewhere reads elsewhere.
        pub fn placed_from(&self, frame: &Frame) -> Placed {
            Placed {
                left: (frame.x - self.left) / self.zoom,
                top: (self.parent_height - frame.y - frame.height - self.top) / self.zoom,
                width: frame.width / self.zoom,
                height: frame.height / self.zoom,
                shown: true,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const BOX: Bounds = Bounds {
            left: 100.0,
            top: 36.0,
            width: 600.0,
            height: 400.0,
        };

        fn close(a: f64, b: f64) -> bool {
            (a - b).abs() < 1e-9
        }

        fn same(placed: &Placed, bounds: &Bounds) -> bool {
            close(placed.left, bounds.left)
                && close(placed.top, bounds.top)
                && close(placed.width, bounds.width)
                && close(placed.height, bounds.height)
                && placed.shown
        }

        #[test]
        fn the_anchor_is_the_identity_when_the_page_fills_the_content_view() {
            let main = Frame {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 800.0,
            };
            let a = Anchor::of(
                800.0,
                main,
                Viewport {
                    width: 1200.0,
                    height: 800.0,
                },
            );
            assert_eq!(
                a,
                Anchor {
                    parent_height: 800.0,
                    left: 0.0,
                    top: 0.0,
                    zoom: 1.0,
                    inset: 0.0
                }
            );
            let f = a.frame_for(&BOX);
            assert_eq!(
                f,
                Frame {
                    x: 100.0,
                    y: 364.0,
                    width: 600.0,
                    height: 400.0
                },
                "the top-left box flipped into AppKit's bottom-left"
            );
            assert!(same(&a.placed_from(&f), &BOX));
        }

        #[test]
        fn a_top_inset_of_28_moves_the_page_down_by_28() {
            let main = Frame {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 800.0,
            };
            let a = Anchor::of(
                800.0,
                main,
                Viewport {
                    width: 1200.0,
                    height: 772.0,
                },
            );
            assert_eq!(a.inset, 28.0);
            assert_eq!(a.top, 28.0);
            assert_eq!(a.frame_for(&BOX).y, 800.0 - 28.0 - 36.0 - 400.0);
        }

        #[test]
        fn a_shorter_lower_main_webview_anchors_the_page_to_its_own_top() {
            let main = Frame {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 760.0,
            };
            let a = Anchor::of(
                800.0,
                main,
                Viewport {
                    width: 1200.0,
                    height: 760.0,
                },
            );
            assert_eq!(a.inset, 0.0);
            assert_eq!(
                a.top, 40.0,
                "the page starts 40 points under the parent's top"
            );
            assert_eq!(a.frame_for(&BOX).y, 800.0 - 40.0 - 36.0 - 400.0);
        }

        #[test]
        fn a_zoom_of_1_5_scales_every_edge() {
            let main = Frame {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 800.0,
            };
            let a = Anchor::of(
                800.0,
                main,
                Viewport {
                    width: 800.0,
                    height: 800.0 / 1.5,
                },
            );
            assert!(close(a.zoom, 1.5));
            assert!(close(a.inset, 0.0));
            let f = a.frame_for(&BOX);
            assert!(close(f.x, 150.0) && close(f.width, 900.0) && close(f.height, 600.0));
            assert!(close(f.y, 800.0 - 54.0 - 600.0));
        }

        #[test]
        fn a_placement_reads_back_as_asked_whatever_the_anchor() {
            let main = Frame {
                x: 4.0,
                y: 2.0,
                width: 1180.0,
                height: 760.0,
            };
            for viewport in [
                Viewport {
                    width: 1180.0,
                    height: 760.0,
                },
                Viewport {
                    width: 1180.0,
                    height: 732.0,
                },
                Viewport {
                    width: 590.0,
                    height: 366.0,
                },
            ] {
                let a = Anchor::of(800.0, main, viewport);
                for bounds in [
                    BOX,
                    Bounds {
                        left: 0.0,
                        top: 0.0,
                        width: 0.0,
                        height: 0.0,
                    },
                ] {
                    let asked = Bounds {
                        width: bounds.width.max(1.0),
                        height: bounds.height.max(1.0),
                        ..bounds
                    };
                    assert!(
                        same(&a.placed_from(&a.frame_for(&bounds)), &asked),
                        "{viewport:?} {bounds:?}"
                    );
                }
            }
            // A frame drawn elsewhere reads elsewhere: the read-back is no echo.
            let a = Anchor::of(
                800.0,
                main,
                Viewport {
                    width: 1180.0,
                    height: 760.0,
                },
            );
            let mut drawn = a.frame_for(&BOX);
            drawn.y += 36.0;
            assert!(close(a.placed_from(&drawn).top, 0.0));
        }
    }
}

/// What of the browser layer is see-through (ide/18): the boxes of the main
/// page's floating overlays — the Notes and Draw panels, their docks, the
/// pet, an addon window — said in the page's CSS pixels like a tab's box
/// (`Bounds`), mapped through the same `Anchor`, and made into **disjoint**
/// pieces, so one test of a point answers both what is drawn (the mask) and
/// what is clicked (`hitTest:`). A lone overlay keeps its rounded corners;
/// overlays that overlap are cut as their union — each one's rounded corners
/// kept where no other overlay touches them, squared where one does — so no
/// sliver of page shows where two meet. Pure, so a test needs no window.
pub mod clear {
    use super::anchor::{Anchor, Frame};
    use super::Bounds;
    use serde::Deserialize;

    /// The most overlays one layer cuts around; more are dropped, never
    /// drawn wrong.
    pub const MAX_CLEARS: usize = 16;

    /// One overlay's box, as the main page measures it: CSS pixels from the
    /// page's top-left, and its corner radius.
    #[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
    pub struct Clear {
        pub left: f64,
        pub top: f64,
        pub width: f64,
        pub height: f64,
        #[serde(default)]
        pub radius: f64,
    }

    /// An overlay's box in the layer's points (bottom-left origin), with
    /// its corner radius in points — never more than half its short side.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Hole {
        pub frame: Frame,
        pub radius: f64,
    }

    /// One disjoint piece of what is see-through.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Piece {
        Rounded(Hole),
        Rect(Frame),
        /// A rounded corner of an overlay that meets others, which no other
        /// overlay touches: the quarter disc about `(cx, cy)` reaching toward
        /// `(sx, sy)`, each ±1.
        Corner {
            cx: f64,
            cy: f64,
            radius: f64,
            sx: f64,
            sy: f64,
        },
    }

    impl Piece {
        /// A corner piece's square — what of the layer it may cover.
        fn square(cx: f64, cy: f64, radius: f64, sx: f64, sy: f64) -> Frame {
            Frame {
                x: cx.min(cx + sx * radius),
                y: cy.min(cy + sy * radius),
                width: radius,
                height: radius,
            }
        }
    }

    fn usable(c: &Clear) -> bool {
        [c.left, c.top, c.width, c.height]
            .iter()
            .all(|v| v.is_finite())
            && c.width > 0.0
            && c.height > 0.0
    }

    /// The overlays' boxes as holes in the layer: each through the anchor
    /// a tab's box goes through, empty or non-finite boxes dropped, at most
    /// `MAX_CLEARS`.
    pub fn holes(anchor: &Anchor, clears: &[Clear]) -> Vec<Hole> {
        clears
            .iter()
            .filter(|c| usable(c))
            .take(MAX_CLEARS)
            .map(|c| {
                let frame = anchor.frame_for(&Bounds {
                    left: c.left,
                    top: c.top,
                    width: c.width,
                    height: c.height,
                });
                let most = frame.width.min(frame.height) / 2.0;
                let radius = if c.radius.is_finite() && c.radius > 0.0 {
                    (c.radius * anchor.zoom).min(most)
                } else {
                    0.0
                };
                Hole { frame, radius }
            })
            .collect()
    }

    fn overlap(a: &Frame, b: &Frame) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    fn inside(f: &Frame, x: f64, y: f64) -> bool {
        x >= f.x && x <= f.x + f.width && y >= f.y && y <= f.y + f.height
    }

    /// The union of overlapping boxes as disjoint rectangles: the grid of
    /// their edges, a cell kept when its middle is in a box, the cells of a
    /// row merged into runs.
    fn union(frames: &[Frame]) -> Vec<Frame> {
        let edges = |pick: &dyn Fn(&Frame) -> [f64; 2]| {
            let mut v: Vec<f64> = frames.iter().flat_map(pick).collect();
            v.sort_by(f64::total_cmp);
            v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            v
        };
        let xs = edges(&|f| [f.x, f.x + f.width]);
        let ys = edges(&|f| [f.y, f.y + f.height]);
        let mut out = Vec::new();
        for row in ys.windows(2) {
            let (y0, y1) = (row[0], row[1]);
            let cy = (y0 + y1) / 2.0;
            let mut run: Option<(f64, f64)> = None;
            for col in xs.windows(2) {
                let (x0, x1) = (col[0], col[1]);
                let cx = (x0 + x1) / 2.0;
                if frames.iter().any(|f| inside(f, cx, cy)) {
                    run = Some(run.map_or((x0, x1), |(a, _)| (a, x1)));
                } else if let Some((a, b)) = run.take() {
                    out.push(Frame {
                        x: a,
                        y: y0,
                        width: b - a,
                        height: y1 - y0,
                    });
                }
            }
            if let Some((a, b)) = run {
                out.push(Frame {
                    x: a,
                    y: y0,
                    width: b - a,
                    height: y1 - y0,
                });
            }
        }
        out
    }

    /// The holes as disjoint pieces: a hole that meets no other keeps its
    /// rounded corners; a group that overlaps becomes its union — each
    /// member's cross (its box less its four corner squares) and the corner
    /// squares another member touches, as rectangles, and every corner no
    /// other member touches as a quarter disc, which lies inside no other
    /// member and so meets no rectangle.
    pub fn pieces(holes: &[Hole]) -> Vec<Piece> {
        // Group the holes that overlap, directly or through another.
        let mut group: Vec<usize> = (0..holes.len()).collect();
        fn root(group: &mut [usize], mut i: usize) -> usize {
            while group[i] != i {
                group[i] = group[group[i]];
                i = group[i];
            }
            i
        }
        for i in 0..holes.len() {
            for j in (i + 1)..holes.len() {
                if overlap(&holes[i].frame, &holes[j].frame) {
                    let (a, b) = (root(&mut group, i), root(&mut group, j));
                    group[a] = b;
                }
            }
        }
        let mut out = Vec::new();
        for i in 0..holes.len() {
            if root(&mut group, i) != i {
                continue;
            }
            let members: Vec<&Hole> = (0..holes.len())
                .filter(|&j| root(&mut group, j) == i)
                .map(|j| &holes[j])
                .collect();
            if let [only] = members.as_slice() {
                out.push(Piece::Rounded(**only));
                continue;
            }
            let mut rects = Vec::new();
            for (k, m) in members.iter().enumerate() {
                let (f, r) = (m.frame, m.radius);
                if r <= 0.0 {
                    rects.push(f);
                    continue;
                }
                rects.push(Frame {
                    x: f.x,
                    y: f.y + r,
                    width: f.width,
                    height: f.height - 2.0 * r,
                });
                rects.push(Frame {
                    x: f.x + r,
                    y: f.y,
                    width: f.width - 2.0 * r,
                    height: f.height,
                });
                for (cx, cy, sx, sy) in [
                    (f.x + r, f.y + r, -1.0, -1.0),
                    (f.x + f.width - r, f.y + r, 1.0, -1.0),
                    (f.x + r, f.y + f.height - r, -1.0, 1.0),
                    (f.x + f.width - r, f.y + f.height - r, 1.0, 1.0),
                ] {
                    let square = Piece::square(cx, cy, r, sx, sy);
                    let touched = members
                        .iter()
                        .enumerate()
                        .any(|(o, other)| o != k && overlap(&square, &other.frame));
                    if touched {
                        rects.push(square);
                    } else {
                        out.push(Piece::Corner {
                            cx,
                            cy,
                            radius: r,
                            sx,
                            sy,
                        });
                    }
                }
            }
            rects.retain(|f| f.width > 1e-9 && f.height > 1e-9);
            out.extend(union(&rects).into_iter().map(Piece::Rect));
        }
        out
    }

    impl Piece {
        /// Whether a point of the layer is in this piece — a rounded corner's
        /// outside belongs to the page.
        pub fn contains(&self, x: f64, y: f64) -> bool {
            match self {
                Piece::Rect(f) => inside(f, x, y),
                Piece::Rounded(Hole {
                    frame: f,
                    radius: r,
                }) => {
                    if !inside(f, x, y) {
                        return false;
                    }
                    let dx = (f.x + r - x).max(x - (f.x + f.width - r)).max(0.0);
                    let dy = (f.y + r - y).max(y - (f.y + f.height - r)).max(0.0);
                    dx * dx + dy * dy <= r * r
                }
                Piece::Corner {
                    cx,
                    cy,
                    radius,
                    sx,
                    sy,
                } => {
                    inside(&Piece::square(*cx, *cy, *radius, *sx, *sy), x, y)
                        && (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
                }
            }
        }
    }

    /// Whether a point of the layer is see-through: what the mask leaves
    /// open is exactly what `hitTest:` hands to the page below.
    pub fn cleared(pieces: &[Piece], x: f64, y: f64) -> bool {
        pieces.iter().any(|p| p.contains(x, y))
    }

    #[cfg(test)]
    mod tests {
        use super::super::anchor::{Anchor, Frame};
        use super::super::Viewport;
        use super::*;

        fn anchor(main_width: f64, main_height: f64, viewport: Viewport) -> Anchor {
            Anchor::of(
                main_height,
                Frame {
                    x: 0.0,
                    y: 0.0,
                    width: main_width,
                    height: main_height,
                },
                viewport,
            )
        }

        const PAGE: Viewport = Viewport {
            width: 1000.0,
            height: 800.0,
        };

        fn clear(left: f64, top: f64, width: f64, height: f64, radius: f64) -> Clear {
            Clear {
                left,
                top,
                width,
                height,
                radius,
            }
        }

        #[test]
        fn an_overlay_is_mapped_through_the_anchor_a_tab_is_placed_by() {
            let a = anchor(1000.0, 800.0, PAGE);
            let h = holes(&a, &[clear(100.0, 50.0, 200.0, 100.0, 0.0)]);
            assert_eq!(
                h[0].frame,
                Frame {
                    x: 100.0,
                    y: 800.0 - 50.0 - 100.0,
                    width: 200.0,
                    height: 100.0
                }
            );
            // A zoom of 1.5 and a content inset of 28 points move it as they move a tab.
            let zoomed = Anchor::of(
                800.0,
                Frame {
                    x: 0.0,
                    y: 0.0,
                    width: 1500.0,
                    height: 800.0,
                },
                Viewport {
                    width: 1000.0,
                    height: (800.0 - 28.0) / 1.5,
                },
            );
            let z = holes(&zoomed, &[clear(100.0, 50.0, 200.0, 100.0, 12.0)]);
            assert_eq!(
                z[0].frame,
                zoomed.frame_for(&Bounds {
                    left: 100.0,
                    top: 50.0,
                    width: 200.0,
                    height: 100.0
                })
            );
            assert!(
                (z[0].radius - 18.0).abs() < 1e-9,
                "the radius scales with the zoom"
            );
        }

        #[test]
        fn a_radius_is_never_more_than_half_the_short_side_nor_less_than_nothing() {
            let a = anchor(1000.0, 800.0, PAGE);
            assert_eq!(
                holes(&a, &[clear(0.0, 0.0, 48.0, 48.0, 9999.0)])[0].radius,
                24.0,
                "a round dock: fully round"
            );
            assert_eq!(
                holes(&a, &[clear(0.0, 0.0, 48.0, 48.0, -3.0)])[0].radius,
                0.0
            );
            assert_eq!(
                holes(&a, &[clear(0.0, 0.0, 48.0, 48.0, f64::NAN)])[0].radius,
                0.0
            );
        }

        #[test]
        fn an_empty_or_non_finite_box_clears_nothing_and_the_count_is_capped() {
            let a = anchor(1000.0, 800.0, PAGE);
            let junk = [
                clear(0.0, 0.0, 0.0, 10.0, 0.0),
                clear(f64::NAN, 0.0, 10.0, 10.0, 0.0),
                clear(0.0, 0.0, 10.0, f64::INFINITY, 0.0),
            ];
            assert!(holes(&a, &junk).is_empty());
            let many: Vec<Clear> = (0..40)
                .map(|i| clear(f64::from(i) * 20.0, 0.0, 10.0, 10.0, 0.0))
                .collect();
            assert_eq!(holes(&a, &many).len(), MAX_CLEARS);
        }

        #[test]
        fn a_lone_overlay_keeps_its_rounded_corners_and_its_corner_outside_is_the_page() {
            let a = anchor(1000.0, 800.0, PAGE);
            let p = pieces(&holes(&a, &[clear(100.0, 100.0, 200.0, 100.0, 16.0)]));
            assert_eq!(p.len(), 1);
            assert!(matches!(p[0], Piece::Rounded(_)));
            let (left, bottom) = (100.0, 800.0 - 100.0 - 100.0);
            assert!(
                cleared(&p, left + 100.0, bottom + 50.0),
                "its middle is see-through"
            );
            assert!(
                !cleared(&p, left + 1.0, bottom + 1.0),
                "the very corner, outside the curve, is the page's"
            );
            assert!(
                cleared(&p, left + 16.0, bottom + 1.0),
                "the edge past the curve is the overlay's"
            );
            assert!(
                !cleared(&p, left - 1.0, bottom + 50.0),
                "a point past its edge is the page's"
            );
        }

        #[test]
        fn overlapping_overlays_are_cut_as_their_union_in_disjoint_pieces() {
            let a = anchor(1000.0, 800.0, PAGE);
            // A panel, a round dock over its bottom-right, and a dock that touches nothing.
            let h = holes(
                &a,
                &[
                    clear(400.0, 200.0, 560.0, 500.0, 16.0),
                    clear(900.0, 640.0, 48.0, 48.0, 24.0),
                    clear(50.0, 50.0, 48.0, 48.0, 24.0),
                ],
            );
            let p = pieces(&h);
            assert_eq!(
                p.iter().filter(|x| matches!(x, Piece::Rounded(_))).count(),
                1,
                "the lone dock stays round"
            );
            let cover = |x: &Piece| match x {
                Piece::Rect(f) => *f,
                Piece::Corner {
                    cx,
                    cy,
                    radius,
                    sx,
                    sy,
                } => Piece::square(*cx, *cy, *radius, *sx, *sy),
                Piece::Rounded(hole) => hole.frame,
            };
            for (i, one) in p.iter().enumerate() {
                for other in &p[i + 1..] {
                    assert!(!overlap(&cover(one), &cover(other)), "no two pieces overlap: an even-odd mask would show the page where they did — {one:?} {other:?}");
                }
            }
            let (panel, dock) = (h[0], h[1]);
            let round = |hole: &Hole, x: f64, y: f64| Piece::Rounded(*hole).contains(x, y);
            // Everything of either shape is cut; the panel's three corners the dock does not touch stay round.
            let mut x = 380.0;
            while x < 1000.0 {
                let mut y = 0.0;
                while y < 800.0 {
                    if round(&panel, x, y) || round(&dock, x, y) {
                        assert!(cleared(&p, x, y), "({x}, {y}) is in an overlay");
                    }
                    y += 3.0;
                }
                x += 3.0;
            }
            let (left, bottom, top) = (
                panel.frame.x,
                panel.frame.y,
                panel.frame.y + panel.frame.height,
            );
            assert!(
                !cleared(&p, left + 0.5, top - 0.5),
                "the panel's free top-left corner is round: its very corner is the page's"
            );
            assert!(
                !cleared(&p, left + 0.5, bottom + 0.5),
                "and so is its free bottom-left one"
            );
            let (dock_left, dock_top) = (dock.frame.x, dock.frame.y + dock.frame.height);
            assert!(
                cleared(&p, dock_left + 24.0, dock_top - 24.0),
                "the dock's middle is cut"
            );
        }
    }
}

/// Where a tab's webview is and whether it shows — said by the main webview
/// as the centre moves, the active tab changes, or a surface opens over it.
/// One placement, then the box actually drawn read back, so the main window
/// can tell when the page is not where it asked.
#[tauri::command]
pub fn browser_bounds<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, BrowserRegistry>,
    key: String,
    bounds: Bounds,
    visible: bool,
    viewport: Viewport,
) -> Result<Placed, String> {
    let view = view_of(&app, &key)?;
    if !visible {
        view.hide().map_err(err)?;
        return Ok(Placed::hidden());
    }
    let main = app
        .get_webview("main")
        .ok_or_else(|| "no main webview to anchor the tab on".to_string())?;
    let (placed, anchored) = place::put(&main, &view, bounds, viewport)?;
    if let Some((anchor, main_frame)) = anchored {
        if registry.note_anchor(&key, anchor) {
            tracing::debug!(
                target: "bisa_desktop",
                key,
                parent_height = anchor.parent_height,
                main_left = main_frame.x,
                main_bottom = main_frame.y,
                main_width = main_frame.width,
                main_height = main_frame.height,
                viewport_width = viewport.width,
                viewport_height = viewport.height,
                zoom = anchor.zoom,
                inset = anchor.inset,
                top = anchor.top,
                "browser anchor"
            );
        }
    }
    Ok(placed)
}

/// The boxes of the main page's floating overlays that stand over a showing
/// tab — the Notes and Draw panels, their docks, the pet, addon windows —
/// in the page's CSS pixels with the page's viewport. The browser layer cuts
/// them out (`layer`), so they show and take their own clicks over a live
/// page. `true` where the platform cuts around them (macOS), `false`
/// elsewhere, where an overlay still stands under a tab.
#[tauri::command]
pub fn browser_clear<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, BrowserRegistry>,
    clears: Vec<clear::Clear>,
    viewport: Viewport,
) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let main = app
            .get_webview("main")
            .ok_or_else(|| "no main webview to cut the overlays around".to_string())?;
        let asked = clears.len();
        let holes = place::clear(&main, clears, viewport)?;
        if registry.note_holes(holes) {
            tracing::debug!(target: "bisa_desktop", overlays = asked, pieces = holes, "browser layer cut");
        }
        Ok(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, registry, clears, viewport);
        Ok(false)
    }
}

/// Hand the keyboard to the main page — what a floating overlay asks as it
/// opens over a showing tab (`BrowserPanel.tsx`): a panel opened by a chord
/// or a menu while a page held the keyboard would otherwise type into the
/// page. A click in an overlay's hole does this by itself (`layer`).
#[tauri::command]
pub fn browser_focus_main<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    app.get_webview("main")
        .ok_or_else(|| "no main webview to hand the keyboard to".to_string())?
        .set_focus()
        .map_err(err)
}

/// Close a tab.
#[tauri::command]
pub fn browser_close<R: Runtime>(
    app: AppHandle<R>,
    registry: State<'_, BrowserRegistry>,
    key: String,
) -> Result<(), String> {
    // Forgotten only once the view is gone: a view whose close failed stays
    // listed, and the page hears why, so nothing is left running unnamed.
    if let Some(view) = app.get_webview(&label_of(&key)) {
        view.close().map_err(err)?;
    }
    registry.remove(&key);
    Ok(())
}

/// A screenshot's width is kept to what a page and a PNG can carry.
pub const MIN_SHOT_WIDTH: u32 = 320;
pub const MAX_SHOT_WIDTH: u32 = 4096;
/// How long the page gets to render a snapshot.
pub const SNAPSHOT_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// What a snapshot of a hidden webview answers, at once: a hidden view
/// renders nothing, and a snapshot asked of one after screen updates would
/// pend until it shows — holding the page's own paints meanwhile.
pub const SNAPSHOT_HIDDEN: &str = "the tab is hidden: a hidden webview renders no snapshot";

/// What a snapshot that did not come within the budget answers.
pub const SNAPSHOT_LATE: &str = "the page did not render a snapshot in time";

/// The width a screenshot is taken at: what was asked, within bounds.
pub fn clamp_width(width: u32) -> u32 {
    width.clamp(MIN_SHOT_WIDTH, MAX_SHOT_WIDTH)
}

/// A PNG of a tab as it shows, `width` pixels wide — the webview's own
/// snapshot, taken on the main thread and answered as raw bytes. The tab
/// must be showing: a hidden webview renders nothing worth keeping, so the
/// main webview waits for it to show before asking (`browserShown.ts`), and
/// a hidden one is refused at once (`SNAPSHOT_HIDDEN`) rather than left to
/// pend.
#[tauri::command]
pub async fn browser_screenshot<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    width: u32,
) -> Result<tauri::ipc::Response, String> {
    let view = view_of(&app, &key)?;
    let width = clamp_width(width);
    let bytes = tauri::async_runtime::spawn_blocking(move || snapshot::take(&view, width))
        .await
        .map_err(err)??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Hand the page's script a request — a pick to mark, a read, a click — as
/// JSON; the page answers through its door.
#[tauri::command]
pub fn browser_drive<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    request: serde_json::Value,
) -> Result<(), String> {
    let json = serde_json::to_string(&request).map_err(err)?;
    // The request is a JSON text inside a JS string: quoted as a JSON
    // string, which is a valid JS string literal.
    let literal = serde_json::to_string(&json).map_err(err)?;
    view_of(&app, &key)?
        .eval(format!(
            "window.__bisaBrowser && window.__bisaBrowser.perform(JSON.parse({literal}))"
        ))
        .map_err(err)
}

/// The `WKWebView` tauri built for a tab, for the readers below.
#[cfg(target_os = "macos")]
mod webkit {
    use objc2_web_kit::WKWebView;
    use tauri::webview::PlatformWebview;

    /// # Safety
    /// `inner()` is the `WKWebView` tauri built for this webview, alive
    /// while the webview is; every caller runs on the main thread, where
    /// WebKit wants to be asked.
    pub unsafe fn view(platform: &PlatformWebview) -> &WKWebView {
        &*platform.inner().cast::<WKWebView>()
    }
}

/// The page's one door out: a script message handler named `bisa` on the
/// tab's user content controller, so `window.webkit.messageHandlers.bisa
/// .postMessage(text)` reaches `say` from every page the tab shows — an
/// origin of this machine or the web alike. The controller keeps the
/// handler for the webview's life; the handler keeps nothing of the
/// webview, so nothing cycles.
#[cfg(target_os = "macos")]
mod door {
    use super::DOOR;
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, ProtocolObject};
    use objc2::{define_class, msg_send, DeclaredClass, MainThreadOnly};
    use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSString};
    use objc2_web_kit::{WKScriptMessage, WKScriptMessageHandler, WKUserContentController};
    use tauri::{Runtime, Webview};

    pub struct Ivars {
        say: Box<dyn Fn(String) + Send>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "BisaPageDoor"]
        #[ivars = Ivars]
        pub struct PageDoor;

        unsafe impl NSObjectProtocol for PageDoor {}

        unsafe impl WKScriptMessageHandler for PageDoor {
            #[unsafe(method(userContentController:didReceiveScriptMessage:))]
            fn did_receive(
                &self,
                _controller: &WKUserContentController,
                message: &WKScriptMessage,
            ) {
                // SAFETY: WebKit hands a live message for the call's duration.
                let body = unsafe { message.body() };
                if let Ok(text) = body.downcast::<NSString>() {
                    (self.ivars().say)(text.to_string());
                }
            }
        }
    );

    impl PageDoor {
        fn new(mtm: MainThreadMarker, say: Box<dyn Fn(String) + Send>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars { say });
            // SAFETY: `init` on an allocated, ivar-set NSObject subclass.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Put the door on a tab's webview; `say` hears every posted text.
    pub fn install<R: Runtime>(webview: &Webview<R>, say: Box<dyn Fn(String) + Send>) {
        // A window that is gone hears nothing: the tab is closing with it.
        let _window_gone = webview.with_webview(move |platform| {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            // SAFETY: on the main thread, over the live `WKWebView` (`webkit::view`).
            unsafe {
                let web = super::webkit::view(&platform);
                let controller = web.configuration().userContentController();
                let door = PageDoor::new(mtm, say);
                controller.addScriptMessageHandler_name(
                    ProtocolObject::from_ref(&*door),
                    &NSString::from_str(DOOR),
                );
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod door {
    use tauri::{Runtime, Webview};

    /// No script message handler is bound on this platform yet: a page's
    /// words do not arrive, as its snapshot does not.
    pub fn install<R: Runtime>(_webview: &Webview<R>, _say: Box<dyn Fn(String) + Send>) {}
}

/// The view every tab stands in (ide/18): a `BisaBrowserLayer` the size of
/// the window's content view, just above the main webview, unflipped like
/// the content view so a tab's frame there is the frame `anchor` computes.
/// A native view paints over every DOM element, so the main page's floating
/// overlays (`clear`) would be covered by a tab; this view is where they are
/// not: its layer's mask leaves a hole wherever one sits — "fully transparent
/// pixels block that content" (Apple, `CALayer.mask`), so the main page
/// shows there — and its `hitTest:` answers nothing for a point in a hole,
/// so the window's own hit-test goes on to the main webview below and the
/// overlay gets the click, the scroll and the keys ("you might want to
/// override it to have a view object hide mouse-down events from its
/// subviews", Apple, `NSView.hitTest(_:)`). The page itself is untouched:
/// full size, live, every pixel outside a hole its own. With no hole the
/// mask is gone, so the common case costs nothing.
#[cfg(target_os = "macos")]
mod layer {
    use super::anchor::Anchor;
    use super::clear::{self, Clear, Piece};
    use objc2::rc::Retained;
    use objc2::runtime::NSObject;
    use objc2::{define_class, msg_send, DeclaredClass, MainThreadOnly};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSResponder, NSView, NSViewLayerContentsRedrawPolicy,
        NSWindowOrderingMode,
    };
    use objc2_core_graphics::CGMutablePath;
    use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize};
    use objc2_quartz_core::{kCAFillRuleEvenOdd, CALayer, CAShapeLayer, CATransaction};
    use std::cell::RefCell;

    #[derive(Default)]
    struct State {
        clears: Vec<Clear>,
        anchor: Option<Anchor>,
        pieces: Vec<Piece>,
        mask: Option<Retained<CAShapeLayer>>,
    }

    #[derive(Default)]
    pub struct Ivars {
        state: RefCell<State>,
    }

    define_class!(
        #[unsafe(super(NSView, NSResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "BisaBrowserLayer"]
        #[ivars = Ivars]
        pub struct BrowserLayer;

        unsafe impl NSObjectProtocol for BrowserLayer {}

        impl BrowserLayer {
            /// A tab under the point, unless the point is in a hole; never
            /// this view itself, which is empty glass the size of the window.
            #[unsafe(method_id(hitTest:))]
            fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
                // SAFETY: NSView's own hit-test, on the main thread where AppKit calls this.
                let hit: Option<Retained<NSView>> = unsafe { msg_send![super(self), hitTest: point] };
                hit.filter(|view| self.passes_to_page(view, point))
            }

            /// The layer is ours to configure (`updateLayer`), never drawn into.
            #[unsafe(method(wantsUpdateLayer))]
            fn wants_update_layer(&self) -> bool {
                true
            }

            /// Sublayers are added in `layout`, as AppKit asks of a layer-backed view.
            #[unsafe(method(layout))]
            fn layout(&self) {
                // SAFETY: NSView's own layout first.
                let _: () = unsafe { msg_send![super(self), layout] };
                self.attach_mask();
            }

            #[unsafe(method(updateLayer))]
            fn update_layer(&self) {
                self.draw_mask();
            }

            #[unsafe(method(setFrameSize:))]
            fn set_frame_size(&self, size: NSSize) {
                // SAFETY: NSView's own resize first; the holes follow the new height.
                let _: () = unsafe { msg_send![super(self), setFrameSize: size] };
                self.recut();
            }

            /// A move between displays: the mask is drawn at the new scale.
            #[unsafe(method(viewDidChangeBackingProperties))]
            fn backing_changed(&self) {
                // SAFETY: NSView's own handling first.
                let _: () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
                self.setNeedsDisplay(true);
            }
        }
    );

    impl BrowserLayer {
        /// Whether what NSView's hit-test found is the tab's to take: not
        /// this view's own empty glass, and not a point in a hole.
        fn passes_to_page(&self, hit: &NSView, point: NSPoint) -> bool {
            if std::ptr::eq(
                (hit as *const NSView).cast::<u8>(),
                (self as *const Self).cast::<u8>(),
            ) {
                return false;
            }
            // SAFETY: the superview, read on the main thread for the call's duration.
            let parent = unsafe { self.superview() };
            let at = self.convertPoint_fromView(point, parent.as_deref());
            !clear::cleared(&self.ivars().state.borrow().pieces, at.x, at.y)
        }

        fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(Ivars::default());
            // SAFETY: `initWithFrame:` on an allocated, ivar-set NSView subclass.
            unsafe { msg_send![super(this), initWithFrame: frame] }
        }

        /// Stand a tab in this view — once, as it opens, while it is hidden.
        pub fn adopt(&self, tab: &NSView) {
            // SAFETY: the tab's superview, read on the main thread.
            let home = unsafe { tab.superview() };
            if home
                .as_deref()
                .is_some_and(|v| std::ptr::eq(v as *const NSView, (self as *const Self).cast()))
            {
                return;
            }
            self.addSubview(tab);
        }

        /// The overlays now over the page, and how many holes they make.
        pub fn clear(&self, clears: Vec<Clear>, anchor: Anchor) -> usize {
            {
                let mut state = self.ivars().state.borrow_mut();
                state.clears = clears;
                state.anchor = Some(anchor);
            }
            self.recut();
            self.ivars().state.borrow().pieces.len()
        }

        /// A placement read the anchor again (a zoom, an inset): the holes follow.
        pub fn anchor_on(&self, anchor: Anchor) {
            if self.ivars().state.borrow().anchor == Some(anchor) {
                return;
            }
            self.ivars().state.borrow_mut().anchor = Some(anchor);
            self.recut();
        }

        fn recut(&self) {
            let height = self.bounds().size.height;
            {
                let mut state = self.ivars().state.borrow_mut();
                let pieces = match state.anchor {
                    Some(mut anchor) if !state.clears.is_empty() => {
                        // The window may have grown since the anchor was read: the page's top
                        // is still where it was, so only the parent's height moves.
                        anchor.parent_height = height;
                        clear::pieces(&clear::holes(&anchor, &state.clears))
                    }
                    _ => Vec::new(),
                };
                state.pieces = pieces;
            }
            self.setNeedsLayout(true);
            self.setNeedsDisplay(true);
        }

        fn attach_mask(&self) {
            let Some(layer) = self.layer() else { return };
            let mut state = self.ivars().state.borrow_mut();
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            if state.pieces.is_empty() {
                if state.mask.take().is_some() {
                    // SAFETY: clearing the mask of our own layer.
                    unsafe { layer.setMask(None) };
                }
            } else {
                let mask = state.mask.get_or_insert_with(CAShapeLayer::new);
                mask.setFrame(layer.bounds());
                let shape: &CALayer = mask;
                // SAFETY: our own shape layer, sized to our layer, as its mask.
                unsafe { layer.setMask(Some(shape)) };
            }
            CATransaction::commit();
        }

        fn draw_mask(&self) {
            let state = self.ivars().state.borrow();
            let Some(mask) = state.mask.as_ref() else {
                return;
            };
            let path = CGMutablePath::new();
            // SAFETY: a fresh path, no transform; every corner radius is at most half its side (`clear::holes`).
            unsafe {
                CGMutablePath::add_rect(Some(&path), std::ptr::null(), self.bounds());
                for piece in &state.pieces {
                    match piece {
                        Piece::Rect(f) => {
                            CGMutablePath::add_rect(Some(&path), std::ptr::null(), rect(f))
                        }
                        Piece::Rounded(h) if h.radius > 0.0 => CGMutablePath::add_rounded_rect(
                            Some(&path),
                            std::ptr::null(),
                            rect(&h.frame),
                            h.radius,
                            h.radius,
                        ),
                        Piece::Rounded(h) => {
                            CGMutablePath::add_rect(Some(&path), std::ptr::null(), rect(&h.frame))
                        }
                        // A quarter disc: from the centre along one edge, the short arc to the other, closed.
                        Piece::Corner {
                            cx,
                            cy,
                            radius,
                            sx,
                            sy,
                        } => {
                            let start = 0.0_f64.atan2(*sx);
                            let end = sy.atan2(0.0);
                            let mut sweep = end - start;
                            while sweep > std::f64::consts::PI {
                                sweep -= std::f64::consts::TAU;
                            }
                            while sweep <= -std::f64::consts::PI {
                                sweep += std::f64::consts::TAU;
                            }
                            CGMutablePath::move_to_point(Some(&path), std::ptr::null(), *cx, *cy);
                            CGMutablePath::add_line_to_point(
                                Some(&path),
                                std::ptr::null(),
                                cx + sx * radius,
                                *cy,
                            );
                            CGMutablePath::add_arc(
                                Some(&path),
                                std::ptr::null(),
                                *cx,
                                *cy,
                                *radius,
                                start,
                                end,
                                sweep < 0.0,
                            );
                            CGMutablePath::close_subpath(Some(&path));
                        }
                    }
                }
            }
            let scale = self.window().map_or(2.0, |w| w.backingScaleFactor());
            CATransaction::begin();
            CATransaction::setDisableActions(true);
            mask.setPath(Some(&path));
            // SAFETY: a constant Core Animation exports.
            mask.setFillRule(unsafe { kCAFillRuleEvenOdd });
            mask.setContentsScale(scale);
            CATransaction::commit();
        }
    }

    fn rect(f: &super::anchor::Frame) -> NSRect {
        NSRect::new(NSPoint::new(f.x, f.y), NSSize::new(f.width, f.height))
    }

    /// The layer over the main webview — found among its siblings, or made
    /// and stood just above it, the content view's size, following it.
    pub fn over(main: &NSView, mtm: MainThreadMarker) -> Option<Retained<BrowserLayer>> {
        // SAFETY: the main webview's superview, read on the main thread.
        let parent = unsafe { main.superview() }?;
        for view in parent.subviews().iter() {
            if let Ok(layer) = view.downcast::<BrowserLayer>() {
                return Some(layer);
            }
        }
        let layer = BrowserLayer::new(mtm, parent.bounds());
        layer.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        layer.setWantsLayer(true);
        layer.setLayerContentsRedrawPolicy(NSViewLayerContentsRedrawPolicy::DuringViewResize);
        parent.addSubview_positioned_relativeTo(&layer, NSWindowOrderingMode::Above, Some(main));
        Some(layer)
    }
}

/// Putting a tab's webview where its box is (ide/18): one closure on the
/// main thread that reads the content view's height and the main webview's
/// frame, anchors the box on them with the page's viewport, sets the
/// child's frame, shows it and reads the frame back through the same map.
#[cfg(target_os = "macos")]
mod place {
    use super::anchor::{Anchor, Frame};
    use super::{err, Bounds, Placed, Viewport, PLACEMENT_BUDGET};
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use std::sync::mpsc;
    use tauri::{Runtime, Webview};

    fn frame_of(view: &NSView) -> Frame {
        let f = view.frame();
        Frame {
            x: f.origin.x,
            y: f.origin.y,
            width: f.size.width,
            height: f.size.height,
        }
    }

    fn ns_rect(f: Frame) -> NSRect {
        NSRect::new(NSPoint::new(f.x, f.y), NSSize::new(f.width, f.height))
    }

    /// A view's frame in another view's points — the main webview's, read in
    /// the layer the tabs stand in (the same points while the layer covers
    /// the content view from its origin, as it does).
    fn frame_of_in(view: &NSView, home: Option<&NSView>, into: &NSView) -> Frame {
        let f = into.convertRect_fromView(view.frame(), home);
        Frame {
            x: f.origin.x,
            y: f.origin.y,
            width: f.size.width,
            height: f.size.height,
        }
    }

    /// Stand a newly opened tab in the browser layer, while it is still
    /// hidden — WebKit's window callbacks cost nothing on a page not shown.
    pub fn adopt<R: Runtime>(main: &Webview<R>, tab: &Webview<R>) -> Result<(), String> {
        let main_ptr = main_view(main)?;
        tab.with_webview(move |platform| {
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            // SAFETY: on the main thread, over the live `WKWebView` (`webkit::view`).
            let child: &NSView = unsafe { super::webkit::view(&platform) };
            // SAFETY: the main webview's `WKWebView`, alive while its window is; on the main thread.
            let main: &NSView = unsafe { &*(main_ptr as *const NSView) };
            if let Some(layer) = super::layer::over(main, mtm) {
                layer.adopt(child);
            }
        })
        .map_err(err)
    }

    /// The overlays' boxes on the browser layer: anchored as a tab is, with
    /// the page's viewport, and cut out of it. How many holes they make.
    pub fn clear<R: Runtime>(
        main: &Webview<R>,
        clears: Vec<super::clear::Clear>,
        viewport: Viewport,
    ) -> Result<usize, String> {
        let (tx, rx) = mpsc::channel::<Result<usize, String>>();
        main.with_webview(move |platform| {
            let cut = (|| {
                let mtm = objc2::MainThreadMarker::new()
                    .ok_or_else(|| "the clear was not asked on the main thread".to_string())?;
                // SAFETY: on the main thread, over the main `WKWebView` (`webkit::view`).
                let main: &NSView = unsafe { super::webkit::view(&platform) };
                let layer = super::layer::over(main, mtm)
                    .ok_or_else(|| "the main webview stands in no view".to_string())?;
                // SAFETY: the main webview's superview, read on the main thread.
                let main_frame = frame_of_in(main, unsafe { main.superview() }.as_deref(), &layer);
                let anchor = Anchor::of(layer.frame().size.height, main_frame, viewport);
                Ok(layer.clear(clears, anchor))
            })();
            if tx.send(cut).is_err() {
                tracing::debug!("the asker stopped waiting");
            }
        })
        .map_err(err)?;
        rx.recv_timeout(PLACEMENT_BUDGET)
            .map_err(|_| "the overlays were not cut out in time".to_string())?
    }

    /// The main webview's `WKWebView`, as a pointer the placement closure
    /// carries to the main thread — alive while the window is.
    fn main_view<R: Runtime>(main: &Webview<R>) -> Result<usize, String> {
        let (tx, rx) = mpsc::channel::<usize>();
        main.with_webview(move |platform| {
            if tx.send(platform.inner() as usize).is_err() {
                tracing::debug!("the asker stopped waiting");
            }
        })
        .map_err(err)?;
        rx.recv_timeout(PLACEMENT_BUDGET)
            .map_err(|_| "the main webview did not answer in time".to_string())
    }

    pub fn put<R: Runtime>(
        main: &Webview<R>,
        tab: &Webview<R>,
        bounds: Bounds,
        viewport: Viewport,
    ) -> Result<(Placed, Option<(Anchor, Frame)>), String> {
        let main_ptr = main_view(main)?;
        let (tx, rx) = mpsc::channel::<Result<(Placed, Anchor, Frame), String>>();
        tab.with_webview(move |platform| {
            let placed = (|| {
                if objc2::MainThreadMarker::new().is_none() {
                    return Err("the placement was not asked on the main thread".to_string());
                }
                let mtm = objc2::MainThreadMarker::new()
                    .ok_or_else(|| "the placement was not asked on the main thread".to_string())?;
                // SAFETY: on the main thread, over the live `WKWebView` (`webkit::view`).
                let child: &NSView = unsafe { super::webkit::view(&platform) };
                // SAFETY: the main webview's `WKWebView`, alive while its window is; on the main thread.
                let main: &NSView = unsafe { &*(main_ptr as *const NSView) };
                // The tab stands in the browser layer (`layer`), which has the content
                // view's size and orientation: its frame there is the anchor's.
                let layer = super::layer::over(main, mtm);
                if let Some(layer) = &layer {
                    layer.adopt(child);
                }
                // SAFETY: the child stands in the layer (or, failing one, the content view) and stays there.
                let parent = unsafe { child.superview() }
                    .ok_or_else(|| "the tab has no parent view".to_string())?;
                let parent_height = parent.frame().size.height;
                // SAFETY: the main webview's superview, read on the main thread.
                let main_frame = frame_of_in(main, unsafe { main.superview() }.as_deref(), &parent);
                let anchor = Anchor::of(parent_height, main_frame, viewport);
                child.setFrame(ns_rect(anchor.frame_for(&bounds)));
                child.setHidden(false);
                if let Some(layer) = &layer {
                    layer.anchor_on(anchor);
                }
                Ok((anchor.placed_from(&frame_of(child)), anchor, main_frame))
            })();
            if tx.send(placed).is_err() {
                tracing::debug!("the asker stopped waiting");
            }
        })
        .map_err(err)?;
        let (placed, anchor, main_frame) = rx
            .recv_timeout(PLACEMENT_BUDGET)
            .map_err(|_| "the tab was not placed in time".to_string())??;
        Ok((placed, Some((anchor, main_frame))))
    }
}

/// Off macOS the runtime's own placement: one atomic `set_bounds` in the
/// window's logical pixels, shown, the box read back — no anchor to speak of.
#[cfg(not(target_os = "macos"))]
mod place {
    use super::anchor::{Anchor, Frame};
    use super::{err, Bounds, Placed, Viewport};
    use tauri::{Runtime, Webview};

    pub fn put<R: Runtime>(
        _main: &Webview<R>,
        tab: &Webview<R>,
        bounds: Bounds,
        _viewport: Viewport,
    ) -> Result<(Placed, Option<(Anchor, Frame)>), String> {
        tab.set_bounds(bounds.rect()).map_err(err)?;
        tab.show().map_err(err)?;
        let drawn = tab.bounds().map_err(err)?;
        let scale = tab.window().scale_factor().map_err(err)?;
        Ok((Placed::of(&drawn, scale), None))
    }
}

/// The page's own verbs and history, asked of the webview itself.
#[cfg(target_os = "macos")]
mod page {
    use super::err;
    use tauri::webview::PlatformWebview;
    use tauri::{Runtime, Webview};

    /// Whether the tab can go back, and forward. Off the main thread — never
    /// the case for a load hook or a `with_webview` closure — nothing is known.
    pub fn history(platform: &PlatformWebview) -> (bool, bool) {
        if objc2::MainThreadMarker::new().is_none() {
            return (false, false);
        }
        // SAFETY: on the main thread, over the live `WKWebView`.
        unsafe {
            let web = super::webkit::view(platform);
            (web.canGoBack(), web.canGoForward())
        }
    }

    fn on<R: Runtime>(
        view: &Webview<R>,
        act: impl FnOnce(&objc2_web_kit::WKWebView) + Send + 'static,
    ) -> Result<(), String> {
        view.with_webview(move |platform| {
            // SAFETY: `with_webview` runs on the main thread over the live `WKWebView`.
            unsafe { act(super::webkit::view(&platform)) }
        })
        .map_err(err)
    }

    pub fn back<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        on(view, |web| unsafe {
            web.goBack();
        })
    }

    pub fn forward<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        on(view, |web| unsafe {
            web.goForward();
        })
    }

    pub fn reload<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        on(view, |web| unsafe {
            web.reload();
        })
    }

    pub fn stop<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        on(view, |web| unsafe { web.stopLoading() })
    }
}

#[cfg(not(target_os = "macos"))]
mod page {
    use super::err;
    use tauri::webview::PlatformWebview;
    use tauri::{Runtime, Webview};

    /// No history is read on this platform yet.
    pub fn history(_platform: &PlatformWebview) -> (bool, bool) {
        (false, false)
    }

    pub fn back<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        view.eval("history.back()").map_err(err)
    }

    pub fn forward<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        view.eval("history.forward()").map_err(err)
    }

    pub fn reload<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        view.eval("location.reload()").map_err(err)
    }

    pub fn stop<R: Runtime>(view: &Webview<R>) -> Result<(), String> {
        view.eval("window.stop()").map_err(err)
    }
}

#[cfg(target_os = "macos")]
mod snapshot {
    use super::{err, SNAPSHOT_BUDGET, SNAPSHOT_HIDDEN, SNAPSHOT_LATE};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSImage;
    use objc2_foundation::{NSError, NSNumber};
    use objc2_web_kit::WKSnapshotConfiguration;
    use std::sync::mpsc;
    use tauri::{Runtime, Webview};

    /// `WKWebView.takeSnapshot` on the main thread, the PNG back over a
    /// channel; the page's own render, scaled to `width`. A hidden view —
    /// its own `hidden`, or an ancestor's — is refused before anything is
    /// asked of WebKit: a snapshot after screen updates never comes from
    /// one, and would hold the page's paints while it pends.
    pub fn take<R: Runtime>(view: &Webview<R>, width: u32) -> Result<Vec<u8>, String> {
        let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();
        view.with_webview(move |platform| {
            let Some(mtm) = MainThreadMarker::new() else {
                if tx
                    .send(Err("the snapshot was not asked on the main thread".into()))
                    .is_err()
                {
                    tracing::debug!("the asker stopped waiting");
                }
                return;
            };
            // SAFETY: on the main thread, over the live `WKWebView` (`webkit::view`).
            let web = unsafe { super::webkit::view(&platform) };
            if web.isHiddenOrHasHiddenAncestor() {
                if tx.send(Err(SNAPSHOT_HIDDEN.into())).is_err() {
                    tracing::debug!("the asker stopped waiting");
                }
                return;
            }
            let config = unsafe { WKSnapshotConfiguration::new(mtm) };
            unsafe {
                config.setSnapshotWidth(Some(&NSNumber::new_u32(width)));
                config.setAfterScreenUpdates(true);
            }
            let handler = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
                if tx.send(png_of(image, error)).is_err() {
                    tracing::debug!("the asker stopped waiting");
                }
            });
            unsafe { web.takeSnapshotWithConfiguration_completionHandler(Some(&config), &handler) };
        })
        .map_err(err)?;
        rx.recv_timeout(SNAPSHOT_BUDGET)
            .map_err(|_| SNAPSHOT_LATE.to_string())?
    }

    /// WebKit's `NSImage` as PNG bytes, or why there is none.
    fn png_of(image: *mut NSImage, error: *mut NSError) -> Result<Vec<u8>, String> {
        // SAFETY: WebKit hands a live object or null for the block's duration;
        // retaining it keeps it for ours.
        let image = unsafe { Retained::retain(image) };
        let Some(image) = image else {
            // SAFETY: the same, for the error.
            let why = unsafe { Retained::retain(error) }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "no image".to_string());
            return Err(format!("the snapshot failed: {why}"));
        };
        let tiff = image
            .TIFFRepresentation()
            .ok_or_else(|| "the snapshot has no bitmap".to_string())?;
        crate::png::from_data(&tiff)
            .ok_or_else(|| "the snapshot could not be encoded as PNG".to_string())
    }
}

#[cfg(not(target_os = "macos"))]
mod snapshot {
    use tauri::{Runtime, Webview};

    /// No snapshot API is bound on this platform yet.
    pub fn take<R: Runtime>(_view: &Webview<R>, _width: u32) -> Result<Vec<u8>, String> {
        Err("screenshots of a tab are taken on macOS only in this build".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screenshot_width_stays_within_bounds() {
        assert_eq!(clamp_width(0), MIN_SHOT_WIDTH);
        assert_eq!(clamp_width(1280), 1280);
        assert_eq!(clamp_width(1 << 20), MAX_SHOT_WIDTH);
    }

    #[test]
    fn a_hidden_tab_is_refused_a_snapshot_at_once_in_its_own_words() {
        assert!(SNAPSHOT_HIDDEN.contains("hidden"), "the refusal says why");
        assert!(
            SNAPSHOT_LATE.contains("in time"),
            "a late one says what ran out"
        );
        assert_ne!(SNAPSHOT_HIDDEN, SNAPSHOT_LATE, "two causes, two sentences");
        assert!(!SNAPSHOT_HIDDEN.is_empty() && !SNAPSHOT_LATE.is_empty());
    }

    #[test]
    fn http_https_and_the_blank_page_are_admitted_and_nothing_else() {
        assert!(admit("http://localhost:5173/").is_ok());
        assert!(admit(" https://example.com ").is_ok());
        assert_eq!(
            admit(BLANK).unwrap().as_str(),
            BLANK,
            "a blank tab is really blank"
        );
        assert!(admit("about:srcdoc").unwrap_err().contains("not about"));
        assert!(admit("file:///etc/passwd")
            .unwrap_err()
            .contains("not file"));
        assert!(admit("javascript:alert(1)").is_err());
        assert!(admit("not a url").unwrap_err().contains("is not a URL"));
        assert!(allowed(&Url::parse(BLANK).unwrap()));
        assert!(!allowed(&Url::parse("data:text/html,hi").unwrap()));
    }

    #[test]
    fn a_new_window_is_a_tab_here_for_a_page_and_nothing_for_the_rest() {
        let page = Url::parse("https://example.com/docs").unwrap();
        assert_eq!(
            opened("b2", &page),
            Some(Opened {
                key: "b2".into(),
                url: "https://example.com/docs".into()
            })
        );
        assert!(opened("b2", &Url::parse("http://localhost:5173/").unwrap()).is_some());
        assert_eq!(
            opened("b2", &Url::parse(BLANK).unwrap()),
            None,
            "a scripted window.open() has no page"
        );
        assert_eq!(
            opened("b2", &Url::parse("javascript:void(0)").unwrap()),
            None
        );
    }

    #[test]
    fn a_page_says_one_json_object_under_the_cap_and_anything_else_is_nothing() {
        assert_eq!(
            message_of(r##"{"type":"bisa:pick","selector":"#a"}"##),
            Some(serde_json::json!({"type": "bisa:pick", "selector": "#a"}))
        );
        assert_eq!(message_of("[1,2]"), None, "a list is not a message");
        assert_eq!(message_of("42"), None);
        assert_eq!(message_of("not json"), None);
        let long = format!("{{\"text\":\"{}\"}}", "x".repeat(MAX_MESSAGE_BYTES));
        assert_eq!(message_of(&long), None, "over the cap: dropped whole");
        assert_eq!(DOOR, "bisa");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn a_box_is_one_logical_rectangle_never_thinner_than_a_pixel_and_reads_back_as_it_was() {
        let asked = Bounds {
            left: 12.5,
            top: 96.0,
            width: 0.0,
            height: 540.0,
        };
        let rect = asked.rect();
        let placed = Placed::of(&rect, 2.0);
        assert_eq!(
            placed,
            Placed {
                left: 12.5,
                top: 96.0,
                width: 1.0,
                height: 540.0,
                shown: true
            },
            "logical in, logical out, whatever the scale"
        );
        assert!(!Placed::hidden().shown);
    }

    #[test]
    fn the_main_window_hears_each_event_under_its_own_name() {
        let names = [NAVIGATED, MESSAGE, TITLED, NEW_WINDOW];
        let mut distinct = names.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), names.len());
        assert!(names.iter().all(|n| n.starts_with("browser:")));
    }

    #[test]
    fn a_tab_is_a_labelled_webview_and_the_registry_forgets_it_once() {
        assert_eq!(label_of("b3"), "browser-b3");
        let r = BrowserRegistry::new();
        r.insert("b1");
        assert!(r.remove("b1"));
        assert!(!r.remove("b1"), "gone once");
        assert!(r.keys().is_empty());
    }

    #[test]
    fn a_tabs_anchor_is_noted_once_per_change() {
        let r = BrowserRegistry::new();
        r.insert("b1");
        let a = anchor::Anchor {
            parent_height: 800.0,
            left: 0.0,
            top: 28.0,
            zoom: 1.0,
            inset: 28.0,
        };
        assert!(r.note_anchor("b1", a), "the first anchor is news");
        assert!(!r.note_anchor("b1", a), "the same anchor again is not");
        assert!(
            r.note_anchor(
                "b1",
                anchor::Anchor {
                    top: 0.0,
                    inset: 0.0,
                    ..a
                }
            ),
            "a changed one is"
        );
        assert!(
            !r.note_anchor("b9", a),
            "a tab that is not there is nothing to note"
        );
    }
}
