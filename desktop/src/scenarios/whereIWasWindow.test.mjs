/**
 * The window opens where it was left, as the sources show it: the shell
 * reads the window's place before the window exists and tells the builder;
 * every move, resize and change of scale is noted; a place is looked for in
 * the space the OS lays its screens out in, and the way into maximized is
 * never taken for a place; the place is written at a held close, when
 * the app is put away, when the window loses focus and at `Exit`; the file
 * is versioned, written beside then placed, and refused rather than
 * migrated. Source assertions, as `tray.test.mjs` makes them — no DOM. Run
 * with `node --test desktop/src/scenarios/whereIWasWindow.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/** The text from one marker up to the next, both of which must be there. */
const between = (text, from, to) => {
  const start = text.indexOf(from);
  assert.ok(start >= 0, `the shell says \`${from}\``);
  const end = text.indexOf(to, start + from.length);
  assert.ok(end > start, `and \`${to}\` after it`);
  return text.slice(start, end);
};

/** Every marker is there, each after the one before it. */
const inOrder = (text, markers, why) => {
  let at = -1;
  for (const marker of markers) {
    const next = text.indexOf(marker, at + 1);
    assert.ok(next > at, `${why}: \`${marker}\``);
    at = next;
  }
};

test("the window's place is read before the window exists, and the builder is told all of it before it builds", () => {
  const main = read("../src-tauri/src/main.rs");
  assert.ok(main.includes("\nmod window_state;\n"), "the module is the shell's");
  const setup = between(main, ".setup(|app| {", ".on_window_event(");
  inOrder(
    setup,
    [
      "window_state::WindowState::load(app.handle())",
      "placed.placement(app.handle(), &config)",
      "app.manage(placed)",
      "tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?",
      ".inner_size(placement.width, placement.height)",
      "Some((x, y)) => window.position(x, y)",
      "None => window.center()",
      ".maximized(placement.maximized)",
      ".fullscreen(placement.fullscreen)",
      ".on_navigation(",
      ".build()?",
    ],
    "loaded, fitted, held as state, then size, place and standing — and only then built",
  );
  assert.equal(setup.split(".build()?").length - 1, 1, "one window is built in setup");
});

test("every move, resize and change of scale of the main window is noted, and the place is written when a person has stopped moving it", () => {
  const main = read("../src-tauri/src/main.rs");
  const events = between(main, ".on_window_event(", ".invoke_handler(");
  const noted = between(events, "WindowEvent::Moved(_)", "WindowEvent::Focused(false)");
  inOrder(noted, ["| WindowEvent::Resized(_)", "| WindowEvent::ScaleFactorChanged { .. }", "if window.label() == tray::MAIN_WINDOW =>"], "a move, a resize, a window crossing to a screen of another scale — the main window's alone, since a browser tab is a child webview and never a window of its own");
  assert.ok(noted.includes(".try_state::<window_state::WindowState>()") && noted.includes("placed.note(window)"), "noted, never written: a drag is an event per frame");
  assert.ok(!noted.includes("window_state::keep("), "no write on a move");
  // The held close still holds and still hands over; the write sits between.
  const close = between(events, "WindowEvent::CloseRequested { api, .. }", "WindowEvent::Moved(_)");
  inOrder(close, ["window.label() == tray::MAIN_WINDOW", "api.prevent_close()", "window_state::keep(window.app_handle())", "window.emit(CLOSE_REQUESTED, ())"], "held, written, handed to the webview");
  const blurred = between(events, "WindowEvent::Focused(false)", "WindowEvent::Focused(true)");
  assert.ok(blurred.includes("window.label() == tray::MAIN_WINDOW") && blurred.includes("window_state::keep(window.app_handle())"), "written when the window loses focus");
  // The arm that was there is there still.
  const front = between(events, "WindowEvent::Focused(true) | WindowEvent::ThemeChanged(_)", "_ => {}");
  assert.ok(front.includes("tray.refresh(window.app_handle())"), "the menu bar's ink is still re-read");
});

test("the place is written when the app is put away and first thing at Exit", () => {
  const main = read("../src-tauri/src/main.rs");
  const hide = between(main, "fn hide_window(", "fn licence_files(");
  inOrder(hide, ["window_state::keep(&app)", "tray::hide_window(&app)"], "written, then put away");
  const exit = between(main, "RunEvent::Exit => {", "_ => {}");
  inOrder(exit, ["window_state::keep(app)", ".shutdown_all()", ".close_all(app)", ".shutdown()", '.goodbye("quit")'], "written before anything is shut down");
  const state = read("../src-tauri/src/window_state.rs");
  const keep = between(state, "pub fn keep(app: &AppHandle)", "\n}\n");
  assert.ok(keep.includes("app.try_state::<WindowState>()") && keep.includes("state.write()"), "one call behind every write, and no state is no write");
});

test("the file is the app's own, versioned, written beside then placed, and only when it changed", () => {
  const state = read("../src-tauri/src/window_state.rs");
  assert.ok(state.includes("pub const VERSION: u32 = 1;"), "the version it is written in");
  assert.ok(state.includes('const FILE: &str = "window.json";') && state.includes("app.path().app_config_dir()"), "window.json under the app's config folder");
  const place = between(state, "fn place(path: &Path, bounds: &Bounds)", "\n}\n");
  inOrder(place, ["std::fs::create_dir_all(dir)", 'path.with_extension("json.tmp")', "std::fs::write(&tmp, text)", "std::fs::rename(&tmp, path)"], "beside, then renamed over");
  assert.ok(!place.includes("std::fs::write(path") && !place.includes("std::fs::write(&path"), "the file itself is never written in place");
  const write = between(state, "pub fn write(&self)", "\n    }\n");
  inOrder(write, ["held.written == Some(bounds)", "place(path, &bounds)", "held.written = Some(bounds)"], "what the file already says is not written again");
  assert.ok(write.includes("tracing::warn!(") && !write.includes("unwrap()") && !write.includes("expect("), "a failed write is a line in the log, never a panic");
});

test("a file of another version, or one that does not parse, is refused — never migrated", () => {
  const state = read("../src-tauri/src/window_state.rs");
  const parse = between(state, "pub fn parse(text: &str) -> Option<Bounds>", "\n}\n");
  inOrder(parse, ["serde_json::from_str(text).ok()?", "stored.v != VERSION", "is_a_size(stored.bounds.width, stored.bounds.height)", "is_a_scale(stored.bounds.scale)"], "parsed, of this version, a size that is a size, a scale that is a scale");
  assert.equal((state.match(/#\[serde\(deny_unknown_fields\)\]/g) ?? []).length, 2, "the bounds and the file around them know their fields");
  assert.ok(!/\bv\s*(==|=>)\s*[02-9]/.test(state) && !state.includes("VERSION - 1"), "no other version is read");
  const load = between(state, "pub fn load(app: &AppHandle) -> Self", "\n    }\n");
  assert.ok(!load.includes("?") && !load.includes("unwrap()") && !load.includes("expect("), "reading never fails the launch");
});

test("the rules take plain numbers, the window is looked up as a window, and every rule has its test", () => {
  const state = read("../src-tauri/src/window_state.rs");
  assert.ok(!state.includes("get_webview_window("), "never as a webview window: a browser tab is a child webview of `main`");
  assert.ok(state.includes("pub fn note(&self, window: &Window)"), "the window is handed in, as a window");
  // The window is asked once an event, so its corner and its scale are of the same moment.
  const sight = between(state, "fn sight(window: &Window) -> tauri::Result<Seen>", "\n}\n");
  for (const asked of ["window.outer_position()?", "window.inner_size()?", "window.scale_factor()?", "window.is_maximized()?", "window.is_fullscreen()?", "window.is_minimized()?"]) assert.ok(sight.includes(asked), asked);
  for (const rule of ["fit", "parse", "text", "moved", "resized", "first_seen", "after"]) {
    const signature = state.match(new RegExp(`pub fn ${rule}\\(([\\s\\S]*?)\\) -> ([^{]*)\\{`));
    assert.ok(signature, `pub fn ${rule}`);
    assert.ok(!/tauri|Window|Monitor|AppHandle|Physical|Logical/.test(signature[0]), `${rule} takes no Tauri type, so it is tested without a window`);
  }
  assert.ok(state.includes("screens: &[Screen],") && state.includes("space: Space,") && state.includes("minimum: (f64, f64),") && state.includes("default: (f64, f64),") && state.includes(") -> Placement {"), "fit: what was saved, the screens, the space they are laid out in, the minimum, the default");
  // The space is the OS's, named in one place; the rules are handed it.
  const here = between(state, "pub fn here() -> Self", "\n    }\n");
  assert.ok(here.includes('cfg!(target_os = "macos")') && here.includes("Self::Logical") && here.includes("Self::Physical"), "logical pixels on macOS, physical elsewhere");
  assert.equal((state.slice(0, state.indexOf("#[cfg(test)]")).match(/Space::here\(\)/g) ?? []).length, 1, "asked once, where the window is placed");
  const tests = state.slice(state.indexOf("#[cfg(test)]"));
  for (const name of [
    "a_place_on_a_monitor_is_kept",
    "a_place_on_no_monitor_is_centred",
    "a_place_is_read_at_its_own_monitors_scale",
    "a_place_is_looked_for_in_the_space_the_screens_are_laid_out_in",
    "a_size_under_the_minimum_is_raised",
    "a_size_over_the_work_area_is_held_in_it",
    "a_maximized_window_keeps_its_normal_bounds",
    "a_corner_is_kept_with_the_scale_it_was_reported_at",
    "a_zoom_that_is_animated_keeps_the_bounds_from_before_it",
    "a_run_that_ends_standing_normally_is_where_the_window_is",
    "a_pause_ends_the_run",
    "a_scale_that_is_no_scale_is_refused",
    "a_minimized_window_moves_nothing",
    "a_window_first_seen_is_taken_as_it_stands",
    "nothing_saved_is_the_default_centred",
    "another_version_is_refused",
    "a_file_that_does_not_parse_is_the_default",
    "a_size_that_is_no_size_is_refused",
    "what_was_written_reads_back_the_same",
    "the_file_is_written_beside_then_placed_and_only_when_it_changed",
  ]) {
    assert.ok(tests.includes(`    #[test]\n    fn ${name}() {`), name);
  }
  // The one test that touches the disk stays inside a folder of its own.
  assert.ok(tests.includes("tempfile::tempdir()") && !tests.includes("app_config_dir") && !tests.includes('"/tmp') && !tests.includes("env::"), "no test reads or writes the machine's own folders");
  // The window's own size stays the config's: the default and the minimum are read from it, never spelt twice.
  const conf = JSON.parse(read("../src-tauri/tauri.conf.json"));
  const window = conf.app.windows.find((w) => w.label === "main");
  assert.ok(window && window.create === false, "the shell builds the main window itself");
  assert.ok(window.width > 0 && window.height > 0 && window.minWidth > 0 && window.minHeight > 0, "a default and a minimum to fit to");
  assert.ok(window.x === undefined && window.y === undefined && window.center === undefined, "the config names no place: the place is the one that was kept, or the centre");
  const placement = between(state, "pub fn placement(&self, app: &AppHandle, config: &WindowConfig) -> Placement", "\n    }\n");
  assert.ok(placement.includes("config.min_width") && placement.includes("config.min_height") && placement.includes("(config.width, config.height)"), "read from the window's config");
});
