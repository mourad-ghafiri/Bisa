//! Identity: key storage, NIP-49 backup, and NIP-OA owner attestation.
//!
//! Key material flows through a [`KeyStore`] abstraction so tests never touch
//! the OS keyring and headless servers can fall back to a 0600 file.
//!
//! **Keys are created exclusively.** The file store opens with `create_new`
//! and mode `0600` inside a `0700` directory; a second writer racing to mint
//! the same name gets an error rather than a silently overwritten secret. A
//! deliberate replacement — importing a backup over an existing key — is a
//! separate call, [`KeyStore::replace`], so it cannot happen by accident.
//!
//! **The file store is the default; the OS keyring is opt-in.** A command-line
//! tool asking the macOS Keychain to hold a key makes the OS ask the person
//! for their login password — a prompt that reads as "this app wants my
//! passwords" even though the entry is the workspace's own key and nothing
//! else is touched. So `Workspace::open` keeps keys as `0600` files under
//! `identity/` unless `BISA_KEYSTORE=keyring` says otherwise, and no
//! keyring API is called at all in the default configuration. When the
//! keyring *is* chosen and fails, the failure is reported with the remedy —
//! never silently turned into a file nobody asked for.
//!
//! The attestation is a NIP-OA-style owner attestation: the 4-element
//! `["auth", <owner-pubkey-hex>, <conditions>, <sig-hex>]` tag whose signing
//! preimage is `nostr:agent-auth:<agent-pubkey-hex>:<conditions>`, signed
//! message `SHA256(preimage)`.

use crate::error::StoreError;
use nostr::event::{Event, Tag};
use nostr::key::Keys;
use nostr::nips::nip19::{FromBech32, ToBech32};
use nostr::nips::nip49::{EncryptedSecretKey, KeySecurity};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

pub(crate) const OWNER_KEY_NAME: &str = "owner";

/// The environment variable that chooses the key store: `file` (the default —
/// `0600` files under `identity/`) or `keyring` (the OS keyring, opt-in).
pub const KEYSTORE_ENV: &str = "BISA_KEYSTORE";

/// Where secrets live. Parsed from [`KEYSTORE_ENV`]; anything but `keyring`
/// is the file store, so a typo cannot summon a Keychain prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStoreChoice {
    File,
    Keyring,
}

impl KeyStoreChoice {
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(v) if v.eq_ignore_ascii_case("keyring") => KeyStoreChoice::Keyring,
            _ => KeyStoreChoice::File,
        }
    }

    /// The choice the environment makes.
    // LCOV_EXCL_START: reads the process environment; `parse` holds the rule, the_file_store_is_the_default_and_the_keyring_is_opt_in its words
    pub fn from_env() -> Self {
        Self::parse(std::env::var(KEYSTORE_ENV).ok().as_deref())
    }
    // LCOV_EXCL_STOP
}

/// A hook start's secret, by its listener: `hook:<host kind>:<host id>:<step>`
/// — colons only, never the key's `/`, so the file keystore's name stays one
/// file.
fn hook_secret_name(key: &bisa_core::ListenerKey) -> String {
    format!("hook:{}:{}:{}", key.host.kind(), key.host.id(), key.step)
}

const KEYRING_SERVICE: &str = "bisa";
/// NIP-OA domain separator, verbatim from the spec.
const AUTH_DOMAIN: &str = "nostr:agent-auth:";

// ---------------------------------------------------------------------------
// KeyStore abstraction
// ---------------------------------------------------------------------------

/// Storage for secret keys, addressed by logical name ("owner",
/// "agent:<pubkey>", "hook:<host kind>:<host id>:<step>"). Values are 64-char
/// hex secret keys.
pub trait KeyStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<String>, StoreError>;
    /// Store a **new** secret. An existing name is an error: minting must
    /// never clobber.
    fn set(&self, name: &str, secret_hex: &str) -> Result<(), StoreError>;
    /// Deliberately overwrite — the import path.
    fn replace(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        self.delete(name)?;
        self.set(name, secret_hex)
    }
    fn delete(&self, name: &str) -> Result<(), StoreError>;
    /// The file a name is kept in, when a file is where it lives — what a
    /// refusal of the owner's key can name. `None` for a keyring and for a
    /// store in memory.
    fn path_of(&self, _name: &str) -> Option<PathBuf> {
        None
    }
    /// Whether values live in the OS keyring rather than in files — what a
    /// route may say about a secret: where it is, never what it is.
    fn uses_keyring(&self) -> bool {
        false
    }
}

/// OS keyring backend (service "bisa").
pub struct KeyringStore;

// LCOV_EXCL_START: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
impl KeyStore for KeyringStore {
    fn uses_keyring(&self) -> bool {
        true
    }

    fn get(&self, name: &str) -> Result<Option<String>, StoreError> {
        match keyring::Entry::new(KEYRING_SERVICE, name) {
            Ok(entry) => match entry.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(StoreError::KeyStore(e.to_string())),
            },
            Err(e) => Err(StoreError::KeyStore(e.to_string())),
        }
    }

    fn set(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        if self.get(name)?.is_some() {
            return Err(StoreError::KeyStore(format!(
                "key {name:?} already exists; a new key never overwrites one"
            )));
        }
        keyring::Entry::new(KEYRING_SERVICE, name)
            .and_then(|entry| entry.set_password(secret_hex))
            .map_err(|e| StoreError::KeyStore(e.to_string()))
    }

    fn replace(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        keyring::Entry::new(KEYRING_SERVICE, name)
            .and_then(|entry| entry.set_password(secret_hex))
            .map_err(|e| StoreError::KeyStore(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        match keyring::Entry::new(KEYRING_SERVICE, name).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(StoreError::KeyStore(e.to_string())),
        }
    }
}
// LCOV_EXCL_STOP

/// Create `dir` if absent and, on unix, make it private (`0700`). Applied to
/// `identity/` and `run/`, the two directories that hold secrets.
pub(crate) fn ensure_private_dir(dir: &Path) -> Result<(), StoreError> {
    std::fs::create_dir_all(dir).map_err(|e| StoreError::io(dir.display().to_string(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| StoreError::io(dir.display().to_string(), e))?;
    }
    Ok(())
}

/// Remove the temporary a secret was written through. A temporary that
/// cannot be removed is a stray file under `identity/`, said at `debug`, and
/// never the reason the write fails: the key itself is in place or not.
fn forget_temporary(tmp: &Path) {
    if let Err(e) = std::fs::remove_file(tmp) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::debug!(target: "bisa_store", path = %tmp.display(), "a key's temporary was not removed: {e}");
        } // LCOV_EXCL_LINE: a temporary already gone is nothing to say
    }
}

/// Write `bytes` to a **new** file with mode `0600`. Fails if the file exists.
///
/// Whole or not there: the bytes land in a temporary beside the file, are
/// synced, and reach the name by one hard link — so a crash between the
/// create and the write can never leave an empty key under the name every
/// later open reads, and a key already there is never overwritten (the link
/// refuses with `AlreadyExists`, as the plain create did).
pub(crate) fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent)?;
    } // LCOV_EXCL_LINE: a key's path has a parent: every name the store joins stands under `identity/`
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".tmp.{}", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    let written = (|| -> std::io::Result<()> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        std::io::Write::write_all(&mut f, bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        forget_temporary(&tmp);
        return Err(StoreError::io(tmp.display().to_string(), e));
    }
    let linked = std::fs::hard_link(&tmp, path);
    forget_temporary(&tmp);
    linked.map_err(|e| StoreError::io(path.display().to_string(), e))?;
    if let Some(parent) = path.parent() {
        crate::paths::sync_dir(parent);
    }
    Ok(())
}

/// File backend for headless machines: `<dir>/<name>.key`, mode 0600.
pub struct FileKeyStore {
    dir: PathBuf,
}

impl FileKeyStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        // Logical names may contain ':' (agent:<pubkey>); keep filenames tame.
        self.dir.join(format!("{}.key", name.replace(':', "_")))
    }
}

impl KeyStore for FileKeyStore {
    fn path_of(&self, name: &str) -> Option<PathBuf> {
        Some(self.path_for(name))
    }

    fn get(&self, name: &str) -> Result<Option<String>, StoreError> {
        let path = self.path_for(name);
        match std::fs::read_to_string(&path) {
            Ok(s) => Ok(Some(s.trim().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }

    fn set(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        let path = self.path_for(name);
        write_secret_file(&path, secret_hex.as_bytes()).map_err(|e| match e {
            StoreError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists => {
                StoreError::KeyStore(format!(
                    "key {name:?} already exists at {}; a new key never overwrites one",
                    path.display()
                ))
            }
            other => other, // LCOV_EXCL_LINE: the one error a new key's own file raises is `AlreadyExists`; any other here is the disk's
        })
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        let path = self.path_for(name);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::io(path.display().to_string(), e)),
        }
    }
}

/// The store `Workspace::open` uses: the file store, or the OS keyring when
/// `BISA_KEYSTORE=keyring` says so.
///
/// With the keyring chosen, reads are lenient — a key written to the file
/// store earlier is still found — and writes are strict: a keyring error is
/// returned with the remedy in the message, never turned into a file nobody
/// asked for. With the file store (the default) the keyring is never called.
pub struct AutoKeyStore {
    keyring: Option<KeyringStore>,
    file: FileKeyStore,
}

impl AutoKeyStore {
    // LCOV_EXCL_START: reads the process environment through `KeyStoreChoice::from_env`; `with_choice` holds the rule
    pub fn new(identity_dir: PathBuf) -> Self {
        Self::with_choice(identity_dir, KeyStoreChoice::from_env())
    }
    // LCOV_EXCL_STOP

    pub fn with_choice(identity_dir: PathBuf, choice: KeyStoreChoice) -> Self {
        Self {
            keyring: match choice {
                KeyStoreChoice::Keyring => Some(KeyringStore),
                KeyStoreChoice::File => None,
            },
            file: FileKeyStore::new(identity_dir),
        }
    }

    /// Whether this store talks to the OS keyring at all.
    pub fn uses_keyring(&self) -> bool {
        self.keyring.is_some()
    }

    // LCOV_EXCL_START: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
    fn keyring_unavailable(&self, e: StoreError) -> StoreError {
        StoreError::KeyStore(format!(
            "the OS keyring is unavailable ({e}). Unset {KEYSTORE_ENV} (or set it to `file`) to keep \
             keys as 0600 files in {} instead; nothing is written there without that choice",
            self.file.dir.display()
        ))
    }
    // LCOV_EXCL_STOP
}

impl KeyStore for AutoKeyStore {
    fn uses_keyring(&self) -> bool {
        AutoKeyStore::uses_keyring(self)
    }

    fn path_of(&self, name: &str) -> Option<PathBuf> {
        self.file.path_of(name)
    }

    fn get(&self, name: &str) -> Result<Option<String>, StoreError> {
        // LCOV_EXCL_START: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
        if let Some(kr) = &self.keyring {
            match kr.get(name) {
                Ok(Some(s)) => return Ok(Some(s)),
                Ok(None) => {}
                Err(e) => tracing::warn!("keyring unavailable ({e}); reading the file key store"),
            }
        }
        // LCOV_EXCL_STOP
        self.file.get(name)
    }

    fn set(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        match &self.keyring {
            // LCOV_EXCL_START: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
            Some(kr) => kr
                .set(name, secret_hex)
                .map_err(|e| self.keyring_unavailable(e)),
            // LCOV_EXCL_STOP
            None => self.file.set(name, secret_hex),
        }
    }

    fn replace(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        match &self.keyring {
            // LCOV_EXCL_START: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
            Some(kr) => kr
                .replace(name, secret_hex)
                .map_err(|e| self.keyring_unavailable(e)),
            // LCOV_EXCL_STOP
            None => self.file.replace(name, secret_hex),
        }
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        let kr = match &self.keyring {
            Some(kr) => kr.delete(name), // LCOV_EXCL_LINE: the OS keyring: a test never reaches the keychain (testing rules); `BISA_KEYSTORE=keyring` is a person's choice on their own machine
            None => Ok(()),
        };
        let f = self.file.delete(name);
        kr.and(f)
    }
}

/// In-memory backend for tests.
#[derive(Default)]
pub struct MemoryKeyStore {
    map: Mutex<HashMap<String, String>>,
}

impl MemoryKeyStore {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl KeyStore for MemoryKeyStore {
    fn get(&self, name: &str) -> Result<Option<String>, StoreError> {
        Ok(self.map().get(name).cloned())
    }

    fn set(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        let mut map = self.map();
        if map.contains_key(name) {
            return Err(StoreError::KeyStore(format!(
                "key {name:?} already exists; a new key never overwrites one"
            )));
        }
        map.insert(name.to_string(), secret_hex.to_string());
        Ok(())
    }

    fn replace(&self, name: &str, secret_hex: &str) -> Result<(), StoreError> {
        self.map().insert(name.to_string(), secret_hex.to_string());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        self.map().remove(name);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Identity manager
// ---------------------------------------------------------------------------

/// Loads/creates the owner identity and mints agent keys.
pub struct Identity {
    store: Box<dyn KeyStore>,
}

impl Identity {
    pub fn new(store: Box<dyn KeyStore>) -> Self {
        Self { store }
    }

    fn load(&self, name: &str) -> Result<Option<Keys>, StoreError> {
        match self.store.get(name)? {
            Some(secret) => Keys::parse(&secret).map(Some).map_err(|e| {
                // The owner's key is the one file that stops an open, and
                // the refusal names it: a torn or empty key is restored from
                // a backup or the keyring, never minted over.
                if name == OWNER_KEY_NAME {
                    StoreError::OwnerKeyUnreadable {
                        path: self
                            .store
                            .path_of(name)
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| name.to_string()),
                        reason: e.to_string(),
                    }
                } else {
                    StoreError::nostr(e)
                }
            }),
            None => Ok(None),
        }
    }

    /// The workspace owner keypair, generated on first use.
    pub fn owner(&self) -> Result<Keys, StoreError> {
        if let Some(keys) = self.load(OWNER_KEY_NAME)? {
            return Ok(keys);
        }
        let keys = Keys::generate();
        self.store
            .set(OWNER_KEY_NAME, &keys.secret_key().to_secret_hex())?;
        Ok(keys)
    }

    /// Mint a fresh agent keypair, stored under `agent:<pubkey>`.
    pub fn mint_agent(&self) -> Result<Keys, StoreError> {
        let keys = Keys::generate();
        let name = format!("agent:{}", keys.public_key().to_hex());
        self.store.set(&name, &keys.secret_key().to_secret_hex())?;
        Ok(keys)
    }

    pub fn agent(&self, pubkey_hex: &str) -> Result<Keys, StoreError> {
        self.load(&format!("agent:{pubkey_hex}"))?
            .ok_or_else(|| StoreError::KeyNotFound(format!("agent:{pubkey_hex}")))
    }

    /// Delete an agent's secret from the keystore (unrecoverable).
    pub fn delete_agent(&self, pubkey_hex: &str) -> Result<(), StoreError> {
        self.store.delete(&format!("agent:{pubkey_hex}"))
    }

    /// Mint a public hook start's secret: 32 fresh CSPRNG bytes stored under
    /// `hook:<host kind>:<host id>:<step>`, returned once. Refused when one
    /// exists — minting never clobbers; rotation is its own act.
    pub fn mint_hook_secret(&self, key: &bisa_core::ListenerKey) -> Result<Vec<u8>, StoreError> {
        let secret = Keys::generate().secret_key().to_secret_bytes();
        self.store
            .set(&hook_secret_name(key), &hex::encode(secret))?;
        Ok(secret.to_vec())
    }

    /// Replace a hook start's secret with a fresh one — rotation, the one
    /// deliberate overwrite of a key. The old secret stops verifying at once.
    pub fn rotate_hook_secret(&self, key: &bisa_core::ListenerKey) -> Result<Vec<u8>, StoreError> {
        let secret = Keys::generate().secret_key().to_secret_bytes();
        self.store
            .replace(&hook_secret_name(key), &hex::encode(secret))?;
        Ok(secret.to_vec())
    }

    pub fn hook_secret(&self, key: &bisa_core::ListenerKey) -> Result<Vec<u8>, StoreError> {
        let name = hook_secret_name(key);
        let hex_str = self
            .store
            .get(&name)?
            .ok_or_else(|| StoreError::KeyNotFound(name.clone()))?;
        hex::decode(hex_str.trim()).map_err(|e| StoreError::KeyStore(format!("{name}: {e}")))
    }

    /// Whether a hook start has its secret — never the secret itself.
    pub fn has_hook_secret(&self, key: &bisa_core::ListenerKey) -> Result<bool, StoreError> {
        Ok(self.store.get(&hook_secret_name(key))?.is_some())
    }

    /// Delete a hook start's secret (unrecoverable). Idempotent.
    pub fn delete_hook_secret(&self, key: &bisa_core::ListenerKey) -> Result<(), StoreError> {
        self.store.delete(&hook_secret_name(key))
    }

    /// A named secret as text — a connector account's API key or token,
    /// under `connector:<connector>:<account>:<field>`. Text, not hex: the
    /// value is a platform's own token and is stored as the person pasted it.
    pub fn secret(&self, name: &str) -> Result<Option<String>, StoreError> {
        self.store.get(name)
    }

    /// Set a named secret, replacing whatever was there — the person pasted
    /// a new key, or a refresh minted a new token.
    pub fn set_secret(&self, name: &str, value: &str) -> Result<(), StoreError> {
        self.store.replace(name, value)
    }

    /// Delete a named secret (unrecoverable). Idempotent.
    pub fn delete_secret(&self, name: &str) -> Result<(), StoreError> {
        self.store.delete(name)
    }

    /// Whether this workspace's secrets live in the OS keyring.
    pub fn uses_keyring(&self) -> bool {
        self.store.uses_keyring()
    }

    /// NIP-49 export of a stored key (`ncryptsec...`).
    pub fn export_encrypted(&self, name: &str, password: &str) -> Result<String, StoreError> {
        let keys = self
            .load(name)?
            .ok_or_else(|| StoreError::KeyNotFound(name.to_string()))?;
        let enc = EncryptedSecretKey::new(keys.secret_key(), password, 16, KeySecurity::Unknown)
            .map_err(StoreError::nostr)?;
        enc.to_bech32().map_err(StoreError::nostr)
    }

    /// NIP-49 import; **replaces** what is stored under `name` — the one
    /// deliberate overwrite — and returns the keys.
    pub fn import_encrypted(
        &self,
        name: &str,
        ncryptsec: &str,
        password: &str,
    ) -> Result<Keys, StoreError> {
        let enc = EncryptedSecretKey::from_bech32(ncryptsec).map_err(StoreError::nostr)?;
        let secret = enc.decrypt(password).map_err(StoreError::nostr)?;
        let keys = Keys::parse(&secret.to_secret_hex()).map_err(StoreError::nostr)?;
        self.store.replace(name, &secret.to_secret_hex())?;
        Ok(keys)
    }
}

// ---------------------------------------------------------------------------
// NIP-OA owner attestation
// ---------------------------------------------------------------------------

fn verify_ctx() -> &'static secp256k1::Secp256k1<secp256k1::VerifyOnly> {
    static CTX: OnceLock<secp256k1::Secp256k1<secp256k1::VerifyOnly>> = OnceLock::new();
    CTX.get_or_init(secp256k1::Secp256k1::verification_only)
}

fn auth_digest(agent_pubkey_hex: &str, conditions: &str) -> [u8; 32] {
    let preimage = format!("{AUTH_DOMAIN}{agent_pubkey_hex}:{conditions}");
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    hasher.finalize().into()
}

/// Produce a NIP-OA `auth` tag: the owner authorizes `agent_pubkey_hex`.
pub fn attest_agent(
    owner: &Keys,
    agent_pubkey_hex: &str,
    conditions: &str,
) -> Result<Tag, StoreError> {
    if owner.public_key().to_hex() == agent_pubkey_hex {
        return Err(StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-self-attestation-invalid-per-nip-oa"
        )));
    }
    validate_conditions_syntax(conditions)?;
    let digest = auth_digest(agent_pubkey_hex, conditions);
    let sig = owner.sign_schnorr(digest);
    Tag::parse([
        "auth",
        &owner.public_key().to_hex(),
        conditions,
        &sig.to_string(),
    ])
    .map_err(StoreError::nostr)
}

fn validate_conditions_syntax(conditions: &str) -> Result<(), StoreError> {
    parse_clauses(conditions).map(|_| ()).map_err(|e| {
        StoreError::Invalid(bisa_core::text!(
            "error-store-invalid-invalid-nip-oa-conditions",
            e = e.to_string()
        ))
    })
}

enum Clause {
    KindEq(u16),
    CreatedBefore(u64),
    CreatedAfter(u64),
}

fn parse_decimal(s: &str, max: u64) -> Result<u64, String> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!("non-canonical decimal {s:?}"));
    }
    let v: u64 = s
        .parse()
        .map_err(|_| format!("decimal out of range {s:?}"))?;
    if v > max {
        return Err(format!("decimal out of range {s:?}"));
    }
    Ok(v)
}

fn parse_clauses(conditions: &str) -> Result<Vec<Clause>, String> {
    if conditions.is_empty() {
        return Ok(vec![]);
    }
    if !conditions.is_ascii() || conditions.contains(char::is_whitespace) {
        return Err("non-ascii or whitespace".into());
    }
    conditions
        .split('&')
        .map(|clause| {
            if let Some(rest) = clause.strip_prefix("kind=") {
                Ok(Clause::KindEq(parse_decimal(rest, 65535)? as u16))
            } else if let Some(rest) = clause.strip_prefix("created_at<") {
                Ok(Clause::CreatedBefore(parse_decimal(rest, 4_294_967_295)?))
            } else if let Some(rest) = clause.strip_prefix("created_at>") {
                Ok(Clause::CreatedAfter(parse_decimal(rest, 4_294_967_295)?))
            } else {
                Err(format!("unsupported clause {clause:?}"))
            }
        })
        .collect()
}

/// Verify the NIP-OA `auth` tag on `event`. Returns the attesting owner's
/// pubkey hex when the tag is valid AND every condition clause is satisfied.
/// Callers MUST have verified `event.verify()` first.
pub fn verify_attestation(event: &Event) -> Option<String> {
    let auth_tags: Vec<&Tag> = event
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(String::as_str) == Some("auth"))
        .collect();
    let [tag] = auth_tags.as_slice() else {
        return None;
    };
    let parts = tag.as_slice();
    if parts.len() != 4 {
        return None;
    }
    let (owner_hex, conditions, sig_hex) = (&parts[1], &parts[2], &parts[3]);
    let event_pubkey = event.pubkey.to_hex();
    if *owner_hex == event_pubkey {
        return None;
    }
    let clauses = parse_clauses(conditions).ok()?;
    for clause in &clauses {
        let ok = match clause {
            Clause::KindEq(k) => event.kind.as_u16() == *k,
            Clause::CreatedBefore(t) => event.created_at.as_secs() < *t,
            Clause::CreatedAfter(t) => event.created_at.as_secs() > *t,
        };
        if !ok {
            return None;
        }
    }
    let owner_pk = secp256k1::XOnlyPublicKey::from_str(owner_hex).ok()?;
    let sig = secp256k1::schnorr::Signature::from_str(sig_hex).ok()?;
    let digest = auth_digest(&event_pubkey, conditions);
    verify_ctx()
        .verify_schnorr(&sig, &digest, &owner_pk)
        .ok()
        .map(|_| owner_hex.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind};

    fn keys_from(byte: u8) -> Keys {
        let mut sk = [0u8; 32];
        sk[31] = byte;
        Keys::parse(&hex::encode(sk)).unwrap()
    }

    #[test]
    fn nip_oa_test_vector() {
        let owner = keys_from(1);
        assert_eq!(
            owner.public_key().to_hex(),
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
        let agent = keys_from(2);
        let conditions = "kind=1&created_at<1713957000";
        let digest = auth_digest(&agent.public_key().to_hex(), conditions);
        assert_eq!(
            hex::encode(digest),
            "08cdecd55af4c28d3801fd69615dcf5cc04fab3bc134b38a840bf157197069a6"
        );
        let tag = attest_agent(&owner, &agent.public_key().to_hex(), conditions).unwrap();
        let event = EventBuilder::new(Kind::from(1u16), "owner-attested agent event")
            .tag(tag)
            .custom_created_at(nostr::types::Timestamp::from_secs(1713956400))
            .finalize(&agent)
            .unwrap();
        event.verify().unwrap();
        assert_eq!(
            verify_attestation(&event),
            Some(owner.public_key().to_hex())
        );
        let spec_sig = "8b7df2575caf0a108374f8471722b233c53f9ff827a8b0f91861966c3b9dd5cb2e189eae9f49d72187674c2f5bd244145e10ff86c9f257ffe65a1ee5f108b369";
        let sig = secp256k1::schnorr::Signature::from_str(spec_sig).unwrap();
        let owner_pk = secp256k1::XOnlyPublicKey::from_str(&owner.public_key().to_hex()).unwrap();
        verify_ctx()
            .verify_schnorr(&sig, &digest, &owner_pk)
            .unwrap();
    }

    #[test]
    fn attestation_rejections() {
        let owner = keys_from(1);
        let agent = keys_from(2);
        let agent_hex = agent.public_key().to_hex();
        assert!(attest_agent(&owner, &owner.public_key().to_hex(), "").is_err());
        assert!(attest_agent(&owner, &agent_hex, "kind=01").is_err());
        assert!(attest_agent(&owner, &agent_hex, "kind=1&").is_err());
        let tag = attest_agent(&owner, &agent_hex, "kind=5").unwrap();
        let event = EventBuilder::new(Kind::from(1u16), "x")
            .tag(tag)
            .finalize(&agent)
            .unwrap();
        assert_eq!(verify_attestation(&event), None);
        let t1 = attest_agent(&owner, &agent_hex, "").unwrap();
        let t2 = attest_agent(&owner, &agent_hex, "kind=1").unwrap();
        let event = EventBuilder::new(Kind::from(1u16), "x")
            .tags([t1, t2])
            .finalize(&agent)
            .unwrap();
        assert_eq!(verify_attestation(&event), None);
        let tag = attest_agent(&owner, &agent_hex, "created_at>100").unwrap();
        let event = EventBuilder::new(Kind::from(1u16), "x")
            .tag(tag)
            .custom_created_at(nostr::types::Timestamp::from_secs(50))
            .finalize(&agent)
            .unwrap();
        assert_eq!(verify_attestation(&event), None);
    }

    #[test]
    fn identity_mint_and_nip49_roundtrip() {
        let id = Identity::new(Box::new(MemoryKeyStore::default()));
        let owner = id.owner().unwrap();
        assert_eq!(id.owner().unwrap().public_key(), owner.public_key());
        let agent = id.mint_agent().unwrap();
        assert_eq!(
            id.agent(&agent.public_key().to_hex()).unwrap().public_key(),
            agent.public_key()
        );
        let exported = id.export_encrypted("owner", "hunter2").unwrap();
        assert!(exported.starts_with("ncryptsec"));
        let re = Identity::new(Box::new(MemoryKeyStore::default()));
        let imported = re.import_encrypted("owner", &exported, "hunter2").unwrap();
        assert_eq!(imported.public_key(), owner.public_key());
        assert!(re.import_encrypted("owner", &exported, "wrong").is_err());
        // Importing over an existing owner is the deliberate replacement.
        let again = re.import_encrypted("owner", &exported, "hunter2").unwrap();
        assert_eq!(again.public_key(), owner.public_key());
    }

    #[test]
    fn a_new_key_never_overwrites_and_files_are_private() {
        let dir = tempfile::tempdir().unwrap();
        let identity = crate::Paths::new(dir.path()).identity_dir();
        let ks = FileKeyStore::new(identity.clone());
        ks.set("owner", "aa").unwrap();
        assert!(ks.set("owner", "bb").is_err(), "create_new refuses");
        assert_eq!(ks.get("owner").unwrap().as_deref(), Some("aa"));
        ks.replace("owner", "cc").unwrap();
        assert_eq!(ks.get("owner").unwrap().as_deref(), Some("cc"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_mode = std::fs::metadata(identity.join("owner.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(file_mode, 0o600);
            let dir_mode = std::fs::metadata(&identity).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
        }
        let mem = MemoryKeyStore::default();
        mem.set("k", "1").unwrap();
        assert!(mem.set("k", "2").is_err());
    }

    #[test]
    fn the_file_store_is_the_default_and_the_keyring_is_opt_in() {
        assert_eq!(KeyStoreChoice::parse(None), KeyStoreChoice::File);
        assert_eq!(KeyStoreChoice::parse(Some("file")), KeyStoreChoice::File);
        assert_eq!(KeyStoreChoice::parse(Some("")), KeyStoreChoice::File);
        assert_eq!(
            KeyStoreChoice::parse(Some("Keychain")),
            KeyStoreChoice::File,
            "a typo never summons a prompt"
        );
        assert_eq!(
            KeyStoreChoice::parse(Some(" keyring ")),
            KeyStoreChoice::Keyring
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(
            !AutoKeyStore::with_choice(dir.path().to_path_buf(), KeyStoreChoice::File)
                .uses_keyring()
        );
        assert!(
            AutoKeyStore::with_choice(dir.path().to_path_buf(), KeyStoreChoice::Keyring)
                .uses_keyring()
        );
    }

    // added by the coverage pass: s1-identity.rs
    #[test]
    fn a_store_in_memory_names_no_file_and_the_auto_store_with_files_chosen_never_calls_the_keyring(
    ) {
        let memory = MemoryKeyStore::default();
        assert_eq!(memory.path_of("owner"), None);
        assert!(!memory.uses_keyring());
        let dir = tempfile::tempdir().unwrap();
        let auto = AutoKeyStore::with_choice(dir.path().join("identity"), KeyStoreChoice::File);
        assert!(!auto.uses_keyring());
        assert!(!KeyStore::uses_keyring(&auto));
        assert!(auto
            .path_of("owner")
            .is_some_and(|p| p.ends_with("owner.key")));
        assert_eq!(auto.get("owner").unwrap(), None);
        auto.set("owner", "aa").unwrap();
        assert_eq!(auto.get("owner").unwrap().as_deref(), Some("aa"));
        auto.replace("owner", "bb").unwrap();
        assert_eq!(auto.get("owner").unwrap().as_deref(), Some("bb"));
        auto.delete("owner").unwrap();
        assert_eq!(auto.get("owner").unwrap(), None);
    }

    #[test]
    fn a_secret_that_is_not_a_key_is_a_nostr_error_unless_it_is_the_owners() {
        let memory = MemoryKeyStore::default();
        memory.set("agent:abc", "not-a-key").unwrap();
        let identity = Identity::new(Box::new(memory));
        assert!(matches!(identity.agent("abc"), Err(StoreError::Nostr(_))));
    }

    #[cfg(unix)]
    #[test]
    fn a_key_file_that_cannot_be_read_or_removed_is_an_io_error_and_a_stray_temporary_refuses_the_write(
    ) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let identity_dir = dir.path().join("identity");
        let store = FileKeyStore::new(identity_dir.clone());
        store.set("owner", "aa").unwrap();
        let file = store.path_of("owner").unwrap();
        let was = std::fs::metadata(&file).unwrap().permissions();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
        let read = store.get("owner");
        std::fs::set_permissions(&file, was).unwrap();
        assert!(matches!(read, Err(StoreError::Io { .. })), "{read:?}");
        // A temporary already standing where the key's own would go: the
        // write refuses, and the stray — a folder, which no temporary of
        // ours is — is said and left as it was found.
        let tmp = identity_dir.join(format!("other.key.tmp.{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(store.set("other", "bb").is_err());
        assert!(tmp.is_dir());
        assert_eq!(store.get("other").unwrap(), None);
        // A folder nobody may write in: the key stays and the removal says so.
        let was = std::fs::metadata(&identity_dir).unwrap().permissions();
        std::fs::set_permissions(&identity_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let removed = store.delete("owner");
        std::fs::set_permissions(&identity_dir, was).unwrap();
        assert!(matches!(removed, Err(StoreError::Io { .. })), "{removed:?}");
        assert_eq!(store.get("owner").unwrap().as_deref(), Some("aa"));
    }

    #[test]
    fn a_condition_out_of_range_or_not_ascii_is_refused_and_a_malformed_auth_tag_attests_nobody() {
        assert!(parse_clauses("kind=70000").is_err());
        assert!(parse_clauses("kind=é").is_err());
        assert!(parse_clauses("created_at<99999999999").is_err());
        let owner = keys_from(1);
        let agent = keys_from(2);
        let three = EventBuilder::new(Kind::from(1u16), "x")
            .tag(Tag::parse(["auth", &owner.public_key().to_hex(), ""]).unwrap())
            .finalize(&agent)
            .unwrap();
        assert_eq!(
            verify_attestation(&three),
            None,
            "an auth tag of three parts"
        );
        let own = EventBuilder::new(Kind::from(1u16), "x")
            .tag(Tag::parse(["auth", &agent.public_key().to_hex(), "", "00"]).unwrap())
            .finalize(&agent)
            .unwrap();
        assert_eq!(
            verify_attestation(&own),
            None,
            "an author cannot attest itself"
        );
    }
}
