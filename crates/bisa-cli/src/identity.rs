//! `bisa identity export | import`: the owner key as a NIP-49 `ncryptsec`,
//! out of this workspace and into one.
//!
//! The keys live in the identity store alone — a file under `identity/`
//! with `--file-keys`, the OS keychain otherwise — so neither verb opens the
//! workspace: an import into a folder nobody initialised yet lands the key
//! before the first open would mint one, and an export reads nothing else.
//!
//! **The passphrase never rides the command line**, where a shell keeps it
//! in history: it is the first line of stdin when stdin is a pipe or a file
//! (the harness door every journey uses), and asked for without echo on the
//! terminal otherwise — twice on an export, so a typo is not the only copy.
//! The `ncryptsec` is the import's argument, a file holding it, or the line
//! before the passphrase on stdin. An export goes to stdout, or to a file
//! created private (`0600`) and never over one that exists; an import
//! refuses to replace the owner key a workspace already has without
//! `--replace`, and refuses while a node holds the workspace — the node
//! signs with the key it opened with, and a key swapped under it would sign
//! nothing it could verify.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context as _, Result};
use bisa_store::{AutoKeyStore, FileKeyStore, Identity, KeyStore, Paths};
use clap::Subcommand;
use nostr::nips::nip19::ToBech32;
use serde_json::json;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum IdentityCmd {
    /// Export the owner key as a NIP-49 `ncryptsec`, encrypted with a passphrase read from stdin or asked without echo
    Export {
        /// Write the export to this file, created private (0600), instead of stdout; a file that exists is refused
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Import an owner key from a NIP-49 `ncryptsec`, decrypted with a passphrase read from stdin or asked without echo
    Import {
        /// The `ncryptsec…` text, or a file holding it; the line before the passphrase on stdin when absent
        source: Option<String>,
        /// Replace the owner key this workspace already has — what it signed so far keeps the old key's name
        #[arg(long)]
        replace: bool,
    },
}

pub async fn identity(ctx: &Ctx, out: &Out, cmd: IdentityCmd) -> Result<()> {
    match cmd {
        IdentityCmd::Export { out: path } => export(ctx, out, path.as_deref()).await,
        IdentityCmd::Import { source, replace } => {
            import(ctx, out, source.as_deref(), replace).await
        }
    }
}

/// The identity store this workspace keeps its keys in, opened without the
/// workspace: the file store under `--file-keys`, the machine's choice
/// otherwise.
fn store(ctx: &Ctx) -> Identity {
    let dir = Paths::new(&ctx.data_dir).identity_dir();
    let ks: Box<dyn KeyStore> = if ctx.file_keys {
        Box::new(FileKeyStore::new(dir))
    } else {
        Box::new(AutoKeyStore::new(dir)) // LCOV_EXCL_LINE the machine's keychain; out by decision
    };
    Identity::new(ks)
}

/// What stdin holds when it is not a terminal: its lines, read once.
fn piped_lines() -> Result<Vec<String>> {
    let text = std::io::read_to_string(std::io::stdin())
        .context(bisa_core::text!("cli-identity-reading-stdin"))?;
    Ok(text
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect())
}

/// A passphrase with something in it, or the refusal.
fn passphrase_of(line: Option<&str>) -> Result<String> {
    match line.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Ok(p.to_string()),
        None => bail!(bisa_core::text!("cli-identity-passphrase-empty")),
    }
}

async fn export(ctx: &Ctx, out: &Out, path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        if path.exists() {
            bail!(bisa_core::text!(
                "cli-identity-file-exists",
                path = path.display().to_string()
            ));
        }
    }
    let passphrase = if std::io::stdin().is_terminal() {
        // LCOV_EXCL_START the person's terminal; out by decision
        let first = prompt_hidden(&out.text(&bisa_core::text!("cli-identity-prompt-passphrase")))?;
        let again = prompt_hidden(&out.text(&bisa_core::text!("cli-identity-prompt-again")))?;
        if first != again {
            bail!(bisa_core::text!("cli-identity-passphrase-mismatch"));
        }
        passphrase_of(Some(&first))?
        // LCOV_EXCL_STOP
    } else {
        passphrase_of(piped_lines()?.first().map(String::as_str))?
    };
    let identity = store(ctx);
    let npub = identity.owner()?.public_key().to_bech32()?;
    let ncryptsec = identity.export_owner(&passphrase)?;
    match path {
        Some(path) => {
            bisa_store::write_private(path, format!("{ncryptsec}\n").as_bytes())?;
            out.say(&bisa_core::text!(
                "cli-identity-exported-to",
                path = path.display().to_string()
            ));
            out.json_value(json!({"npub": npub, "written": path.display().to_string()}));
        }
        None => {
            out.human(&ncryptsec);
            out.json_value(json!({"npub": npub, "ncryptsec": ncryptsec}));
        }
    }
    Ok(())
}

async fn import(ctx: &Ctx, out: &Out, source: Option<&str>, replace: bool) -> Result<()> {
    if ctx.node_client().await.is_some() {
        bail!(bisa_core::text!("cli-identity-import-needs-no-node"));
    }
    // The ncryptsec: the argument as text, the file it names, or stdin's
    // first line — then the passphrase, from stdin's next line or the terminal.
    let (ncryptsec, passphrase) = if std::io::stdin().is_terminal() {
        // LCOV_EXCL_START the person's terminal; out by decision
        let Some(source) = source else {
            bail!(bisa_core::text!("cli-identity-ncryptsec-missing"));
        };
        let ncryptsec = ncryptsec_of(source)?;
        let passphrase =
            prompt_hidden(&out.text(&bisa_core::text!("cli-identity-prompt-passphrase")))?;
        (ncryptsec, passphrase_of(Some(&passphrase))?)
        // LCOV_EXCL_STOP
    } else {
        let lines = piped_lines()?;
        match source {
            Some(source) => (
                ncryptsec_of(source)?,
                passphrase_of(lines.first().map(String::as_str))?,
            ),
            None => {
                let Some(first) = lines.first().map(|l| l.trim()).filter(|l| !l.is_empty()) else {
                    bail!(bisa_core::text!("cli-identity-ncryptsec-missing"));
                };
                (
                    first.to_string(),
                    passphrase_of(lines.get(1).map(String::as_str))?,
                )
            }
        }
    };
    let identity = store(ctx);
    if identity.has_owner()? && !replace {
        bail!(bisa_core::text!("cli-identity-import-needs-replace"));
    }
    let keys = identity.import_owner(&ncryptsec, &passphrase)?;
    let npub = keys.public_key().to_bech32()?;
    out.say(&bisa_core::text!(
        "cli-identity-imported",
        npub = npub.clone()
    ));
    out.json_value(json!({"npub": npub, "imported": true, "replaced": replace}));
    Ok(())
}

/// The `ncryptsec` an argument names: the text itself, or the first line of
/// the file at that path.
fn ncryptsec_of(source: &str) -> Result<String> {
    let trimmed = source.trim();
    if trimmed.starts_with("ncryptsec") {
        return Ok(trimmed.to_string());
    }
    let text = std::fs::read_to_string(trimmed).with_context(|| {
        bisa_core::text!("cli-identity-reading-file", path = trimmed.to_string())
    })?;
    match text.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(line) => Ok(line.to_string()),
        None => bail!(bisa_core::text!("cli-identity-ncryptsec-missing")),
    }
}

/// One line typed at the terminal with echo off — the passphrase — through
/// `/dev/tty`, so it is the person's even when stdout is a pipe.
// LCOV_EXCL_START the person's terminal; out by decision
#[cfg(unix)]
fn prompt_hidden(prompt: &str) -> Result<String> {
    use std::io::{BufRead as _, Write as _};
    use std::os::fd::AsRawFd as _;
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .context(bisa_core::text!("cli-identity-no-terminal"))?;
    write!(tty, "{prompt}")?;
    tty.flush()?;
    let fd = tty.as_raw_fd();
    // SAFETY: `tcgetattr` fills a `termios` this function owns, for a
    // descriptor it holds open; `tcsetattr` writes the same struct back.
    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut term) } != 0 {
        bail!(bisa_core::text!("cli-identity-no-terminal"));
    }
    let saved = term;
    term.c_lflag &= !libc::ECHO;
    // SAFETY: as above — the struct came from `tcgetattr` on this descriptor.
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &term) };
    let mut line = String::new();
    let read = std::io::BufReader::new(&tty).read_line(&mut line);
    // SAFETY: restoring what `tcgetattr` answered, whatever the read did.
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
    writeln!(tty)?;
    read?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

#[cfg(not(unix))]
fn prompt_hidden(_prompt: &str) -> Result<String> {
    bail!(bisa_core::text!("cli-identity-no-terminal"))
}
// LCOV_EXCL_STOP

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passphrase_has_something_in_it_and_an_ncryptsec_is_text_or_a_files_first_line() {
        assert_eq!(passphrase_of(Some("  hunter2 ")).unwrap(), "hunter2");
        assert!(passphrase_of(Some("   ")).is_err());
        assert!(passphrase_of(None).is_err());
        assert_eq!(ncryptsec_of(" ncryptsec1abc ").unwrap(), "ncryptsec1abc");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("owner.ncryptsec");
        std::fs::write(&file, "\n  ncryptsec1fromfile\nignored\n").unwrap();
        assert_eq!(
            ncryptsec_of(file.to_str().unwrap()).unwrap(),
            "ncryptsec1fromfile"
        );
        std::fs::write(&file, "\n\n").unwrap();
        assert!(
            ncryptsec_of(file.to_str().unwrap()).is_err(),
            "an empty file holds no key"
        );
        assert!(ncryptsec_of(dir.path().join("missing").to_str().unwrap()).is_err());
    }
}
