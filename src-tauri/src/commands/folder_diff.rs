use crate::engine::folder_diff::{
    compare_folders as compare, read_text_file, FolderDiffEntry, ReadFileResult,
};
use std::path::PathBuf;

#[tauri::command]
pub async fn compare_folders(
    left_root: String,
    right_root: String,
) -> Result<Vec<FolderDiffEntry>, String> {
    tokio::task::spawn_blocking(move || {
        compare(&PathBuf::from(left_root), &PathBuf::from(right_root))
    })
    .await
    .map_err(|error| format!("Folder comparison task failed: {error}"))?
}

#[tauri::command]
pub async fn read_folder_diff_file(path: String) -> Result<ReadFileResult, String> {
    tokio::task::spawn_blocking(move || read_text_file(&PathBuf::from(path)))
        .await
        .map_err(|error| format!("File preview task failed: {error}"))?
}
