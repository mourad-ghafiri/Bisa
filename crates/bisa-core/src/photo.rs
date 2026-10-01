//! A photo as the platform keeps one (ide/14 §Photos): a picture the desktop
//! scaled once, held in the attachment store, named by an [`AttachmentRef`]
//! and checked the same way wherever it is attached — a project's, a
//! group's, an agent's or a team's **picture**, a person's **face**. The two
//! profiles differ in one thing: how big they may be. A picture is drawn
//! under 32 px and stays on this machine, so 512 KiB is an honest bound for
//! one set by another client. A face travels — it rides the collaboration
//! control channel, gift-wrapped once per recipient — so it is bounded at
//! 16 KiB, a 96 px JPEG, and the desktop encodes it to fit.
//!
//! The check is pure and lives here because three layers ask the same four
//! questions of a photo: the node before it attaches one, the host before
//! it keeps a member's face, the guest before it holds one a host sent.

use crate::attachment::{AttachmentRef, MAX_FACE_BYTES, MAX_PHOTO_BYTES};

/// Which photo a ref is, and so how big it may be and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhotoProfile {
    /// A project's, a group's, an agent's, a team's: 256 px, PNG, on this machine.
    Picture,
    /// A person's: 96 px, JPEG, small enough to travel with their profile.
    Face,
}

impl PhotoProfile {
    /// The square the desktop scales the picture to, in pixels.
    pub const fn edge(self) -> u32 {
        match self {
            PhotoProfile::Picture => 256,
            PhotoProfile::Face => 96,
        }
    }

    /// The most bytes the stored photo may hold.
    pub const fn max_bytes(self) -> u64 {
        match self {
            PhotoProfile::Picture => MAX_PHOTO_BYTES,
            PhotoProfile::Face => MAX_FACE_BYTES,
        }
    }

    /// The word a refusal names the profile by.
    pub const fn word(self) -> &'static str {
        match self {
            PhotoProfile::Picture => "photo",
            PhotoProfile::Face => "face",
        }
    }
}

/// Why a ref is not a photo of the profile asked.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PhotoRefusal {
    #[error("{word}.sha256 is not a content hash")]
    NotAHash { word: &'static str },
    #[error("{word} is not on this machine — upload it first")]
    NotHeld { word: &'static str },
    #[error("{word} is not a picture")]
    NotAPicture { word: &'static str },
    #[error("{word} is {bytes} bytes; the limit is {max} — the desktop scales a {word} to a {edge} px square")]
    TooLarge {
        word: &'static str,
        bytes: u64,
        max: u64,
        edge: u32,
    },
}

/// The image type a file's own header says it is — PNG, JPEG, GIF or WebP —
/// or `None`. A short, closed list checked against the bytes: an HTML file
/// called `photo.png` is not a picture here.
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const GIF87: &[u8] = b"GIF87a";
    const GIF89: &[u8] = b"GIF89a";
    if bytes.starts_with(PNG) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(GIF87) || bytes.starts_with(GIF89) {
        return Some("image/gif");
    }
    // RIFF....WEBP
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Whether `r`, whose bytes are `bytes` when this machine holds them, is a
/// photo of `profile`: a content hash, held, a picture by its header, and
/// within the profile's cap — each refusal by name, in the order a caller
/// can act on.
pub fn check_photo(
    bytes: Option<&[u8]>,
    r: &AttachmentRef,
    profile: PhotoProfile,
) -> Result<(), PhotoRefusal> {
    let word = profile.word();
    if !AttachmentRef::is_valid_hash(&r.sha256) {
        return Err(PhotoRefusal::NotAHash { word });
    }
    let Some(bytes) = bytes else {
        return Err(PhotoRefusal::NotHeld { word });
    };
    if image_type(bytes).is_none() {
        return Err(PhotoRefusal::NotAPicture { word });
    }
    if bytes.len() as u64 > profile.max_bytes() {
        return Err(PhotoRefusal::TooLarge {
            word,
            bytes: bytes.len() as u64,
            max: profile.max_bytes(),
            edge: profile.edge(),
        });
    }
    Ok(())
}

/// Whether bytes are a face as a host keeps and hands one: a picture within
/// [`MAX_FACE_BYTES`] — what a `WantFace` is answered with, and nothing else.
pub fn is_face(bytes: &[u8]) -> bool {
    image_type(bytes).is_some() && bytes.len() as u64 <= MAX_FACE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(len: usize) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.resize(len, 0);
        b
    }

    fn r(sha: &str) -> AttachmentRef {
        AttachmentRef {
            sha256: sha.into(),
            name: "a.png".into(),
            mime: "image/png".into(),
            size: 0,
        }
    }

    #[test]
    fn the_header_names_the_picture_and_nothing_else_is_one() {
        assert_eq!(image_type(&png(16)), Some("image/png"));
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"GIF89a...."), Some("image/gif"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"<html>"),
            None,
            "a file called photo.png is not a picture"
        );
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn a_photo_is_refused_by_name_in_the_order_a_caller_can_act_on() {
        let good = "a".repeat(64);
        assert_eq!(
            check_photo(Some(&png(100)), &r("nope"), PhotoProfile::Picture),
            Err(PhotoRefusal::NotAHash { word: "photo" })
        );
        assert_eq!(
            check_photo(None, &r(&good), PhotoProfile::Face),
            Err(PhotoRefusal::NotHeld { word: "face" })
        );
        assert_eq!(
            check_photo(Some(b"<html>"), &r(&good), PhotoProfile::Picture),
            Err(PhotoRefusal::NotAPicture { word: "photo" })
        );
        assert!(check_photo(Some(&png(100)), &r(&good), PhotoProfile::Picture).is_ok());
        assert!(
            check_photo(
                Some(&png(MAX_FACE_BYTES as usize)),
                &r(&good),
                PhotoProfile::Face
            )
            .is_ok(),
            "the cap is inclusive"
        );
        let over = check_photo(
            Some(&png(MAX_FACE_BYTES as usize + 1)),
            &r(&good),
            PhotoProfile::Face,
        );
        assert_eq!(
            over,
            Err(PhotoRefusal::TooLarge {
                word: "face",
                bytes: MAX_FACE_BYTES + 1,
                max: MAX_FACE_BYTES,
                edge: 96
            })
        );
        assert!(
            over.unwrap_err().to_string().contains("96 px square"),
            "the refusal says how the desktop would have fit it"
        );
        assert!(check_photo(
            Some(&png(MAX_PHOTO_BYTES as usize)),
            &r(&good),
            PhotoProfile::Picture
        )
        .is_ok());
        assert!(check_photo(
            Some(&png(MAX_PHOTO_BYTES as usize + 1)),
            &r(&good),
            PhotoProfile::Picture
        )
        .is_err());
    }

    #[test]
    fn a_face_is_a_small_picture_and_the_profiles_differ_in_their_bounds() {
        assert!(is_face(&png(1000)));
        assert!(
            !is_face(&png(MAX_FACE_BYTES as usize + 1)),
            "a picture over the face cap is not handed out"
        );
        assert!(!is_face(b"not a picture at all"));
        assert_eq!(PhotoProfile::Picture.edge(), 256);
        assert_eq!(PhotoProfile::Face.edge(), 96);
        assert!(PhotoProfile::Face.max_bytes() < PhotoProfile::Picture.max_bytes());
        assert_eq!(MAX_FACE_BYTES, 16 * 1024);
    }
}
