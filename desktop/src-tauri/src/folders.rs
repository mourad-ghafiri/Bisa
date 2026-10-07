//! Two doors that need no node: the diagnostic log's folder and the
//! workspace's data folder, opened in the file manager. The webview's other
//! roads to a folder go through the node (`GET /workspace`, `GET /log`),
//! which is the one thing a person reading a crash card or a node that will
//! not start does not have. The shell knows both folders on its own: the
//! log's from the file it is writing (`bisa_log::Handle::root`), and both
//! from what the `bisa` binary said at the boot (`sidecar::WorkspaceDirs`).

use bisa_log::Handle;
use std::path::PathBuf;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::sidecar::NodeState;

/// The folder to open: the one being written when there is one — the
/// fallback folder under the app's own when no workspace was named — else
/// the one the binary named; none is the error.
fn folder_to_open(writing: Option<PathBuf>, named: Option<PathBuf>) -> Result<PathBuf, String> {
    writing
        .or(named)
        .ok_or_else(|| "no folder is known yet: no bisa binary named the workspace".to_string())
}

/// Open the diagnostic log's folder in the file manager.
#[tauri::command]
pub fn reveal_logs_dir(
    app: tauri::AppHandle,
    log: State<'_, Handle>,
    node: State<'_, NodeState>,
) -> Result<(), String> {
    let dir = folder_to_open(log.root(), node.dirs().map(|d| d.logs_dir))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Open the workspace's data folder in the file manager.
#[tauri::command]
pub fn reveal_data_dir(app: tauri::AppHandle, node: State<'_, NodeState>) -> Result<(), String> {
    let dir = folder_to_open(None, node.dirs().map(|d| d.data_dir))?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_being_written_wins_then_the_one_the_binary_named_then_nothing() {
        assert_eq!(
            folder_to_open(Some("/w".into()), Some("/n".into())).unwrap(),
            PathBuf::from("/w")
        );
        assert_eq!(
            folder_to_open(None, Some("/n".into())).unwrap(),
            PathBuf::from("/n")
        );
        let refused = folder_to_open(None, None).unwrap_err();
        assert!(refused.contains("no folder is known yet"), "{refused}");
    }
}
