//! The way out, held (ide/03, ide/13). Every `terminate:` AppKit sends the
//! app — ⌘Q and the application menu's *Quit Bisa*, the Dock's *Quit*, an
//! AppleScript `quit`, a logout or a shutdown — is answered *later*: the
//! window comes forward, the webview's one close flow asks and saves
//! (`shell/useCloseGuard.ts`), and the answer goes back to AppKit through
//! `replyToApplicationShouldTerminate:`. A *yes* ends the app the way AppKit
//! ends one — `applicationWillTerminate:`, tao's `LoopDestroyed`, the one
//! `RunEvent::Exit` in `main.rs` — and a *no* leaves everything as it was;
//! a logout waits for the answer and is told it, instead of being quietly
//! aborted. Off macOS no OS asks an app to quit: the window's close is held
//! in `main.rs`, and Ctrl+Q is the keymap's `quit`, which ends in
//! `quit_app` like every other way out.
//!
//! Why this exists: tao's app delegate implements no
//! `applicationShouldTerminate:`, so AppKit's default — terminate now — stood,
//! and the `RunEvent::ExitRequested { code: None }` the shell holds in
//! `main.rs` only ever comes from a destroyed last window, which this shell
//! never has. So ⌘Q ended Bisa with no question and no save, whatever the
//! *Confirm before quitting* switch said. The method is given to tao's
//! delegate here: a subclass of its class with the one method, the instance
//! moved into it (`object_setClass`) so tao's own state stays where tao looks
//! for it, and the delegate set again so AppKit re-reads what it answers to.
//!
//! The reply is sent by the two sync commands (`quit_app`, `quit_declined`),
//! which run on the main thread inside WebKit's message callout — never
//! through `run_on_main_thread` from another thread: that is drained inside
//! tao's callback lock, and a *yes* makes AppKit terminate at once, which
//! takes the same lock (`AppState::exit`). One question at a time: a second
//! `terminate:` while one is held is refused — the question up answers for
//! both — which a logout arriving over a ⌘Q question is told as a no.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::AppHandle;

/// Whether a `terminate:` is held for the webview's answer, and whether the
/// webview is there to ask — it says so once it listens (`quit_ready`).
pub struct Hold {
    held: AtomicBool,
    ready: AtomicBool,
}

impl Hold {
    pub const fn new() -> Self {
        Self {
            held: AtomicBool::new(false),
            ready: AtomicBool::new(false),
        }
    }

    /// The webview listens for `bisa:quit-requested` from now on.
    pub fn ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Take the hold: true when it was free and is now held.
    pub fn take(&self) -> bool {
        self.held
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Let it go: true when it was held.
    pub fn release(&self) -> bool {
        self.held.swap(false, Ordering::SeqCst)
    }
}

static HOLD: Hold = Hold::new();
static APP: OnceLock<AppHandle> = OnceLock::new();

/// The webview listens: a quit the OS asks for can be held for its question.
/// Before this the exit stands, as it did — there is nobody to ask through.
pub fn ready() {
    HOLD.ready();
    tracing::debug!(target: "bisa_desktop", "the webview listens: a quit the OS asks for is held for its question from now on");
}

/// Give the app delegate its `applicationShouldTerminate:`, once, from
/// `setup` — after the window and the menu bar icon exist, since a held quit
/// brings the window forward through the icon's door (`tray::show_window`).
pub fn install(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        tracing::warn!(target: "bisa_desktop", "the way out was installed twice; the first stands");
        return;
    }
    platform::install(app);
}

/// The webview's answer to a held quit: `go` ends the app through AppKit,
/// a no leaves it. True when a quit was held and has been answered; false
/// when nothing was held — the caller's own exit, or a no with nothing to
/// say it to.
pub fn answer(app: &AppHandle, go: bool) -> bool {
    if !HOLD.release() {
        return false;
    }
    platform::reply(app, go);
    true
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{APP, HOLD};
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
    use std::ffi::CStr;
    use tauri::AppHandle;

    /// tao's delegate class with the one method more.
    const CLASS: &CStr = c"BisaAppDelegate";

    pub fn install(_app: &AppHandle) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!(target: "bisa_desktop", "the way out is installed off the main thread; ⌘Q is not held");
            return;
        };
        let ns_app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = ns_app.delegate() else {
            tracing::warn!(target: "bisa_desktop", "the app has no delegate; ⌘Q is not held");
            return;
        };
        let object: &AnyObject = delegate.as_ref();
        let patched = match AnyClass::get(CLASS) {
            Some(class) => class,
            None => {
                let Some(mut builder) = ClassBuilder::new(CLASS, object.class()) else {
                    tracing::warn!(target: "bisa_desktop", "the app delegate's class could not be extended; ⌘Q is not held");
                    return;
                };
                // Safety: the signature is the protocol's — `NSUInteger
                // (id self, SEL _cmd, id sender)` — and the body never unwinds.
                unsafe {
                    builder.add_method(
                        sel!(applicationShouldTerminate:),
                        should_terminate as unsafe extern "C-unwind" fn(_, _, _) -> _,
                    );
                }
                builder.register()
            }
        };
        // Safety: the subclass adds no ivars, so the instance's layout is its
        // class's; tao keeps reading its own state through the superclass.
        unsafe {
            objc2::ffi::object_setClass(object as *const AnyObject as *mut AnyObject, patched);
        }
        ns_app.setDelegate(Some(&delegate));
        tracing::info!(target: "bisa_desktop", class = %patched.name().to_string_lossy(), "the way out is held: a quit waits for the webview's answer");
    }

    /// AppKit asks whether the app may end. The decision is `decide`'s; a
    /// panic in it must not cross into AppKit, so it ends as the OS meant.
    unsafe extern "C-unwind" fn should_terminate(
        _this: &AnyObject,
        _cmd: Sel,
        _sender: *mut AnyObject,
    ) -> usize {
        let reply = std::panic::catch_unwind(decide).unwrap_or_else(|_| {
            tracing::error!(target: "bisa_desktop", "deciding on a quit panicked; the exit stands");
            NSApplicationTerminateReply::TerminateNow
        });
        reply.0
    }

    /// Hold the quit for the webview's question when there is a webview to
    /// ask and no question up already; else let the OS have its way, or,
    /// over a question already up, say no — that question answers for both.
    fn decide() -> NSApplicationTerminateReply {
        let Some(app) = APP.get() else {
            return NSApplicationTerminateReply::TerminateNow;
        };
        if !HOLD.is_ready() {
            tracing::info!(target: "bisa_desktop", "a quit before the webview listens: the exit stands");
            return NSApplicationTerminateReply::TerminateNow;
        }
        let Some(window) = crate::tray::main_window(app) else {
            tracing::info!(target: "bisa_desktop", "a quit with no window to ask in: the exit stands");
            return NSApplicationTerminateReply::TerminateNow;
        };
        if !HOLD.take() {
            tracing::info!(target: "bisa_desktop", "a quit while one is being asked about: refused, the question up answers");
            return NSApplicationTerminateReply::TerminateCancel;
        }
        // The question is asked in the window: a window minimised, or an app
        // put away and quit from the Dock's menu, comes forward first.
        crate::tray::show_window(app);
        if let Err(e) = tauri::Emitter::emit(&window, crate::QUIT_REQUESTED, ()) {
            HOLD.release();
            tracing::error!(target: "bisa_desktop", "the quit request did not reach the webview, so the exit stands: {e}");
            return NSApplicationTerminateReply::TerminateNow;
        }
        tracing::info!(target: "bisa_desktop", "a quit is held for the webview's question");
        NSApplicationTerminateReply::TerminateLater
    }

    /// The answer back to AppKit, on the main thread it is waiting on. The
    /// callers are sync commands, which run there; the other road — tao's
    /// proxy — is taken only for a no, which AppKit answers by going on,
    /// never for a yes, which would terminate inside tao's callback lock.
    pub fn reply(app: &AppHandle, go: bool) {
        match MainThreadMarker::new() {
            Some(mtm) => {
                NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(go)
            }
            None if !go => {
                tracing::warn!(target: "bisa_desktop", "a no to a held quit off the main thread; sent through the loop");
                if let Err(e) = app.run_on_main_thread(|| {
                    if let Some(mtm) = MainThreadMarker::new() {
                        NSApplication::sharedApplication(mtm)
                            .replyToApplicationShouldTerminate(false);
                    }
                }) {
                    tracing::error!(target: "bisa_desktop", "the no did not reach AppKit: {e}");
                }
            }
            None => {
                // Not a road a sync command takes; said loudly rather than
                // risked, and the hold is given back so a later no can land.
                HOLD.take();
                tracing::error!(target: "bisa_desktop", "a yes to a held quit off the main thread is not sent; the quit stays held");
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use tauri::AppHandle;

    /// No OS here asks an app to quit: the window's close is held in
    /// `main.rs`, and Ctrl+Q is the keymap's `quit`.
    pub fn install(_app: &AppHandle) {
        tracing::debug!(target: "bisa_desktop", "no OS quit to hold on this platform; the window's close and Ctrl+Q are the ways out");
    }

    pub fn reply(_app: &AppHandle, _go: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quit_is_held_once_and_a_second_waits_for_the_first_to_be_answered() {
        let hold = Hold::new();
        assert!(!hold.is_ready(), "nobody listens until the webview says so");
        assert!(hold.take(), "free, so held");
        assert!(
            !hold.take(),
            "held already: the question up answers for both"
        );
        assert!(hold.release(), "it was held");
        assert!(hold.take(), "free again once answered");
        assert!(hold.release());
    }

    #[test]
    fn a_release_of_nothing_held_is_nothing() {
        let hold = Hold::new();
        assert!(!hold.release(), "nothing was held: the caller's own exit");
        assert!(!hold.release(), "and still nothing");
        hold.ready();
        assert!(hold.is_ready());
        assert!(!hold.release(), "ready is not held");
    }

    #[test]
    fn an_answer_with_nothing_held_says_so_and_sends_nothing() {
        // The statics: nothing is held at the start of the process, and
        // `answer` is honest about it — the caller then exits on its own.
        assert!(!HOLD.release());
        assert!(
            !HOLD.is_ready() || HOLD.is_ready(),
            "ready is independent of the hold"
        );
    }
}
