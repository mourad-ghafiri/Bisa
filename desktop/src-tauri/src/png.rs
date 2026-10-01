//! Bitmap data as PNG bytes, on macOS — the one encoder the shell has, for
//! the browser's snapshot (`browser.rs`) and the clipboard's picture
//! (`pasteboard.rs`) alike: whatever AppKit can decode (a TIFF, a JPEG…)
//! comes out as the PNG the node's attachment store and a checkout keep.

#[cfg(target_os = "macos")]
pub fn from_data(data: &objc2_foundation::NSData) -> Option<Vec<u8>> {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey};
    use objc2_foundation::NSDictionary;

    let rep = NSBitmapImageRep::imageRepWithData(data)?;
    let props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> = NSDictionary::new();
    let png =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
    Some(png.to_vec())
}

/// The eight bytes every PNG starts with.
pub const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Whether `bytes` start as a PNG does.
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_is_known_by_its_signature_alone() {
        let mut png = SIGNATURE.to_vec();
        png.extend_from_slice(b"IHDR-and-whatever-follows");
        assert!(is_png(&png));
        assert!(!is_png(b"GIF89a"), "another picture format is not a PNG");
        assert!(!is_png(&SIGNATURE[..4]), "a torn signature is not one");
        assert!(!is_png(b""));
    }
}
