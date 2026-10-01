//! The faces a guest holds and the profile it carries (14-collaboration): a
//! **port**, since a guest session has no workspace of its own — the node
//! that embeds it gives it one over its attachment store, a mobile client
//! keeps one in memory. A face a host sends lands here under its content
//! hash, so whatever serves `GET /attachments/{sha}` on this node draws it;
//! the owner's own label and face are read here to be said to every host.

use bisa_core::sync::Locked;
use bisa_core::AttachmentRef;
use std::collections::HashMap;
use std::sync::Mutex;

/// This person's profile as their own node has it: what every host is told.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnerProfile {
    pub label: Option<String>,
    pub photo: Option<AttachmentRef>,
}

/// Where a guest keeps faces and reads its own profile.
pub trait FaceStore: Send + Sync {
    /// The bytes of a face this node holds, by content hash.
    fn bytes(&self, sha256: &str) -> Option<Vec<u8>>;
    /// Keep a face the host sent, verified by the caller, under its hash.
    fn hold(&self, sha256: &str, bytes: &[u8]) -> Result<(), String>;
    /// This person's own label and face.
    fn profile(&self) -> OwnerProfile;
}

/// A face store in memory — a mobile client's, and the tests'.
#[derive(Default)]
pub struct MemoryFaces {
    faces: Mutex<HashMap<String, Vec<u8>>>,
    profile: Mutex<OwnerProfile>,
}

impl MemoryFaces {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set what this person says of themselves.
    pub fn set_profile(&self, profile: OwnerProfile) {
        *self.profile.locked() = profile;
    }

    /// How many faces are held.
    pub fn count(&self) -> usize {
        self.faces.locked().len()
    }
}

impl FaceStore for MemoryFaces {
    fn bytes(&self, sha256: &str) -> Option<Vec<u8>> {
        self.faces.locked().get(sha256).cloned()
    }

    fn hold(&self, sha256: &str, bytes: &[u8]) -> Result<(), String> {
        self.faces
            .locked()
            .insert(sha256.to_string(), bytes.to_vec());
        Ok(())
    }

    fn profile(&self) -> OwnerProfile {
        self.profile.locked().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_store_holds_faces_by_hash_and_says_the_profile_it_was_given() {
        let faces = MemoryFaces::new();
        assert_eq!(faces.bytes("a"), None);
        faces.hold("a", b"png").unwrap();
        assert_eq!(faces.bytes("a").as_deref(), Some(b"png".as_slice()));
        assert_eq!(faces.count(), 1);
        assert_eq!(faces.profile(), OwnerProfile::default());
        faces.set_profile(OwnerProfile {
            label: Some("Bob".into()),
            photo: None,
        });
        assert_eq!(faces.profile().label.as_deref(), Some("Bob"));
    }
}
