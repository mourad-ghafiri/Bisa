//! The menu bar icon's pixels: the mark the shell ships, tinted to the menu
//! bar's ink, with a status dot painted on it — or not.
//!
//! The mark is `icons/tray/36x36.png`, `just tray-icon`'s rasterisation of
//! `logo/tray-mark.svg`: the split B and its rail in one ink, no squircle.
//! 36 px is 18 pt at 2× — the height every status item is drawn at
//! (tauri-apps/tray-icon, `platform_impl/macos`: `icon_height = 18.0`).
//! The shell keeps only its alpha: every pixel takes the ink — black on a
//! light menu bar, white on a dark one, from the system's appearance
//! (`platform::Platform::menu_bar_ink`; the window's theme is the page's,
//! not the menu bar's) — so the mark reads like a native item. A *template*
//! image would do that by itself and lose the dot's colour (templates are
//! alpha alone), which is why the tinting is done here.
//!
//! The dot sits at the lower right, a sixth of the width across, with a
//! clear ring around it so it never touches the mark. Everything here is a
//! pure function over bytes; `mod tests` paints a synthetic mark.

use super::report::Rgb;
use std::sync::LazyLock;
use tauri::image::Image;
use tauri::Theme;

/// The shipped mark, as `just tray-icon` wrote it.
static MARK_PNG: &[u8] = include_bytes!("../../icons/tray/36x36.png");

/// The ring between the dot and the mark, in pixels beyond the dot's radius.
const GAP: f32 = 1.5;

/// Straight (not premultiplied) RGBA, row-major from the top — what
/// `Image::new_owned` takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Bitmap {
    /// One pixel's four bytes.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[at],
            self.rgba[at + 1],
            self.rgba[at + 2],
            self.rgba[at + 3],
        ]
    }
}

impl From<&Bitmap> for Image<'static> {
    fn from(b: &Bitmap) -> Self {
        Image::new_owned(b.rgba.clone(), b.width, b.height)
    }
}

/// The mark, decoded once. A PNG the shell ships is a PNG; a build that
/// bundled something else fails here at the first paint, loudly.
pub fn mark() -> &'static Bitmap {
    static MARK: LazyLock<Bitmap> = LazyLock::new(|| {
        let image = Image::from_bytes(MARK_PNG).expect("icons/tray/36x36.png is a PNG");
        Bitmap {
            width: image.width(),
            height: image.height(),
            rgba: image.rgba().to_vec(),
        }
    });
    &MARK
}

/// The menu bar's ink: what the mark is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// A light menu bar: black.
    Dark,
    /// A dark menu bar: white.
    Light,
}

impl Ink {
    /// The ink for a window theme, where the OS does not say what the menu
    /// bar's is — a dark theme means a dark menu bar, so the mark goes light.
    pub fn for_theme(theme: Theme) -> Ink {
        match theme {
            Theme::Dark => Ink::Light,
            _ => Ink::Dark,
        }
    }

    fn rgb(self) -> Rgb {
        match self {
            Ink::Dark => [0, 0, 0],
            Ink::Light => [255, 255, 255],
        }
    }
}

/// How much of a pixel centred `distance` from the disc's centre the disc
/// of `radius` covers: a half-pixel edge, so the dot is round and not a stair.
fn coverage(distance: f32, radius: f32) -> f32 {
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

/// The mark in `ink`, with `dot` painted at the lower right when there is one.
pub fn compose(mark: &Bitmap, ink: Ink, dot: Option<Rgb>) -> Bitmap {
    let mut out = mark.clone();
    let rgb = ink.rgb();
    for px in out.rgba.as_chunks_mut::<4>().0 {
        px[..3].copy_from_slice(&rgb);
    }
    let Some(colour) = dot else {
        return out;
    };
    let radius = mark.width as f32 / 6.0;
    let cx = mark.width as f32 - radius - 1.0;
    let cy = mark.height as f32 - radius - 1.0;
    for y in 0..mark.height {
        for x in 0..mark.width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            let ring = coverage(distance, radius + GAP);
            if ring <= 0.0 {
                continue;
            }
            let at = ((y * mark.width + x) * 4) as usize;
            // The ring clears the mark under and around the dot.
            let cleared = (out.rgba[at + 3] as f32 * (1.0 - ring)).round() as u8;
            let fill = coverage(distance, radius);
            if fill > 0.0 {
                out.rgba[at..at + 3].copy_from_slice(&colour);
                out.rgba[at + 3] = cleared.max((fill * 255.0).round() as u8);
            } else {
                out.rgba[at + 3] = cleared;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::report::GREEN;

    /// A 24×24 mark: a grey square filling everything but a one-pixel border.
    fn synthetic() -> Bitmap {
        let (w, h) = (24u32, 24u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let at = ((y * w + x) * 4) as usize;
                rgba[at..at + 4].copy_from_slice(&[90, 90, 90, 200]);
            }
        }
        Bitmap {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn the_ink_replaces_the_colour_and_keeps_the_alpha() {
        let mark = synthetic();
        let light = compose(&mark, Ink::Light, None);
        assert_eq!(light.pixel(5, 5), [255, 255, 255, 200]);
        assert_eq!(
            light.pixel(0, 0),
            [255, 255, 255, 0],
            "a clear pixel stays clear"
        );
        let dark = compose(&mark, Ink::Dark, None);
        assert_eq!(dark.pixel(5, 5), [0, 0, 0, 200]);
        assert_eq!(Ink::for_theme(Theme::Dark), Ink::Light);
        assert_eq!(Ink::for_theme(Theme::Light), Ink::Dark);
    }

    #[test]
    fn a_dot_is_opaque_in_its_colour_with_a_clear_ring_and_the_rest_untouched() {
        let mark = synthetic();
        let dotted = compose(&mark, Ink::Dark, Some(GREEN));
        // radius 4, centre (19, 19): the centre pixel is the colour, opaque.
        assert_eq!(dotted.pixel(18, 18), [52, 199, 89, 255]);
        assert_eq!(dotted.pixel(19, 19), [52, 199, 89, 255]);
        // Just outside the dot, inside the ring: cleared.
        assert_eq!(
            dotted.pixel(14, 19)[3],
            0,
            "the ring separates the dot from the mark"
        );
        assert!(
            dotted.pixel(13, 19)[3] < 200,
            "and fades out over one pixel"
        );
        // Well away from the dot: the mark in its ink, as without a dot.
        assert_eq!(dotted.pixel(5, 5), [0, 0, 0, 200]);
        assert_eq!(dotted.pixel(1, 22), [0, 0, 0, 200]);
        let calm = compose(&mark, Ink::Dark, None);
        assert_eq!(
            calm.pixel(19, 19),
            [0, 0, 0, 200],
            "no dot leaves the corner as the mark had it"
        );
    }

    #[test]
    fn the_edge_is_soft_and_the_coverage_is_bounded() {
        assert_eq!(coverage(0.0, 4.0), 1.0);
        assert_eq!(
            coverage(4.0, 4.0),
            0.5,
            "a pixel centred on the rim is half covered"
        );
        assert_eq!(coverage(9.0, 4.0), 0.0);
    }

    #[test]
    fn the_shipped_mark_is_the_36_pixel_square_just_tray_icon_writes() {
        let mark = mark();
        assert_eq!((mark.width, mark.height), (36, 36));
        assert_eq!(mark.rgba.len(), 36 * 36 * 4);
        assert!(
            mark.rgba.as_chunks::<4>().0.iter().any(|px| px[3] > 0),
            "the mark has ink somewhere"
        );
        let image: Image<'static> = (&compose(mark, Ink::Light, None)).into();
        assert_eq!((image.width(), image.height()), (36, 36));
    }
}
