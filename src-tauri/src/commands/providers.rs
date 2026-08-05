use crate::commands::settings::SettingsState;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use tauri::State;

const SETTINGS_KEY: &str = "translation_provider_configs";
const FALLBACK_KEY: &str = "translation_fallback_provider_ids";
const POLICY_KEY: &str = "translation_policy";
const KEYRING_SERVICE: &str = "com.sabitech.sbtdesktool.translation";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub enabled: bool,
    pub model: String,
    pub base_url: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_retries")]
    pub retries: u32,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranslationPolicy {
    pub use_dictionary: bool,
    pub use_translation_memory: bool,
    pub use_cache: bool,
    pub save_translation_memory: bool,
    #[serde(default = "default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,
}

impl Default for TranslationPolicy {
    fn default() -> Self {
        Self {
            use_dictionary: true,
            use_translation_memory: true,
            use_cache: true,
            save_translation_memory: true,
            cache_ttl_seconds: default_cache_ttl_seconds(),
        }
    }
}

pub(crate) fn translation_policy(settings: &Map<String, Value>) -> TranslationPolicy {
    settings
        .get(POLICY_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettingsView {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub model: String,
    pub base_url: String,
    pub has_api_key: bool,
    pub requires_api_key: bool,
    pub implemented: bool,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub concurrency: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderConnection {
    pub id: String,
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_seconds: u64,
    pub retries: u32,
    pub concurrency: usize,
}

fn default_timeout_seconds() -> u64 {
    60
}

fn default_retries() -> u32 {
    2
}

fn default_concurrency() -> usize {
    4
}

fn default_cache_ttl_seconds() -> u64 {
    900
}

fn defaults(id: &str) -> ProviderConfig {
    let (model, base_url) = match id {
        "gemini" => (
            "gemini-3.6-flash",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        "openai" => ("gpt-4.1-mini", "https://api.openai.com/v1"),
        "claude" => ("claude-sonnet-4-0", "https://api.anthropic.com"),
        "deepl" => ("", "https://api.deepl.com"),
        "local" => ("", "http://localhost:11434/v1"),
        _ => ("", ""),
    };
    ProviderConfig {
        id: id.into(),
        enabled: id == "google",
        model: model.into(),
        base_url: base_url.into(),
        timeout_seconds: default_timeout_seconds(),
        retries: default_retries(),
        concurrency: if id == "gemini" {
            1
        } else {
            default_concurrency()
        },
    }
}

fn entry(id: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, id).map_err(|error| error.to_string())
}

fn has_secret(id: &str) -> bool {
    entry(id)
        .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
        .is_ok()
}

fn requires_api_key(id: &str) -> bool {
    matches!(id, "gemini" | "openai" | "claude" | "deepl")
}

pub(crate) fn connection(
    settings: &Map<String, Value>,
    name: &str,
) -> Result<ProviderConnection, String> {
    let provider = crate::engine::providers::list()
        .into_iter()
        .find(|provider| provider.name == name)
        .ok_or_else(|| format!("Unknown translation provider: {name}"))?;
    let config = load_configs(settings)
        .remove(provider.id)
        .unwrap_or_else(|| defaults(provider.id));
    let api_key = stored_api_key(provider.id);
    connection_from_config(provider, config, api_key)
}

fn stored_api_key(id: &str) -> Option<String> {
    if !requires_api_key(id) {
        return None;
    }
    entry(id).ok().and_then(|entry| entry.get_password().ok())
}

fn connection_from_config(
    provider: crate::engine::providers::ProviderInfo,
    config: ProviderConfig,
    api_key: Option<String>,
) -> Result<ProviderConnection, String> {
    if provider.id == "google" {
        return Ok(ProviderConnection {
            id: provider.id.into(),
            name: provider.name.into(),
            model: String::new(),
            base_url: String::new(),
            api_key: None,
            timeout_seconds: config.timeout_seconds,
            retries: config.retries,
            concurrency: config.concurrency,
        });
    }
    if !provider.ready {
        return Err(format!("{} is not implemented yet.", provider.name));
    }
    if !config.enabled {
        return Err(format!(
            "{} is disabled in Translation Providers.",
            provider.name
        ));
    }
    if provider.id == "agent_cli" && config.model.trim().is_empty() {
        return Err("Configure an executable for Agent CLI.".into());
    }
    if provider.id != "agent_cli"
        && ((provider.id != "deepl" && config.model.trim().is_empty())
            || config.base_url.trim().is_empty())
    {
        return Err(format!(
            "Configure a model and base URL for {}.",
            provider.name
        ));
    }
    if requires_api_key(provider.id) && api_key.is_none() {
        return Err(format!("Configure an API key for {}.", provider.name));
    }
    Ok(ProviderConnection {
        id: provider.id.into(),
        name: provider.name.into(),
        model: config.model,
        base_url: config.base_url,
        api_key,
        timeout_seconds: config.timeout_seconds,
        retries: config.retries,
        concurrency: config.concurrency,
    })
}

fn load_configs(settings: &Map<String, Value>) -> BTreeMap<String, ProviderConfig> {
    let mut configs: BTreeMap<String, ProviderConfig> = settings
        .get(SETTINGS_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    if let Some(gemini) = configs.get_mut("gemini") {
        if gemini.model == "gemini-2.5-flash" {
            gemini.model = "gemini-3.6-flash".into();
            gemini.concurrency = 1;
        } else if gemini.concurrency == default_concurrency() {
            // Migrate the former generic default. Users can increase it again
            // explicitly after confirming their Gemini project quota.
            gemini.concurrency = 1;
        }
    }
    configs
}

pub(crate) fn fallback_connections(
    settings: &Map<String, Value>,
    primary_name: &str,
) -> Vec<ProviderConnection> {
    settings
        .get(FALLBACK_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|id| {
            let provider = crate::engine::providers::list()
                .into_iter()
                .find(|provider| provider.id == id && provider.name != primary_name)?;
            connection(settings, provider.name).ok()
        })
        .collect()
}

fn views(settings: &Map<String, Value>) -> Vec<ProviderSettingsView> {
    let configs = load_configs(settings);
    crate::engine::providers::list()
        .into_iter()
        .map(|provider| {
            let config = configs
                .get(provider.id)
                .cloned()
                .unwrap_or_else(|| defaults(provider.id));
            let requires_api_key = requires_api_key(provider.id);
            ProviderSettingsView {
                id: provider.id.into(),
                name: provider.name.into(),
                enabled: config.enabled,
                model: config.model,
                base_url: config.base_url,
                has_api_key: requires_api_key && has_secret(provider.id),
                requires_api_key,
                implemented: provider.ready,
                timeout_seconds: config.timeout_seconds,
                retries: config.retries,
                concurrency: config.concurrency,
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_translation_provider_settings(
    state: State<SettingsState>,
) -> Result<Vec<ProviderSettingsView>, String> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    Ok(views(&settings))
}

#[tauri::command]
pub fn get_translation_fallback(state: State<SettingsState>) -> Result<Vec<String>, String> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    Ok(settings
        .get(FALLBACK_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect())
}

#[tauri::command]
pub fn save_translation_fallback(
    ids: Vec<String>,
    state: State<SettingsState>,
) -> Result<Vec<String>, String> {
    let known: Vec<&str> = crate::engine::providers::list()
        .iter()
        .map(|provider| provider.id)
        .collect();
    let mut unique = Vec::new();
    for id in ids {
        if known.contains(&id.as_str()) && !unique.contains(&id) {
            unique.push(id);
        }
    }
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    settings.insert(
        FALLBACK_KEY.into(),
        serde_json::to_value(&unique).map_err(|error| error.to_string())?,
    );
    crate::save_settings_to_disk(&settings);
    Ok(unique)
}

#[tauri::command]
pub fn get_translation_policy(state: State<SettingsState>) -> Result<TranslationPolicy, String> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    Ok(translation_policy(&settings))
}

#[tauri::command]
pub fn save_translation_policy(
    mut policy: TranslationPolicy,
    state: State<SettingsState>,
) -> Result<TranslationPolicy, String> {
    policy.cache_ttl_seconds = policy.cache_ttl_seconds.clamp(1, 86_400);
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    settings.insert(
        POLICY_KEY.into(),
        serde_json::to_value(&policy).map_err(|error| error.to_string())?,
    );
    crate::save_settings_to_disk(&settings);
    Ok(policy)
}

#[tauri::command]
pub fn save_translation_provider_settings(
    mut config: ProviderConfig,
    api_key: Option<String>,
    state: State<SettingsState>,
) -> Result<Vec<ProviderSettingsView>, String> {
    if !crate::engine::providers::list()
        .iter()
        .any(|provider| provider.id == config.id)
    {
        return Err("Unknown translation provider".into());
    }
    config.timeout_seconds = config.timeout_seconds.clamp(5, 600);
    config.retries = config.retries.min(10);
    config.concurrency = config.concurrency.clamp(1, 32);
    if let Some(api_key) = api_key {
        let entry = entry(&config.id)?;
        if api_key.trim().is_empty() {
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => return Err(error.to_string()),
            }
        } else {
            entry
                .set_password(api_key.trim())
                .map_err(|error| error.to_string())?;
        }
    }
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    let mut configs = load_configs(&settings);
    configs.insert(config.id.clone(), config);
    settings.insert(
        SETTINGS_KEY.into(),
        serde_json::to_value(configs).map_err(|error| error.to_string())?,
    );
    crate::save_settings_to_disk(&settings);
    Ok(views(&settings))
}

#[tauri::command]
pub fn clear_translation_provider_key(
    id: String,
    state: State<SettingsState>,
) -> Result<Vec<ProviderSettingsView>, String> {
    let credential = entry(&id)?;
    match credential.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(error.to_string()),
    }
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    Ok(views(&settings))
}

#[tauri::command]
pub async fn test_translation_provider(
    mut config: ProviderConfig,
    api_key: Option<String>,
    state: State<'_, SettingsState>,
) -> Result<String, String> {
    let (provider, strategy) = {
        let settings = state.0.lock().map_err(|error| error.to_string())?;
        let registered = crate::engine::providers::list()
            .into_iter()
            .find(|provider| provider.id == config.id)
            .ok_or_else(|| "Unknown translation provider".to_string())?;
        config.timeout_seconds = config.timeout_seconds.clamp(5, 600);
        config.retries = config.retries.min(10);
        config.concurrency = config.concurrency.clamp(1, 32);
        let api_key = api_key
            .filter(|key| !key.trim().is_empty())
            .or_else(|| stored_api_key(registered.id));
        let provider = connection_from_config(registered, config, api_key)?;
        let strategy = settings
            .get("network_strategy")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u8;
        (provider, strategy)
    };
    crate::engine::translation_manager::test_connection(provider, strategy).await
}

#[cfg(test)]
mod tests {
    use super::{connection_from_config, defaults};

    fn provider(id: &str) -> crate::engine::providers::ProviderInfo {
        crate::engine::providers::list()
            .into_iter()
            .find(|provider| provider.id == id)
            .expect("registered provider")
    }

    #[test]
    fn connection_test_uses_current_enabled_form_and_new_api_key() {
        let mut config = defaults("openai");
        config.enabled = true;

        let connection =
            connection_from_config(provider("openai"), config, Some("new-form-key".into()))
                .expect("current form config should be testable before save");

        assert_eq!(connection.id, "openai");
        assert_eq!(connection.api_key.as_deref(), Some("new-form-key"));
    }

    #[test]
    fn connection_test_rejects_disabled_current_form() {
        let error = connection_from_config(
            provider("openai"),
            defaults("openai"),
            Some("new-form-key".into()),
        )
        .expect_err("disabled current form");

        assert_eq!(error, "OpenAI is disabled in Translation Providers.");
    }

    #[test]
    fn agent_cli_uses_model_as_executable_and_base_url_as_argument_lines() {
        let mut config = defaults("agent_cli");
        config.enabled = true;
        config.model = "codex".into();
        config.base_url = "exec\n-\n--color\nnever".into();

        let connection = connection_from_config(provider("agent_cli"), config, None)
            .expect("configured Agent CLI");

        assert_eq!(connection.model, "codex");
        assert_eq!(connection.base_url, "exec\n-\n--color\nnever");
        assert!(connection.api_key.is_none());
    }

    #[test]
    fn agent_cli_requires_an_executable() {
        let mut config = defaults("agent_cli");
        config.enabled = true;

        let error = connection_from_config(provider("agent_cli"), config, None)
            .expect_err("missing executable");

        assert_eq!(error, "Configure an executable for Agent CLI.");
    }
}
