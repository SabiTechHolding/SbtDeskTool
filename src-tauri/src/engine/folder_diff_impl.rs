use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;
const HEXDUMP_LINE_LEN: usize = 16;

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

#[derive(Debug, Clone, Serialize)]
pub struct ReadFileResult {
    pub content: String,
}

fn is_likely_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let sample = &bytes[..bytes.len().min(8192)];
    sample.contains(&0x00)
        || sample
            .iter()
            .filter(|&&b| b < 0x08 || (b > 0x0D && b < 0x20))
            .count() as f64
            / sample.len() as f64
            > 0.1
}

fn format_hexdump(bytes: &[u8]) -> String {
    let total = bytes.len().div_ceil(HEXDUMP_LINE_LEN);
    let mut out = String::with_capacity(bytes.len() * 5);
    for (idx, chunk) in bytes.chunks(HEXDUMP_LINE_LEN).enumerate() {
        let offset = idx * HEXDUMP_LINE_LEN;
        use std::fmt::Write;
        let _ = write!(out, "{offset:08x}  ");
        for (i, c) in chunk.iter().enumerate() {
            if i == 8 {
                out.push(' ');
            }
            let _ = write!(out, "{c:02x} ");
        }
        if chunk.len() < HEXDUMP_LINE_LEN {
            let pad = (HEXDUMP_LINE_LEN - chunk.len()) * 3 + if chunk.len() <= 8 { 1 } else { 0 };
            for _ in 0..pad {
                out.push(' ');
            }
        }
        out.push(' ');
        out.push('|');
        for c in chunk {
            out.push(if c.is_ascii_graphic() || *c == b' ' {
                *c as char
            } else {
                '.'
            });
        }
        out.push('|');
        if idx + 1 < total {
            out.push('\n');
        }
    }
    out
}

pub fn read_text_file(path: &Path) -> Result<ReadFileResult, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        let bytes = read_head(path, MAX_PREVIEW_BYTES)?;
        return Ok(if is_likely_binary(&bytes) {
            ReadFileResult {
                content: format_hexdump(&bytes),
            }
        } else {
            ReadFileResult {
                content: decode_text(&bytes)?,
            }
        });
    }
    let bytes = read_head(path, MAX_PREVIEW_BYTES)?;
    if is_likely_binary(&bytes) {
        Ok(ReadFileResult {
            content: format_hexdump(&bytes),
        })
    } else {
        let text = decode_text(&bytes)?;
        Ok(ReadFileResult { content: text })
    }
}

fn read_head(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut handle = file.take(limit);
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024) as usize);
    handle
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    Ok(bytes)
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Ok(text);
    }

    let encoding = encoding_rs::Encoding::for_bom(bytes);
    if let Some((enc, bom_len)) = encoding {
        let (decoded, _, _) = enc.decode(&bytes[bom_len..]);
        return Ok(decoded.into_owned());
    }

    let text = String::from_utf8_lossy(bytes);
    if !text.contains('\u{FFFD}') {
        return Ok(text.into_owned());
    }

    for enc in &[
        encoding_rs::WINDOWS_1252,
        encoding_rs::SHIFT_JIS,
        encoding_rs::EUC_JP,
        encoding_rs::GBK,
        encoding_rs::BIG5,
    ] {
        let (decoded, had_error) = enc.decode_without_bom_handling(bytes);
        if !had_error {
            return Ok(decoded.into_owned());
        }
    }

    let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    Ok(decoded.into_owned())
}
