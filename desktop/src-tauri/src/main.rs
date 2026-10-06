// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod artifacts;
mod attribution;
mod browser;
mod disk;
mod edit_menu;
mod gpu;
mod logging;
mod login_env;
mod loose;
mod navigation;
// Beside every async command that borrows (a `State<'_>`), `#[tauri::command]`
// emits a compile-time check — `const _: () = if false { let _: &dyn
// AsyncCommandMustReturnResult = &_check; }` — that the shell's
// `let_underscore_must_use` reads as a dropped `Result`. The check is the
// macro's, at module level where no attribute of ours reaches it but this one;
// the modules' own results are handled by hand.
#[allow(clippy::let_underscore_must_use)]
mod network;
mod pasteboard;
mod permissions;
mod png;
#[allow(clippy::let_underscore_must_use)]
mod ports;
mod probe;
mod second_launch;
mod sidecar;
#[allow(clippy::let_underscore_must_use)]
mod stats;
mod sync;
#[allow(clippy::let_underscore_must_use)]
mod terminal;
mod tray;
mod window_state;
mod words;

use sidecar::{NodeState, NodeStatus};
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager, RunEvent, Runtime, State, WindowEvent};

/// Where the node answers. An error is the reason it is not running yet;
/// the webview shows it and waits for `node:restarted`.
#[tauri::command]
fn api_base(node: State<NodeState>) -> Result<String, String> {
    node.api_base()
}

#[tauri::command]
fn api_token(node: State<NodeState>) -> String {
    node.api_token()
}

/// The node as the shell supervises it: running or not, its pid, whether a
/// person started it, how many restarts, how long healthy.
#[tauri::command]
fn node_status(node: State<NodeState>) -> NodeStatus {
    node.status()
}

/// The webview's word that the quit it was asked about may go ahead: a
/// programmatic exit, which the `ExitRequested` arm below lets through.
/// Every way out ends here — ⌘Q, the menu bar's *Quit Bisa*, and the window
/// closing while *Closing the window keeps Bisa running* is off.
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// The event that hands a person's ⌘Q — or the menu bar's *Quit Bisa* — to
/// the webview's one close flow (`shell/useCloseGuard.ts`): it asks if the
/// switch says so, saves, and calls `quit_app`.
const QUIT_REQUESTED: &str = "bisa:quit-requested";

/// The event that hands the window's red button to the same flow: the shell
/// holds every close request (`on_window_event` below) and the webview
/// decides — hide the window while `desktop.close_keeps_running` is on, so
/// the app lives on in the menu bar (`tray/`), else the close flow. The
/// webview can never lose its window to a close it did not choose.
const CLOSE_REQUESTED: &str = "bisa:close-requested";

/// The webview's answer to a held close while *Closing the window keeps
/// Bisa running* is on: the app put away, running on in the menu bar
/// (`tray::hide_window` — the app hidden on macOS, the window elsewhere).
/// Where the window stood is written first: an app put away may be ended
/// from the menu bar without its window ever coming back.
#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    window_state::keep(&app);
    tray::hide_window(&app);
}

/// Where the licence and the third-party notices are on this machine, for
/// About to reveal: the bundle's `Resources/` when the app is bundled
/// (`scripts/macos/lib.sh` puts both there), else the repository's own
/// files beside this crate, for a development run. A file that is in
/// neither place is `None`, and About draws no door for it.
#[tauri::command]
fn licence_files(app: tauri::AppHandle) -> LicenceFiles {
    let resources = app.path().resource_dir().ok();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let find = |name: &str| -> Option<String> {
        [resources.as_deref(), Some(repo.as_path())]
            .into_iter()
            .flatten()
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
            .map(|p| p.display().to_string())
    };
    LicenceFiles {
        licence: find("LICENSE"),
        notices: find("THIRD-PARTY-NOTICES.md"),
    }
}

/// The two files About reveals, by path — or `None` where one is not on this machine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct LicenceFiles {
    licence: Option<String>,
    notices: Option<String>,
}

/// The webview's report of what the platform is doing, painted on the menu
/// bar icon (`tray::Tray::present`).
#[tauri::command]
fn tray_report(app: tauri::AppHandle, tray: State<tray::Tray>, report: tray::TrayReport) {
    tray.present(&app, report);
}

/// The `desktop.dock_icon` setting applied: the app's icon in the Dock shown
/// or put away; the answer says whether this platform has a Dock at all.
#[tauri::command]
fn tray_dock(app: tauri::AppHandle, tray: State<tray::Tray>, visible: bool) -> tray::DockAnswer {
    tray.set_dock(&app, visible)
}

/// The words the shell's own menus say, in the language the webview speaks
/// (17 — Internationalisation): the menu bar icon's fixed lines and, on
/// macOS, the Edit menu's verbs. A word not sent keeps its English.
#[tauri::command]
fn shell_words(app: tauri::AppHandle, tray: State<tray::Tray>, words: words::ShellWords) {
    tray.say(&words);
    if let Some(verbs) = app.try_state::<edit_menu::Verbs<tauri::Wry>>() {
        verbs.say(&words);
    }
}

/// Write bytes the webview produced (an exported diagram) to the path the
/// person chose in the save dialog. A machine capability, so it is the
/// shell's: the node never writes outside the workspace on the UI's behalf.
#[tauri::command]
fn export_file(path: String, bytes: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {path}: {e}"))
}

/// What the mounted theme family's surfaces are made of — the page's word
/// (`FAMILIES[].material` in `shell/theme.ts`), parsed once at the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Material {
    /// The page paints its ground fully; the window is only a window.
    Opaque,
    /// The page's surfaces carry alpha and frost; the OS's own under-window
    /// effect goes behind them so there is something to frost.
    Glass,
}

impl Material {
    fn parse(word: &str) -> Result<Self, String> {
        match word {
            "opaque" => Ok(Self::Opaque),
            "glass" => Ok(Self::Glass),
            other => Err(format!("unknown material `{other}`")),
        }
    }

    /// The window effect this material asks the OS for, if it has one to
    /// give. macOS blurs the desktop under the window; Windows 11 lays Mica
    /// behind it; elsewhere the page frosts its own ground and that is the
    /// whole effect. The same pair is the window's `windowEffects` in
    /// `tauri.conf.json`, so a fresh window — the default family is glass —
    /// is frosted from its first frame, before any script asks. An opaque family asks for nothing — its page hides the
    /// window entirely — and the shell hands the OS `None`, which Windows
    /// clears and macOS leaves as it was, invisibly, behind a solid page.
    fn effects(self) -> Option<tauri::utils::config::WindowEffectsConfig> {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::Effect;
        match self {
            Self::Opaque => None,
            Self::Glass => Some(WindowEffectsConfig {
                effects: vec![Effect::UnderWindowBackground, Effect::Mica],
                state: None,
                radius: None,
                color: None,
            }),
        }
    }
}

/// Put `text` on the clipboard.
///
/// The webview writes its clipboard only inside a user gesture, and a menu's
/// chosen action runs after the menu has left — a moment later, no gesture —
/// so a *Copy path* from a context menu was refused and swallowed. The shell
/// has no such rule: the process owns the clipboard, and every copy in the
/// app goes through this door in the desktop (`ui/clipboard.ts`).
#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// What `copy_image` says of bytes that are not a PNG.
const NOT_A_PNG: &str = "the picture is not a PNG";

/// Put a PNG on the clipboard as a picture — `copy_text`'s twin, for the
/// browser's screenshot (ide/18 §Screenshots).
///
/// The webview writes a picture to its clipboard only inside a user
/// gesture, and a screenshot is copied seconds after the click, once the
/// shell has rendered it — no gesture left. The shell has no such rule.
#[tauri::command]
fn copy_image(app: tauri::AppHandle, png: Vec<u8>) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    if !png::is_png(&png) {
        return Err(NOT_A_PNG.to_string());
    }
    let image = tauri::image::Image::from_bytes(&png).map_err(|e| e.to_string())?;
    app.clipboard()
        .write_image(&image)
        .map_err(|e| e.to_string())
}

/// Put the window's own chrome on the side the page landed on (ADR-0053),
/// and its material behind the page.
///
/// The palette is the page's; the title bar and the native controls are the
/// OS's, and they only follow `prefers-color-scheme` unless told otherwise.
/// A dark Dune over a light title bar is the flash people remember, so the
/// shell stamps the side whenever `data-scheme` changes — and with it the
/// family's material, so a glass family gets the desktop blurred under a
/// transparent window and an opaque one gets nothing behind a page that
/// paints itself fully.
#[tauri::command]
fn set_window_appearance(
    window: tauri::Window,
    scheme: String,
    material: String,
) -> Result<(), String> {
    let theme = match scheme.as_str() {
        "dark" => tauri::Theme::Dark,
        "light" => tauri::Theme::Light,
        other => return Err(format!("unknown scheme `{other}`")),
    };
    let material = Material::parse(&material)?;
    window.set_theme(Some(theme)).map_err(|e| e.to_string())?;
    window
        .set_effects(material.effects())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn restart_node(app: tauri::AppHandle, node: State<NodeState>) -> Result<NodeStatus, String> {
    let (status, restarted) = node.restart()?;
    sidecar::reattach_log(&app, &node);
    if let Err(e) = app.emit(sidecar::NODE_RESTARTED, restarted) {
        tracing::warn!(target: "bisa_desktop", "the restart did not reach the webview: {e}");
    }
    Ok(status)
}

/// The application menu on macOS, built by hand rather than taken from
/// Tauri's default: a menu key equivalent fires before the webview ever sees
/// the key. The default *Window* menu binds ⌘W to *Close Window*, which
/// would close the whole window under the keymap's `close_tab`; the
/// predefined Edit items would claim ⌘X ⌘C ⌘V ⌘A the same way and leave a
/// Files tree — no field, so no `paste:` — with nothing (ide/15 §Dispatch).
/// So the four editing verbs are the shell's own items (`edit_menu.rs`): the
/// verb runs natively down the responder chain and the webview is told, so
/// a field pastes as before and a tree's `paste_entry` fires. Undo and redo
/// stay predefined. Off macOS no menu is built: a menubar accelerator would
/// only shadow the keymap, and WebView2 and WebKitGTK take Ctrl+C/V/X/A
/// themselves.
#[cfg(target_os = "macos")]
fn app_menu<R: Runtime>(handle: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
    let app = SubmenuBuilder::new(handle, "Bisa")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let verbs = edit_menu::items(handle)?;
    let mut edit = SubmenuBuilder::new(handle, "Edit")
        .undo()
        .redo()
        .separator();
    for item in verbs.items() {
        edit = edit.item(item);
    }
    let edit = edit.build()?;
    // Kept, so `shell_words` can say the verbs in the webview's language.
    handle.manage(verbs);
    let window = SubmenuBuilder::new(handle, "Window")
        .minimize()
        .maximize()
        .separator()
        .fullscreen()
        .build()?;
    MenuBuilder::new(handle)
        .items(&[&app, &edit, &window])
        .build()
}

fn main() {
    // The subscriber first — the file comes in `setup` (`sidecar::boot`),
    // under the workspace the `bisa` binary names, so a node that fails
    // to start is a line in the file, not on a stderr nobody reads.
    // Nothing before `build` spawns a process or writes a file: a second
    // launch of the app ends inside `build`, below, having left nothing.
    let log = logging::install();
    // The app's context first: its identifier names the config folder where
    // the login shell's `PATH` is remembered between launches (`login_env`),
    // and the memo must be in place before the first process asks for it.
    let context = tauri::generate_context!();
    login_env::remember_in(dirs::config_dir().map(|dir| dir.join(&context.config().identifier)));

    let builder = tauri::Builder::default()
        // One Bisa at a time (02 §I64), and first: a second launch of the
        // app hands its arguments — a `bisa://join/…` link on Windows and
        // Linux, where the OS starts a process for every link — to the
        // running one and exits here, before any other plugin's setup and
        // before `setup` below starts the node (`second_launch.rs`).
        .plugin(tauri_plugin_single_instance::init(second_launch::hand_off))
        .plugin(tauri_plugin_notification::init())
        // The folder picker behind "Import a folder", and Reveal in Finder on
        // a project's path. Both are plugin commands, so registering the
        // plugin is only half of it — see `capabilities/default.json`.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // `bisa://join/…` invitation links (14-collaboration).
        .plugin(tauri_plugin_deep_link::init())
        // The clipboard, for `copy_text` below — this crate's own command, so
        // no capability line: the webview never calls the plugin itself.
        .plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu).on_menu_event(|app, event| {
        if let Some(verb) = edit_menu::EditVerb::from_id(event.id().as_ref()) {
            edit_menu::perform(app, verb);
        }
    });
    builder
        .manage(log)
        // An `Arc` rather than the registry itself: the reader thread behind
        // every PTY has to reach the registry to forget its session when the
        // shell exits, and a `State` borrow cannot outlive the command that
        // took it.
        .manage(Arc::new(terminal::TerminalRegistry::new()))
        .manage(browser::BrowserRegistry::new())
        .manage(Arc::new(stats::Table::new()))
        // The node is kept up by the shell, not by luck: a child that dies
        // unasked is started again on its port, and the webview is told.
        .setup(|app| {
            // The node, from here and never from `main`: a second launch
            // has left `build` through the single-instance plugin above,
            // and a node started before that would be nobody's. A node
            // that cannot start is not a reason for no window: the shell
            // opens, says why, and the watchdog keeps trying (`sidecar.rs`).
            // Managed before the window exists: a command's `State` panics
            // on a state nobody manages.
            let node = sidecar::boot(app.handle());
            app.manage(node);
            // Still no file — no binary named the workspace and no node
            // answered: the app's own log folder, the last resort.
            logging::attach_fallback(app.handle());
            // The main window is built here rather than by the config alone
            // (`"create": false`), so it carries a navigation policy: the
            // app's own origin, the empty page and an addon's bundle on the
            // node, in every frame — an addon's document cannot navigate
            // itself anywhere else (`navigation.rs`, 18 — Addons).
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == tray::MAIN_WINDOW)
                .cloned()
                .ok_or("tauri.conf.json names no main window")?;
            // The window opens where it was left (`window_state.rs`): the
            // size and the place it had, if a screen still holds that place,
            // else the config's size, centred. Held as state before the
            // window exists, so its first move is already noted.
            let placed = window_state::WindowState::load(app.handle());
            let placement = placed.placement(app.handle(), &config);
            app.manage(placed);
            let window = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
                .inner_size(placement.width, placement.height);
            let window = match placement.position {
                Some((x, y)) => window.position(x, y),
                None => window.center(),
            };
            let handle = app.handle().clone();
            window
                .maximized(placement.maximized)
                .fullscreen(placement.fullscreen)
                .on_navigation(move |url| {
                    let base = handle
                        .try_state::<NodeState>()
                        .and_then(|node| node.api_base().ok());
                    let ok = navigation::allowed(url, base.as_deref());
                    if !ok {
                        tracing::warn!(target: "bisa_desktop", url = %url, "a navigation was refused");
                    }
                    ok
                })
                .build()?;
            NodeState::supervise(app.handle().clone());
            // The menu bar icon, before the window has drawn a frame: the
            // app lives there from now until `Exit`.
            let tray = tray::install(app.handle())?;
            app.manage(tray);
            Ok(())
        })
        // The window's red button is held and handed to the webview, as ⌘Q
        // is: hide or quit is the webview's decision (`useCloseGuard.ts`).
        // The window coming to the front, or its theme moving, is the moment
        // to re-read the menu bar's ink for the mark (`tray::Tray::refresh`).
        // Where the window stands is noted at every move, resize and change
        // of scale — a window crossing to another screen — and written when
        // a person has stopped moving it: at a held close and when the
        // window loses focus (`window_state.rs`).
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == tray::MAIN_WINDOW => {
                api.prevent_close();
                window_state::keep(window.app_handle());
                if let Err(e) = window.emit(CLOSE_REQUESTED, ()) {
                    tracing::warn!(target: "bisa_desktop", "the close request did not reach the webview: {e}");
                }
            }
            WindowEvent::Moved(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
                if window.label() == tray::MAIN_WINDOW =>
            {
                if let Some(placed) = window
                    .app_handle()
                    .try_state::<window_state::WindowState>()
                {
                    placed.note(window);
                }
            }
            WindowEvent::Focused(false) if window.label() == tray::MAIN_WINDOW => {
                window_state::keep(window.app_handle());
            }
            WindowEvent::Focused(true) | WindowEvent::ThemeChanged(_) => {
                if let Some(tray) = window.app_handle().try_state::<tray::Tray>() {
                    tray.refresh(window.app_handle());
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            api_base,
            api_token,
            node_status,
            quit_app,
            hide_window,
            licence_files,
            tray_report,
            tray_dock,
            shell_words,
            export_file,
            artifacts::open_artifact,
            artifacts::copy_artifact,
            loose::read_loose_file,
            loose::write_loose_file,
            loose::dropped_paths,
            pasteboard::pasteboard_holds,
            pasteboard::pasteboard_image,
            pasteboard::paste_into,
            pasteboard::paste_image_into,
            pasteboard::paste_image_to_temp,
            set_window_appearance,
            copy_text,
            copy_image,
            restart_node,
            logging::log_event,
            logging::log_configure,
            permissions::system_permission_status,
            permissions::system_permission_request,
            ports::listening_ports,
            ports::stop_port,
            stats::host_info,
            stats::resource_usage,
            disk::data_dir_usage,
            network::network_facts,
            browser::browser_open,
            browser::browser_navigate,
            browser::browser_back,
            browser::browser_forward,
            browser::browser_reload,
            browser::browser_stop,
            browser::browser_bounds,
            browser::browser_close,
            browser::browser_screenshot,
            browser::browser_drive,
            browser::browser_clear,
            browser::browser_focus_main,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_answered,
            terminal::terminal_resize,
            terminal::terminal_close,
            terminal::terminal_cwd,
            terminal::terminal_scrollback_read,
            terminal::terminal_scrollback_write,
            terminal::terminal_scrollback_forget,
        ])
        .build(context)
        .expect("error building tauri app")
        .run(|app, event| match event {
            // ⌘Q, the Dock's Quit: held, and handed to the webview's one close
            // flow, which asks (when `desktop.confirm_quit` says so), saves
            // what is dirty, then quits through `quit_app` — whose own exit
            // request carries a code and is not held. With no window to ask,
            // the exit stands.
            RunEvent::ExitRequested {
                code: None, api, ..
            } => {
                if let Some(window) = tray::main_window(app) {
                    api.prevent_exit();
                    if let Err(e) = window.emit(QUIT_REQUESTED, ()) {
                        tracing::warn!(target: "bisa_desktop", "the quit request did not reach the webview: {e}");
                    }
                }
            }
            // The Dock icon clicked with no window showing — a window
            // hidden on its own, not the app — comes back. A close never
            // destroys the window (`on_window_event` above), so nothing
            // outlives it by accident; the terminals and the node go with
            // `Exit`.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => tray::show_window(app),
            RunEvent::Exit => {
                // Where the window stood, before anything is shut down: the
                // one write no later failure can cost.
                window_state::keep(app);
                app.state::<Arc<terminal::TerminalRegistry>>()
                    .shutdown_all();
                app.state::<browser::BrowserRegistry>().close_all(app);
                app.state::<NodeState>().shutdown();
                app.state::<bisa_log::Handle>().goodbye("quit");
            }
            _ => {}
        });
}
