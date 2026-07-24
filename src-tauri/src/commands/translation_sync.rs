use crate::{
    commands::settings::SettingsState,
    engine::translation_memory::{self, RemoteSyncChange, SyncOutboxItem},
};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::Duration;
use tauri::State;

const CONFIG_KEY: &str = "translation_sync_config";
const CURSOR_KEY: &str = "translation_sync_cursor";
const KEYRING_SERVICE: &str = "com.sabitech.sbtdesktool.translation-sync";
const TOKEN_ACCOUNT: &str = "enterprise-api-token";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSyncConfig {
    pub server_url: String,
    pub workspace_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSyncSettingsView {
    #[serde(flatten)]
    pub config: TranslationSyncConfig,
    pub has_token: bool,
    pub cursor: Option<String>,
    pub pending: u64,
    pub failed: u64,
    pub last_error: Option<String>,
    pub conflicts: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub pending: u64,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRequest {
    workspace_id: String,
    device_id: String,
    cursor: Option<String>,
    changes: Vec<SyncOutboxItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncResponse {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    acknowledged_ids: Vec<String>,
    #[serde(default)]
    changes: Vec<RemoteSyncChange>,
}

fn token_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, TOKEN_ACCOUNT).map_err(|error| error.to_string())
}

fn default_config() -> TranslationSyncConfig {
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "desktop".into());
    TranslationSyncConfig {
        server_url: "https://sbt-desk-translation-api.thangngo-it195.workers.dev".into(),
        workspace_id: "sabitech".into(),
        device_id: host.to_lowercase(),
    }
}

fn config(settings: &Map<String, Value>) -> TranslationSyncConfig {
    settings
        .get(CONFIG_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(default_config)
}

fn validate_config(config: &TranslationSyncConfig) -> Result<(), String> {
    let url = url::Url::parse(config.server_url.trim())
        .map_err(|error| format!("Invalid sync server URL: {error}"))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(local && url.scheme() == "http") {
        return Err("Sync server must use HTTPS (HTTP is allowed only for localhost)".into());
    }
    if config.workspace_id.trim().is_empty() || config.device_id.trim().is_empty() {
        return Err("Workspace ID and Device ID are required".into());
    }
    Ok(())
}

fn endpoint(config: &TranslationSyncConfig, path: &str) -> String {
    format!("{}{}", config.server_url.trim_end_matches('/'), path)
}

fn bearer_token() -> Result<String, String> {
    if let Ok(token) = std::env::var("SBTDESK_SYNC_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token);
        }
    }
    token_entry()?
        .get_password()
        .map_err(|error| format!("Configure an enterprise sync API token first: {error}"))
}

#[tauri::command]
pub fn get_translation_sync_settings(
    state: State<SettingsState>,
) -> Result<TranslationSyncSettingsView, String> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    let (failed, last_error) = translation_memory::sync_failure_summary()?;
    Ok(TranslationSyncSettingsView {
        config: config(&settings),
        has_token: token_entry()?.get_password().is_ok(),
        cursor: settings
            .get(CURSOR_KEY)
            .and_then(Value::as_str)
            .map(str::to_owned),
        pending: translation_memory::pending_sync_count()?,
        failed,
        last_error,
        conflicts: translation_memory::memory_conflict_count()?,
    })
}

#[tauri::command]
pub fn save_translation_sync_settings(
    config: TranslationSyncConfig,
    token: Option<String>,
    state: State<SettingsState>,
) -> Result<TranslationSyncSettingsView, String> {
    validate_config(&config)?;
    if let Some(token) = token {
        let entry = token_entry()?;
        if token.trim().is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => return Err(error.to_string()),
            }
        } else {
            entry
                .set_password(token.trim())
                .map_err(|error| error.to_string())?;
        }
    }
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    settings.insert(
        CONFIG_KEY.into(),
        serde_json::to_value(&config).map_err(|error| error.to_string())?,
    );
    crate::save_settings_to_disk(&settings);
    let (failed, last_error) = translation_memory::sync_failure_summary()?;
    Ok(TranslationSyncSettingsView {
        config,
        has_token: token_entry()?.get_password().is_ok(),
        cursor: settings
            .get(CURSOR_KEY)
            .and_then(Value::as_str)
            .map(str::to_owned),
        pending: translation_memory::pending_sync_count()?,
        failed,
        last_error,
        conflicts: translation_memory::memory_conflict_count()?,
    })
}

#[tauri::command]
pub async fn test_translation_sync_connection(
    state: State<'_, SettingsState>,
) -> Result<String, String> {
    let sync_config = {
        let settings = state.0.lock().map_err(|error| error.to_string())?;
        config(&settings)
    };
    validate_config(&sync_config)?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?
        .get(endpoint(&sync_config, "/api/v1/health"))
        .bearer_auth(bearer_token()?)
        .send()
        .await
        .map_err(|error| format!("Sync server connection failed: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Sync server rejected the request: {error}"))?;
    Ok(format!(
        "Sync server connected (HTTP {})",
        response.status()
    ))
}

#[tauri::command]
pub async fn perform_translation_sync(
    state: State<'_, SettingsState>,
) -> Result<TranslationSyncResult, String> {
    let (sync_config, cursor) = {
        let settings = state.0.lock().map_err(|error| error.to_string())?;
        (
            config(&settings),
            settings
                .get(CURSOR_KEY)
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    };
    validate_config(&sync_config)?;
    let changes = translation_memory::pending_outbox(200)?;
    let attempted_ids: Vec<String> = changes.iter().map(|change| change.id.clone()).collect();
    let request = SyncRequest {
        workspace_id: sync_config.workspace_id.clone(),
        device_id: sync_config.device_id.clone(),
        cursor,
        changes,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let token = bearer_token()?;
    let mut response = None;
    let mut last_error = String::new();
    for attempt in 0..3u32 {
        match client
            .post(endpoint(&sync_config, "/api/v1/translation/sync"))
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
        {
            Ok(candidate) if candidate.status().is_success() => {
                response = Some(candidate);
                break;
            }
            Ok(candidate) => {
                let status = candidate.status();
                last_error = format!("Sync server returned HTTP {status}");
                if !(status.as_u16() == 429 || status.is_server_error()) {
                    break;
                }
            }
            Err(error) => last_error = format!("Sync request failed: {error}"),
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(500 * (1u64 << attempt))).await;
        }
    }
    let Some(response) = response else {
        translation_memory::record_sync_failure(&attempted_ids, &last_error)?;
        return Err(last_error);
    };
    let response: SyncResponse = response
        .json()
        .await
        .map_err(|error| format!("Invalid sync server response: {error}"))?;
    translation_memory::acknowledge_outbox(&response.acknowledged_ids)?;
    let pulled = translation_memory::apply_remote_changes(&response.changes)?;
    if let Some(cursor) = &response.cursor {
        let mut settings = state.0.lock().map_err(|error| error.to_string())?;
        settings.insert(CURSOR_KEY.into(), Value::String(cursor.clone()));
        crate::save_settings_to_disk(&settings);
    }
    Ok(TranslationSyncResult {
        pushed: response.acknowledged_ids.len(),
        pulled,
        pending: translation_memory::pending_sync_count()?,
        cursor: response.cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::{bearer_token, validate_config, SyncResponse, TranslationSyncConfig};

    fn config(server_url: &str) -> TranslationSyncConfig {
        TranslationSyncConfig {
            server_url: server_url.into(),
            workspace_id: "sabitech-vn".into(),
            device_id: "test-device".into(),
        }
    }

    #[test]
    fn sync_requires_https_except_for_localhost() {
        assert!(validate_config(&config("https://translate.example.com")).is_ok());
        assert!(validate_config(&config("http://localhost:8787")).is_ok());
        assert!(validate_config(&config("http://translate.example.com")).is_err());
    }

    #[test]
    fn parses_camel_case_sync_response() {
        let response: SyncResponse = serde_json::from_value(serde_json::json!({
            "cursor": "42",
            "acknowledgedIds": ["outbox-1"],
            "changes": [{
                "entityType": "dictionary",
                "entityId": "dict-1",
                "operation": "upsert",
                "payload": {"sourceLang":"ja","targetLang":"vi","sourceText":"保存","translation":"Lưu"},
                "updatedAt": "2026-07-24T00:00:00Z",
                "version": 2
            }]
        }))
        .expect("valid sync response");
        assert_eq!(response.cursor.as_deref(), Some("42"));
        assert_eq!(response.acknowledged_ids, ["outbox-1"]);
        assert_eq!(response.changes.len(), 1);
    }

    #[test]
    fn enterprise_server_migration_is_valid_sqlite() {
        let connection = rusqlite::Connection::open_in_memory().expect("in-memory SQLite");
        connection
            .execute_batch(include_str!(
                "../../../src-admin/migrations/0001_translation_sync.sql"
            ))
            .expect("valid enterprise sync migration");
        connection
            .execute_batch(include_str!(
                "../../../src-admin/migrations/0002_device_tokens.sql"
            ))
            .expect("valid device-token migration");
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('translation_records','sync_events','sync_devices','audit_log','workspaces','workspace_members','review_decisions','sync_device_tokens')",
                [],
                |row| row.get(0),
            )
            .expect("migration table count");
        assert_eq!(table_count, 8);
    }

    #[tokio::test]
    #[ignore = "requires deployed Cloudflare Worker and Windows credential"]
    async fn live_enterprise_sync_smoke() {
        let device_id = std::env::var("COMPUTERNAME")
            .unwrap_or_else(|_| "desktop".into())
            .to_lowercase();
        let response = reqwest::Client::new()
            .post("https://sbt-desk-translation-api.thangngo-it195.workers.dev/api/v1/translation/sync")
            .bearer_auth(bearer_token().expect("stored enterprise token"))
            .json(&serde_json::json!({
                "workspaceId":"sabitech",
                "deviceId":device_id,
                "cursor":null,
                "changes":[]
            }))
            .send()
            .await
            .expect("sync request")
            .error_for_status()
            .expect("successful sync response");
        let body: SyncResponse = response.json().await.expect("sync JSON");
        assert!(body.cursor.is_some());
    }
}
