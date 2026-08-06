#[path = "folder_diff_impl.rs"]
mod folder_diff_impl;
pub use folder_diff_impl::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sbt-folder-diff-{unique}"))
    }

    #[test]
    fn classifies_recursive_files_by_relative_path_and_content() {
        let root = fixture_root();
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(left.join("nested")).unwrap();
        fs::create_dir_all(right.join("nested")).unwrap();
        fs::write(left.join("same.txt"), "same").unwrap();
        fs::write(right.join("same.txt"), "same").unwrap();
        fs::write(left.join("nested/different.txt"), "left").unwrap();
        fs::write(right.join("nested/different.txt"), "right").unwrap();
        fs::write(left.join("left-only.txt"), "left").unwrap();
        fs::write(right.join("right-only.txt"), "right").unwrap();

        let entries = compare_folders(&left, &right).unwrap();
        let statuses = entries
            .iter()
            .map(|entry| (entry.relative_path.as_str(), entry.status.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(
            statuses,
            vec![
                ("left-only.txt", "left_only"),
                ("nested/different.txt", "different"),
                ("right-only.txt", "right_only"),
                ("same.txt", "equal"),
            ]
        );
        assert_eq!(entries[1].left_size, Some(4));
        assert_eq!(entries[1].right_size, Some(5));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_a_missing_folder() {
        let root = fixture_root();
        fs::create_dir_all(&root).unwrap();
        let error = compare_folders(&root, &root.join("missing")).unwrap_err();
        assert!(error.contains("not a folder"));
        fs::remove_dir_all(root).unwrap();
    }
}
