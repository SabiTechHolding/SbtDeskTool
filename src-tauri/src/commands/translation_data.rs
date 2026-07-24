use crate::engine::translation_memory::{self, DictionaryEntry, TranslationMemoryEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDictionaryEntry {
    pub id: Option<String>,
    pub source_lang: String,
    pub target_lang: String,
    pub source_text: String,
    pub translation: String,
}

#[tauri::command]
pub fn list_dictionary_entries() -> Result<Vec<DictionaryEntry>, String> {
    translation_memory::list_dictionary()
}

#[tauri::command]
pub fn save_dictionary_entry(entry: SaveDictionaryEntry) -> Result<Vec<DictionaryEntry>, String> {
    translation_memory::upsert_dictionary(
        entry.id.as_deref(),
        &entry.source_lang,
        &entry.target_lang,
        &entry.source_text,
        &entry.translation,
    )?;
    translation_memory::list_dictionary()
}

#[tauri::command]
pub fn delete_dictionary_entry(id: String) -> Result<Vec<DictionaryEntry>, String> {
    translation_memory::delete_dictionary(&id)?;
    translation_memory::list_dictionary()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DictionaryCsvRow {
    source_lang: String,
    target_lang: String,
    source_text: String,
    translation: String,
}

#[tauri::command]
pub fn export_dictionary_csv(path: String) -> Result<usize, String> {
    let entries = translation_memory::list_dictionary()?;
    let mut writer = csv::Writer::from_path(path).map_err(|error| error.to_string())?;
    for entry in &entries {
        writer
            .serialize(DictionaryCsvRow {
                source_lang: entry.source_lang.clone(),
                target_lang: entry.target_lang.clone(),
                source_text: entry.source_text.clone(),
                translation: entry.translation.clone(),
            })
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(entries.len())
}

#[tauri::command]
pub fn import_dictionary_csv(path: String) -> Result<Vec<DictionaryEntry>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|error| error.to_string())?;
    for row in reader.deserialize::<DictionaryCsvRow>() {
        let row = row.map_err(|error| error.to_string())?;
        translation_memory::upsert_dictionary(
            None,
            &row.source_lang,
            &row.target_lang,
            &row.source_text,
            &row.translation,
        )?;
    }
    translation_memory::list_dictionary()
}

#[tauri::command]
pub fn get_translation_sync_status() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"pending": translation_memory::pending_sync_count()?}))
}

#[tauri::command]
pub fn list_translation_memory_entries() -> Result<Vec<TranslationMemoryEntry>, String> {
    translation_memory::list_memory()
}

#[tauri::command]
pub fn delete_translation_memory_entry(id: String) -> Result<Vec<TranslationMemoryEntry>, String> {
    translation_memory::delete_memory(&id)?;
    translation_memory::list_memory()
}

#[tauri::command]
pub fn update_translation_memory_status(
    id: String,
    status: String,
) -> Result<Vec<TranslationMemoryEntry>, String> {
    translation_memory::update_memory_status(&id, &status)?;
    translation_memory::list_memory()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslationMemoryCsvRow {
    source_lang: String,
    target_lang: String,
    source_text: String,
    translation: String,
    provider: String,
    #[serde(default = "default_memory_status")]
    status: String,
}

fn default_memory_status() -> String {
    "suggested".into()
}

#[tauri::command]
pub fn export_translation_memory_csv(path: String) -> Result<usize, String> {
    let entries = translation_memory::list_memory()?;
    let mut writer = csv::Writer::from_path(path).map_err(|error| error.to_string())?;
    for entry in &entries {
        writer
            .serialize(TranslationMemoryCsvRow {
                source_lang: entry.source_lang.clone(),
                target_lang: entry.target_lang.clone(),
                source_text: entry.source_text.clone(),
                translation: entry.translation.clone(),
                provider: entry.provider.clone(),
                status: entry.status.clone(),
            })
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(entries.len())
}

#[tauri::command]
pub fn import_translation_memory_csv(path: String) -> Result<Vec<TranslationMemoryEntry>, String> {
    let mut reader = csv::Reader::from_path(path).map_err(|error| error.to_string())?;
    for row in reader.deserialize::<TranslationMemoryCsvRow>() {
        let row = row.map_err(|error| error.to_string())?;
        translation_memory::store_with_status(
            &row.source_text,
            &row.translation,
            &row.source_lang,
            &row.target_lang,
            if row.provider.trim().is_empty() {
                "CSV Import"
            } else {
                &row.provider
            },
            &row.status,
        )?;
    }
    translation_memory::list_memory()
}
