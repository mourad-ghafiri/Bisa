//! `ArtifactRef`: something an agent made for a person to look at, carried by a
//! message and rendered live where the person reads.
//!
//! # An artifact is an attachment with a purpose
//!
//! The bytes are an attachment's bytes: content-addressed, moved on demand by
//! hash, bounded by [`MAX_ATTACHMENT_BYTES`]. What makes a file an artifact is
//! what rides beside the descriptor — a **title** (the same title on a later
//! post is a new version of the same thing), a **kind** the desktop picks a
//! renderer by, and, when the file was written inside a checkout or a goal's
//! scratch, a **source** naming that root and the path under it, so the reader
//! can open the file itself in the IDE. An attachment is a file handed over as
//! a file; an artifact is a page, a chart, a report, a sheet, a deck.
//!
//! The descriptor rides the typed body of the message event, so a peer that
//! syncs the message knows the title and the kind before it holds a byte, and
//! fetches the bytes exactly as it fetches an attachment's.

use crate::attachment::{AttachmentRef, MAX_ATTACHMENT_BYTES};
use crate::path::{FileScope, RelPath};
use serde::{Deserialize, Serialize};

/// How many artifacts one message may carry. A reply that made more than
/// this has made a folder, not a set of things to look at.
pub const MAX_ARTIFACTS_PER_MESSAGE: usize = 8;

/// A title is a line, not a paragraph.
pub const MAX_ARTIFACT_TITLE_BYTES: usize = 200;

/// What an artifact is, and therefore how it is shown. Closed: the desktop
/// keeps one renderer per word and a test holds its list equal to this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A web page, rendered live in a sandbox.
    Html,
    Svg,
    Image,
    Video,
    Audio,
    Pdf,
    /// A spreadsheet: csv, tsv, xlsx.
    Sheet,
    /// A word-processor document: docx.
    Document,
    /// A deck: pptx.
    Slides,
    Markdown,
    /// A Mermaid diagram source.
    Diagram,
    /// Source code, shown read-only with its language.
    Code,
    /// Structured data: json, yaml, toml, xml.
    Data,
    Text,
    /// Bytes with a name and nothing to draw: the card alone.
    File,
}

impl ArtifactKind {
    pub const ALL: [ArtifactKind; 15] = [
        ArtifactKind::Html,
        ArtifactKind::Svg,
        ArtifactKind::Image,
        ArtifactKind::Video,
        ArtifactKind::Audio,
        ArtifactKind::Pdf,
        ArtifactKind::Sheet,
        ArtifactKind::Document,
        ArtifactKind::Slides,
        ArtifactKind::Markdown,
        ArtifactKind::Diagram,
        ArtifactKind::Code,
        ArtifactKind::Data,
        ArtifactKind::Text,
        ArtifactKind::File,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Html => "html",
            ArtifactKind::Svg => "svg",
            ArtifactKind::Image => "image",
            ArtifactKind::Video => "video",
            ArtifactKind::Audio => "audio",
            ArtifactKind::Pdf => "pdf",
            ArtifactKind::Sheet => "sheet",
            ArtifactKind::Document => "document",
            ArtifactKind::Slides => "slides",
            ArtifactKind::Markdown => "markdown",
            ArtifactKind::Diagram => "diagram",
            ArtifactKind::Code => "code",
            ArtifactKind::Data => "data",
            ArtifactKind::Text => "text",
            ArtifactKind::File => "file",
        }
    }

    /// The kind a name and a declared media type make. The extension decides
    /// first — it is what the maker chose — and the mime family answers for a
    /// name without one. `File` is the honest fallback, never a guess.
    pub fn of(name: &str, mime: &str) -> ArtifactKind {
        // A name with an extension is decided by it alone: one this table
        // does not know is a `File` — a card — whatever type was declared
        // beside it, never a renderer guessed from a header.
        if let Some(ext) = extension_of(name) {
            return kind_of_extension(ext).unwrap_or(ArtifactKind::File);
        }
        let mime = mime
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match mime.as_str() {
            "text/html" => ArtifactKind::Html,
            "image/svg+xml" => ArtifactKind::Svg,
            "application/pdf" => ArtifactKind::Pdf,
            "text/csv" | "text/tab-separated-values" => ArtifactKind::Sheet,
            "text/markdown" => ArtifactKind::Markdown,
            "application/json" | "application/xml" | "text/xml" => ArtifactKind::Data,
            "text/plain" => ArtifactKind::Text,
            m if m.starts_with("image/") => ArtifactKind::Image,
            m if m.starts_with("video/") => ArtifactKind::Video,
            m if m.starts_with("audio/") => ArtifactKind::Audio,
            m if m.starts_with("text/") => ArtifactKind::Text,
            _ => ArtifactKind::File,
        }
    }
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ArtifactKind {
    type Err = crate::CoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ArtifactKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| crate::CoreError::UnknownArtifactKind(s.to_string()))
    }
}

/// The extension, lowercase, without the dot; `None` for a bare name or a
/// dotfile.
fn extension_of(name: &str) -> Option<String> {
    let name = name.rsplit('/').next().unwrap_or(name);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// The source extensions a session commonly writes. Anything else with an
/// unknown extension is a `File`: a reader is shown a card, never a wrong
/// renderer.
const CODE_EXTENSIONS: &[&str] = &[
    "rs",
    "ts",
    "tsx",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "mts",
    "py",
    "rb",
    "go",
    "java",
    "kt",
    "swift",
    "c",
    "h",
    "cpp",
    "hpp",
    "cc",
    "cs",
    "php",
    "sh",
    "bash",
    "zsh",
    "fish",
    "sql",
    "css",
    "scss",
    "less",
    "lua",
    "r",
    "scala",
    "ex",
    "exs",
    "erl",
    "hs",
    "ml",
    "clj",
    "dart",
    "vue",
    "svelte",
    "astro",
    "graphql",
    "proto",
    "dockerfile",
    "makefile",
    "cmake",
    "nix",
    "tf",
    "ini",
    "cfg",
    "conf",
    "env",
    "diff",
    "patch",
];

fn kind_of_extension(ext: String) -> Option<ArtifactKind> {
    Some(match ext.as_str() {
        "html" | "htm" => ArtifactKind::Html,
        "svg" => ArtifactKind::Svg,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "heic" | "ico" => {
            ArtifactKind::Image
        }
        "mp4" | "webm" | "mov" | "m4v" => ArtifactKind::Video,
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" => ArtifactKind::Audio,
        "pdf" => ArtifactKind::Pdf,
        "csv" | "tsv" | "xlsx" | "xls" | "ods" => ArtifactKind::Sheet,
        "docx" => ArtifactKind::Document,
        "pptx" => ArtifactKind::Slides,
        "md" | "mdx" | "markdown" => ArtifactKind::Markdown,
        "mmd" | "mermaid" => ArtifactKind::Diagram,
        "json" | "jsonl" | "yaml" | "yml" | "toml" | "xml" => ArtifactKind::Data,
        "txt" | "log" => ArtifactKind::Text,
        e if CODE_EXTENSIONS.contains(&e) => ArtifactKind::Code,
        _ => return None,
    })
}

/// A media type from a file name — the one table for a file an agent or a
/// person hands over. A short, closed table rather than a mime database:
/// these are the types a conversation carries, and everything else is bytes
/// with a name, which `application/octet-stream` says honestly. Whether a
/// viewer renders anything inline is decided elsewhere, from the bytes.
pub fn mime_of_name(name: &str) -> &'static str {
    let Some(ext) = extension_of(name) else {
        return "application/octet-stream";
    };
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "heic" => "image/heic",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "json" | "jsonl" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "xml" => "application/xml",
        "md" | "mdx" | "markdown" => "text/markdown",
        "mmd" | "mermaid" => "text/vnd.mermaid",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" | "cjs" => "text/javascript",
        "txt" | "log" => "text/plain",
        e if CODE_EXTENSIONS.contains(&e) => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Where an artifact was written, when it was written inside a root the
/// desktop can open: the checkout a session worked in, or a goal's scratch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSource {
    pub scope: FileScope,
    pub id: String,
    pub path: RelPath,
}

/// One artifact on a message: an attachment's descriptor, a title, a kind,
/// and where it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    /// Lowercase hex. The blob's address, and the proof of what it contains.
    pub sha256: String,
    /// What the maker called the file. Display and the named copy only —
    /// never a path anything joins to a directory unsanitised.
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// What it is called where it renders. The same title on a later post is
    /// a new version of the same thing.
    pub title: String,
    pub kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ArtifactSource>,
}

impl ArtifactRef {
    /// An artifact from stored bytes: the kind from the name and the type, the
    /// title given or the name's stem.
    pub fn from_attachment(
        file: AttachmentRef,
        title: Option<String>,
        source: Option<ArtifactSource>,
    ) -> Self {
        let title = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| default_title(&file.name));
        Self {
            kind: ArtifactKind::of(&file.name, &file.mime),
            sha256: file.sha256,
            name: file.name,
            mime: file.mime,
            size: file.size,
            title,
            source,
        }
    }

    /// The attachment this artifact's bytes are.
    pub fn file(&self) -> AttachmentRef {
        AttachmentRef {
            sha256: self.sha256.clone(),
            name: self.name.clone(),
            mime: self.mime.clone(),
            size: self.size,
        }
    }

    /// The hash is a hash, the name and the title are lines, the size is one
    /// attachment's.
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        if !AttachmentRef::is_valid_hash(&self.sha256) {
            return Err(crate::CoreError::InvalidHash(self.sha256.clone()));
        }
        if self.name.trim().is_empty() || self.name.contains('\0') || self.name.contains('/') {
            return Err(crate::CoreError::InvalidArtifactName(self.name.clone()));
        }
        if self.title.trim().is_empty() {
            return Err(crate::CoreError::InvalidArtifactTitle(self.title.clone()));
        }
        if self.title.len() > MAX_ARTIFACT_TITLE_BYTES {
            return Err(crate::CoreError::ArtifactTitleTooLong {
                bytes: self.title.len(),
            });
        }
        if self.size > MAX_ATTACHMENT_BYTES {
            return Err(crate::CoreError::ArtifactTooLarge { bytes: self.size });
        }
        Ok(())
    }
}

/// `dashboard.html` is titled *dashboard*; a dotfile keeps its whole name.
fn default_title(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_with_an_extension_is_decided_by_it_alone_and_a_bare_name_by_its_type() {
        // Known: the extension wins over whatever type was declared beside it.
        assert_eq!(
            ArtifactKind::of("report.pdf", "text/plain"),
            ArtifactKind::Pdf
        );
        assert_eq!(
            ArtifactKind::of("dir.v2/Chart.SVG", "image/png"),
            ArtifactKind::Svg
        );
        assert_eq!(ArtifactKind::of("main.rs", ""), ArtifactKind::Code);
        // Unknown: a card, never a renderer guessed from a header.
        for mime in [
            "image/png",
            "text/html",
            "text/plain",
            "application/pdf",
            "",
        ] {
            assert_eq!(
                ArtifactKind::of("photo.unknownext", mime),
                ArtifactKind::File,
                "{mime}"
            );
        }
        // No extension — none, a dotfile, a trailing dot: the type answers.
        for bare in ["README", ".gitignore", "notes.", "a/b.c/plain"] {
            assert_eq!(
                ArtifactKind::of(bare, "text/markdown"),
                ArtifactKind::Markdown,
                "{bare}"
            );
            assert_eq!(
                ArtifactKind::of(bare, "Image/PNG; charset=binary"),
                ArtifactKind::Image,
                "{bare}"
            );
            assert_eq!(
                ArtifactKind::of(bare, "application/x-unknown"),
                ArtifactKind::File,
                "{bare}"
            );
            assert_eq!(ArtifactKind::of(bare, ""), ArtifactKind::File, "{bare}");
        }
        // Every kind has a name, and every name reads back.
        for kind in ArtifactKind::ALL {
            assert_eq!(kind.as_str().parse::<ArtifactKind>().ok(), Some(kind));
        }
    }

    fn file(name: &str, mime: &str) -> AttachmentRef {
        AttachmentRef {
            sha256: "a".repeat(64),
            name: name.into(),
            mime: mime.into(),
            size: 10,
        }
    }

    #[test]
    fn kind_follows_extension_then_mime() {
        assert_eq!(ArtifactKind::of("index.html", ""), ArtifactKind::Html);
        assert_eq!(
            ArtifactKind::of("chart.SVG", "text/plain"),
            ArtifactKind::Svg
        );
        assert_eq!(ArtifactKind::of("shot.png", ""), ArtifactKind::Image);
        assert_eq!(ArtifactKind::of("demo.mp4", ""), ArtifactKind::Video);
        assert_eq!(ArtifactKind::of("talk.mp3", ""), ArtifactKind::Audio);
        assert_eq!(ArtifactKind::of("report.pdf", ""), ArtifactKind::Pdf);
        assert_eq!(ArtifactKind::of("data.csv", ""), ArtifactKind::Sheet);
        assert_eq!(ArtifactKind::of("book.xlsx", ""), ArtifactKind::Sheet);
        assert_eq!(
            ArtifactKind::of("old.xls", ""),
            ArtifactKind::Sheet,
            "the formats SheetJS reads are sheets too"
        );
        assert_eq!(ArtifactKind::of("open.ods", ""), ArtifactKind::Sheet);
        assert_eq!(ArtifactKind::of("photo.heic", ""), ArtifactKind::Image);
        assert_eq!(ArtifactKind::of("favicon.ico", ""), ArtifactKind::Image);
        assert_eq!(mime_of_name("old.xls"), "application/vnd.ms-excel");
        assert_eq!(mime_of_name("photo.heic"), "image/heic");
        assert_eq!(ArtifactKind::of("memo.docx", ""), ArtifactKind::Document);
        assert_eq!(ArtifactKind::of("deck.pptx", ""), ArtifactKind::Slides);
        assert_eq!(ArtifactKind::of("notes.md", ""), ArtifactKind::Markdown);
        assert_eq!(ArtifactKind::of("flow.mmd", ""), ArtifactKind::Diagram);
        assert_eq!(ArtifactKind::of("main.rs", ""), ArtifactKind::Code);
        assert_eq!(ArtifactKind::of("out.json", ""), ArtifactKind::Data);
        assert_eq!(ArtifactKind::of("run.log", ""), ArtifactKind::Text);
        // No extension: the declared type answers.
        assert_eq!(
            ArtifactKind::of("README", "text/markdown"),
            ArtifactKind::Markdown
        );
        assert_eq!(
            ArtifactKind::of("blob", "image/webp; q=1"),
            ArtifactKind::Image
        );
        assert_eq!(ArtifactKind::of("blob", "video/webm"), ArtifactKind::Video);
        // Unknown both ways is a file, not a guess.
        assert_eq!(
            ArtifactKind::of("thing.xyz", "application/octet-stream"),
            ArtifactKind::File
        );
        assert_eq!(ArtifactKind::of(".env", ""), ArtifactKind::File);
        assert_eq!(
            "sheet".parse::<ArtifactKind>().unwrap(),
            ArtifactKind::Sheet
        );
        assert!("deck".parse::<ArtifactKind>().is_err());
        for k in ArtifactKind::ALL {
            assert_eq!(k.as_str().parse::<ArtifactKind>().unwrap(), k);
        }
    }

    #[test]
    fn mime_of_name_covers_every_kind_and_falls_back_honestly() {
        assert_eq!(mime_of_name("a.png"), "image/png");
        assert_eq!(mime_of_name("a.JPG"), "image/jpeg");
        assert_eq!(mime_of_name("a.svg"), "image/svg+xml");
        assert_eq!(mime_of_name("a.mp4"), "video/mp4");
        assert_eq!(mime_of_name("a.wav"), "audio/wav");
        assert_eq!(mime_of_name("a.pdf"), "application/pdf");
        assert_eq!(mime_of_name("a.csv"), "text/csv");
        assert_eq!(
            mime_of_name("a.xlsx"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(
            mime_of_name("a.docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(
            mime_of_name("a.pptx"),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        );
        assert_eq!(mime_of_name("a.md"), "text/markdown");
        assert_eq!(mime_of_name("a.mmd"), "text/vnd.mermaid");
        assert_eq!(mime_of_name("a.html"), "text/html");
        assert_eq!(mime_of_name("a.json"), "application/json");
        assert_eq!(mime_of_name("a.rs"), "text/plain");
        assert_eq!(mime_of_name("a.txt"), "text/plain");
        assert_eq!(mime_of_name("a.zip"), "application/octet-stream");
        assert_eq!(mime_of_name("noext"), "application/octet-stream");
        assert_eq!(mime_of_name(".gitignore"), "application/octet-stream");
        // Every kind but File has an extension whose mime is not the fallback.
        for k in ArtifactKind::ALL {
            if k == ArtifactKind::File {
                continue;
            }
            let ext = match k {
                ArtifactKind::Html => "html",
                ArtifactKind::Svg => "svg",
                ArtifactKind::Image => "png",
                ArtifactKind::Video => "mp4",
                ArtifactKind::Audio => "mp3",
                ArtifactKind::Pdf => "pdf",
                ArtifactKind::Sheet => "csv",
                ArtifactKind::Document => "docx",
                ArtifactKind::Slides => "pptx",
                ArtifactKind::Markdown => "md",
                ArtifactKind::Diagram => "mmd",
                ArtifactKind::Code => "rs",
                ArtifactKind::Data => "json",
                ArtifactKind::Text => "txt",
                ArtifactKind::File => unreachable!(),
            };
            let name = format!("x.{ext}");
            assert_ne!(mime_of_name(&name), "application/octet-stream", "{name}");
            assert_eq!(ArtifactKind::of(&name, mime_of_name(&name)), k);
        }
    }

    #[test]
    fn the_title_defaults_to_the_stem_and_the_kind_to_the_name() {
        let a = ArtifactRef::from_attachment(file("dashboard.html", "text/html"), None, None);
        assert_eq!(a.title, "dashboard");
        assert_eq!(a.kind, ArtifactKind::Html);
        assert!(a.validate().is_ok());
        let b = ArtifactRef::from_attachment(
            file("x.csv", "text/csv"),
            Some("  Q3 numbers ".into()),
            None,
        );
        assert_eq!(b.title, "Q3 numbers");
        let c = ArtifactRef::from_attachment(file(".profile", ""), Some("   ".into()), None);
        assert_eq!(c.title, ".profile");
        assert_eq!(b.file(), file("x.csv", "text/csv"));
    }

    #[test]
    fn a_post_refuses_a_ninth_artifact_and_a_bad_hash() {
        let one = ArtifactRef::from_attachment(file("a.png", "image/png"), None, None);
        let body = crate::MessageBody::Post {
            text: String::new(),
            context: vec![],
            artifacts: vec![one.clone(); MAX_ARTIFACTS_PER_MESSAGE + 1],
            thinking: None,
            said: None,
        };
        assert!(matches!(
            body.validate(),
            Err(crate::CoreError::TooManyArtifacts(9))
        ));
        let eight = crate::MessageBody::Post {
            text: String::new(),
            context: vec![],
            artifacts: vec![one.clone(); MAX_ARTIFACTS_PER_MESSAGE],
            thinking: None,
            said: None,
        };
        assert!(eight.validate().is_ok());
        let bad = ArtifactRef {
            sha256: "nope".into(),
            ..one.clone()
        };
        assert!(matches!(
            bad.validate(),
            Err(crate::CoreError::InvalidHash(_))
        ));
        let long = ArtifactRef {
            title: "t".repeat(MAX_ARTIFACT_TITLE_BYTES + 1),
            ..one.clone()
        };
        assert!(matches!(
            long.validate(),
            Err(crate::CoreError::ArtifactTitleTooLong { .. })
        ));
        let slash = ArtifactRef {
            name: "a/b.png".into(),
            ..one.clone()
        };
        assert!(matches!(
            slash.validate(),
            Err(crate::CoreError::InvalidArtifactName(_))
        ));
        let huge = ArtifactRef {
            size: MAX_ATTACHMENT_BYTES + 1,
            ..one
        };
        assert!(matches!(
            huge.validate(),
            Err(crate::CoreError::ArtifactTooLarge { .. })
        ));
    }

    #[test]
    fn artifacts_serialise_inside_the_body_and_absent_when_empty() {
        let plain = serde_json::to_value(crate::MessageBody::post("hi")).unwrap();
        assert!(plain.get("artifacts").is_none());
        let source = ArtifactSource {
            scope: FileScope::Workstream,
            id: "01J".into(),
            path: RelPath::new("out/chart.svg").unwrap(),
        };
        let a = ArtifactRef::from_attachment(
            file("chart.svg", "image/svg+xml"),
            Some("Chart".into()),
            Some(source),
        );
        let body = crate::MessageBody::Post {
            text: "here".into(),
            context: vec![],
            artifacts: vec![a.clone()],
            thinking: None,
            said: None,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["body"], "post");
        assert_eq!(json["artifacts"][0]["title"], "Chart");
        assert_eq!(json["artifacts"][0]["kind"], "svg");
        assert_eq!(json["artifacts"][0]["source"]["scope"], "workstream");
        assert_eq!(json["artifacts"][0]["source"]["path"], "out/chart.svg");
        assert_eq!(
            serde_json::from_value::<crate::MessageBody>(json).unwrap(),
            body
        );
        let no_source = serde_json::to_value(ArtifactRef { source: None, ..a }).unwrap();
        assert!(no_source.get("source").is_none());
    }

    // added by the coverage pass: artifact.rs

    #[test]
    fn an_artifact_needs_a_title_that_is_words() {
        let blank = ArtifactRef {
            sha256: "0".repeat(64),
            name: "report.pdf".into(),
            mime: "application/pdf".into(),
            size: 1,
            title: " ".into(),
            kind: ArtifactKind::Pdf,
            source: None,
        };
        assert!(matches!(
            blank.validate(),
            Err(crate::CoreError::InvalidArtifactTitle(t)) if t == " "
        ));
    }

    // added by the coverage pass: b5-artifact.rs
    #[test]
    fn a_kind_prints_as_its_word() {
        assert_eq!(ArtifactKind::Html.to_string(), ArtifactKind::Html.as_str());
        assert_eq!(ArtifactKind::File.to_string(), "file");
    }
}
