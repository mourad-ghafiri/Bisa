//! Where the window was: its size, its place on the screen and whether it
//! stood maximized or fullscreen, kept across a restart so the app opens
//! where it was left.
//!
//! **What is kept** is the window's *normal* bounds — the ones it comes back
//! to when it is un-maximized — with its standing beside them, so a window
//! quit maximized opens maximized and still has a size of its own under it.
//! The position is kept in physical pixels, as the OS reported it, beside
//! the scale the window had there; the size in logical pixels, so it means
//! the same on a screen of another scale.
//!
//! **Where**: `window.json` under the app's config folder
//! (`app.path().app_config_dir()`), as `{ "v": 1, "bounds": { … } }`. A
//! machine's fact, so it is the shell's and never the workspace's: the same
//! workspace opened on another machine has other screens.
//!
//! **When it is written**: on a held close, when the app is put away, when
//! the window loses focus and at `Exit` — never on a move. A drag is an event
//! per frame, so `note` only remembers and `write` is for the moments a
//! person has stopped; a write that would say what the file already says is
//! no write at all.
//!
//! **What is refused**: a file of another version, one that does not parse,
//! a size that is no size, a scale that is no scale. Each is the default —
//! the config's size, centred — and nothing is migrated: the next write
//! replaces the file.
//!
//! **A place is kept only where it can be reached**: a monitor unplugged
//! since, or a resolution changed, would open the window where no pointer
//! can take its title bar. `fit` keeps a saved position only when that
//! corner lands on a screen this machine has now, and holds the size inside
//! that screen. The corner is looked for in the space the OS lays its
//! screens out in (`Space`): macOS has one space of logical pixels, and what
//! it reports as a physical position is the logical one times the scale of
//! the screen the window is on — so two screens of different scales overlap
//! in physical pixels, and only the logical reading tells them apart.
//!
//! **The way into maximized is not a place**: macOS animates a zoom, reports
//! each of its frames as a resize and says the window is zoomed only at the
//! last. `Track` keeps the bounds from before a run of changes, and a run
//! that ends maximized or fullscreen goes back to them.
//!
//! The rules — `fit`, `parse`, `moved`, `resized`, `first_seen`, `Track` —
//! take plain numbers and no Tauri type, so they are tested without a
//! window; `WindowState` is the thin part that asks the window and the disk.

use crate::sync::Locked;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, Monitor, Window};

/// The version the file is written in and the only one read.
pub const VERSION: u32 = 1;

/// The file's name, under the app's config folder.
const FILE: &str = "window.json";

/// How far in from the window's top-left corner the point that has to land
/// on a screen is, in the space's pixels: the start of the title bar, where
/// a pointer takes the window. A corner a few pixels off the edge — a
/// snapped window's invisible border — still counts as on the screen.
const MARGIN: f64 = 24.0;

/// How long a run of moves and resizes may pause and still be one run: an
/// animation's frames are a sixtieth of a second apart, a hand that resizes
/// and then reaches for the zoom button takes longer than this.
const RUN: Duration = Duration::from_millis(250);

/// The window's normal bounds and its standing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    /// The outer top-left corner, in physical pixels, as the OS reported it.
    pub x: i32,
    pub y: i32,
    /// The scale the window had when the corner was reported.
    pub scale: f64,
    /// The inner size, in logical pixels.
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
    pub fullscreen: bool,
}

/// The file: the bounds under the version that wrote them.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    v: u32,
    bounds: Bounds,
}

/// The space an OS lays its screens out in — what a window's corner is a
/// point of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// One space of logical pixels across every screen (macOS). A physical
    /// number is the logical one times the scale of the screen it is on, so
    /// it is read back by dividing by the scale it was reported at.
    Logical,
    /// One space of physical pixels across every screen (Windows, Linux).
    Physical,
}

impl Space {
    /// The space of the OS this shell is built for.
    pub fn here() -> Self {
        if cfg!(target_os = "macos") {
            Self::Logical
        } else {
            Self::Physical
        }
    }

    /// A saved corner as a point of the space.
    fn point(self, saved: &Bounds) -> (f64, f64) {
        let by = match self {
            Self::Logical => or_one(saved.scale),
            Self::Physical => 1.0,
        };
        (f64::from(saved.x) / by, f64::from(saved.y) / by)
    }

    /// A point of the space as the window builder reads it: logical pixels,
    /// at the scale of the screen the point is on.
    fn logical(self, point: (f64, f64), home: &Screen) -> (f64, f64) {
        match self {
            Self::Logical => point,
            Self::Physical => (point.0 / home.scale(), point.1 / home.scale()),
        }
    }
}

/// One monitor's work area, in physical pixels as the OS reports them, and
/// its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl Screen {
    /// The scale to divide by. A monitor that reports none it can be divided
    /// by is read at one.
    fn scale(&self) -> f64 {
        or_one(self.scale)
    }

    /// The area as the space lays it out: its left, its top, its width and
    /// its height.
    fn area(&self, space: Space) -> (f64, f64, f64, f64) {
        let by = match space {
            Space::Logical => self.scale(),
            Space::Physical => 1.0,
        };
        (
            f64::from(self.x) / by,
            f64::from(self.y) / by,
            f64::from(self.width) / by,
            f64::from(self.height) / by,
        )
    }

    /// Whether a point of the space is inside the area. The area ends before
    /// its width, not on it.
    fn holds(&self, space: Space, point: (f64, f64)) -> bool {
        let (left, top, width, height) = self.area(space);
        point.0 >= left && point.0 < left + width && point.1 >= top && point.1 < top + height
    }

    /// The area's size in logical pixels.
    fn logical_size(&self) -> (f64, f64) {
        (
            f64::from(self.width) / self.scale(),
            f64::from(self.height) / self.scale(),
        )
    }
}

/// How the window is opened: what the builder is told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The outer top-left corner in logical pixels; `None` is centred.
    pub position: Option<(f64, f64)>,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
    pub fullscreen: bool,
}

/// How the window stands at the moment of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Standing {
    pub maximized: bool,
    pub fullscreen: bool,
    pub minimized: bool,
}

impl Standing {
    /// Neither maximized, fullscreen nor minimized: the bounds the window
    /// reports are its own.
    pub fn normal(self) -> bool {
        !(self.maximized || self.fullscreen || self.minimized)
    }

    /// The bounds wearing this standing. A minimized window says nothing of
    /// how it stands — it is an icon, and what it answers is about the icon
    /// — so what was held stays as it was until the window is back.
    fn over(self, bounds: Bounds) -> Bounds {
        if self.minimized {
            return bounds;
        }
        Bounds {
            maximized: self.maximized,
            fullscreen: self.fullscreen,
            ..bounds
        }
    }
}

/// What the window says of itself, asked in one go so that its corner and
/// its scale are of the same moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    /// The outer top-left corner, in physical pixels.
    pub position: (i32, i32),
    /// The inner size, in physical pixels.
    pub size: (u32, u32),
    pub scale: f64,
    pub standing: Standing,
}

/// Whether a width and a height are a size a window can have.
fn is_a_size(width: f64, height: f64) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

/// Whether a scale is one a pixel can be divided by.
fn is_a_scale(scale: f64) -> bool {
    scale.is_finite() && scale > 0.0
}

/// The scale, or one for a scale nothing can be divided by.
fn or_one(scale: f64) -> f64 {
    if is_a_scale(scale) {
        scale
    } else {
        1.0
    }
}

/// A physical size in logical pixels — or nothing, for a size of no pixels
/// or a scale nothing can be divided by.
fn logical(size: (u32, u32), scale: f64) -> Option<(f64, f64)> {
    if !is_a_scale(scale) || size.0 == 0 || size.1 == 0 {
        return None;
    }
    Some((f64::from(size.0) / scale, f64::from(size.1) / scale))
}

/// A size held inside a screen's work area, then raised to the minimum. The
/// minimum is applied last: the window refuses to be smaller, so on a screen
/// smaller than the minimum it is the minimum that stands. With no screen
/// known the size is only raised.
fn held_in(size: (f64, f64), screen: Option<&Screen>, minimum: (f64, f64)) -> (f64, f64) {
    let (mut width, mut height) = size;
    if let Some(screen) = screen {
        let (w, h) = screen.logical_size();
        width = width.min(w);
        height = height.min(h);
    }
    (width.max(minimum.0), height.max(minimum.1))
}

/// Where the window opens, from what was saved and the screens there are
/// now.
///
/// Nothing saved is the default size, centred. A saved position is kept only
/// when the start of the window's title bar — its top-left corner, moved in
/// by `MARGIN` — is inside a screen's area, both read in the space the OS
/// lays its screens out in; what the builder is told is that corner in
/// logical pixels. A position on no screen is centred. The size is held
/// inside the screen the window opens on — the one its position landed on,
/// else the first, which is where a centred window goes — and raised to the
/// minimum.
pub fn fit(
    saved: Option<&Bounds>,
    screens: &[Screen],
    space: Space,
    minimum: (f64, f64),
    default: (f64, f64),
) -> Placement {
    let Some(saved) = saved else {
        let (width, height) = held_in(default, screens.first(), minimum);
        return Placement {
            position: None,
            width,
            height,
            maximized: false,
            fullscreen: false,
        };
    };
    let corner = space.point(saved);
    let grip = (corner.0 + MARGIN, corner.1 + MARGIN);
    let home = screens.iter().find(|s| s.holds(space, grip));
    let position = home.map(|s| space.logical(corner, s));
    let (width, height) = held_in(
        (saved.width, saved.height),
        home.or(screens.first()),
        minimum,
    );
    Placement {
        position,
        width,
        height,
        maximized: saved.maximized,
        fullscreen: saved.fullscreen,
    }
}

/// The bounds a file's text holds — or nothing, for text that does not
/// parse, another version, a size that is no size or a scale that is no
/// scale.
pub fn parse(text: &str) -> Option<Bounds> {
    let stored: Stored = serde_json::from_str(text).ok()?;
    if stored.v != VERSION {
        return None;
    }
    (is_a_size(stored.bounds.width, stored.bounds.height) && is_a_scale(stored.bounds.scale))
        .then_some(stored.bounds)
}

/// The file's text for these bounds.
pub fn text(bounds: &Bounds) -> Result<String, String> {
    serde_json::to_string_pretty(&Stored {
        v: VERSION,
        bounds: *bounds,
    })
    .map_err(|e| format!("the window's place did not serialise: {e}"))
}

/// The bounds after the window moved: `position` in physical pixels,
/// reported at `scale`. The position follows only a window standing
/// normally: a maximized or fullscreen window sits at its screen's corner, a
/// minimized one nowhere, and neither is where it comes back to.
pub fn moved(bounds: Bounds, position: (i32, i32), scale: f64, standing: Standing) -> Bounds {
    let mut next = standing.over(bounds);
    if standing.normal() && is_a_scale(scale) {
        next.x = position.0;
        next.y = position.1;
        next.scale = scale;
    }
    next
}

/// The bounds after the window was resized: `size` in physical pixels, read
/// at `scale`. The size follows only a window standing normally.
pub fn resized(bounds: Bounds, size: (u32, u32), scale: f64, standing: Standing) -> Bounds {
    let mut next = standing.over(bounds);
    if !standing.normal() {
        return next;
    }
    if let Some((width, height)) = logical(size, scale) {
        next.width = width;
        next.height = height;
    }
    next
}

/// The bounds of a window nothing was held for, at its first event. A window
/// standing normally is taken as it is. One already maximized or fullscreen
/// has no bounds of its own to report, so it takes the corner it stands at —
/// a corner of the screen it is on — and the size it was opened with. A
/// minimized one is not a first sight: nothing is held until it is back.
pub fn first_seen(seen: Seen, placed: Option<(f64, f64)>) -> Option<Bounds> {
    if seen.standing.minimized || !is_a_scale(seen.scale) {
        return None;
    }
    let (width, height) = if seen.standing.normal() {
        logical(seen.size, seen.scale)?
    } else {
        placed?
    };
    Some(Bounds {
        x: seen.position.0,
        y: seen.position.1,
        scale: seen.scale,
        width,
        height,
        maximized: seen.standing.maximized,
        fullscreen: seen.standing.fullscreen,
    })
}

/// The bounds as they are followed while the app runs: what they are, and
/// what they were before the run of changes they are in.
///
/// A run is changes no further apart than `RUN`. One that ends with the
/// window standing normally — a drag, a resize by hand, the way back from
/// maximized — leaves the bounds where its last change put them. One that
/// ends maximized or fullscreen was the way in, and its frames were never a
/// place the window stood: the bounds are the ones from before it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    bounds: Bounds,
    before: Bounds,
    /// When the last change landed; `None` before any.
    at: Option<Instant>,
}

impl Track {
    /// Bounds nothing has changed yet.
    pub fn of(bounds: Bounds) -> Self {
        Self {
            bounds,
            before: bounds,
            at: None,
        }
    }

    /// The bounds as they stand.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// The track after the window was seen at `now`.
    pub fn after(self, seen: Seen, now: Instant) -> Self {
        let running = self
            .at
            .is_some_and(|at| now.saturating_duration_since(at) <= RUN);
        let before = if running { self.before } else { self.bounds };
        let base = if seen.standing.normal() || seen.standing.minimized {
            self.bounds
        } else {
            before
        };
        let bounds = resized(
            moved(base, seen.position, seen.scale, seen.standing),
            seen.size,
            seen.scale,
            seen.standing,
        );
        Self {
            bounds,
            before,
            at: Some(now),
        }
    }
}

/// What is held while the app runs.
struct Held {
    /// The window's bounds as last noted; what the file held, until then.
    track: Option<Track>,
    /// What the file says, so a write that would change nothing is skipped.
    written: Option<Bounds>,
    /// The size the window was opened with, for a window first seen already
    /// maximized.
    placed: Option<(f64, f64)>,
}

/// The window's place, held as app state.
pub struct WindowState {
    inner: Mutex<Held>,
    /// Where the file is; `None` when the machine names no config folder,
    /// and then nothing is read or written.
    path: Option<PathBuf>,
}

impl WindowState {
    /// What the file under the app's config folder holds. Never fails the
    /// launch: a missing file, an unreadable one or a refused one is the
    /// default, and the reason is a line in the log.
    pub fn load(app: &AppHandle) -> Self {
        let path = match app.path().app_config_dir() {
            Ok(dir) => Some(dir.join(FILE)),
            Err(e) => {
                tracing::warn!(target: "bisa_desktop", "no config folder, so the window's place is not kept: {e}");
                None
            }
        };
        Self::at(path)
    }

    /// The state over a file at `path`, read now.
    fn at(path: Option<PathBuf>) -> Self {
        let saved = path.as_deref().and_then(read);
        Self {
            inner: Mutex::new(Held {
                track: saved.map(Track::of),
                written: saved,
                placed: None,
            }),
            path,
        }
    }

    /// How the window is opened: what was saved, fitted to the screens this
    /// machine has now, with the minimum and the default the window's config
    /// names.
    pub fn placement(&self, app: &AppHandle, config: &WindowConfig) -> Placement {
        let screens = screens(app);
        let minimum = (
            config.min_width.unwrap_or(0.0),
            config.min_height.unwrap_or(0.0),
        );
        let mut held = self.inner.locked();
        let saved = held.track.map(|track| track.bounds());
        let placement = fit(
            saved.as_ref(),
            &screens,
            Space::here(),
            minimum,
            (config.width, config.height),
        );
        held.placed = Some((placement.width, placement.height));
        placement
    }

    /// Remember where the window stands now — after a move, a resize or a
    /// change of scale. A window that does not answer leaves what is held as
    /// it was.
    pub fn note(&self, window: &Window) {
        let seen = match sight(window) {
            Ok(seen) => seen,
            Err(e) => {
                tracing::debug!(target: "bisa_desktop", "the window did not say how it stands, so its move is not noted: {e}");
                return;
            }
        };
        let now = Instant::now();
        let mut held = self.inner.locked();
        let (track, placed) = (held.track, held.placed);
        held.track = match track {
            Some(track) => Some(track.after(seen, now)),
            None => first_seen(seen, placed).map(Track::of),
        };
    }

    /// Put what is held in the file — only when it differs from what the
    /// file says. A failure is a line in the log: the window's place is a
    /// convenience, never a reason to stop a close or a quit.
    pub fn write(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let mut held = self.inner.locked();
        let Some(bounds) = held.track.map(|track| track.bounds()) else {
            return;
        };
        if held.written == Some(bounds) {
            return;
        }
        match place(path, &bounds) {
            Ok(()) => held.written = Some(bounds),
            Err(e) => {
                tracing::warn!(target: "bisa_desktop", "the window's place was not kept: {e}")
            }
        }
    }
}

/// Write what the app holds of its window, if it holds any: the one call
/// behind every moment the place is kept (`main.rs`). Before `setup` has
/// managed the state there is nothing to write.
pub fn keep(app: &AppHandle) {
    if let Some(state) = app.try_state::<WindowState>() {
        state.write();
    }
}

/// What the window says of itself, now.
fn sight(window: &Window) -> tauri::Result<Seen> {
    let position = window.outer_position()?;
    let size = window.inner_size()?;
    Ok(Seen {
        position: (position.x, position.y),
        size: (size.width, size.height),
        scale: window.scale_factor()?,
        standing: Standing {
            maximized: window.is_maximized()?,
            fullscreen: window.is_fullscreen()?,
            minimized: window.is_minimized()?,
        },
    })
}

/// The screens this machine has now, the primary first: a centred window
/// opens on the primary, so that is the screen its size is held in.
fn screens(app: &AppHandle) -> Vec<Screen> {
    let primary = match app.primary_monitor() {
        Ok(monitor) => monitor,
        Err(e) => {
            tracing::debug!(target: "bisa_desktop", "the primary monitor was not named: {e}");
            None
        }
    };
    let all = match app.available_monitors() {
        Ok(monitors) => monitors,
        Err(e) => {
            tracing::debug!(target: "bisa_desktop", "the monitors were not listed: {e}");
            Vec::new()
        }
    };
    let mut screens: Vec<Screen> = Vec::new();
    for monitor in primary.iter().chain(all.iter()) {
        let screen = screen_of(monitor);
        if !screens.contains(&screen) {
            screens.push(screen);
        }
    }
    screens
}

fn screen_of(monitor: &Monitor) -> Screen {
    let area = monitor.work_area();
    Screen {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
        scale: monitor.scale_factor(),
    }
}

/// What the file at `path` holds. No file is the first launch and says
/// nothing; a file that is refused says why.
fn read(path: &Path) -> Option<Bounds> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(target: "bisa_desktop", "could not read {}: {e}", path.display());
            return None;
        }
    };
    let bounds = parse(&text);
    if bounds.is_none() {
        tracing::warn!(target: "bisa_desktop", "{} is not a window's place this version wrote: the default stands", path.display());
    }
    bounds
}

/// Put the bounds in the file at `path`, making its folder on the first
/// write.
fn place(path: &Path, bounds: &Bounds) -> Result<(), String> {
    let text = text(bounds)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    // Write beside, then rename: a crash mid-write leaves the old place,
    // never half of a new one.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("could not place {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMUM: (f64, f64) = (900.0, 600.0);
    const DEFAULT: (f64, f64) = (1240.0, 820.0);

    /// A laptop's own screen: 1440×900 logical at scale two.
    const LAPTOP: Screen = Screen {
        x: 0,
        y: 0,
        width: 2880,
        height: 1800,
        scale: 2.0,
    };

    /// A monitor to its right at scale one, as an OS that lays its screens
    /// out in physical pixels reports it.
    const DESK: Screen = Screen {
        x: 2880,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };

    /// The same monitor as macOS reports it: at the laptop's logical right
    /// edge, times its own scale — inside the laptop's physical pixels.
    const DESK_IN_POINTS: Screen = Screen {
        x: 1440,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };

    /// The desk monitor alone, so at the origin.
    const DESK_AT_ORIGIN: Screen = Screen {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };

    const NORMAL: Standing = Standing {
        maximized: false,
        fullscreen: false,
        minimized: false,
    };

    const MAXIMIZED: Standing = Standing {
        maximized: true,
        fullscreen: false,
        minimized: false,
    };

    const FULLSCREEN: Standing = Standing {
        maximized: false,
        fullscreen: true,
        minimized: false,
    };

    /// Bounds reported at scale two, standing normally.
    fn bounds(x: i32, y: i32, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            scale: 2.0,
            width,
            height,
            maximized: false,
            fullscreen: false,
        }
    }

    /// The window seen at scale two.
    fn seen(position: (i32, i32), size: (u32, u32), standing: Standing) -> Seen {
        Seen {
            position,
            size,
            scale: 2.0,
            standing,
        }
    }

    fn physical(saved: &Bounds, screens: &[Screen]) -> Placement {
        fit(Some(saved), screens, Space::Physical, MINIMUM, DEFAULT)
    }

    #[test]
    fn a_place_on_a_monitor_is_kept() {
        let saved = bounds(200, 100, 1000.0, 700.0);
        let placement = physical(&saved, &[LAPTOP]);
        assert_eq!(placement.position, Some((100.0, 50.0)));
        assert_eq!((placement.width, placement.height), (1000.0, 700.0));
        assert!(!placement.maximized && !placement.fullscreen);
        // A corner a few pixels off the edge is a snapped window's border.
        let snapped = bounds(-7, 0, 1000.0, 700.0);
        assert!(physical(&snapped, &[DESK_AT_ORIGIN]).position.is_some());
    }

    #[test]
    fn a_place_on_no_monitor_is_centred() {
        // Saved on the desk monitor, which is unplugged now.
        let saved = bounds(3400, 200, 1000.0, 700.0);
        let placement = physical(&saved, &[LAPTOP]);
        assert_eq!(placement.position, None);
        assert_eq!(
            (placement.width, placement.height),
            (1000.0, 700.0),
            "the size is kept"
        );
        // Above and to the left of every screen.
        for lost in [
            bounds(-4000, 100, 1000.0, 700.0),
            bounds(100, -4000, 1000.0, 700.0),
        ] {
            assert_eq!(physical(&lost, &[LAPTOP, DESK]).position, None);
        }
        // Past the last pixel: the area ends before its width, not on it.
        let edge = bounds(2880 - 24, 100, 1000.0, 700.0);
        assert_eq!(physical(&edge, &[LAPTOP]).position, None);
        // With no screen known nothing says the place can be reached.
        assert_eq!(physical(&saved, &[]).position, None);
    }

    #[test]
    fn a_place_is_read_at_its_own_monitors_scale() {
        let screens = [LAPTOP, DESK];
        let on_laptop = bounds(400, 300, 1000.0, 700.0);
        assert_eq!(
            physical(&on_laptop, &screens).position,
            Some((200.0, 150.0)),
            "scale two"
        );
        let on_desk = bounds(3000, 300, 1000.0, 700.0);
        assert_eq!(
            physical(&on_desk, &screens).position,
            Some((3000.0, 300.0)),
            "scale one"
        );
        // A monitor that reports no scale to divide by is read at one.
        let odd = Screen {
            scale: 0.0,
            ..DESK_AT_ORIGIN
        };
        assert_eq!(
            physical(&bounds(40, 60, 1000.0, 700.0), &[odd]).position,
            Some((40.0, 60.0))
        );
    }

    #[test]
    fn a_place_is_looked_for_in_the_space_the_screens_are_laid_out_in() {
        let screens = [LAPTOP, DESK_IN_POINTS];
        // On the desk monitor, 160 points in from its left edge: reported at
        // the desk's scale of one — a number the laptop's physical pixels
        // hold too.
        let on_desk = Bounds {
            scale: 1.0,
            ..bounds(1600, 100, 1000.0, 700.0)
        };
        let placement = fit(Some(&on_desk), &screens, Space::Logical, MINIMUM, DEFAULT);
        assert_eq!(
            placement.position,
            Some((1600.0, 100.0)),
            "the desk monitor, where it was"
        );
        // Read as physical pixels it would have landed on the laptop.
        assert_eq!(
            physical(&on_desk, &screens).position,
            Some((800.0, 50.0)),
            "the reading the space refuses"
        );
        // On the laptop, reported at its scale of two.
        let on_laptop = bounds(400, 300, 1000.0, 700.0);
        assert_eq!(
            fit(Some(&on_laptop), &screens, Space::Logical, MINIMUM, DEFAULT).position,
            Some((200.0, 150.0))
        );
        // The size is held in the screen the corner landed on.
        let large = Bounds {
            scale: 1.0,
            ..bounds(1600, 100, 2400.0, 1600.0)
        };
        let held = fit(Some(&large), &screens, Space::Logical, MINIMUM, DEFAULT);
        assert_eq!((held.width, held.height), (1920.0, 1080.0));
        // The desk monitor unplugged: 1600 points is past the laptop's 1440.
        assert_eq!(
            fit(Some(&on_desk), &[LAPTOP], Space::Logical, MINIMUM, DEFAULT).position,
            None
        );
    }

    #[test]
    fn a_size_under_the_minimum_is_raised() {
        let saved = bounds(200, 100, 300.0, 200.0);
        let placement = physical(&saved, &[LAPTOP]);
        assert_eq!((placement.width, placement.height), MINIMUM);
        // With no screen known the size is only raised.
        let unknown = physical(&saved, &[]);
        assert_eq!((unknown.width, unknown.height), MINIMUM);
        let large = bounds(200, 100, 5000.0, 4000.0);
        let kept = physical(&large, &[]);
        assert_eq!(
            (kept.width, kept.height),
            (5000.0, 4000.0),
            "nothing to hold it in"
        );
        // A screen smaller than the minimum: the window refuses to be
        // smaller, so the minimum stands.
        let small = Screen {
            x: 0,
            y: 0,
            width: 800,
            height: 500,
            scale: 1.0,
        };
        let held = physical(&saved, &[small]);
        assert_eq!((held.width, held.height), MINIMUM);
    }

    #[test]
    fn a_size_over_the_work_area_is_held_in_it() {
        let saved = bounds(200, 100, 2400.0, 1600.0);
        let on_laptop = physical(&saved, &[LAPTOP, DESK]);
        assert_eq!(
            (on_laptop.width, on_laptop.height),
            (1440.0, 900.0),
            "the laptop's logical area"
        );
        let on_desk = physical(&bounds(3000, 100, 2400.0, 1600.0), &[LAPTOP, DESK]);
        assert_eq!(
            (on_desk.width, on_desk.height),
            (1920.0, 1080.0),
            "the screen it landed on, not the first"
        );
        // A place on no monitor is centred on the first screen, and held in it.
        let lost = physical(&bounds(9000, 100, 2400.0, 1600.0), &[LAPTOP, DESK]);
        assert_eq!(lost.position, None);
        assert_eq!((lost.width, lost.height), (1440.0, 900.0));
    }

    #[test]
    fn a_maximized_window_keeps_its_normal_bounds() {
        let normal = bounds(200, 100, 1000.0, 700.0);
        let after = resized(
            moved(normal, (0, 50), 2.0, MAXIMIZED),
            (2880, 1750),
            2.0,
            MAXIMIZED,
        );
        assert_eq!(
            after,
            Bounds {
                maximized: true,
                ..normal
            },
            "the flag follows, the bounds stay"
        );
        let after = resized(
            moved(normal, (0, 0), 2.0, FULLSCREEN),
            (2880, 1800),
            2.0,
            FULLSCREEN,
        );
        assert_eq!(
            after,
            Bounds {
                fullscreen: true,
                ..normal
            }
        );
        // Back to normal: the flags fall and the bounds move again.
        let back = resized(
            moved(after, (240, 120), 2.0, NORMAL),
            (2200, 1500),
            2.0,
            NORMAL,
        );
        assert_eq!(back, bounds(240, 120, 1100.0, 750.0));
        // It opens maximized, over the bounds it comes back to.
        let saved = Bounds {
            maximized: true,
            ..normal
        };
        let placement = physical(&saved, &[LAPTOP]);
        assert!(placement.maximized);
        assert_eq!(placement.position, Some((100.0, 50.0)));
        assert_eq!((placement.width, placement.height), (1000.0, 700.0));
    }

    #[test]
    fn a_corner_is_kept_with_the_scale_it_was_reported_at() {
        let on_laptop = bounds(400, 300, 1000.0, 700.0);
        // Dragged to the desk monitor: the corner and the scale move together.
        let on_desk = moved(on_laptop, (1600, 100), 1.0, NORMAL);
        assert_eq!((on_desk.x, on_desk.y, on_desk.scale), (1600, 100, 1.0));
        assert_eq!(
            (on_desk.width, on_desk.height),
            (1000.0, 700.0),
            "a move is no resize"
        );
        // A scale that is no scale moves nothing: the corner would be unreadable.
        for odd in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(moved(on_laptop, (1600, 100), odd, NORMAL), on_laptop);
        }
    }

    #[test]
    fn a_minimized_window_moves_nothing() {
        let held = Bounds {
            maximized: true,
            ..bounds(200, 100, 1000.0, 700.0)
        };
        let minimized = Standing {
            minimized: true,
            ..NORMAL
        };
        // Where a minimized window is said to be is nowhere a window stands.
        assert_eq!(moved(held, (-32000, -32000), 2.0, minimized), held);
        assert_eq!(resized(held, (0, 0), 1.0, minimized), held);
        assert_eq!(
            first_seen(seen((-32000, -32000), (0, 0), minimized), Some(DEFAULT)),
            None
        );
        let track =
            Track::of(held).after(seen((-32000, -32000), (0, 0), minimized), Instant::now());
        assert_eq!(track.bounds(), held);
        // A size of no pixels, or read at no scale, is no resize either.
        let normal = bounds(200, 100, 1000.0, 700.0);
        assert_eq!(resized(normal, (0, 0), 1.0, NORMAL), normal);
        assert_eq!(resized(normal, (1200, 800), 0.0, NORMAL), normal);
        assert_eq!(resized(normal, (1200, 800), f64::NAN, NORMAL), normal);
    }

    #[test]
    fn a_window_first_seen_is_taken_as_it_stands() {
        assert_eq!(
            first_seen(seen((400, 300), (2000, 1400), NORMAL), Some(DEFAULT)),
            Some(bounds(400, 300, 1000.0, 700.0))
        );
        // Already maximized: the corner it stands at, the size it was opened with.
        assert_eq!(
            first_seen(seen((0, 50), (2880, 1750), MAXIMIZED), Some(DEFAULT)),
            Some(Bounds {
                maximized: true,
                ..bounds(0, 50, DEFAULT.0, DEFAULT.1)
            })
        );
        assert_eq!(
            first_seen(seen((0, 50), (2880, 1750), MAXIMIZED), None),
            None
        );
        // A window that reports no scale is not seen yet.
        let unscaled = Seen {
            scale: 0.0,
            ..seen((400, 300), (2000, 1400), NORMAL)
        };
        assert_eq!(first_seen(unscaled, Some(DEFAULT)), None);
    }

    #[test]
    fn a_zoom_that_is_animated_keeps_the_bounds_from_before_it() {
        let normal = bounds(200, 100, 1000.0, 700.0);
        let start = Instant::now();
        let frame = Duration::from_millis(16);
        // The frames of the way in say the window stands normally, each
        // larger than the last; only the last says it is zoomed.
        let mut track = Track::of(normal);
        let frames = [
            (seen((150, 80), (2300, 1500), NORMAL), 1),
            (seen((80, 60), (2600, 1650), NORMAL), 2),
            (seen((10, 50), (2860, 1740), NORMAL), 3),
            (seen((0, 50), (2880, 1750), MAXIMIZED), 4),
        ];
        for (seen, n) in frames {
            track = track.after(seen, start + frame * n);
        }
        assert_eq!(
            track.bounds(),
            Bounds {
                maximized: true,
                ..normal
            },
            "the bounds from before the run, wearing the standing"
        );
        // A move reported after the resize, in the same run, changes nothing.
        track = track.after(seen((0, 50), (2880, 1750), MAXIMIZED), start + frame * 5);
        assert_eq!(
            track.bounds(),
            Bounds {
                maximized: true,
                ..normal
            }
        );
        // The same for the way into fullscreen.
        let mut track = Track::of(normal);
        track = track.after(seen((100, 50), (2400, 1500), NORMAL), start + frame);
        track = track.after(seen((0, 0), (2880, 1800), FULLSCREEN), start + frame * 2);
        assert_eq!(
            track.bounds(),
            Bounds {
                fullscreen: true,
                ..normal
            }
        );
    }

    #[test]
    fn a_run_that_ends_standing_normally_is_where_the_window_is() {
        let start = Instant::now();
        let frame = Duration::from_millis(16);
        // A drag: a frame a move, the last one is where it was let go.
        let mut track = Track::of(bounds(200, 100, 1000.0, 700.0));
        for n in 1..=30 {
            let at = (200 + 10 * i32::try_from(n).unwrap(), 100);
            track = track.after(seen(at, (2000, 1400), NORMAL), start + frame * n);
        }
        assert_eq!(track.bounds(), bounds(500, 100, 1000.0, 700.0));
        // The way back from maximized: the frames shrink to the bounds the
        // window comes back to, and the standing falls.
        let mut track = Track::of(Bounds {
            maximized: true,
            ..bounds(200, 100, 1000.0, 700.0)
        });
        let later = start + Duration::from_secs(5);
        track = track.after(seen((40, 60), (2700, 1700), NORMAL), later + frame);
        track = track.after(seen((200, 100), (2000, 1400), NORMAL), later + frame * 2);
        assert_eq!(track.bounds(), bounds(200, 100, 1000.0, 700.0));
    }

    #[test]
    fn a_pause_ends_the_run() {
        let start = Instant::now();
        // Resized by hand, then — a second later — maximized: the size the
        // hand gave is the one the window comes back to.
        let mut track = Track::of(bounds(200, 100, 1000.0, 700.0));
        track = track.after(seen((200, 100), (2400, 1600), NORMAL), start);
        track = track.after(
            seen((0, 50), (2880, 1750), MAXIMIZED),
            start + Duration::from_secs(1),
        );
        assert_eq!(
            track.bounds(),
            Bounds {
                maximized: true,
                ..bounds(200, 100, 1200.0, 800.0)
            }
        );
        // The pause is measured from the last change, not from the first:
        // a long drag is one run.
        let mut track = Track::of(bounds(200, 100, 1000.0, 700.0));
        let step = Duration::from_millis(200);
        for n in 1..=10 {
            track = track.after(seen((300, 100), (2000, 1400), NORMAL), start + step * n);
        }
        track = track.after(seen((0, 50), (2880, 1750), MAXIMIZED), start + step * 11);
        assert_eq!(
            track.bounds(),
            Bounds {
                maximized: true,
                ..bounds(200, 100, 1000.0, 700.0)
            },
            "two seconds of changes, none a pause"
        );
        // A clock that went back is no pause and no panic.
        let track = Track::of(bounds(200, 100, 1000.0, 700.0))
            .after(seen((300, 100), (2000, 1400), NORMAL), start + step)
            .after(seen((0, 50), (2880, 1750), MAXIMIZED), start);
        assert!(track.bounds().maximized);
    }

    #[test]
    fn nothing_saved_is_the_default_centred() {
        for space in [Space::Physical, Space::Logical] {
            let placement = fit(None, &[LAPTOP, DESK], space, MINIMUM, DEFAULT);
            assert_eq!(
                placement,
                Placement {
                    position: None,
                    width: DEFAULT.0,
                    height: DEFAULT.1,
                    maximized: false,
                    fullscreen: false,
                }
            );
            let unknown = fit(None, &[], space, MINIMUM, DEFAULT);
            assert_eq!((unknown.width, unknown.height), DEFAULT);
            assert_eq!(unknown.position, None);
        }
    }

    #[test]
    fn another_version_is_refused() {
        let ours = text(&bounds(200, 100, 1000.0, 700.0)).unwrap();
        assert!(ours.contains("\"v\": 1"), "{ours}");
        for other in [0, 2, 99] {
            let theirs = ours.replace("\"v\": 1", &format!("\"v\": {other}"));
            assert_ne!(theirs, ours);
            assert_eq!(parse(&theirs), None, "v {other}");
        }
        // No version at all is another version.
        assert_eq!(
            parse(
                r#"{"bounds":{"x":1,"y":2,"scale":2.0,"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false}}"#
            ),
            None
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_the_default() {
        for broken in [
            "",
            "not json",
            "{",
            "[]",
            "null",
            r#"{"v":1}"#,
            r#"{"v":1,"bounds":{"x":1,"y":2}}"#,
            // No scale the corner was reported at.
            r#"{"v":1,"bounds":{"x":1,"y":2,"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false}}"#,
            // A field this version does not know.
            r#"{"v":1,"bounds":{"x":1,"y":2,"scale":2.0,"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false,"monitor":"left"}}"#,
            r#"{"v":1,"at":0,"bounds":{"x":1,"y":2,"scale":2.0,"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false}}"#,
            // A position that is no pixel.
            r#"{"v":1,"bounds":{"x":1.5,"y":2,"scale":2.0,"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false}}"#,
        ] {
            assert_eq!(parse(broken), None, "{broken}");
            let placement = fit(
                parse(broken).as_ref(),
                &[LAPTOP],
                Space::Physical,
                MINIMUM,
                DEFAULT,
            );
            assert_eq!(placement.position, None, "{broken}");
            assert_eq!((placement.width, placement.height), DEFAULT, "{broken}");
        }
    }

    #[test]
    fn a_size_that_is_no_size_is_refused() {
        for (width, height) in [
            ("0", "700.0"),
            ("1000.0", "0"),
            ("-1000.0", "700.0"),
            ("1000.0", "-1"),
            ("null", "700.0"),
            ("1e999", "700.0"),
        ] {
            let file = format!(
                r#"{{"v":1,"bounds":{{"x":1,"y":2,"scale":2.0,"width":{width},"height":{height},"maximized":false,"fullscreen":false}}}}"#
            );
            assert_eq!(parse(&file), None, "{file}");
        }
        assert!(!is_a_size(f64::NAN, 700.0));
        assert!(!is_a_size(1000.0, f64::INFINITY));
        assert!(is_a_size(1000.0, 700.0));
    }

    #[test]
    fn a_scale_that_is_no_scale_is_refused() {
        for scale in ["0", "-2.0", "null", "1e999", "\"2\""] {
            let file = format!(
                r#"{{"v":1,"bounds":{{"x":1,"y":2,"scale":{scale},"width":1000.0,"height":700.0,"maximized":false,"fullscreen":false}}}}"#
            );
            assert_eq!(parse(&file), None, "{file}");
        }
        assert!(!is_a_scale(f64::NAN));
        assert!(is_a_scale(1.25));
    }

    #[test]
    fn what_was_written_reads_back_the_same() {
        for held in [
            bounds(200, 100, 1000.0, 700.0),
            Bounds {
                scale: 1.25,
                ..bounds(-1900, -40, 1234.5, 987.25)
            },
            Bounds {
                maximized: true,
                ..bounds(0, 0, 900.0, 600.0)
            },
            Bounds {
                fullscreen: true,
                ..bounds(i32::MAX, i32::MIN, 1240.0, 820.0)
            },
        ] {
            let written = text(&held).unwrap();
            assert_eq!(parse(&written), Some(held), "{written}");
        }
    }

    /// What a state holds of the window's bounds.
    fn held_by(state: &WindowState) -> Option<Bounds> {
        state.inner.locked().track.map(|track| track.bounds())
    }

    #[test]
    fn the_file_is_written_beside_then_placed_and_only_when_it_changed() {
        let dir = tempfile::tempdir().unwrap();
        // A folder that is not there yet: the first write makes it.
        let path = dir.path().join("dev.bisa.bisa").join(FILE);
        let state = WindowState::at(Some(path.clone()));
        assert_eq!(held_by(&state), None, "no file is nothing saved");
        state.write();
        assert!(!path.exists(), "nothing held is nothing written");

        let held = Bounds {
            maximized: true,
            ..bounds(200, 100, 1000.0, 700.0)
        };
        state.inner.locked().track = Some(Track::of(held));
        state.write();
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()), Some(held));
        assert!(
            !path.with_extension("json.tmp").exists(),
            "nothing is left beside"
        );
        assert_eq!(
            held_by(&WindowState::at(Some(path.clone()))),
            Some(held),
            "the next launch reads it"
        );

        // What the file already says is not written again.
        std::fs::write(&path, "touched by hand").unwrap();
        state.write();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "touched by hand");
        // A change is.
        let next = moved(held, (240, 120), 2.0, NORMAL);
        state.inner.locked().track = Some(Track::of(next));
        state.write();
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()), Some(next));

        // A refused file is the default, and the next write replaces it.
        std::fs::write(&path, r#"{"v":2,"bounds":{}}"#).unwrap();
        let fresh = WindowState::at(Some(path.clone()));
        assert_eq!(held_by(&fresh), None);
        fresh.inner.locked().track = Some(Track::of(held));
        fresh.write();
        assert_eq!(parse(&std::fs::read_to_string(&path).unwrap()), Some(held));

        // No folder named: nothing is read, nothing written, nothing fails.
        let nowhere = WindowState::at(None);
        nowhere.inner.locked().track = Some(Track::of(held));
        nowhere.write();
        assert_eq!(nowhere.inner.locked().written, None);
    }
}
