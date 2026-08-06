use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

fn diff_paths_from_args(args: &[String], cwd: &Path) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(|argument| {
            let path = PathBuf::from(argument);
            if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            }
        })
        .filter(|path| path.is_file() || path.is_dir())
        .take(2)
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

pub fn open_diff_paths(app: &AppHandle, args: &[String], cwd: &str) {
    let paths = diff_paths_from_args(args, Path::new(cwd));
    if paths.len() != 2 {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit("diff-open-paths", serde_json::json!({ "paths": paths }));
}

#[tauri::command]
pub fn get_initial_diff_paths() -> Vec<String> {
    let args = std::env::args().collect::<Vec<_>>();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    diff_paths_from_args(&args, &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uses_two_existing_positional_paths() {
        let root = std::env::temp_dir();
        let args = vec![
            "app".into(),
            root.to_string_lossy().into_owned(),
            "--ignored".into(),
            root.to_string_lossy().into_owned(),
        ];
        assert_eq!(
            diff_paths_from_args(&args, Path::new(".")),
            vec![
                root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned()
            ]
        );
    }
}
