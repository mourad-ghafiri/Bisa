//! The Redactor: rules that recognise a secret, the vault that swaps it for a
//! placeholder and back, and the walks over text and JSON.
//!
//! A placeholder is `«secret:<kind>:<tag>»`. The kind is the rule's id, so a
//! reader — a person or an agent — knows *what sort* of thing was there
//! without learning *what*. The tag is the first six hex characters of
//! SHA-256(nonce ‖ secret), where the nonce is minted once per process by the
//! caller: the same secret reads the same everywhere in one run, and nothing
//! about it survives a restart. The guillemets are chosen because no shell,
//! path, token or JSON grammar uses them, so a placeholder never changes the
//! meaning of the text around it and is trivial to find again.
//!
//! The vault is the only place a secret and its placeholder meet. It lives in
//! memory, is never serialised, and its `Debug` prints counts.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A value shorter than this is never treated as a secret by an `EnvValue`
/// detector — redacting `1` or `yes` would shred ordinary text.
pub const MIN_ENV_SECRET_LEN: usize = 4;

/// The stricter minimum for a built-in `EnvValue` detector: one the platform
/// armed from a variable's *name* alone, where a short value is far more
/// likely a mode word (`oauth2`, `basic`) than a credential.
pub const AUTO_ENV_MIN_LEN: usize = 8;

const OPEN: &str = "«secret:";
const CLOSE: &str = "»";

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"«secret:([^«»\s:][^«»\s]*?):([0-9a-f]{6,})»").expect("placeholder regex")
    })
}

/// Whether the text carries at least one placeholder.
pub fn has_placeholder(text: &str) -> bool {
    placeholder_re().is_match(text)
}

/// Where a rule came from — shipped with the platform or written by a person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "RuleOrigin")]
pub enum Origin {
    Builtin,
    #[default]
    User,
}

/// How a rule recognises a secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schemars(rename = "RedactDetector")]
pub enum Detector {
    /// A regular expression. When it has a group named `secret`, that group
    /// is the secret and the rest of the match stays; otherwise the whole
    /// match is.
    Pattern { regex: String },
    /// The value of one environment variable of the node's own process, read
    /// when the redactor is built. The name is stored; the value never is.
    EnvValue { name: String },
}

/// One redaction rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RedactRule {
    pub id: String,
    pub label: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub detector: Detector,
    #[serde(default)]
    pub origin: Origin,
}

fn yes() -> bool {
    true
}

/// A placeholder, parsed or about to be printed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Placeholder {
    pub kind: String,
    pub tag: String,
}

impl Placeholder {
    /// Parses exactly one placeholder — the whole string, nothing around it.
    pub fn parse(text: &str) -> Option<Self> {
        let caps = placeholder_re().captures(text)?;
        (caps.get(0)?.as_str() == text).then(|| Placeholder {
            kind: caps[1].to_string(),
            tag: caps[2].to_string(),
        })
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{OPEN}{}:{}{CLOSE}", self.kind, self.tag)
    }
}

/// What a redaction did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redaction {
    pub text: String,
    /// Secrets replaced, placeholders already present not counted.
    pub count: usize,
    /// The kinds replaced, each once, in order of first appearance.
    pub kinds: Vec<String>,
}

/// What a restore did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub text: String,
    /// Placeholders swapped back.
    pub restored: usize,
    /// Placeholders this vault has never seen — a restart, another node, or a
    /// made-up one. Left in the text as they were.
    pub unresolved: Vec<String>,
}

#[derive(Default)]
struct Maps {
    by_placeholder: HashMap<String, String>,
    by_secret: HashMap<String, Placeholder>,
}

/// The one place a secret and its placeholder meet.
pub struct Vault {
    nonce: String,
    maps: Mutex<Maps>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("secrets", &self.len())
            .finish()
    }
}

impl Vault {
    /// A vault whose tags are derived from this nonce — mint one per process
    /// from the OS's randomness.
    pub fn new(nonce: impl Into<String>) -> Self {
        Vault {
            nonce: nonce.into(),
            maps: Mutex::new(Maps::default()),
        }
    }

    /// How many secrets the vault holds.
    pub fn len(&self) -> usize {
        self.maps.lock().map(|m| m.by_secret.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The placeholder for this secret — the same one every time within this
    /// process. A tag collision between two secrets of one kind lengthens the
    /// newer tag until it is unique.
    pub fn placeholder_for(&self, kind: &str, secret: &str) -> Placeholder {
        let mut maps = self.maps.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = maps.by_secret.get(secret) {
            return existing.clone();
        }
        let digest = {
            let mut h = Sha256::new();
            h.update(self.nonce.as_bytes());
            h.update([0u8]);
            h.update(secret.as_bytes());
            h.finalize()
        };
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let mut width = 6;
        let placeholder = loop {
            let candidate = Placeholder {
                kind: kind.to_string(),
                tag: hex[..width.min(hex.len())].to_string(),
            };
            match maps.by_placeholder.get(&candidate.to_string()) {
                Some(other) if other != secret && width < hex.len() => width += 2,
                _ => break candidate,
            }
        };
        maps.by_placeholder
            .insert(placeholder.to_string(), secret.to_string());
        maps.by_secret
            .insert(secret.to_string(), placeholder.clone());
        placeholder
    }

    /// Swaps every known placeholder back for its secret. Unknown ones stay
    /// and are named in `unresolved`.
    pub fn restore(&self, text: &str) -> Restored {
        if !has_placeholder(text) {
            return Restored {
                text: text.to_string(),
                ..Restored::default()
            };
        }
        let maps = self.maps.lock().unwrap_or_else(|e| e.into_inner());
        let mut restored = 0;
        let mut unresolved = Vec::new();
        let out = placeholder_re().replace_all(text, |caps: &regex::Captures<'_>| {
            let whole = &caps[0];
            match maps.by_placeholder.get(whole) {
                Some(secret) => {
                    restored += 1;
                    secret.clone()
                }
                None => {
                    if !unresolved.iter().any(|u| u == whole) {
                        unresolved.push(whole.to_string());
                    }
                    whole.to_string()
                }
            }
        });
        Restored {
            text: out.into_owned(),
            restored,
            unresolved,
        }
    }

    /// [`Vault::restore`] over every string leaf of a JSON value.
    pub fn restore_value(&self, value: &mut Value) -> Restored {
        let mut total = Restored::default();
        walk_strings(value, &mut |s| {
            let r = self.restore(s);
            total.restored += r.restored;
            for u in r.unresolved {
                if !total.unresolved.contains(&u) {
                    total.unresolved.push(u);
                }
            }
            r.text
        });
        total
    }
}

enum Compiled {
    Pattern {
        kind: String,
        regex: Regex,
        has_group: bool,
    },
    Literal {
        kind: String,
        value: String,
    },
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compiled::Pattern { kind, regex, .. } => {
                write!(f, "Pattern({kind}, {})", regex.as_str())
            }
            Compiled::Literal { kind, .. } => write!(f, "Literal({kind}, <redacted>)"),
        }
    }
}

/// Why a rule could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("rule {rule}: {reason}")]
pub struct BadRule {
    pub rule: String,
    pub reason: String,
}

/// The compiled, enabled rules.
#[derive(Debug, Default)]
pub struct Redactor {
    detectors: Vec<Compiled>,
}

impl Redactor {
    /// Compiles the enabled rules. `env` answers an environment variable's
    /// value; a variable that is unset or too short yields no detector — too
    /// short being [`MIN_ENV_SECRET_LEN`] for a rule a person wrote and
    /// [`AUTO_ENV_MIN_LEN`] for one the platform armed from a name. A rule
    /// whose regex does not compile is reported, not fatal.
    pub fn compile(
        rules: &[RedactRule],
        env: &dyn Fn(&str) -> Option<String>,
    ) -> (Self, Vec<BadRule>) {
        let mut detectors = Vec::new();
        let mut problems = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            match &rule.detector {
                Detector::Pattern { regex } => match Regex::new(regex) {
                    Ok(re) => {
                        let has_group = re.capture_names().any(|n| n == Some("secret"));
                        detectors.push(Compiled::Pattern {
                            kind: rule.id.clone(),
                            regex: re,
                            has_group,
                        });
                    }
                    Err(e) => problems.push(BadRule {
                        rule: rule.id.clone(),
                        reason: e.to_string(),
                    }),
                },
                Detector::EnvValue { name } => {
                    let min = match rule.origin {
                        Origin::Builtin => AUTO_ENV_MIN_LEN,
                        Origin::User => MIN_ENV_SECRET_LEN,
                    };
                    if let Some(value) = env(name) {
                        if value.trim().len() >= min {
                            detectors.push(Compiled::Literal {
                                kind: rule.id.clone(),
                                value: value.trim().to_string(),
                            });
                        }
                    }
                }
            }
        }
        (Redactor { detectors }, problems)
    }

    /// Whether any detector is armed.
    pub fn is_empty(&self) -> bool {
        self.detectors.is_empty()
    }

    /// Replaces every secret in `text` with its placeholder. Text already
    /// holding a placeholder is left as it is — redaction is idempotent.
    pub fn redact(&self, vault: &Vault, text: &str) -> Redaction {
        if self.detectors.is_empty() || text.is_empty() {
            return Redaction {
                text: text.to_string(),
                ..Redaction::default()
            };
        }
        // Spans already occupied by a placeholder are off limits.
        let occupied: Vec<(usize, usize)> = placeholder_re()
            .find_iter(text)
            .map(|m| (m.start(), m.end()))
            .collect();
        let mut spans: Vec<(usize, usize, &str)> = Vec::new();
        for det in &self.detectors {
            match det {
                Compiled::Pattern {
                    kind,
                    regex,
                    has_group,
                } => {
                    for caps in regex.captures_iter(text) {
                        let m = if *has_group {
                            caps.name("secret")
                        } else {
                            caps.get(0)
                        };
                        if let Some(m) = m {
                            if m.end() > m.start() {
                                spans.push((m.start(), m.end(), kind));
                            }
                        }
                    }
                }
                Compiled::Literal { kind, value } => {
                    for (at, _) in text.match_indices(value.as_str()) {
                        spans.push((at, at + value.len(), kind));
                    }
                }
            }
        }
        spans.retain(|(s, e, _)| !occupied.iter().any(|(os, oe)| s < oe && os < e));
        // Earliest first; on a tie the longest wins. A span inside an
        // accepted one adds nothing; one that starts inside it and runs past
        // its end *extends* it — dropped whole, its tail would stay in the
        // text, and a secret half hidden is a secret shown. The union wears
        // the first span's kind.
        spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut accepted: Vec<(usize, usize, &str)> = Vec::new();
        for span in spans {
            match accepted.last_mut() {
                Some(last) if span.0 < last.1 => {
                    if span.1 > last.1 {
                        last.1 = span.1;
                    }
                }
                _ => accepted.push(span),
            }
        }
        if accepted.is_empty() {
            return Redaction {
                text: text.to_string(),
                ..Redaction::default()
            };
        }
        let mut out = String::with_capacity(text.len());
        let mut kinds: Vec<String> = Vec::new();
        let mut cursor = 0;
        for (start, end, kind) in &accepted {
            out.push_str(&text[cursor..*start]);
            let placeholder = vault.placeholder_for(kind, &text[*start..*end]);
            out.push_str(&placeholder.to_string());
            if !kinds.iter().any(|k| k == kind) {
                kinds.push((*kind).to_string());
            }
            cursor = *end;
        }
        out.push_str(&text[cursor..]);
        Redaction {
            text: out,
            count: accepted.len(),
            kinds,
        }
    }

    /// [`Redactor::redact`] over every string leaf of a JSON value — keys are
    /// structure and stay; the shape of the value never changes.
    pub fn redact_value(&self, vault: &Vault, value: &mut Value) -> Redaction {
        let mut total = Redaction::default();
        if self.detectors.is_empty() {
            return total;
        }
        walk_strings(value, &mut |s| {
            let r = self.redact(vault, s);
            total.count += r.count;
            for k in r.kinds {
                if !total.kinds.contains(&k) {
                    total.kinds.push(k);
                }
            }
            r.text
        });
        total
    }
}

fn walk_strings(value: &mut Value, f: &mut dyn FnMut(&str) -> String) {
    match value {
        Value::String(s) => {
            let next = f(s);
            if next != *s {
                *s = next;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| walk_strings(v, f)),
        Value::Object(map) => map.values_mut().for_each(|v| walk_strings(v, f)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAKE_GITHUB: &str = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

    fn rules() -> Vec<RedactRule> {
        vec![
            RedactRule {
                id: "github_token".into(),
                label: "GitHub token".into(),
                enabled: true,
                detector: Detector::Pattern {
                    regex: r"\bgh[pousr]_[A-Za-z0-9]{36,}\b".into(),
                },
                origin: Origin::Builtin,
            },
            RedactRule {
                id: "env:FAKE_KEY".into(),
                label: "FAKE_KEY".into(),
                enabled: true,
                detector: Detector::EnvValue {
                    name: "FAKE_KEY".into(),
                },
                origin: Origin::User,
            },
            RedactRule {
                id: "assignment".into(),
                label: "key = value".into(),
                enabled: true,
                detector: Detector::Pattern {
                    regex: r"(?i)\bpassword\s*=\s*(?P<secret>\S{8,})".into(),
                },
                origin: Origin::Builtin,
            },
        ]
    }

    fn env(name: &str) -> Option<String> {
        (name == "FAKE_KEY").then(|| "fake-value-0001".to_string())
    }

    fn redactor() -> Redactor {
        let (r, problems) = Redactor::compile(&rules(), &env);
        assert!(problems.is_empty(), "{problems:?}");
        r
    }

    #[test]
    fn a_secret_becomes_a_placeholder_and_the_round_trip_is_identity() {
        let vault = Vault::new("nonce");
        let text = format!("use {FAKE_GITHUB} to push, and the key is fake-value-0001");
        let out = redactor().redact(&vault, &text);
        assert_eq!(out.count, 2);
        assert_eq!(out.kinds, vec!["github_token", "env:FAKE_KEY"]);
        assert!(
            !out.text.contains(FAKE_GITHUB) && !out.text.contains("fake-value-0001"),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("«secret:github_token:")
                && out.text.contains("«secret:env:FAKE_KEY:")
        );
        let back = vault.restore(&out.text);
        assert_eq!(back.text, text);
        assert_eq!(back.restored, 2);
        assert!(back.unresolved.is_empty());
    }

    #[test]
    fn the_same_secret_reads_the_same_and_redaction_is_idempotent() {
        let vault = Vault::new("nonce");
        let r = redactor();
        let once = r.redact(&vault, &format!("{FAKE_GITHUB} and again {FAKE_GITHUB}"));
        let placeholder = vault
            .placeholder_for("github_token", FAKE_GITHUB)
            .to_string();
        assert_eq!(once.text, format!("{placeholder} and again {placeholder}"));
        let twice = r.redact(&vault, &once.text);
        assert_eq!(twice.count, 0);
        assert_eq!(twice.text, once.text);
        assert_eq!(vault.len(), 1);
    }

    #[test]
    fn a_named_group_keeps_the_rest_of_the_match() {
        let vault = Vault::new("nonce");
        let out = redactor().redact(&vault, "password = hunter2hunter2 ok");
        assert!(
            out.text.starts_with("password = «secret:assignment:"),
            "{}",
            out.text
        );
        assert!(out.text.ends_with("» ok"));
    }

    #[test]
    fn an_unknown_placeholder_is_left_and_named() {
        let vault = Vault::new("nonce");
        let text = "run with «secret:github_token:abcdef» please";
        let back = vault.restore(text);
        assert_eq!(back.text, text);
        assert_eq!(back.restored, 0);
        assert_eq!(back.unresolved, vec!["«secret:github_token:abcdef»"]);
        assert!(has_placeholder(text));
        assert!(!has_placeholder("nothing here"));
    }

    #[test]
    fn a_json_value_keeps_its_shape_and_only_its_strings_change() {
        let vault = Vault::new("nonce");
        let mut v = json!({
            "instructions": format!("token {FAKE_GITHUB}"),
            "steps": [{ "output": "fake-value-0001" }, 3, true],
            FAKE_GITHUB: "a key is structure, not content"
        });
        let out = redactor().redact_value(&vault, &mut v);
        assert_eq!(out.count, 2);
        assert!(v["instructions"]
            .as_str()
            .unwrap()
            .starts_with("token «secret:github_token:"));
        assert!(v["steps"][0]["output"]
            .as_str()
            .unwrap()
            .starts_with("«secret:env:FAKE_KEY:"));
        assert_eq!(v["steps"][1], json!(3));
        assert!(v.get(FAKE_GITHUB).is_some());
        let back = vault.restore_value(&mut v);
        assert_eq!(back.restored, 2);
        assert_eq!(v["steps"][0]["output"], json!("fake-value-0001"));
    }

    #[test]
    fn a_disabled_rule_a_short_env_value_and_a_bad_regex_are_handled() {
        let mut rs = rules();
        rs[0].enabled = false;
        rs.push(RedactRule {
            id: "broken".into(),
            label: "broken".into(),
            enabled: true,
            detector: Detector::Pattern {
                regex: "(unclosed".into(),
            },
            origin: Origin::User,
        });
        let (r, problems) =
            Redactor::compile(&rs, &|n| (n == "FAKE_KEY").then(|| "ab".to_string()));
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].rule, "broken");
        let vault = Vault::new("nonce");
        let out = r.redact(&vault, &format!("{FAKE_GITHUB} ab"));
        assert_eq!(
            out.count, 0,
            "disabled token rule and a two-character env value redact nothing"
        );
    }

    #[test]
    fn the_vault_debug_prints_a_count_and_never_a_secret() {
        let vault = Vault::new("nonce");
        vault.placeholder_for("github_token", FAKE_GITHUB);
        let dbg = format!("{vault:?}");
        assert!(dbg.contains("secrets: 1"), "{dbg}");
        assert!(!dbg.contains(FAKE_GITHUB));
        let (r, _) = Redactor::compile(&rules(), &env);
        assert!(!format!("{r:?}").contains("fake-value-0001"));
    }

    #[test]
    fn a_placeholder_parses_only_whole() {
        let p = Placeholder {
            kind: "env:FAKE_KEY".into(),
            tag: "0123ab".into(),
        };
        assert_eq!(Placeholder::parse(&p.to_string()), Some(p.clone()));
        assert_eq!(Placeholder::parse(&format!("x {p}")), None);
    }

    #[test]
    fn two_nonces_give_two_tags_for_one_secret() {
        let a = Vault::new("one").placeholder_for("k", FAKE_GITHUB);
        let b = Vault::new("two").placeholder_for("k", FAKE_GITHUB);
        assert_ne!(a.tag, b.tag);
        assert_eq!(a.tag.len(), 6);
    }

    /// Two detectors landing on overlapping stretches: the second starts
    /// inside the first's match and runs past its end. Dropped whole, its
    /// tail would stay readable; merged, the union is one placeholder and
    /// the round trip is still identity.
    #[test]
    fn overlapping_matches_are_merged_so_no_tail_of_a_secret_stays() {
        let vault = Vault::new("nonce");
        let (r, problems) = Redactor::compile(
            &[
                RedactRule {
                    id: "head".into(),
                    label: "head".into(),
                    enabled: true,
                    detector: Detector::Pattern {
                        regex: r"SECRET-[a-z]{4}".into(),
                    },
                    origin: Origin::Builtin,
                },
                RedactRule {
                    id: "tail".into(),
                    label: "tail".into(),
                    enabled: true,
                    detector: Detector::Pattern {
                        regex: r"[a-z]{4}-TAIL-[0-9]{4}".into(),
                    },
                    origin: Origin::User,
                },
            ],
            &|_| None,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let text = "token SECRET-abcd-TAIL-1234 end";
        let out = r.redact(&vault, text);
        assert!(!out.text.contains("abcd"), "{}", out.text);
        assert!(
            !out.text.contains("1234"),
            "the tail of the second match must not stay: {}",
            out.text
        );
        assert!(!out.text.contains("TAIL"), "{}", out.text);
        assert!(
            out.text.starts_with("token ") && out.text.ends_with(" end"),
            "{}",
            out.text
        );
        assert_eq!(out.count, 1, "one placeholder for the union");
        assert_eq!(
            vault.restore(&out.text).text,
            text,
            "and the round trip is identity"
        );
        // A match wholly inside another adds nothing.
        let (r, _) = Redactor::compile(
            &[
                RedactRule {
                    id: "outer".into(),
                    label: "outer".into(),
                    enabled: true,
                    detector: Detector::Pattern {
                        regex: r"KEY-[0-9]{8}".into(),
                    },
                    origin: Origin::Builtin,
                },
                RedactRule {
                    id: "inner".into(),
                    label: "inner".into(),
                    enabled: true,
                    detector: Detector::Pattern {
                        regex: r"[0-9]{4}".into(),
                    },
                    origin: Origin::User,
                },
            ],
            &|_| None,
        );
        let out = r.redact(&vault, "KEY-12345678");
        assert_eq!(out.count, 1, "{}", out.text);
        assert!(
            !out.text.contains("1234") && !out.text.contains("KEY"),
            "{}",
            out.text
        );
    }

    // added by the coverage pass: redact.rs

    /// Two secrets whose digests begin alike get tags told apart by length;
    /// an empty vault says so; a named group that took no part in a match
    /// redacts nothing; a redactor with no detectors leaves a value alone.
    #[test]
    fn colliding_tags_are_lengthened_and_the_empty_cases_do_nothing() {
        let vault = Vault::new("nonce");
        assert!(vault.is_empty());
        // Find two secrets whose first six hex digits agree, the way the
        // vault digests them: sha256(nonce ‖ 0 ‖ secret).
        let digest6 = |secret: &str| {
            let mut h = Sha256::new();
            h.update(b"nonce");
            h.update([0u8]);
            h.update(secret.as_bytes());
            let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
            hex[..6].to_string()
        };
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut pair = None;
        for i in 0..200_000u32 {
            let secret = format!("secret-{i}");
            if let Some(other) = seen.insert(digest6(&secret), secret.clone()) {
                pair = Some((other, secret));
                break;
            }
        }
        let (a, b) = pair.expect("two secrets with one prefix within the search");
        let pa = vault.placeholder_for("k", &a);
        let pb = vault.placeholder_for("k", &b);
        assert_eq!(pa.tag.len(), 6);
        assert_eq!(pb.tag.len(), 8, "the newer tag is lengthened: {pb:?}");
        assert_ne!(pa.to_string(), pb.to_string());
        assert!(!vault.is_empty());
        assert_eq!(vault.len(), 2);

        let optional_group = vec![RedactRule {
            id: "maybe".into(),
            label: "maybe".into(),
            enabled: true,
            detector: Detector::Pattern {
                regex: r"(?P<secret>sk_live_[a-z0-9]{8})?END".into(),
            },
            origin: Origin::User,
        }];
        let (r, problems) = Redactor::compile(&optional_group, &env);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            r.redact(&vault, "nothing END here").text,
            "nothing END here"
        );

        let (empty, problems) = Redactor::compile(&[], &env);
        assert!(problems.is_empty(), "{problems:?}");
        let mut value = json!({ "k": "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" });
        let before = value.clone();
        let redaction = empty.redact_value(&vault, &mut value);
        assert_eq!(redaction.count, 0);
        assert_eq!(value, before);
    }

    // added by the coverage pass: redact2.rs

    #[test]
    fn an_empty_text_is_left_alone_and_a_placeholder_unresolved_twice_is_named_once() {
        let vault = Vault::new("nonce");
        let r = redactor();
        let empty = r.redact(&vault, "");
        assert_eq!(empty.text, "");
        assert_eq!(empty.count, 0);
        let mut value = json!({
            "a": "«secret:github_token:deadbe»",
            "b": "again «secret:github_token:deadbe»"
        });
        let restored = vault.restore_value(&mut value);
        assert_eq!(restored.restored, 0);
        assert_eq!(
            restored.unresolved,
            vec!["«secret:github_token:deadbe»".to_string()]
        );
    }
}
