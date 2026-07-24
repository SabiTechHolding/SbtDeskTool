//! Local-first translation memory.
//!
//! The records intentionally include sync-oriented metadata from day one. The
//! desktop client currently uses SQLite only; a future enterprise sync service
//! can use `id`, `workspace_id`, `version`, and timestamps without changing
//! the local data model.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
pub struct MemoryHit {
    pub translated: String,
    pub source: String,
}

fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn database() -> Result<Connection, String> {
    let path = crate::get_data_dir().join("translation.sqlite3");
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS translation_memory (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL DEFAULT 'local',
            source_lang TEXT NOT NULL,
            target_lang TEXT NOT NULL,
            source_normalized TEXT NOT NULL,
            source_text TEXT NOT NULL DEFAULT '',
            translation TEXT NOT NULL,
            provider TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'suggested',
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            UNIQUE(workspace_id, source_lang, target_lang, source_normalized)
        );
        CREATE INDEX IF NOT EXISTS idx_translation_memory_lookup
          ON translation_memory(workspace_id, source_lang, target_lang, source_normalized);
        CREATE TABLE IF NOT EXISTS dictionary (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL DEFAULT 'local',
            source_lang TEXT NOT NULL,
            target_lang TEXT NOT NULL,
            source_normalized TEXT NOT NULL,
            source_text TEXT NOT NULL,
            translation TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'approved',
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            UNIQUE(workspace_id, source_lang, target_lang, source_normalized)
        );
        CREATE INDEX IF NOT EXISTS idx_dictionary_lookup
          ON dictionary(workspace_id, source_lang, target_lang, source_normalized);
        CREATE TABLE IF NOT EXISTS sync_outbox (
            id TEXT PRIMARY KEY NOT NULL, entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL, operation TEXT NOT NULL,
            payload TEXT NOT NULL, created_at TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT,
            base_version INTEGER NOT NULL DEFAULT 0
        );
        ",
    )
    .map_err(|error| error.to_string())?;
    let has_source_text = {
        let mut statement = conn
            .prepare("PRAGMA table_info(translation_memory)")
            .map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        columns
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|column| column == "source_text")
    };
    if !has_source_text {
        conn.execute(
            "ALTER TABLE translation_memory ADD COLUMN source_text TEXT NOT NULL DEFAULT ''",
            [],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE translation_memory SET source_text=source_normalized WHERE source_text=''",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    let has_base_version = {
        let mut statement = conn
            .prepare("PRAGMA table_info(sync_outbox)")
            .map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        columns
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|column| column == "base_version")
    };
    if !has_base_version {
        conn.execute(
            "ALTER TABLE sync_outbox ADD COLUMN base_version INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(conn)
}

pub fn lookup_dictionary(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Option<MemoryHit>, String> {
    let source_normalized = normalize(text);
    let conn = database()?;
    conn.query_row(
        "SELECT translation FROM dictionary WHERE workspace_id='local' AND source_lang=?1 AND target_lang=?2 AND source_normalized=?3 AND status='approved' AND deleted_at IS NULL LIMIT 1",
        params![source_lang, target_lang, source_normalized],
        |row| Ok(MemoryHit { translated: row.get(0)?, source: "Dictionary".into() }),
    ).optional().map_err(|error| error.to_string())
}

pub fn dictionary_terms(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<(String, String)>, String> {
    let conn = database()?;
    let mut statement = conn
        .prepare(
            "SELECT source_text,translation FROM dictionary
             WHERE workspace_id='local' AND source_lang=?1 AND target_lang=?2
               AND status='approved' AND deleted_at IS NULL",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![source_lang, target_lang], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut terms = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|(source, _)| source != text && !source.is_empty() && text.contains(source))
        .collect::<Vec<_>>();
    terms.sort_by_key(|term| std::cmp::Reverse(term.0.len()));
    Ok(terms)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
    pub source_text: String,
    pub translation: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationMemoryEntry {
    pub id: String,
    pub source_lang: String,
    pub target_lang: String,
    pub source_text: String,
    pub translation: String,
    pub provider: String,
    pub status: String,
    pub updated_at: String,
}

pub fn list_dictionary() -> Result<Vec<DictionaryEntry>, String> {
    let conn = database()?;
    let mut statement = conn.prepare("SELECT id,source_lang,target_lang,source_text,translation,status,updated_at FROM dictionary WHERE deleted_at IS NULL ORDER BY updated_at DESC").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(DictionaryEntry {
                id: row.get(0)?,
                source_lang: row.get(1)?,
                target_lang: row.get(2)?,
                source_text: row.get(3)?,
                translation: row.get(4)?,
                status: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn upsert_dictionary(
    id: Option<&str>,
    source_lang: &str,
    target_lang: &str,
    source_text: &str,
    translation: &str,
) -> Result<(), String> {
    if source_text.trim().is_empty() || translation.trim().is_empty() {
        return Err("Source and translation are required".into());
    }
    let conn = database()?;
    let now = Utc::now().to_rfc3339();
    let generated = format!(
        "dict-{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let source_normalized = normalize(source_text);
    let requested_id = id.unwrap_or(&generated);
    if let Some(existing_id) = id {
        let updated = conn.execute(
            "UPDATE dictionary SET source_lang=?2,target_lang=?3,source_normalized=?4,source_text=?5,translation=?6,updated_at=?7,version=version+1,deleted_at=NULL WHERE id=?1",
            params![existing_id, source_lang, target_lang, source_normalized, source_text.trim(), translation.trim(), now],
        ).map_err(|error| error.to_string())?;
        if updated == 0 {
            return Err("Dictionary entry no longer exists".into());
        }
    } else {
        conn.execute("INSERT INTO dictionary(id,source_lang,target_lang,source_normalized,source_text,translation,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7) ON CONFLICT(workspace_id,source_lang,target_lang,source_normalized) DO UPDATE SET source_text=excluded.source_text,translation=excluded.translation,updated_at=excluded.updated_at,version=dictionary.version+1,deleted_at=NULL", params![requested_id,source_lang,target_lang,source_normalized,source_text.trim(),translation.trim(),now]).map_err(|error| error.to_string())?;
    }
    let entity_id: String = conn
        .query_row(
            "SELECT id FROM dictionary WHERE workspace_id='local' AND source_lang=?1 AND target_lang=?2 AND source_normalized=?3",
            params![source_lang, target_lang, source_normalized],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    enqueue_outbox(
        &conn,
        "dictionary",
        &entity_id,
        "upsert",
        serde_json::json!({"id":entity_id,"sourceLang":source_lang,"targetLang":target_lang,"sourceText":source_text.trim(),"translation":translation.trim()}),
    )?;
    Ok(())
}

pub fn delete_dictionary(id: &str) -> Result<(), String> {
    let conn = database()?;
    conn.execute(
        "UPDATE dictionary SET deleted_at=?2,updated_at=?2,version=version+1 WHERE id=?1",
        params![id, Utc::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;
    enqueue_outbox(
        &conn,
        "dictionary",
        id,
        "delete",
        serde_json::json!({"id":id}),
    )?;
    Ok(())
}

pub fn lookup(
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<Option<MemoryHit>, String> {
    let source_normalized = normalize(text);
    if source_normalized.is_empty() {
        return Ok(None);
    }
    let conn = database()?;
    conn.query_row(
        "SELECT translation, provider FROM translation_memory
         WHERE workspace_id = 'local' AND source_lang = ?1 AND target_lang = ?2
           AND source_normalized = ?3 AND status = 'approved' AND deleted_at IS NULL
         LIMIT 1",
        params![source_lang, target_lang, source_normalized],
        |row| {
            Ok(MemoryHit {
                translated: row.get(0)?,
                source: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn list_memory() -> Result<Vec<TranslationMemoryEntry>, String> {
    let conn = database()?;
    let mut statement = conn
        .prepare(
            "SELECT id,source_lang,target_lang,source_text,translation,provider,status,updated_at
             FROM translation_memory WHERE deleted_at IS NULL ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(TranslationMemoryEntry {
                id: row.get(0)?,
                source_lang: row.get(1)?,
                target_lang: row.get(2)?,
                source_text: row.get(3)?,
                translation: row.get(4)?,
                provider: row.get(5)?,
                status: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn delete_memory(id: &str) -> Result<(), String> {
    let conn = database()?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE translation_memory SET deleted_at=?2,updated_at=?2,version=version+1 WHERE id=?1",
        params![id, now],
    )
    .map_err(|error| error.to_string())?;
    enqueue_outbox(
        &conn,
        "translation_memory",
        id,
        "delete",
        serde_json::json!({"id":id}),
    )?;
    Ok(())
}

pub fn update_memory_status(id: &str, status: &str) -> Result<(), String> {
    if !matches!(status, "suggested" | "approved" | "rejected" | "conflict") {
        return Err("Invalid Translation Memory review status".into());
    }
    let conn = database()?;
    let now = Utc::now().to_rfc3339();
    let updated = conn
        .execute(
            "UPDATE translation_memory SET status=?2,updated_at=?3,version=version+1 WHERE id=?1 AND deleted_at IS NULL",
            params![id, status, now],
        )
        .map_err(|error| error.to_string())?;
    if updated == 0 {
        return Err("Translation Memory entry no longer exists".into());
    }
    let payload = conn
        .query_row(
            "SELECT source_lang,target_lang,source_text,translation,provider FROM translation_memory WHERE id=?1",
            [id],
            |row| {
                Ok(serde_json::json!({
                    "id": id,
                    "sourceLang": row.get::<_, String>(0)?,
                    "targetLang": row.get::<_, String>(1)?,
                    "source": row.get::<_, String>(2)?,
                    "translation": row.get::<_, String>(3)?,
                    "provider": row.get::<_, String>(4)?,
                    "status": status
                }))
            },
        )
        .map_err(|error| error.to_string())?;
    enqueue_outbox(&conn, "translation_memory", id, "upsert", payload)
}

pub fn store(
    text: &str,
    translation: &str,
    source_lang: &str,
    target_lang: &str,
    provider: &str,
) -> Result<(), String> {
    store_with_status(
        text,
        translation,
        source_lang,
        target_lang,
        provider,
        "suggested",
    )
}

pub fn store_with_status(
    text: &str,
    translation: &str,
    source_lang: &str,
    target_lang: &str,
    provider: &str,
    status: &str,
) -> Result<(), String> {
    if !matches!(status, "suggested" | "approved" | "rejected" | "conflict") {
        return Err("Invalid Translation Memory review status".into());
    }
    let source_normalized = normalize(text);
    if source_normalized.is_empty() || translation.trim().is_empty() {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339();
    let id = format!(
        "local-{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let conn = database()?;
    conn.execute(
        "INSERT INTO translation_memory
          (id, source_lang, target_lang, source_normalized, source_text, translation, provider, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
         ON CONFLICT(workspace_id, source_lang, target_lang, source_normalized) DO UPDATE SET
           source_text = excluded.source_text,
           translation = excluded.translation,
           provider = excluded.provider,
           status = excluded.status,
           updated_at = excluded.updated_at,
           version = translation_memory.version + 1,
           deleted_at = NULL",
        params![id, source_lang, target_lang, source_normalized, text, translation, provider, status, now],
    )
    .map_err(|error| error.to_string())?;
    let stored_id: String = conn
        .query_row(
            "SELECT id FROM translation_memory WHERE workspace_id='local' AND source_lang=?1 AND target_lang=?2 AND source_normalized=?3",
            params![source_lang, target_lang, source_normalized],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    enqueue_outbox(
        &conn,
        "translation_memory",
        &stored_id,
        "upsert",
        serde_json::json!({"id":stored_id,"sourceLang":source_lang,"targetLang":target_lang,"source":text,"translation":translation,"provider":provider,"status":status}),
    )?;
    Ok(())
}

fn enqueue_outbox(
    conn: &Connection,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    let id = format!(
        "sync-{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let table = match entity_type {
        "dictionary" => "dictionary",
        "translation_memory" => "translation_memory",
        _ => return Err("Unsupported sync entity type".into()),
    };
    let version: i64 = conn
        .query_row(
            &format!("SELECT version FROM {table} WHERE id=?1"),
            [entity_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(1);
    let base_version = version.saturating_sub(1);
    conn.execute("INSERT INTO sync_outbox(id,entity_type,entity_id,operation,payload,created_at,base_version) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,entity_type,entity_id,operation,payload.to_string(),Utc::now().to_rfc3339(),base_version]).map_err(|e|e.to_string())?;
    Ok(())
}

pub fn pending_sync_count() -> Result<u64, String> {
    let conn = database()?;
    conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

pub fn sync_failure_summary() -> Result<(u64, Option<String>), String> {
    let conn = database()?;
    let failed = conn
        .query_row(
            "SELECT COUNT(*) FROM sync_outbox WHERE attempts > 0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let last_error = conn
        .query_row(
            "SELECT last_error FROM sync_outbox WHERE attempts > 0 ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    Ok((failed, last_error))
}

pub fn memory_conflict_count() -> Result<u64, String> {
    let conn = database()?;
    conn.query_row(
        "SELECT COUNT(*) FROM translation_memory WHERE status='conflict' AND deleted_at IS NULL",
        [],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutboxItem {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub payload: serde_json::Value,
    pub created_at: String,
    pub base_version: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSyncChange {
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default = "default_version")]
    pub version: i64,
}

fn default_version() -> i64 {
    1
}

pub fn pending_outbox(limit: usize) -> Result<Vec<SyncOutboxItem>, String> {
    let conn = database()?;
    let mut statement = conn
        .prepare(
            "SELECT id,entity_type,entity_id,operation,payload,created_at,base_version
             FROM sync_outbox ORDER BY created_at ASC LIMIT ?1",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([limit.min(500) as i64], |row| {
            let payload: String = row.get(4)?;
            Ok(SyncOutboxItem {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                operation: row.get(3)?,
                payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
                created_at: row.get(5)?,
                base_version: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn acknowledge_outbox(ids: &[String]) -> Result<(), String> {
    let mut conn = database()?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    for id in ids {
        transaction
            .execute("DELETE FROM sync_outbox WHERE id=?1", [id])
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())
}

pub fn record_sync_failure(ids: &[String], error: &str) -> Result<(), String> {
    let mut conn = database()?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    for id in ids {
        transaction
            .execute(
                "UPDATE sync_outbox SET attempts=attempts+1,last_error=?2 WHERE id=?1",
                params![id, error],
            )
            .map_err(|db_error| db_error.to_string())?;
    }
    transaction
        .commit()
        .map_err(|db_error| db_error.to_string())
}

pub fn apply_remote_changes(changes: &[RemoteSyncChange]) -> Result<usize, String> {
    let mut conn = database()?;
    let transaction = conn.transaction().map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    let mut applied = 0usize;
    for change in changes {
        if change.operation == "delete" {
            let table = match change.entity_type.as_str() {
                "dictionary" => "dictionary",
                "translation_memory" => "translation_memory",
                _ => continue,
            };
            transaction
                .execute(
                    &format!(
                        "UPDATE {table} SET deleted_at=?2,updated_at=?2,version=?3 WHERE id=?1"
                    ),
                    params![change.entity_id, timestamp(change, &now), change.version],
                )
                .map_err(|error| error.to_string())?;
            applied += 1;
            continue;
        }

        let source_lang = json_text(&change.payload, "sourceLang")?;
        let target_lang = json_text(&change.payload, "targetLang")?;
        let translation = json_text(&change.payload, "translation")?;
        let updated_at = timestamp(change, &now);
        match change.entity_type.as_str() {
            "dictionary" => {
                let source_text = json_text(&change.payload, "sourceText")?;
                transaction.execute(
                    "INSERT INTO dictionary(id,workspace_id,source_lang,target_lang,source_normalized,source_text,translation,status,version,created_at,updated_at,deleted_at)
                     VALUES(?1,'local',?2,?3,?4,?5,?6,'approved',?7,?8,?8,NULL)
                     ON CONFLICT(id) DO UPDATE SET source_lang=excluded.source_lang,target_lang=excluded.target_lang,source_normalized=excluded.source_normalized,source_text=excluded.source_text,translation=excluded.translation,status=excluded.status,version=excluded.version,updated_at=excluded.updated_at,deleted_at=NULL",
                    params![change.entity_id,source_lang,target_lang,normalize(source_text),source_text,translation,change.version,updated_at],
                ).map_err(|error| error.to_string())?;
                applied += 1;
            }
            "translation_memory" => {
                let source_text = change
                    .payload
                    .get("source")
                    .or_else(|| change.payload.get("sourceText"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or("Remote Translation Memory change is missing source")?;
                let provider = change
                    .payload
                    .get("provider")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Enterprise Sync");
                let status = change
                    .payload
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .filter(|status| {
                        matches!(*status, "suggested" | "approved" | "rejected" | "conflict")
                    })
                    .unwrap_or("suggested");
                transaction.execute(
                    "INSERT INTO translation_memory(id,workspace_id,source_lang,target_lang,source_normalized,source_text,translation,provider,status,version,created_at,updated_at,deleted_at)
                     VALUES(?1,'local',?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,NULL)
                     ON CONFLICT(id) DO UPDATE SET source_lang=excluded.source_lang,target_lang=excluded.target_lang,source_normalized=excluded.source_normalized,source_text=excluded.source_text,translation=excluded.translation,provider=excluded.provider,status=excluded.status,version=excluded.version,updated_at=excluded.updated_at,deleted_at=NULL",
                    params![change.entity_id,source_lang,target_lang,normalize(source_text),source_text,translation,provider,status,change.version,updated_at],
                ).map_err(|error| error.to_string())?;
                applied += 1;
            }
            _ => {}
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(applied)
}

fn json_text<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("Remote change is missing {key}"))
}

fn timestamp<'a>(change: &'a RemoteSyncChange, fallback: &'a str) -> &'a str {
    if change.updated_at.trim().is_empty() {
        fallback
    } else {
        &change.updated_at
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalize_trims_and_collapses_whitespace() {
        assert_eq!(normalize("  注文   番号\n"), "注文 番号");
    }
}
