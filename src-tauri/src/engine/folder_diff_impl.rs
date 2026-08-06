use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct FolderDiffEntry {
    pub relative_path: String,
    pub status: String,
    pub left_path: Option<String>,
    pub right_path: Option<String>,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
}

fn ensure_folder(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(format!("{} is not a folder", path.display()))
    }
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    for item in fs::read_dir(directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?
    {
        let item = item.map_err(|error| format!("Could not read directory entry: {error}"))?;
        let path = item.path();
        let file_type = item
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("Could not calculate relative path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, path);
        }
    }
    Ok(())
}

fn files_are_equal(left: &Path, right: &Path) -> Result<bool, String> {
    if fs::metadata(left)
        .map_err(|error| format!("Could not inspect {}: {error}", left.display()))?
        .len()
        != fs::metadata(right)
            .map_err(|error| format!("Could not inspect {}: {error}", right.display()))?
            .len()
    {
        return Ok(false);
    }

    let mut left_reader = BufReader::new(
        fs::File::open(left)
            .map_err(|error| format!("Could not open {}: {error}", left.display()))?,
    );
    let mut right_reader = BufReader::new(
        fs::File::open(right)
            .map_err(|error| format!("Could not open {}: {error}", right.display()))?,
    );
    let mut left_buffer = [0u8; 64 * 1024];
    let mut right_buffer = [0u8; 64 * 1024];
    loop {
        let left_read = left_reader
            .read(&mut left_buffer)
            .map_err(|error| format!("Could not read {}: {error}", left.display()))?;
        let right_read = right_reader
            .read(&mut right_buffer)
            .map_err(|error| format!("Could not read {}: {error}", right.display()))?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

pub fn compare_folders(
    left_root: &Path,
    right_root: &Path,
) -> Result<Vec<FolderDiffEntry>, String> {
    ensure_folder(left_root)?;
    ensure_folder(right_root)?;
    let mut left_files = BTreeMap::new();
    let mut right_files = BTreeMap::new();
    collect_files(left_root, left_root, &mut left_files)?;
    collect_files(right_root, right_root, &mut right_files)?;
    let paths = left_files
        .keys()
        .chain(right_files.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    paths
        .into_iter()
        .map(|relative_path| {
            let left_path = left_files.get(&relative_path);
            let right_path = right_files.get(&relative_path);
            let left_size = left_path
                .map(|path| {
                    fs::metadata(path)
                        .map(|metadata| metadata.len())
                        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))
                })
                .transpose()?;
            let right_size = right_path
                .map(|path| {
                    fs::metadata(path)
                        .map(|metadata| metadata.len())
                        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))
                })
                .transpose()?;
            let status = match (left_path, right_path) {
                (Some(left), Some(right)) if files_are_equal(left, right)? => "equal",
                (Some(_), Some(_)) => "different",
                (Some(_), None) => "left_only",
                (None, Some(_)) => "right_only",
                (None, None) => unreachable!("paths is built from both maps"),
            };
            Ok(FolderDiffEntry {
                relative_path,
                status: status.into(),
                left_path: left_path.map(|path| path.to_string_lossy().into_owned()),
                right_path: right_path.map(|path| path.to_string_lossy().into_owned()),
                left_size,
                right_size,
            })
        })
        .collect()
}

pub fn read_text_file(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Err(format!(
            "{} is larger than the 4 MB preview limit",
            path.display()
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("{} is not a UTF-8 text file", path.display()))
}
