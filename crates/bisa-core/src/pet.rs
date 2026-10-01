//! `Pet`: an animated companion, in Codex's package format, exactly. Local
//! state; no GEP kind. The record and its validation live here; the copying
//! lives in the store.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Largest sprite sheet we will copy in, in bytes.
pub const MAX_SPRITESHEET_BYTES: u64 = 8 * 1024 * 1024;

/// The manifest, with the format's own `camelCase` field names — the four
/// the platform always read, and what a drawn pack says beyond them: how
/// each state is animated (`animations`, one row of the sheet each, its
/// frames and their durations) and who the pet is (`x-bisa-pets`: a tagline,
/// an archetype, a mood). A pet with no `animations` is animated from the
/// sheet alone, as before. The manifest is a pack's own file, in a format
/// the platform did not write: a key it does not read is left alone, never
/// refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pet {
    /// Also the directory name — see [`validate_pet_id`].
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    /// Relative to the package directory. Resolved inside it, never outside.
    pub spritesheet_path: String,
    /// The pack's frame rate, when it names one; the durations below decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_animation: Option<String>,
    /// One row of the sheet per state named, with its frame count and each
    /// frame's duration — what makes one pet quick and another slow.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub animations: BTreeMap<String, PetAnimation>,
    /// Who the pet is, in the pack's own words.
    #[serde(
        default,
        rename = "x-bisa-pets",
        skip_serializing_if = "Option::is_none"
    )]
    pub flavour: Option<PetFlavour>,
    /// Where the pet comes from: the platform ships it, or a person installed
    /// it. A manifest on disk says nothing; the store stamps a built-in.
    #[serde(default, skip_serializing_if = "PetOrigin::is_local")]
    pub origin: PetOrigin,
}

/// One state's row of the sheet: which row, how many of its cells are frames,
/// and how long each frame is shown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PetAnimation {
    pub row: u32,
    pub frames: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frame_durations_ms: Vec<u32>,
}

/// The pack's own words about a pet; every field optional, the rendering
/// hints a drawing tool wrote are not read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PetFlavour {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archetype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
}

/// Where a pet comes from.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PetOrigin {
    /// Shipped inside the platform; listed always, installed and removed never.
    Catalog,
    /// A package a person installed into this workspace.
    #[default]
    Local,
}

impl PetOrigin {
    pub fn is_local(&self) -> bool {
        matches!(self, PetOrigin::Local)
    }
}

/// What a manifest's animations can get wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PetError {
    #[error("pet {id}: {why}")]
    Animation { id: String, why: String },
}

impl Pet {
    /// Every animation the manifest names is a state the sheet has, on that
    /// state's row, with one to `SHEET_COLS` frames and a duration per frame.
    pub fn validate_animations(&self) -> Result<(), PetError> {
        let bad = |why: String| PetError::Animation {
            id: self.id.clone(),
            why,
        };
        for (state, a) in &self.animations {
            let Some(row) = STATES.iter().position(|s| s == state) else {
                return Err(bad(format!("`{state}` is not one of the sheet's states")));
            };
            if a.row as usize != row {
                return Err(bad(format!(
                    "`{state}` is row {row} of the sheet, not {}",
                    a.row
                )));
            }
            if a.frames == 0 || a.frames > SHEET_COLS {
                return Err(bad(format!(
                    "`{state}` has {} frames; a row holds 1 to {SHEET_COLS}",
                    a.frames
                )));
            }
            if !a.frame_durations_ms.is_empty() {
                if a.frame_durations_ms.len() != a.frames as usize {
                    return Err(bad(format!(
                        "`{state}` names {} durations for {} frames",
                        a.frame_durations_ms.len(),
                        a.frames
                    )));
                }
                if a.frame_durations_ms.contains(&0) {
                    return Err(bad(format!("`{state}` has a frame shown for no time")));
                }
            }
        }
        Ok(())
    }
}

/// The nine animation rows, in the order the sheet lays them out. This is the
/// contract a pet was drawn against.
pub const STATES: [&str; 9] = [
    "idle",
    "running-right",
    "running-left",
    "waving",
    "jumping",
    "failed",
    "waiting",
    "running",
    "review",
];

pub const SHEET_COLS: u32 = 8;
pub const SHEET_ROWS: u32 = 9;
pub const CELL_WIDTH: u32 = 192;
pub const CELL_HEIGHT: u32 = 208;

/// Longest pet id. Generous because real ids are namespaced —
/// `bisa-pets.midnight-shipping.moonrice`.
pub const MAX_PET_ID_LEN: usize = 128;

/// Is this a pet id we are willing to make a directory out of? Lowercase ASCII
/// letters, digits, `-`, `_` and `.` — never two dots together, never leading,
/// never trailing.
pub fn validate_pet_id(id: &str) -> Result<(), crate::CoreError> {
    let invalid = || crate::CoreError::InvalidId {
        what: "pet".into(),
        value: id.to_string(),
    };
    if id.is_empty() || id.len() > MAX_PET_ID_LEN {
        return Err(invalid());
    }
    if id.starts_with('.') || id.ends_with('.') || id.contains("..") {
        return Err(invalid());
    }
    for b in id.bytes() {
        let ok =
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.';
        if !ok {
            return Err(invalid());
        }
    }
    Ok(())
}

/// Is this a WebP file, according to the bytes rather than the name?
pub fn is_webp(head: &[u8]) -> bool {
    head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP"
}

/// A WebP's canvas size from its extended header (`VP8X`: the width and
/// height minus one as 24-bit little-endian at bytes 24 and 27), or nothing
/// for a file that is not one — a sheet is judged against `SHEET_COLS ×
/// CELL_WIDTH` by `SHEET_ROWS × CELL_HEIGHT` before it ships.
pub fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !is_webp(bytes) || bytes.len() < 30 || &bytes[12..16] != b"VP8X" {
        return None;
    }
    let u24 = |at: usize| {
        u32::from(bytes[at]) | u32::from(bytes[at + 1]) << 8 | u32::from(bytes[at + 2]) << 16
    };
    Some((u24(24) + 1, u24(27) + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_ids_pass_and_traversal_fails() {
        assert!(validate_pet_id("bisa-pets.midnight-shipping.moonrice").is_ok());
        for bad in [
            "",
            "..",
            "a..b",
            ".hidden",
            "trailing.",
            "Upper",
            "a/b",
            "a\\b",
            "nul\0",
        ] {
            assert!(validate_pet_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_bytes_decide() {
        let mut head = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert!(is_webp(&head));
        head[8] = b'X';
        assert!(!is_webp(&head));
        assert!(!is_webp(b"RIFF"));
    }

    #[test]
    fn manifest_uses_camel_case() {
        let p = Pet {
            id: "x.y".into(),
            display_name: "Moonrice".into(),
            description: String::new(),
            spritesheet_path: "spritesheet.webp".into(),
            fps: None,
            default_animation: None,
            animations: BTreeMap::new(),
            flavour: None,
            origin: PetOrigin::Local,
        };
        let json = serde_json::to_value(&p).unwrap();
        assert!(json.get("displayName").is_some());
        assert!(json.get("spritesheetPath").is_some());
        assert!(
            json.get("origin").is_none(),
            "a local pet says nothing of its origin: {json}"
        );
        assert!(json.get("animations").is_none(), "no animations, no key");
        assert_eq!(STATES.len() as u32, SHEET_ROWS);
    }

    #[test]
    fn a_drawn_packs_manifest_reads_whole_and_round_trips_its_own_names() {
        let text = r#"{
          "id": "bisa-pets.p.x", "displayName": "X", "description": "d", "spritesheetPath": "spritesheet.webp",
          "frame": {"width": 192, "height": 208}, "fps": 8, "defaultAnimation": "idle",
          "animations": {"idle": {"row": 0, "frames": 2, "frameDurationsMs": [500, 700]}, "review": {"row": 8, "frames": 1, "frameDurationsMs": [900]}},
          "x-bisa-pets": {"pack": "p", "slug": "x", "tagline": "Hi.", "archetype": "ghost", "personality": "calm", "mood": "serene", "render": {"fitScale": 1.2}}
        }"#;
        let p: Pet = serde_json::from_str(text).unwrap();
        assert_eq!(p.fps, Some(8));
        assert_eq!(p.animations["idle"].frame_durations_ms, vec![500, 700]);
        assert_eq!(p.flavour.as_ref().unwrap().tagline.as_deref(), Some("Hi."));
        assert_eq!(p.origin, PetOrigin::Local);
        p.validate_animations().unwrap();
        let json = serde_json::to_value(&p).unwrap();
        assert!(
            json.get("x-bisa-pets").is_some(),
            "the pack's key, as it wrote it"
        );
        assert!(json["animations"]["idle"].get("frameDurationsMs").is_some());
        assert!(
            json.get("frame").is_none(),
            "what the platform does not read it does not keep"
        );
        let back: Pet = serde_json::from_value(json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn a_bad_animation_is_named() {
        let base = || Pet {
            id: "x".into(),
            display_name: "X".into(),
            description: String::new(),
            spritesheet_path: "s.webp".into(),
            fps: None,
            default_animation: None,
            animations: BTreeMap::new(),
            flavour: None,
            origin: PetOrigin::Local,
        };
        let anim = |row: u32, frames: u32, d: &[u32]| PetAnimation {
            row,
            frames,
            frame_durations_ms: d.to_vec(),
        };
        let cases: [(&str, PetAnimation, &str); 5] = [
            (
                "dancing",
                anim(0, 1, &[100]),
                "not one of the sheet's states",
            ),
            ("idle", anim(3, 1, &[100]), "row 0"),
            ("idle", anim(0, 9, &[]), "1 to 8"),
            ("idle", anim(0, 2, &[100]), "1 durations for 2 frames"),
            ("idle", anim(0, 1, &[0]), "no time"),
        ];
        for (state, a, needle) in cases {
            let mut p = base();
            p.animations.insert(state.into(), a);
            let err = p.validate_animations().unwrap_err().to_string();
            assert!(err.contains(needle), "{state}: {err}");
        }
        let mut ok = base();
        ok.animations.insert("idle".into(), anim(0, 3, &[]));
        ok.validate_animations().unwrap();
    }

    #[test]
    fn a_webps_canvas_is_read_from_its_extended_header() {
        // RIFF size WEBP VP8X size flags(4) width-1 (24-bit LE) height-1 (24-bit LE)
        let mut b = b"RIFF\x00\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00".to_vec();
        b.extend_from_slice(&[0xff, 0x05, 0x00]); // 1535 → 1536
        b.extend_from_slice(&[0x4f, 0x07, 0x00]); // 1871 → 1872
        assert_eq!(webp_dimensions(&b), Some((1536, 1872)));
        assert_eq!(webp_dimensions(b"RIFF\x00\x00\x00\x00WEBPVP8 "), None);
        assert_eq!(webp_dimensions(b"\x89PNG"), None);
    }
}
