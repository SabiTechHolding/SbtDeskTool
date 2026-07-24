use crate::commands::{providers, settings::SettingsState};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateExcelRequest {
    pub task_id: String,
    pub input_path: String,
    pub output_path: String,
    pub source_lang: String,
    pub target_lang: String,
    pub engine: String,
    #[serde(default = "default_true")]
    pub skip_formulas: bool,
    #[serde(default = "default_true")]
    pub skip_product_codes: bool,
    #[serde(default)]
    pub excluded_sheets: Vec<String>,
    #[serde(default)]
    pub excluded_columns: Vec<String>,
    #[serde(default)]
    pub excluded_ranges: Vec<String>,
}

static CANCELLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
const GOOGLE_JOB_ITEMS: usize = 20;
const AI_JOB_ITEMS: usize = 20;
const AI_JOB_CHARACTERS: usize = 12_000;
const CHECKPOINT_VERSION: u32 = 1;

#[tauri::command]
pub fn cancel_excel_translation(task_id: String) -> Result<(), String> {
    CANCELLED
        .get_or_init(Default::default)
        .lock()
        .map_err(|e| e.to_string())?
        .insert(task_id);
    Ok(())
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateExcelProgress {
    pub phase: &'static str,
    pub completed: usize,
    pub total: usize,
    pub translated: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateExcelResult {
    pub output_path: String,
    pub scanned: usize,
    pub translated: usize,
    pub skipped: usize,
    pub unique_texts: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    pub log_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelWorkbookInfo {
    pub sheets: Vec<String>,
    pub cells: usize,
}

#[tauri::command]
pub fn inspect_excel_file(path: String) -> Result<ExcelWorkbookInfo, String> {
    let input = PathBuf::from(path);
    if !input.is_file() {
        return Err("Input Excel file does not exist".into());
    }
    let book = umya_spreadsheet::reader::xlsx::read(&input)
        .map_err(|e| format!("Unable to read Excel file: {e}"))?;
    Ok(ExcelWorkbookInfo {
        sheets: book
            .sheet_collection()
            .iter()
            .map(|s| s.name().to_string())
            .collect(),
        cells: book
            .sheet_collection()
            .iter()
            .map(|s| s.cells().len())
            .sum(),
    })
}

#[tauri::command]
pub fn has_excel_translation_checkpoint(output_path: String) -> bool {
    translation_checkpoint_path(Path::new(&output_path)).is_file()
}

#[derive(Debug)]
struct CellTarget {
    sheet: usize,
    sheet_name: String,
    coordinate: String,
    source: String,
}

#[derive(Debug, PartialEq)]
struct CellRange {
    sheet: Option<String>,
    start_col: u32,
    end_col: u32,
    start_row: u32,
    end_row: u32,
}

impl CellRange {
    fn contains(&self, sheet: &str, column: u32, row: u32) -> bool {
        self.sheet
            .as_deref()
            .map(|expected| expected == sheet)
            .unwrap_or(true)
            && (self.start_col..=self.end_col).contains(&column)
            && (self.start_row..=self.end_row).contains(&row)
    }
}

#[derive(Clone)]
struct TranslationJobContext {
    source_lang: String,
    target_lang: String,
    connections: Vec<providers::ProviderConnection>,
    strategy: u8,
    policy: providers::TranslationPolicy,
}

#[derive(Debug, Deserialize, Serialize)]
struct TranslationCheckpoint {
    version: u32,
    signature: String,
    translations: HashMap<String, String>,
}

#[tauri::command]
pub async fn translate_excel_file(
    request: TranslateExcelRequest,
    state: State<'_, SettingsState>,
    app: AppHandle,
) -> Result<TranslateExcelResult, String> {
    let input = PathBuf::from(&request.input_path);
    let output = PathBuf::from(&request.output_path);
    validate_paths(&input, &output)?;

    let mut book = umya_spreadsheet::reader::xlsx::read(&input)
        .map_err(|error| format!("Unable to read Excel file: {error}"))?;
    let mut targets = Vec::new();
    let mut scanned = 0usize;
    let mut skipped = 0usize;
    let excluded_columns: HashSet<u32> = request
        .excluded_columns
        .iter()
        .filter_map(|value| parse_column(value))
        .collect();
    let excluded_ranges = request
        .excluded_ranges
        .iter()
        .map(|value| {
            parse_range(value).ok_or_else(|| {
                format!("Invalid excluded range '{value}'. Use A1:D20 or Sheet1!A1:D20.")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (sheet_index, sheet) in book.sheet_collection().iter().enumerate() {
        if request
            .excluded_sheets
            .iter()
            .any(|name| name == sheet.name())
        {
            continue;
        }
        for cell in sheet.cells_sorted() {
            scanned += 1;
            if excluded_columns.contains(&cell.coordinate().col_num()) {
                skipped += 1;
                continue;
            }
            if excluded_ranges.iter().any(|range| {
                range.contains(
                    sheet.name(),
                    cell.coordinate().col_num(),
                    cell.coordinate().row_num(),
                )
            }) {
                skipped += 1;
                continue;
            }
            let value = cell.value().into_owned();
            if !eligible(
                cell,
                &value,
                request.skip_formulas,
                request.skip_product_codes,
            ) {
                skipped += 1;
                continue;
            }
            targets.push(CellTarget {
                sheet: sheet_index,
                sheet_name: sheet.name().to_string(),
                coordinate: cell.coordinate().to_string(),
                source: value,
            });
        }
    }

    let (strategy, connections, policy) = {
        let settings = state.0.lock().map_err(|error| error.to_string())?;
        let strategy = settings
            .get("network_strategy")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u8;
        let primary = providers::connection(&settings, &request.engine)?;
        let mut connections = vec![primary];
        connections.extend(providers::fallback_connections(&settings, &request.engine));
        let policy = providers::translation_policy(&settings);
        (strategy, connections, policy)
    };
    let mut unique = Vec::<String>::new();
    let mut positions = HashMap::<String, usize>::new();
    for target in &targets {
        if !positions.contains_key(&target.source) {
            positions.insert(target.source.clone(), unique.len());
            unique.push(target.source.clone());
        }
    }
    let checkpoint_path = translation_checkpoint_path(&output);
    let checkpoint_signature = translation_checkpoint_signature(&request, &input)?;
    let mut checkpoint = load_translation_checkpoint(&checkpoint_path, &checkpoint_signature)?;
    let concurrency = connections
        .first()
        .map(|provider| provider.concurrency)
        .unwrap_or(1)
        .clamp(1, 32);
    let primary_uses_prompt_batch = connections.first().is_some_and(|provider| {
        matches!(
            provider.id.as_str(),
            "gemini" | "openai" | "claude" | "local"
        )
    });
    let job_size = match connections.first().map(|provider| provider.id.as_str()) {
        Some("google") => GOOGLE_JOB_ITEMS,
        Some("gemini" | "openai" | "claude" | "local") => AI_JOB_ITEMS,
        _ => 1,
    };
    let job_character_limit = if primary_uses_prompt_batch {
        AI_JOB_CHARACTERS
    } else {
        usize::MAX
    };
    let mut translations = vec![None; unique.len()];
    let mut unique_errors = vec![None; unique.len()];
    for (index, text) in unique.iter().enumerate() {
        if let Some(value) = checkpoint.translations.get(text) {
            translations[index] = Some(value.clone());
        }
    }
    let mut successful_unique = translations.iter().filter(|value| value.is_some()).count();
    let mut completed_unique = successful_unique;
    emit_progress(
        &app,
        if successful_unique > 0 {
            "resuming"
        } else {
            "translating"
        },
        completed_unique,
        unique.len(),
        successful_unique,
        skipped,
        0,
    );
    let pending_indices = translations
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.is_none().then_some(index))
        .collect::<Vec<_>>();
    let mut next_pending = 0usize;
    let mut jobs = tokio::task::JoinSet::new();
    let job_context = TranslationJobContext {
        source_lang: request.source_lang.clone(),
        target_lang: request.target_lang.clone(),
        connections,
        strategy,
        policy,
    };
    while next_pending < pending_indices.len() && jobs.len() < concurrency {
        let end = translation_job_end(
            &pending_indices,
            next_pending,
            job_size,
            job_character_limit,
            &unique,
        );
        spawn_translation_job(
            &mut jobs,
            pending_indices[next_pending..end]
                .iter()
                .map(|&index| (index, unique[index].clone()))
                .collect(),
            job_context.clone(),
        );
        next_pending = end;
    }
    while let Some(completed) = jobs.join_next().await {
        if is_cancelled(&request.task_id)? {
            jobs.abort_all();
            clear_cancelled(&request.task_id);
            return Err(format!(
                "Translation cancelled. Run Resume to continue from checkpoint: {}",
                checkpoint_path.display()
            ));
        }
        let completed = completed.map_err(|error| format!("Translation worker failed: {error}"))?;
        let mut stop_error = None;
        for (index, result) in completed {
            match result {
                Ok(value) => {
                    checkpoint
                        .translations
                        .insert(unique[index].clone(), value.translated.clone());
                    translations[index] = Some(value.translated);
                    successful_unique += 1;
                }
                Err(error) => {
                    unique_errors[index] = Some(error.clone());
                    stop_error.get_or_insert(error);
                }
            }
            completed_unique += 1;
        }
        save_translation_checkpoint(&checkpoint_path, &checkpoint)?;
        emit_progress(
            &app,
            "translating",
            completed_unique,
            unique.len(),
            successful_unique,
            skipped,
            unique_errors.iter().filter(|error| error.is_some()).count(),
        );
        if let Some(error) = stop_error {
            jobs.abort_all();
            clear_cancelled(&request.task_id);
            return Err(format!(
                "API translation stopped early after retries and fallbacks failed: {error}. Run Resume to continue from checkpoint: {}",
                checkpoint_path.display()
            ));
        }
        if next_pending < pending_indices.len() {
            let end = translation_job_end(
                &pending_indices,
                next_pending,
                job_size,
                job_character_limit,
                &unique,
            );
            spawn_translation_job(
                &mut jobs,
                pending_indices[next_pending..end]
                    .iter()
                    .map(|&index| (index, unique[index].clone()))
                    .collect(),
                job_context.clone(),
            );
            next_pending = end;
        }
    }

    let mut translated_cells = 0usize;
    let mut failed_cells = 0usize;
    let mut errors = Vec::new();
    for target in &targets {
        let index = positions[&target.source];
        if let Some(value) = &translations[index] {
            book.sheet_mut(target.sheet)
                .map_err(|error| error.to_string())?
                .cell_mut(target.coordinate.as_str())
                .set_value_string(value.clone());
            translated_cells += 1;
        } else {
            failed_cells += 1;
            let reason = unique_errors[index]
                .as_deref()
                .unwrap_or("Unknown translation error");
            errors.push(format!(
                "{}!{}: {}",
                target.sheet_name, target.coordinate, reason
            ));
        }
    }
    let temp = temporary_output(&output);
    emit_progress(
        &app,
        "writing",
        unique.len(),
        unique.len(),
        translated_cells,
        skipped,
        failed_cells,
    );
    umya_spreadsheet::writer::xlsx::write(&book, &temp)
        .map_err(|error| format!("Unable to write translated workbook: {error}"))?;
    std::fs::rename(&temp, &output)
        .map_err(|error| format!("Unable to finalize output file: {error}"))?;
    let log_path = translation_log_path(&output);
    write_translation_log(
        &log_path,
        &request,
        scanned,
        translated_cells,
        skipped,
        failed_cells,
        &errors,
    )?;
    if checkpoint_path.exists() {
        std::fs::remove_file(&checkpoint_path)
            .map_err(|error| format!("Unable to remove completed checkpoint: {error}"))?;
    }
    emit_progress(
        &app,
        "complete",
        unique.len(),
        unique.len(),
        translated_cells,
        skipped,
        failed_cells,
    );
    clear_cancelled(&request.task_id);
    Ok(TranslateExcelResult {
        output_path: output.to_string_lossy().into_owned(),
        scanned,
        translated: translated_cells,
        skipped,
        unique_texts: unique.len(),
        failed: failed_cells,
        errors,
        log_path: log_path.to_string_lossy().into_owned(),
    })
}

fn translation_job_end(
    pending_indices: &[usize],
    start: usize,
    max_items: usize,
    max_characters: usize,
    texts: &[String],
) -> usize {
    let maximum_end = (start + max_items.max(1)).min(pending_indices.len());
    let mut end = start;
    let mut characters = 0usize;
    while end < maximum_end {
        let next_characters = texts[pending_indices[end]].chars().count();
        if end > start && characters.saturating_add(next_characters) > max_characters {
            break;
        }
        characters = characters.saturating_add(next_characters);
        end += 1;
    }
    end.max(start + 1).min(pending_indices.len())
}

fn translation_checkpoint_path(output: &Path) -> PathBuf {
    let file_name = output
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("translated.xlsx");
    output.with_file_name(format!("{file_name}.translation-resume.json"))
}

fn translation_checkpoint_signature(
    request: &TranslateExcelRequest,
    input: &Path,
) -> Result<String, String> {
    let metadata = std::fs::metadata(input)
        .map_err(|error| format!("Unable to inspect input file: {error}"))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs());
    serde_json::to_string(&serde_json::json!({
        "inputPath": request.input_path,
        "inputLength": metadata.len(),
        "inputModified": modified,
        "sourceLang": request.source_lang,
        "targetLang": request.target_lang,
        "engine": request.engine,
        "skipFormulas": request.skip_formulas,
        "skipProductCodes": request.skip_product_codes,
        "excludedSheets": request.excluded_sheets,
        "excludedColumns": request.excluded_columns,
        "excludedRanges": request.excluded_ranges,
    }))
    .map_err(|error| error.to_string())
}

fn load_translation_checkpoint(
    path: &Path,
    signature: &str,
) -> Result<TranslationCheckpoint, String> {
    if !path.exists() {
        return Ok(TranslationCheckpoint {
            version: CHECKPOINT_VERSION,
            signature: signature.into(),
            translations: HashMap::new(),
        });
    }
    let serialized = std::fs::read_to_string(path)
        .map_err(|error| format!("Unable to read translation checkpoint: {error}"))?;
    let checkpoint: TranslationCheckpoint = serde_json::from_str(&serialized)
        .map_err(|error| format!("Invalid translation checkpoint: {error}"))?;
    if checkpoint.version != CHECKPOINT_VERSION || checkpoint.signature != signature {
        return Ok(TranslationCheckpoint {
            version: CHECKPOINT_VERSION,
            signature: signature.into(),
            translations: HashMap::new(),
        });
    }
    Ok(checkpoint)
}

fn save_translation_checkpoint(
    path: &Path,
    checkpoint: &TranslationCheckpoint,
) -> Result<(), String> {
    let serialized =
        serde_json::to_vec(checkpoint).map_err(|error| format!("Checkpoint error: {error}"))?;
    let temp = path.with_extension("resume.tmp");
    std::fs::write(&temp, serialized)
        .map_err(|error| format!("Unable to write translation checkpoint: {error}"))?;
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Unable to replace translation checkpoint: {error}"))?;
    }
    std::fs::rename(&temp, path)
        .map_err(|error| format!("Unable to finalize translation checkpoint: {error}"))
}

fn spawn_translation_job(
    jobs: &mut tokio::task::JoinSet<
        Vec<(
            usize,
            Result<crate::engine::translator::TranslateResult, String>,
        )>,
    >,
    work: Vec<(usize, String)>,
    context: TranslationJobContext,
) {
    jobs.spawn(async move {
        let TranslationJobContext {
            source_lang,
            target_lang,
            connections,
            strategy,
            policy,
        } = context;
        let (indices, texts): (Vec<_>, Vec<_>) = work.into_iter().unzip();
        match crate::engine::translation_manager::translate_many_with_fallback(
            &texts,
            &source_lang,
            &target_lang,
            connections,
            strategy,
            policy,
        )
        .await
        {
            Ok(results) => indices.into_iter().zip(results).collect(),
            Err(error) => indices
                .into_iter()
                .map(|index| (index, Err(error.clone())))
                .collect(),
        }
    });
}

#[tauri::command]
pub fn open_excel_output_location(path: String) -> Result<(), String> {
    let output = PathBuf::from(path);
    if !output.exists() {
        return Err("The output file no longer exists".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg("/select,").arg(&output);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&output);
        command
    };
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(output.parent().unwrap_or(Path::new(".")));
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Unable to open output location: {error}"))
}

fn is_cancelled(task_id: &str) -> Result<bool, String> {
    Ok(CANCELLED
        .get_or_init(Default::default)
        .lock()
        .map_err(|e| e.to_string())?
        .contains(task_id))
}
fn clear_cancelled(task_id: &str) {
    if let Ok(mut values) = CANCELLED.get_or_init(Default::default).lock() {
        values.remove(task_id);
    }
}

fn eligible(
    cell: &umya_spreadsheet::Cell,
    value: &str,
    skip_formulas: bool,
    skip_codes: bool,
) -> bool {
    let value = value.trim();
    if value.is_empty() || cell.value_number().is_some() || (skip_formulas && cell.is_formula()) {
        return false;
    }
    !(skip_codes && looks_like_product_code(value))
}

fn looks_like_product_code(value: &str) -> bool {
    value.len() >= 3
        && value.bytes().any(|byte| byte.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/' | b'.'))
}

fn parse_column(value: &str) -> Option<u32> {
    let mut result = 0u32;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    for byte in value.bytes() {
        let upper = byte.to_ascii_uppercase();
        if !upper.is_ascii_uppercase() {
            return None;
        }
        result = result
            .checked_mul(26)?
            .checked_add((upper - b'A' + 1) as u32)?;
    }
    Some(result)
}

fn parse_cell_reference(value: &str) -> Option<(Option<u32>, Option<u32>)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let letters = value
        .bytes()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    let (column, row) = value.split_at(letters);
    if row.bytes().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }
    let column = (!column.is_empty()).then(|| parse_column(column)).flatten();
    let row = if row.is_empty() {
        None
    } else {
        row.parse::<u32>().ok().filter(|row| *row > 0)
    };
    (column.is_some() || row.is_some()).then_some((column, row))
}

fn parse_range(value: &str) -> Option<CellRange> {
    let value = value.trim();
    let (sheet, cells) = match value.rsplit_once('!') {
        Some((sheet, cells)) => (
            Some(sheet.trim().trim_matches('\'').to_string()),
            cells.trim(),
        ),
        None => (None, value),
    };
    if sheet.as_ref().is_some_and(|sheet| sheet.is_empty()) {
        return None;
    }
    let (start, end) = cells.split_once(':').unwrap_or((cells, cells));
    let (start_col, start_row) = parse_cell_reference(start)?;
    let (end_col, end_row) = parse_cell_reference(end)?;
    if start_col.is_some() != end_col.is_some() || start_row.is_some() != end_row.is_some() {
        return None;
    }
    let (start_col, end_col) = match (start_col, end_col) {
        (Some(start), Some(end)) => (start.min(end), start.max(end)),
        (None, None) => (1, u32::MAX),
        _ => return None,
    };
    let (start_row, end_row) = match (start_row, end_row) {
        (Some(start), Some(end)) => (start.min(end), start.max(end)),
        (None, None) => (1, u32::MAX),
        _ => return None,
    };
    Some(CellRange {
        sheet,
        start_col,
        end_col,
        start_row,
        end_row,
    })
}

fn validate_paths(input: &Path, output: &Path) -> Result<(), String> {
    if !input.is_file() {
        return Err("Input Excel file does not exist".into());
    }
    if input
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| !value.eq_ignore_ascii_case("xlsx"))
        .unwrap_or(true)
    {
        return Err("Only .xlsx files are supported".into());
    }
    if input == output {
        return Err("Output must be different from the input file".into());
    }
    if output.exists() {
        return Err("Output file already exists".into());
    }
    let parent = output
        .parent()
        .ok_or("Output path has no parent directory")?;
    if !parent.is_dir() {
        return Err("Output directory does not exist".into());
    }
    Ok(())
}

fn temporary_output(output: &Path) -> PathBuf {
    let stamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    output.with_file_name(format!(".sbt-translate-{stamp}.tmp.xlsx"))
}

fn translation_log_path(output: &Path) -> PathBuf {
    output.with_extension("translation.log")
}

fn write_translation_log(
    path: &Path,
    request: &TranslateExcelRequest,
    scanned: usize,
    translated: usize,
    skipped: usize,
    failed: usize,
    errors: &[String],
) -> Result<(), String> {
    let mut lines = vec![
        format!("Timestamp: {}", chrono::Utc::now().to_rfc3339()),
        format!("Input: {}", request.input_path),
        format!("Output: {}", request.output_path),
        format!("Engine: {}", request.engine),
        format!("Languages: {} -> {}", request.source_lang, request.target_lang),
        format!(
            "Summary: {scanned} scanned, {translated} translated, {skipped} skipped, {failed} failed"
        ),
    ];
    if !errors.is_empty() {
        lines.push(String::new());
        lines.push("Cell errors:".into());
        lines.extend(errors.iter().cloned());
    }
    std::fs::write(path, lines.join("\r\n"))
        .map_err(|error| format!("Unable to write translation log: {error}"))
}

fn emit_progress(
    app: &AppHandle,
    phase: &'static str,
    completed: usize,
    total: usize,
    translated: usize,
    skipped: usize,
    failed: usize,
) {
    let _ = app.emit(
        "translation-file-progress",
        TranslateExcelProgress {
            phase,
            completed,
            total,
            translated,
            skipped,
            failed,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        load_translation_checkpoint, looks_like_product_code, parse_column, parse_range,
        save_translation_checkpoint, translation_job_end, CellRange, TranslationCheckpoint,
        CHECKPOINT_VERSION,
    };
    use std::collections::HashMap;

    #[test]
    fn detects_product_codes_without_skipping_normal_text() {
        assert!(looks_like_product_code("AB-1234"));
        assert!(!looks_like_product_code("注文番号123"));
        assert!(!looks_like_product_code("保存"));
    }

    #[test]
    fn parses_excel_columns() {
        assert_eq!(parse_column("A"), Some(1));
        assert_eq!(parse_column("AA"), Some(27));
        assert_eq!(parse_column("2"), None);
    }

    #[test]
    fn parses_excel_ranges() {
        assert_eq!(
            parse_range("Sheet 1!A1:D20"),
            Some(CellRange {
                sheet: Some("Sheet 1".into()),
                start_col: 1,
                end_col: 4,
                start_row: 1,
                end_row: 20,
            })
        );
        assert_eq!(parse_range("C:C").unwrap().start_col, 3);
        assert_eq!(parse_range("2:5").unwrap().end_row, 5);
        assert!(parse_range("A1:5").is_none());
    }

    #[test]
    fn prompt_jobs_respect_item_and_character_limits() {
        let texts = vec!["12345".into(), "67890".into(), "abc".into()];
        let pending = vec![0, 1, 2];
        assert_eq!(translation_job_end(&pending, 0, 20, 8, &texts), 1);
        assert_eq!(translation_job_end(&pending, 0, 2, 20, &texts), 2);
        assert_eq!(translation_job_end(&pending, 2, 20, 1, &texts), 3);
    }

    #[test]
    fn checkpoint_round_trip_supports_resume() {
        let path = std::env::temp_dir().join(format!(
            "sbt-translation-checkpoint-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let checkpoint = TranslationCheckpoint {
            version: CHECKPOINT_VERSION,
            signature: "same-workbook-and-settings".into(),
            translations: HashMap::from([("保存".into(), "Lưu".into())]),
        };
        save_translation_checkpoint(&path, &checkpoint).expect("save checkpoint");
        let restored = load_translation_checkpoint(&path, "same-workbook-and-settings")
            .expect("load checkpoint");
        assert_eq!(restored.translations.get("保存"), Some(&"Lưu".to_string()));
        std::fs::remove_file(&path).expect("remove test checkpoint");
    }
}
