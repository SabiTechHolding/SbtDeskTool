use crate::models::notes::Note;
use std::collections::HashMap;
use std::path::Path;

fn parse_notes(content: &str) -> Result<(Vec<Note>, bool), String> {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(content).map_err(|e| e.to_string())?;
    let base_id = chrono::Utc::now().timestamp_millis();
    let mut migrated = false;
    let notes = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let object = value.as_object().ok_or("Invalid note entry")?;
            let id = object
                .get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| {
                    migrated = true;
                    base_id + index as i64
                });
            let title = object
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let body = object
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let created_at = object
                .get("created_at")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    migrated = true;
                    now_iso()
                });
            let updated_at = object
                .get("updated_at")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    migrated = true;
                    created_at.clone()
                });
            Ok(Note {
                id,
                title,
                body,
                created_at,
                updated_at,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((notes, migrated))
}

fn read_notes(path: &Path) -> Result<Vec<Note>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (notes, migrated) = parse_notes(&content)?;
    if migrated {
        write_notes(path, &notes)?;
    }
    Ok(notes)
}

fn write_notes(path: &Path, notes: &[Note]) -> Result<(), String> {
    let content = serde_json::to_string_pretty(notes).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

fn reorder_notes_by_ids(notes: Vec<Note>, ids: &[i64]) -> Vec<Note> {
    let original_order: Vec<i64> = notes.iter().map(|note| note.id).collect();
    let mut notes_by_id: HashMap<i64, Note> =
        notes.into_iter().map(|note| (note.id, note)).collect();
    let mut reordered = Vec::with_capacity(notes_by_id.len());

    for id in ids.iter().chain(original_order.iter()) {
        if let Some(note) = notes_by_id.remove(id) {
            reordered.push(note);
        }
    }

    reordered
}

pub fn merge_note_bodies(existing: &str, incoming: &str) -> String {
    if existing == incoming {
        return incoming.to_string();
    }
    if existing.trim().is_empty() {
        return incoming.to_string();
    }
    if incoming.trim().is_empty() {
        return existing.to_string();
    }

    if existing.contains(incoming) {
        return existing.to_string();
    }
    if incoming.contains(existing) {
        return incoming.to_string();
    }

    let lines_existing: Vec<&str> = existing.lines().collect();
    let lines_incoming: Vec<&str> = incoming.lines().collect();

    let mut merged: Vec<String> = Vec::new();
    let mut existing_idx = 0;
    let mut incoming_idx = 0;

    while existing_idx < lines_existing.len() || incoming_idx < lines_incoming.len() {
        if existing_idx < lines_existing.len() && incoming_idx < lines_incoming.len() {
            if lines_existing[existing_idx] == lines_incoming[incoming_idx] {
                merged.push(lines_existing[existing_idx].to_string());
                existing_idx += 1;
                incoming_idx += 1;
            } else {
                let in_existing_pos = lines_existing[existing_idx..]
                    .iter()
                    .position(|&l| l == lines_incoming[incoming_idx]);
                let in_incoming_pos = lines_incoming[incoming_idx..]
                    .iter()
                    .position(|&l| l == lines_existing[existing_idx]);

                match (in_existing_pos, in_incoming_pos) {
                    (Some(pos_e), Some(pos_i)) => {
                        if pos_i <= pos_e {
                            merged.push(lines_incoming[incoming_idx].to_string());
                            incoming_idx += 1;
                        } else {
                            merged.push(lines_existing[existing_idx].to_string());
                            existing_idx += 1;
                        }
                    }
                    (None, Some(_)) => {
                        merged.push(lines_existing[existing_idx].to_string());
                        existing_idx += 1;
                    }
                    (Some(_), None) => {
                        merged.push(lines_incoming[incoming_idx].to_string());
                        incoming_idx += 1;
                    }
                    (None, None) => {
                        merged.push(lines_existing[existing_idx].to_string());
                        merged.push(lines_incoming[incoming_idx].to_string());
                        existing_idx += 1;
                        incoming_idx += 1;
                    }
                }
            }
        } else if existing_idx < lines_existing.len() {
            merged.push(lines_existing[existing_idx].to_string());
            existing_idx += 1;
        } else {
            merged.push(lines_incoming[incoming_idx].to_string());
            incoming_idx += 1;
        }
    }

    merged.join("\n")
}

#[tauri::command]
pub fn list_notes() -> Result<Vec<Note>, String> {
    read_notes(&crate::get_data_dir().join("notes.json"))
}

#[tauri::command]
pub fn save_note(app: tauri::AppHandle, note: Note) -> Result<(), String> {
    use tauri::Emitter;
    let path = crate::get_data_dir().join("notes.json");
    let mut notes = read_notes(&path)?;

    if let Some(existing) = notes.iter_mut().find(|entry| entry.id == note.id) {
        let merged_body = merge_note_bodies(&existing.body, &note.body);
        let merged_title = if note.title.trim().is_empty() || note.title == "Untitled" {
            existing.title.clone()
        } else {
            note.title.clone()
        };
        existing.title = merged_title;
        existing.body = merged_body;
        existing.updated_at = now_iso();
    } else {
        let mut new_note = note;
        new_note.created_at = now_iso();
        new_note.updated_at = now_iso();
        notes.push(new_note);
    }
    write_notes(&path, &notes)?;
    let _ = app.emit("notes-updated", ());
    Ok(())
}

#[tauri::command]
pub fn delete_note(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    use tauri::Emitter;
    let path = crate::get_data_dir().join("notes.json");
    let mut notes = read_notes(&path)?;
    notes.retain(|note| note.id != id);
    write_notes(&path, &notes)?;
    let _ = app.emit("notes-updated", ());
    Ok(())
}

#[tauri::command]
pub fn reorder_notes(app: tauri::AppHandle, ids: Vec<i64>) -> Result<(), String> {
    use tauri::Emitter;
    let path = crate::get_data_dir().join("notes.json");
    let notes = read_notes(&path)?;
    let notes = reorder_notes_by_ids(notes, &ids);
    write_notes(&path, &notes)?;
    let _ = app.emit("notes-updated", ());
    Ok(())
}

#[tauri::command]
pub async fn flush_notes() -> Result<(), String> {
    Ok(())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_older_note_records() {
        let (notes, migrated) = parse_notes(r#"[{"title":"Legacy","body":"Text"}]"#).unwrap();
        assert!(migrated);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Legacy");
        assert!(notes[0].id > 0);
    }

    #[test]
    fn reorders_notes_and_preserves_unspecified_entries() {
        let note = |id| Note {
            id,
            title: format!("Note {id}"),
            body: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let reordered = reorder_notes_by_ids(vec![note(1), note(2), note(3)], &[3, 1]);
        let ids: Vec<i64> = reordered.into_iter().map(|entry| entry.id).collect();

        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn merges_note_bodies_without_losing_lines() {
        assert_eq!(merge_note_bodies("hello\nworld", "hello\nworld"), "hello\nworld");
        assert_eq!(merge_note_bodies("hello", "hello\nworld"), "hello\nworld");
        assert_eq!(merge_note_bodies("line1\nlineA", "line1\nlineB"), "line1\nlineA\nlineB");
    }
}
