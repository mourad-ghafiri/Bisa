//! Public keys, read from their `.pub` line — the algorithm, the comment and
//! the SHA-256 fingerprint OpenSSH prints, computed here from the base64
//! blob so no program runs and no private key is ever opened. Pure.

use base64::Engine as _;
use sha2::Digest as _;
use std::path::PathBuf;

/// One public key on disk.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct PublicKey {
    /// The key pair's name — the private key's file name, without `.pub`.
    pub name: String,
    /// The private key's path, as `-i` and `IdentityFile` want it; never read.
    pub path: PathBuf,
    /// `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`, `sk-ssh-ed25519@openssh.com` …
    pub algorithm: String,
    pub comment: String,
    /// `SHA256:…`, as `ssh-keygen -lf` and `ssh-add -l` print it.
    pub fingerprint: String,
    /// The `.pub` line itself, for *Copy public key*.
    pub public_line: String,
}

/// `SHA256:<base64 without padding>` over the decoded blob — OpenSSH's own
/// fingerprint format.
pub fn fingerprint_sha256(blob: &[u8]) -> String {
    let digest = sha2::Sha256::digest(blob);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}

/// Read one `.pub` line — `<algorithm> <base64 blob> [comment]`. `None` for a
/// line that is not one (a comment, an empty file, a blob that is not base64).
pub fn parse_public_key(name: &str, private_path: PathBuf, line: &str) -> Option<PublicKey> {
    let line = line.lines().find(|l| !l.trim().is_empty())?.trim();
    let mut words = line.splitn(3, char::is_whitespace);
    let algorithm = words.next()?;
    let blob64 = words.next()?;
    let comment = words.next().unwrap_or_default().trim();
    if !is_key_type(algorithm) {
        return None;
    }
    let blob = base64::engine::general_purpose::STANDARD
        .decode(blob64)
        .ok()?;
    if !blob_is_of(&blob, algorithm) {
        return None;
    }
    Some(PublicKey {
        name: name.to_string(),
        path: private_path,
        algorithm: algorithm.to_string(),
        comment: comment.to_string(),
        fingerprint: fingerprint_sha256(&blob),
        public_line: line.to_string(),
    })
}

/// The first word of a public key line OpenSSH writes: `ssh-ed25519`,
/// `ssh-rsa`, `ecdsa-sha2-*`, and the hardware-backed `sk-ssh-ed25519@…` and
/// `sk-ecdsa-sha2-*@…`.
fn is_key_type(word: &str) -> bool {
    let word = word.strip_prefix("sk-").unwrap_or(word);
    word.starts_with("ssh-") || word.starts_with("ecdsa-")
}

/// Whether a key blob says it is of `algorithm`: every OpenSSH public key
/// opens with its own type as a length-prefixed string, the same word the
/// line starts with. A line whose two halves disagree is a damaged file, not
/// a key to show a fingerprint of.
fn blob_is_of(blob: &[u8], algorithm: &str) -> bool {
    let Some((len, rest)) = blob.split_first_chunk::<4>() else {
        return false;
    };
    let len = u32::from_be_bytes(*len) as usize;
    rest.get(..len)
        .is_some_and(|name| name == algorithm.as_bytes())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A synthetic ed25519 public key: the blob is the wire form of the
    /// algorithm name plus 32 zero bytes — not a key anybody holds.
    pub(crate) fn synthetic_pub_line(comment: &str) -> String {
        let mut blob = Vec::new();
        let name = b"ssh-ed25519";
        blob.extend_from_slice(&(name.len() as u32).to_be_bytes());
        blob.extend_from_slice(name);
        blob.extend_from_slice(&32u32.to_be_bytes());
        blob.extend_from_slice(&[0u8; 32]);
        format!(
            "ssh-ed25519 {} {comment}",
            base64::engine::general_purpose::STANDARD.encode(&blob)
        )
    }

    #[test]
    fn a_public_line_reads_into_its_parts_and_the_fingerprint_is_openssh_shaped() {
        let line = synthetic_pub_line("ada@laptop");
        let key = parse_public_key(
            "id_ed25519_acme",
            PathBuf::from("/home/ada/.ssh/id_ed25519_acme"),
            &format!("\n{line}\n"),
        )
        .unwrap();
        assert_eq!(key.name, "id_ed25519_acme");
        assert_eq!(key.algorithm, "ssh-ed25519");
        assert_eq!(key.comment, "ada@laptop");
        assert!(
            key.fingerprint.starts_with("SHA256:"),
            "{}",
            key.fingerprint
        );
        assert_eq!(
            key.fingerprint.len(),
            7 + 43,
            "43 base64 characters, no padding"
        );
        assert!(!key.fingerprint.ends_with('='));
        assert_eq!(key.public_line, line);
        let same = parse_public_key(
            "other",
            PathBuf::from("/x"),
            &synthetic_pub_line("different comment"),
        )
        .unwrap();
        assert_eq!(
            same.fingerprint, key.fingerprint,
            "the fingerprint is the blob's, not the comment's"
        );
    }

    #[test]
    fn what_is_not_a_public_key_line_is_none() {
        assert!(parse_public_key("k", PathBuf::from("/k"), "").is_none());
        assert!(parse_public_key("k", PathBuf::from("/k"), "# just a comment").is_none());
        assert!(parse_public_key("k", PathBuf::from("/k"), "ssh-ed25519 not*base64 c").is_none());
        assert!(
            parse_public_key(
                "k",
                PathBuf::from("/k"),
                "-----BEGIN OPENSSH PRIVATE KEY-----"
            )
            .is_none(),
            "a private key block is never a public key"
        );
        let no_comment =
            parse_public_key("k", PathBuf::from("/k"), synthetic_pub_line("").trim()).unwrap();
        assert_eq!(no_comment.comment, "");
    }

    /// A synthetic public line of any type: the type as the blob's opening
    /// string, then zero bytes — not a key anybody holds.
    fn synthetic_line_of(algorithm: &str, claims: &str) -> String {
        let mut blob = Vec::new();
        blob.extend_from_slice(&(claims.len() as u32).to_be_bytes());
        blob.extend_from_slice(claims.as_bytes());
        blob.extend_from_slice(&[0u8; 40]);
        format!(
            "{algorithm} {} ada@example",
            base64::engine::general_purpose::STANDARD.encode(&blob)
        )
    }

    #[test]
    fn every_openssh_key_type_is_read_and_a_line_whose_halves_disagree_is_not() {
        for algorithm in [
            "ssh-ed25519",
            "ssh-rsa",
            "ecdsa-sha2-nistp256",
            "sk-ssh-ed25519@openssh.com",
            "sk-ecdsa-sha2-nistp256@openssh.com",
        ] {
            let key = parse_public_key(
                "k",
                PathBuf::from("/k"),
                &synthetic_line_of(algorithm, algorithm),
            )
            .unwrap_or_else(|| panic!("{algorithm} is a key type"));
            assert_eq!(key.algorithm, algorithm);
            assert!(
                key.fingerprint.starts_with("SHA256:"),
                "{}",
                key.fingerprint
            );
        }
        // The word says one type and the bytes another: a damaged file.
        let crossed = synthetic_line_of("ssh-rsa", "ssh-ed25519");
        assert!(parse_public_key("k", PathBuf::from("/k"), &crossed).is_none());
        // Valid base64 that is no key blob at all: too short, or a length
        // that runs past the end.
        for blob in ["AAAA", "AAAAIHNzaA==", "////"] {
            let line = format!("ssh-ed25519 {blob} c");
            assert!(
                parse_public_key("k", PathBuf::from("/k"), &line).is_none(),
                "{blob}"
            );
        }
        // A word that is no key type, however well-formed the rest.
        let other = synthetic_line_of("pgp-rsa", "pgp-rsa");
        assert!(parse_public_key("k", PathBuf::from("/k"), &other).is_none());
    }
}
