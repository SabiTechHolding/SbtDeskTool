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
    let bytes = read_head(path, MAX_PREVIEW_BYTES)?;
    if let Some(text) = decode_probably_text(&bytes) {
        Ok(ReadFileResult { content: text })
    } else {
        Ok(ReadFileResult {
            content: format_hexdump(&bytes),
        })
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

fn decode_probably_text(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return Some(String::new());
    }

    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (decoded, _, _) = enc.decode(&bytes[bom_len..]);
        return Some(decoded.into_owned());
    }

    if bytes[..bytes.len().min(8192)].contains(&0x00) {
        return decode_utf16_heuristic(bytes);
    }

    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Some(text);
    }

    if let Some(text) = decode_utf16_heuristic(bytes) {
        return Some(text);
    }

    if is_likely_binary(bytes) {
        return None;
    }

    Some(decode_legacy(bytes))
}

fn decode_utf16_heuristic(bytes: &[u8]) -> Option<String> {
    let sample = &bytes[..bytes.len().min(65536)];
    if sample.len() < 4 {
        return None;
    }
    let char_count = sample.len() / 2;
    let is_text_byte = |b: u8| b == b'\n' || b == b'\r' || b == b'\t' || (0x20..=0x7E).contains(&b);
    let (le_text, be_text) = sample
        .chunks_exact(2)
        .fold((0usize, 0usize), |(le, be), pair| {
            (
                le + usize::from(is_text_byte(pair[0]) && pair[1] == 0x00),
                be + usize::from(is_text_byte(pair[1]) && pair[0] == 0x00),
            )
        });
    let endianness = if le_text >= char_count / 2 && le_text > be_text * 2 + 2 {
        encoding_rs::UTF_16LE
    } else if be_text >= char_count / 2 && be_text > le_text * 2 + 2 {
        encoding_rs::UTF_16BE
    } else {
        return None;
    };
    let (decoded, had_error) = endianness.decode_without_bom_handling(bytes);
    if had_error {
        return None;
    }
    Some(decoded.into_owned())
}

fn is_likely_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    sample.contains(&0x00)
        || sample
            .iter()
            .filter(|&&b| b < 0x08 || (b > 0x0D && b < 0x20))
            .count() as f64
            / sample.len() as f64
            > 0.1
}

fn decode_legacy(bytes: &[u8]) -> String {
    let candidates = [
        encoding_rs::GBK,
        encoding_rs::SHIFT_JIS,
        encoding_rs::EUC_JP,
        encoding_rs::BIG5,
        encoding_rs::WINDOWS_1252,
    ];
    let is_japanese_enc =
        |enc: &'static encoding_rs::Encoding| matches!(enc.name(), "Shift_JIS" | "EUC-JP");
    let mut best_key: (i64, bool, u8) = (i64::MIN, false, 0);
    let mut best_text = String::new();
    for enc in candidates {
        let (decoded, _) = enc.decode_without_bom_handling(bytes);
        let mut score: i64 = 0;
        let mut kana = 0usize;
        for ch in decoded.chars() {
            score += match ch {
                '\n' | '\r' | '\t' => 1,
                '\u{20}'..='\u{7E}' => 1,
                '\u{FFFD}' => -2,
                '\u{3040}'..='\u{30FF}' => {
                    kana += 1;
                    6
                }
                '\u{3000}'..='\u{303F}' => 3,
                '\u{4E00}'..='\u{9FFF}' => 2,
                '\u{FF00}'..='\u{FF5F}' => 3,
                c if (c as u32) < 0x20 || (c as u32) == 0x7F => -4,
                _ => 0,
            };
        }
        let (round, _, _) = enc.encode(&decoded);
        if round == bytes {
            score += 100;
        }
        let has_kana = kana > 0;
        let jp_rank = if is_japanese_enc(enc) { 0 } else { 1 };
        let tie = if has_kana { 1 - jp_rank } else { jp_rank };
        let key = (score, has_kana, tie);
        if key > best_key {
            best_key = key;
            best_text = decoded.into_owned();
        }
    }
    best_text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(s: &str) -> String {
        decode_probably_text(s.as_bytes()).unwrap()
    }

    #[test]
    fn utf8_passthrough() {
        assert_eq!(
            decode("SELECT id, name\nFROM users;\n"),
            "SELECT id, name\nFROM users;\n"
        );
    }

    #[test]
    fn utf16_le_without_bom_detected_as_text() {
        let text = "SELECT id, name FROM users;";
        let mut bytes = Vec::new();
        for b in text.bytes() {
            bytes.push(b);
            bytes.push(0x00);
        }
        assert_eq!(decode_probably_text(&bytes).unwrap(), text);
    }

    #[test]
    fn utf16_be_without_bom_detected_as_text() {
        let text = "SELECT name FROM roles;";
        let mut bytes = Vec::new();
        for b in text.bytes() {
            bytes.push(0x00);
            bytes.push(b);
        }
        assert_eq!(decode_probably_text(&bytes).unwrap(), text);
    }

    #[test]
    fn null_bytes_are_binary() {
        let bytes = [0x00u8, 0x01, 0x02, 0x7f, 0x00, 0x00, 0x11, 0x22];
        assert!(decode_probably_text(&bytes).is_none());
    }

    #[test]
    fn gbk_selects_over_windows1252() {
        let source = "SELECT id FROM 用户表;";
        let (gbk, _, _) = encoding_rs::GBK.encode(source);
        assert_eq!(decode_probably_text(&gbk).unwrap(), source);
    }

    #[test]
    fn shift_jis_japanese_wins_over_gbk() {
        let source = "SELECT id FROM ユーザー WHERE 名前 = '山田';";
        let (sjs, _, _) = encoding_rs::SHIFT_JIS.encode(source);
        assert_eq!(decode_probably_text(&sjs).unwrap(), source);
    }

    #[test]
    fn euc_jp_japanese_wins_over_gbk() {
        let source = "SELECT id FROM 注文表 WHERE 日付 = '2026-08-07' です;";
        let (euc, _, _) = encoding_rs::EUC_JP.encode(source);
        assert_eq!(decode_probably_text(&euc).unwrap(), source);
    }
}
