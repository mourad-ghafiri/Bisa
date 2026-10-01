//! The bytes a `file` parameter names, read through a port the engine fills:
//! the crate never touches a disk, and a test hands it bytes from memory.

use async_trait::async_trait;

/// A file is never read past this: an upload larger than the cap is a
/// streaming job this shape does not do, and the refusal says so.
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;

/// One file's bytes and what a part may announce about them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileData {
    /// The file's own name, for a part that announces none.
    pub filename: String,
    /// The media type the reader knows, if any.
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// Where a `file` parameter's path is read — the engine's run checkout, a
/// test's map. The answer is the bytes, or the reason in one sentence; the
/// client names the parameter.
#[async_trait]
pub trait Files: Send + Sync {
    async fn read(&self, path: &str) -> Result<FileData, String>;
}

/// A call with no checkout — a poll, an account check. Every file is refused.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoFiles;

#[async_trait]
impl Files for NoFiles {
    async fn read(&self, _path: &str) -> Result<FileData, String> {
        Err("this call has no checkout to read a file from".into())
    }
}
