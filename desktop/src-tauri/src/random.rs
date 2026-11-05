//! Randomness from the OS, as hex. No dependency: the platform's own
//! randomness is what every key on this machine already comes from. The API
//! token the node is minted (`sidecar.rs`) and a staged page's name
//! (`pages.rs`) both come from here.

use std::io::Read;

/// `bytes` random bytes as lowercase hex, or why the OS would not give them.
pub(crate) fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .map_err(|e| e.to_string())?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_of_the_asked_length_and_never_the_same_twice() {
        let a = random_hex(16).unwrap();
        let b = random_hex(16).unwrap();
        assert_eq!(a.len(), 32);
        assert!(a
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b);
        assert_eq!(random_hex(0).unwrap(), "");
    }
}
