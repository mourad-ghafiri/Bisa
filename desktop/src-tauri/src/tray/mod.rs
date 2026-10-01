//! The menu bar icon the app lives in (guide/the-desktop.md §The menu bar
//! icon): there from launch to quit, whatever the window does.
//!
//! Three things happen here and nowhere else. **The icon is alive**: the
//! webview reports what the platform is doing (`TrayReport`, through the
//! `tray_report` command) and the icon paints it — a dot on the mark in the
//! state's colour, the count of things that need the person as the text
//! beside it, the words in the tooltip and in the menu's first two lines.
//! **The window hides instead of quitting**: the shell holds every close
//! request (`main.rs`, `bisa:close-requested`) and the webview decides —
//! hide while *Closing the window keeps Bisa running* is on, the one close
//! flow else (`hide_window`: on macOS the whole app is hidden, as ⌘H does,
//! so ⌘Tab and the Dock bring it back too) — and the window comes back from
//! a left click here, from the Dock (`RunEvent::Reopen`) or from *Open
//! Bisa*. **Quitting is deliberate**:
//! *Quit Bisa* shows the window and hands the request to the same close flow
//! ⌘Q uses, so the question is asked and the documents are saved.
//!
//! A left click opens the window; a right click opens the menu. Linux emits
//! no clicks on a tray icon, so there the menu is the only door — one reason
//! *Open Bisa* is a line in it. What differs by OS beyond that — the Dock,
//! hiding and un-hiding the app — is `platform.rs`'s.
//!
//! **The window is looked up as a window** (`main_window`), never as a
//! webview window: Tauri's `get_webview_window` answers only for a window
//! whose one webview is its own, and the IDE's browser tabs are child
//! webviews of `main` (`browser.rs`), so with a tab open that lookup finds
//! nothing and every door here would stay shut.

mod glyph;
mod platform;
mod report;

pub use report::{TrayReport, TrayState};

use crate::sync::Locked;
use crate::words::ShellWords;
use glyph::Ink;
use serde::Serialize;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Theme, Window};

/// The one tray icon's id.
const TRAY_ID: &str = "bisa";

/// The main window's label — the one every door here opens.
pub const MAIN_WINDOW: &str = "main";

/// The main window, by its label. A `tauri::Window`, not a `WebviewWindow`:
/// the latter is a window whose only webview is its own, and `main` carries
/// the IDE's browser tabs as child webviews (`browser::label_of`), so
/// `get_webview_window` on `MAIN_WINDOW` is `None` as soon as one is open. A
/// `Window` has every move the tray makes — show, un-minimise, focus, emit,
/// its theme.
pub fn main_window(app: &AppHandle) -> Option<Window> {
    app.get_window(MAIN_WINDOW)
}

/// The webview hears where the menu sends it: `{to: "inbox"}`.
pub const GO_EVENT: &str = "tray:go";

/// The webview hears the menu's *Show in Dock* toggle — `{visible}` — and
/// records it as the `desktop.dock_icon` setting, which is the Dock's one
/// truth: the setting re-applies itself through `tray_dock` on every change.
pub const DOCK_EVENT: &str = "tray:dock";

/// The menu's lines, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayItem {
    /// What the platform is doing — a sentence, never a door.
    Status,
    /// How many things need the person — a door to the Inbox while owed.
    Needs,
    /// Show and focus the window.
    Open,
    /// The app's icon in the Dock — a check item, built where there is a Dock.
    Dock,
    /// Quit through the one close flow.
    Quit,
}

impl TrayItem {
    /// Every line, in the menu's order.
    pub const ALL: [TrayItem; 5] = [
        TrayItem::Status,
        TrayItem::Needs,
        TrayItem::Open,
        TrayItem::Dock,
        TrayItem::Quit,
    ];

    /// The menu item's id — `tray:<word>`, the namespace `scenarios/tray.test.mjs` reads.
    pub fn id(self) -> &'static str {
        match self {
            TrayItem::Status => "tray:status",
            TrayItem::Needs => "tray:needs",
            TrayItem::Open => "tray:open",
            TrayItem::Dock => "tray:dock",
            TrayItem::Quit => "tray:quit",
        }
    }

    /// The line by its item id.
    pub fn from_id(id: &str) -> Option<TrayItem> {
        TrayItem::ALL.into_iter().find(|i| i.id() == id)
    }

    /// The fixed lines' titles; the two live lines take the report's words.
    pub fn title(self) -> &'static str {
        match self {
            TrayItem::Status | TrayItem::Needs => "",
            TrayItem::Open => "Open Bisa",
            TrayItem::Dock => "Show in Dock",
            TrayItem::Quit => "Quit Bisa",
        }
    }
}

/// What the webview hears back from `tray_dock`: whether the icon shows,
/// and whether this platform has a Dock to show it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DockAnswer {
    pub visible: bool,
    pub supported: bool,
    /// Why `supported` is false, for a log line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// What the icon shows right now, so a repeated report or ink is no work.
struct Seen {
    report: TrayReport,
    ink: Ink,
    dock_visible: bool,
}

/// The icon, its two live menu lines and the Dock check, held as app state.
pub struct Tray {
    icon: TrayIcon,
    status: MenuItem<tauri::Wry>,
    needs: MenuItem<tauri::Wry>,
    open: MenuItem<tauri::Wry>,
    dock: Option<CheckMenuItem<tauri::Wry>>,
    quit: MenuItem<tauri::Wry>,
    seen: Mutex<Seen>,
}

/// The menu bar's ink right now: the OS's word where it has one
/// (`platform::menu_bar_ink`), else the main window's theme, else light.
fn ink_now(app: &AppHandle) -> Ink {
    platform::current().menu_bar_ink().unwrap_or_else(|| {
        Ink::for_theme(
            main_window(app)
                .and_then(|w| w.theme().ok())
                .unwrap_or(Theme::Light),
        )
    })
}

fn warn(what: &str, e: impl std::fmt::Display) {
    tracing::warn!(target: "bisa_desktop", "the menu bar icon {what}: {e}");
}

/// Build the icon and its menu; the caller `manage`s the result.
pub fn install(app: &AppHandle) -> tauri::Result<Tray> {
    let report = TrayReport::default();
    let ink = ink_now(app);
    let status = MenuItemBuilder::with_id(TrayItem::Status.id(), &report.status)
        .enabled(false)
        .build(app)?;
    let needs = MenuItemBuilder::with_id(TrayItem::Needs.id(), &report.needs_words)
        .enabled(report.needs_is_a_door())
        .build(app)?;
    let open = MenuItemBuilder::with_id(TrayItem::Open.id(), TrayItem::Open.title()).build(app)?;
    let dock = if platform::current().has_dock() {
        Some(
            CheckMenuItemBuilder::with_id(TrayItem::Dock.id(), TrayItem::Dock.title())
                .checked(true)
                .build(app)?,
        )
    } else {
        None
    };
    let quit = MenuItemBuilder::with_id(TrayItem::Quit.id(), TrayItem::Quit.title()).build(app)?;
    let mut menu = MenuBuilder::new(app)
        .item(&status)
        .item(&needs)
        .separator()
        .item(&open);
    if let Some(dock) = &dock {
        menu = menu.item(dock);
    }
    let menu = menu.separator().item(&quit).build()?;

    let icon = TrayIconBuilder::with_id(TRAY_ID)
        .icon((&glyph::compose(glyph::mark(), ink, report.state.dot())).into())
        .tooltip(report.tooltip())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| {
            if let Some(item) = TrayItem::from_id(event.id().as_ref()) {
                fire(app, item);
            }
        })
        .build(app)?;

    Ok(Tray {
        icon,
        status,
        needs,
        open,
        dock,
        quit,
        seen: Mutex::new(Seen {
            report,
            ink,
            dock_visible: true,
        }),
    })
}

/// A menu line chosen.
fn fire(app: &AppHandle, item: TrayItem) {
    match item {
        TrayItem::Status => {}
        TrayItem::Needs => open_inbox(app),
        TrayItem::Open => show_window(app),
        TrayItem::Dock => {
            // A click before the state is managed has nothing to toggle yet.
            if let Some(tray) = app.try_state::<Tray>() {
                let visible = !tray.dock_visible();
                let answer = tray.set_dock(app, visible);
                if answer.supported {
                    if let Err(e) =
                        app.emit(DOCK_EVENT, serde_json::json!({ "visible": answer.visible }))
                    {
                        warn("did not tell the webview about the Dock", e);
                    }
                }
            }
        }
        TrayItem::Quit => request_quit(app),
    }
}

/// Bring the window back: the app first (a ⌘H'd app stays hidden otherwise),
/// then the window shown, un-minimised and focused — and the mark's ink
/// re-read, since the appearance may have changed while nothing was shown.
pub fn show_window(app: &AppHandle) {
    if let Some(tray) = app.try_state::<Tray>() {
        tray.refresh(app);
    }
    platform::current().reveal(app);
    let Some(window) = main_window(app) else {
        tracing::warn!(target: "bisa_desktop", "the menu bar icon found no window to show");
        return;
    };
    for (what, done) in [
        ("show", window.show()),
        ("unminimize", window.unminimize()),
        ("focus", window.set_focus()),
    ] {
        if let Err(e) = done {
            warn(&format!("could not {what} the window"), e);
        }
    }
}

/// Put the app away while it runs on — the webview's answer to a held close
/// while *Closing the window keeps Bisa running* is on. How is the
/// platform's (`Platform::conceal`): on macOS the app hides, as ⌘H does, so
/// ⌘Tab, the Dock and the icon all bring it back; elsewhere the window
/// hides, the one move those platforms have.
pub fn hide_window(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        tracing::warn!(target: "bisa_desktop", "no window to put away");
        return;
    };
    platform::current().conceal(app, &window);
}

/// The needs line: the window, on the Inbox.
fn open_inbox(app: &AppHandle) {
    show_window(app);
    if let Err(e) = app.emit(GO_EVENT, serde_json::json!({ "to": "inbox" })) {
        warn("did not send the webview to the Inbox", e);
    }
}

/// *Quit Bisa*: the window shown — the question is asked in it — and the
/// request handed to the webview's one close flow, as ⌘Q's is. With no
/// window to ask in, the exit stands.
fn request_quit(app: &AppHandle) {
    match main_window(app) {
        Some(window) => {
            show_window(app);
            if let Err(e) = window.emit(crate::QUIT_REQUESTED, ()) {
                warn("could not hand the quit to the webview", e);
            }
        }
        None => app.exit(0),
    }
}

impl Tray {
    /// Paint the webview's report: the two lines, the title, the tooltip,
    /// the icon. The same report again is nothing — unless the menu bar's
    /// ink moved meanwhile, which repaints the mark alone.
    pub fn present(&self, app: &AppHandle, report: TrayReport) {
        let mut seen = self.seen.locked();
        let ink = ink_now(app);
        if seen.report == report {
            if seen.ink != ink {
                seen.ink = ink;
                self.paint(ink, report.state);
            }
            return;
        }
        if let Err(e) = self.status.set_text(&report.status) {
            warn("did not relabel its status line", e);
        }
        if let Err(e) = self
            .needs
            .set_text(&report.needs_words)
            .and_then(|()| self.needs.set_enabled(report.needs_is_a_door()))
        {
            warn("did not relabel its needs line", e);
        }
        if let Err(e) = self.icon.set_title(report.title()) {
            warn("did not set its title", e);
        }
        if let Err(e) = self.icon.set_tooltip(Some(report.tooltip())) {
            warn("did not set its tooltip", e);
        }
        if seen.report.state != report.state || seen.ink != ink {
            self.paint(ink, report.state);
        }
        seen.ink = ink;
        seen.report = report;
    }

    /// A nudge — the window focused, its theme moved, the window shown —
    /// to re-read the menu bar's ink; the mark repaints only when it changed.
    pub fn refresh(&self, app: &AppHandle) {
        let mut seen = self.seen.locked();
        let ink = ink_now(app);
        if seen.ink == ink {
            return;
        }
        seen.ink = ink;
        self.paint(ink, seen.report.state);
    }

    fn paint(&self, ink: Ink, state: TrayState) {
        let bitmap = glyph::compose(glyph::mark(), ink, state.dot());
        if let Err(e) = self.icon.set_icon(Some((&bitmap).into())) {
            warn("did not repaint", e);
        }
    }

    /// Whether the app's icon shows in the Dock, as last applied.
    /// The fixed lines in the webview's language (`words.rs`): *Open Bisa*,
    /// *Show in Dock*, *Quit Bisa*; a word not given keeps its English.
    pub fn say(&self, words: &ShellWords) {
        if let Some(w) = &words.open {
            if let Err(e) = self.open.set_text(w) {
                warn("the Open line's words", e);
            }
        }
        if let (Some(dock), Some(w)) = (&self.dock, &words.dock) {
            if let Err(e) = dock.set_text(w) {
                warn("the Dock line's words", e);
            }
        }
        if let Some(w) = &words.quit {
            if let Err(e) = self.quit.set_text(w) {
                warn("the Quit line's words", e);
            }
        }
    }

    pub fn dock_visible(&self) -> bool {
        self.seen.locked().dock_visible
    }

    /// Show or put away the Dock icon, and mirror it on the menu's check.
    /// Where there is no Dock the answer says so and nothing changes.
    pub fn set_dock(&self, app: &AppHandle, visible: bool) -> DockAnswer {
        match platform::current().set_dock_visible(app, visible) {
            Ok(()) => {
                self.seen.locked().dock_visible = visible;
                if let Some(dock) = &self.dock {
                    if let Err(e) = dock.set_checked(visible) {
                        warn("did not mirror the Dock on its menu", e);
                    }
                }
                DockAnswer {
                    visible,
                    supported: true,
                    detail: None,
                }
            }
            Err(platform::Unsupported(why)) => DockAnswer {
                visible: true,
                supported: false,
                detail: Some(why),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_lines_have_ids_under_one_namespace_that_round_trip() {
        assert_eq!(TrayItem::ALL.len(), 5);
        for item in TrayItem::ALL {
            assert_eq!(TrayItem::from_id(item.id()), Some(item), "{}", item.id());
            assert!(
                item.id().starts_with("tray:"),
                "the namespace the webview's guard reads"
            );
        }
        assert_eq!(
            TrayItem::from_id("edit:copy"),
            None,
            "an Edit verb is not a tray line"
        );
        for fixed in [TrayItem::Open, TrayItem::Dock, TrayItem::Quit] {
            assert!(
                !fixed.title().is_empty(),
                "{:?} has a title of its own",
                fixed
            );
        }
        for live in [TrayItem::Status, TrayItem::Needs] {
            assert!(
                live.title().is_empty(),
                "{:?} takes the report's words",
                live
            );
        }
        assert_eq!(GO_EVENT, "tray:go");
        assert_eq!(DOCK_EVENT, "tray:dock");
        assert_eq!(MAIN_WINDOW, "main");
    }

    #[test]
    fn the_main_window_is_found_by_its_own_label_and_no_browser_tab_ever_wears_it() {
        // `main_window` looks the window up by label; a browser tab is a child
        // webview of that window under a label of its own, so the lookup can
        // never land on a tab — and the lookup is by *window*, since a webview
        // window is one whose only webview is its own (`browser.rs` attaches
        // more).
        for key in ["a", "main", "01TAB", "x-y"] {
            let label = crate::browser::label_of(key);
            assert_ne!(label, MAIN_WINDOW, "{key}");
            assert!(label.starts_with("browser-"), "{label}");
        }
    }

    #[test]
    fn an_unsupported_dock_answers_without_a_detail_on_the_wire_only_when_there_is_none() {
        let no = DockAnswer {
            visible: true,
            supported: false,
            detail: Some("not this platform".to_string()),
        };
        let json = serde_json::to_string(&no).unwrap();
        assert!(
            json.contains(r#""supported":false"#) && json.contains("not this platform"),
            "{json}"
        );
        let yes = DockAnswer {
            visible: false,
            supported: true,
            detail: None,
        };
        assert!(!serde_json::to_string(&yes).unwrap().contains("detail"));
    }
}
