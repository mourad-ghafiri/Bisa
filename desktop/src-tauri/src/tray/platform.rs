//! The operating system under the menu bar icon: the few moves that differ
//! by platform, behind one trait, so the tray, the webview and the docs
//! keep their shape when Linux and Windows arrive.
//!
//! macOS is first, in `permissions.rs`'s manner: the one `target_os` cfg
//! holds the AppKit-backed answers, and every other platform compiles the
//! same trait with `Elsewhere`, which has no Dock and says so. What the
//! others add later, each as its own `Platform`:
//!
//! - **Windows** draws no title beside a notification-area icon; the count
//!   goes through `Window::set_overlay_icon` instead, and the Dock question
//!   does not arise (`has_dock` false).
//! - **Linux** emits no click on the icon (tauri-apps/tray-icon), so the
//!   menu is the only door; the title is drawn, the tooltip is not.
//!
//! `conceal` and `reveal` are a pair: what a held close does to the app,
//! and what every door back undoes. On macOS both are the app's — hide as
//! ⌘H does, un-hide — so ⌘Tab and the Dock work as the OS makes them work;
//! elsewhere the window itself hides and shows.

use super::glyph::Ink;
use tauri::{AppHandle, Window};

/// Why a move was not made here: not this platform, or this build cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

/// What the tray asks of the OS beyond drawing itself.
pub trait Platform: Send + Sync {
    /// Whether this OS has a Dock (or equivalent) the app can leave.
    fn has_dock(&self) -> bool;
    /// Show or put away the app's icon in the Dock.
    fn set_dock_visible(&self, app: &AppHandle, visible: bool) -> Result<(), Unsupported>;
    /// Put the app away while it runs on: the whole app where the OS has
    /// that move (macOS), else the window.
    fn conceal(&self, app: &AppHandle, window: &Window);
    /// Bring the app itself back — a ⌘H'd or concealed app on macOS — before its window is shown.
    fn reveal(&self, app: &AppHandle);
    /// The ink the menu bar draws its items in right now, where the OS says;
    /// `None` leaves it to the window's theme. The page sets the window's
    /// theme to its own scheme (`set_window_appearance`), and tao applies it
    /// to the whole app, so neither the window's theme nor the app's
    /// effective appearance says what the *menu bar* looks like — only the
    /// system's appearance does.
    fn menu_bar_ink(&self) -> Option<Ink>;
}

#[cfg(target_os = "macos")]
struct MacOs;

#[cfg(target_os = "macos")]
impl Platform for MacOs {
    fn has_dock(&self) -> bool {
        true
    }

    /// Off, the app becomes an *accessory*: no Dock icon, no place in ⌘Tab,
    /// no application menu — the menu bar icon is its only door. On, a
    /// regular app again.
    fn set_dock_visible(&self, app: &AppHandle, visible: bool) -> Result<(), Unsupported> {
        app.set_dock_visibility(visible)
            .map_err(|e| Unsupported(format!("the Dock did not answer: {e}")))
    }

    /// The app hides as ⌘H hides it — every window with it — so the OS
    /// itself brings it back from ⌘Tab and the Dock, and `reveal` is the
    /// exact undo. A window hidden on its own would come back from neither.
    fn conceal(&self, app: &AppHandle, _window: &Window) {
        if let Err(e) = app.hide() {
            tracing::warn!(target: "bisa_desktop", "the app did not hide: {e}");
        }
    }

    fn reveal(&self, app: &AppHandle) {
        if let Err(e) = app.show() {
            tracing::debug!(target: "bisa_desktop", "the app was not un-hidden: {e}");
        }
    }

    /// The system appearance, as the global defaults record it:
    /// `AppleInterfaceStyle` is `Dark` in dark mode and absent in light.
    fn menu_bar_ink(&self) -> Option<Ink> {
        use objc2_foundation::{NSString, NSUserDefaults};
        let style = NSUserDefaults::standardUserDefaults()
            .stringForKey(&NSString::from_str("AppleInterfaceStyle"))
            .map(|s| s.to_string());
        Some(if style.as_deref() == Some("Dark") {
            Ink::Light
        } else {
            Ink::Dark
        })
    }
}

/// Every other platform, for now: no Dock, the window hides and shows itself.
#[cfg_attr(target_os = "macos", allow(dead_code))]
struct Elsewhere;

impl Platform for Elsewhere {
    fn has_dock(&self) -> bool {
        false
    }

    fn set_dock_visible(&self, _app: &AppHandle, _visible: bool) -> Result<(), Unsupported> {
        Err(Unsupported(
            "not this platform: the Dock is macOS's".to_string(),
        ))
    }

    fn conceal(&self, _app: &AppHandle, window: &Window) {
        if let Err(e) = window.hide() {
            tracing::warn!(target: "bisa_desktop", "the window did not hide: {e}");
        }
    }

    fn reveal(&self, _app: &AppHandle) {}

    fn menu_bar_ink(&self) -> Option<Ink> {
        None
    }
}

/// The platform this build runs on.
pub fn current() -> &'static dyn Platform {
    #[cfg(target_os = "macos")]
    {
        static P: MacOs = MacOs;
        &P
    }
    #[cfg(not(target_os = "macos"))]
    {
        static P: Elsewhere = Elsewhere;
        &P
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elsewhere_has_no_dock_and_says_why() {
        assert!(!Elsewhere.has_dock());
        assert_eq!(Elsewhere.menu_bar_ink(), None, "the window's theme decides");
        // No `AppHandle` is made in a unit test; the refusal is decided before one is needed,
        // which is what `has_dock` promises: a caller asks it first.
    }

    #[test]
    fn this_build_names_its_platform() {
        let p = current();
        assert_eq!(p.has_dock(), cfg!(target_os = "macos"));
    }
}
