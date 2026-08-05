use crate::commands::settings::SettingsState;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use tauri::State;

const SETTINGS_KEY: &str = "translation_provider_configs";
const FALLBACK_KEY: &str = "translation_fallback_provider_ids";
const DELETED_KEY: &str = "translation_deleted_provider_ids";
const POLICY_KEY: &str = "translation_policy";
const KEYRING_SERVICE: &str = "com.sabitech.sbtdesktool.translation";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
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
    pub kind: String,
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
        name: String::new(),
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

fn is_agent_cli_id(id: &str) -> bool {
    id == "agent_cli" || id.starts_with("agent_cli:")
}

fn is_custom_provider_id(id: &str) -> bool {
    id.starts_with("custom:")
}

fn is_dynamic_provider_id(id: &str) -> bool {
    is_agent_cli_id(id) || is_custom_provider_id(id)
}

fn dynamic_provider_name(config: &ProviderConfig) -> String {
    let name = config.name.trim();
    if name.is_empty() {
        if is_custom_provider_id(&config.id) {
            "Custom Provider".into()
        } else {
            "Agent CLI".into()
        }
    } else {
        name.into()
    }
}

fn provider_definition(id: &str) -> Option<crate::engine::providers::ProviderInfo> {
    let registered_id = if is_agent_cli_id(id) {
        "agent_cli"
    } else if is_custom_provider_id(id) {
        "local"
    } else {
        id
    };
    crate::engine::providers::list()
        .into_iter()
        .find(|provider| provider.id == registered_id)
}

fn deleted_provider_ids(settings: &Map<String, Value>) -> BTreeSet<String> {
    settings
        .get(DELETED_KEY)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
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
    let mut configs = load_configs(settings);
    let (provider, config) = if let Some(provider) = crate::engine::providers::list()
        .into_iter()
        .find(|provider| provider.id != "agent_cli" && provider.name == name)
    {
        let config = configs
            .remove(provider.id)
            .unwrap_or_else(|| defaults(provider.id));
        (provider, config)
    } else {
        let instance_id = configs
            .iter()
            .find(|(id, config)| {
                is_dynamic_provider_id(id) && dynamic_provider_name(config) == name
            })
            .map(|(id, _)| id.clone())
            .ok_or_else(|| format!("Unknown translation provider: {name}"))?;
        let config = configs
            .remove(&instance_id)
            .ok_or_else(|| format!("Unknown translation provider: {name}"))?;
        let provider = provider_definition(&instance_id)
            .ok_or_else(|| format!("Unknown translation provider: {name}"))?;
        (provider, config)
    };
    let api_key = stored_api_key(&config.id, provider.id);
    connection_from_config(provider, config, api_key)
}

fn stored_api_key(config_id: &str, provider_id: &str) -> Option<String> {
    if !is_custom_provider_id(config_id) && !requires_api_key(provider_id) {
        return None;
    }
    entry(config_id)
        .ok()
        .and_then(|entry| entry.get_password().ok())
}

fn connection_from_config(
    provider: crate::engine::providers::ProviderInfo,
    config: ProviderConfig,
    api_key: Option<String>,
) -> Result<ProviderConnection, String> {
    let connection_name = if is_dynamic_provider_id(&config.id) {
        dynamic_provider_name(&config)
    } else {
        provider.name.into()
    };
    if provider.id == "google" {
        return Ok(ProviderConnection {
            id: provider.id.into(),
            name: connection_name,
            model: String::new(),
            base_url: String::new(),
            api_key: None,
            timeout_seconds: config.timeout_seconds,
            retries: config.retries,
            concurrency: config.concurrency,
        });
    }
    if !provider.ready {
        return Err(format!("{connection_name} is not implemented yet."));
    }
    if !config.enabled {
        return Err(format!(
            "{connection_name} is disabled in Translation Providers."
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
            "Configure a model and base URL for {connection_name}."
        ));
    }
    if requires_api_key(provider.id) && api_key.is_none() {
        return Err(format!("Configure an API key for {connection_name}."));
    }
    Ok(ProviderConnection {
        id: provider.id.into(),
        name: connection_name,
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
            let provider = views(settings)
                .into_iter()
                .find(|provider| provider.id == id && provider.name != primary_name)?;
            connection(settings, &provider.name).ok()
        })
        .collect()
}

pub(crate) fn views(settings: &Map<String, Value>) -> Vec<ProviderSettingsView> {
    let configs = load_configs(settings);
    let deleted = deleted_provider_ids(settings);
    let mut result: Vec<ProviderSettingsView> = crate::engine::providers::list()
        .into_iter()
        .filter(|provider| {
            provider.id != "agent_cli"
                && (provider.id == "google" || !deleted.contains(provider.id))
        })
        .map(|provider| {
            let config = configs
                .get(provider.id)
                .cloned()
                .unwrap_or_else(|| defaults(provider.id));
            let requires_api_key = requires_api_key(provider.id);
            ProviderSettingsView {
                id: provider.id.into(),
                kind: provider.id.into(),
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
        .collect();
    if let Some(agent_cli) = provider_definition("agent_cli") {
        result.extend(
            configs
                .iter()
                .filter(|(id, _)| is_agent_cli_id(id))
                .map(|(id, config)| ProviderSettingsView {
                    id: id.clone(),
                    kind: "agent_cli".into(),
                    name: dynamic_provider_name(config),
                    enabled: config.enabled,
                    model: config.model.clone(),
                    base_url: config.base_url.clone(),
                    has_api_key: false,
                    requires_api_key: false,
                    implemented: agent_cli.ready,
                    timeout_seconds: config.timeout_seconds,
                    retries: config.retries,
                    concurrency: config.concurrency,
                }),
        );
    }
    if let Some(custom) = provider_definition("custom:template") {
        result.extend(
            configs
                .iter()
                .filter(|(id, _)| is_custom_provider_id(id))
                .map(|(id, config)| ProviderSettingsView {
                    id: id.clone(),
                    kind: "custom".into(),
                    name: dynamic_provider_name(config),
                    enabled: config.enabled,
                    model: config.model.clone(),
                    base_url: config.base_url.clone(),
                    has_api_key: has_secret(id),
                    requires_api_key: false,
                    implemented: custom.ready,
                    timeout_seconds: config.timeout_seconds,
                    retries: config.retries,
                    concurrency: config.concurrency,
                }),
        );
    }
    result
}

#[tauri::command]
pub fn get_translation_provider_settings(
    state: State<SettingsState>,
) -> Result<Vec<ProviderSettingsView>, String> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    Ok(views(&settings))
}

#[tauri::command]
pub fn get_agent_cli_profiles() -> Vec<crate::engine::agent_cli::AgentCliProfile> {
    crate::engine::agent_cli::profiles()
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
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    let known: Vec<String> = views(&settings)
        .into_iter()
        .map(|provider| provider.id)
        .collect();
    let mut unique = Vec::new();
    for id in ids {
        if known.contains(&id) && !unique.contains(&id) {
            unique.push(id);
        }
    }
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
    if provider_definition(&config.id).is_none() {
        return Err("Unknown translation provider".into());
    }
    if is_dynamic_provider_id(&config.id) {
        config.name = config.name.trim().to_string();
        if config.name.is_empty() {
            return Err("Enter a display name for this provider.".into());
        }
    }
    config.timeout_seconds = config.timeout_seconds.clamp(5, 600);
    config.retries = config.retries.min(10);
    config.concurrency = config.concurrency.clamp(1, 32);
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    let mut configs = load_configs(&settings);
    if is_dynamic_provider_id(&config.id)
        && views(&settings).iter().any(|existing| {
            existing.id != config.id && existing.name.eq_ignore_ascii_case(&config.name)
        })
    {
        return Err(format!(
            "A provider named '{}' already exists.",
            config.name
        ));
    }
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
    let mut deleted = deleted_provider_ids(&settings);
    deleted.remove(&config.id);
    configs.insert(config.id.clone(), config);
    settings.insert(
        SETTINGS_KEY.into(),
        serde_json::to_value(configs).map_err(|error| error.to_string())?,
    );
    settings.insert(
        DELETED_KEY.into(),
        serde_json::to_value(deleted).map_err(|error| error.to_string())?,
    );
    crate::save_settings_to_disk(&settings);
    Ok(views(&settings))
}

#[tauri::command]
pub fn delete_translation_provider(
    id: String,
    state: State<SettingsState>,
) -> Result<Vec<ProviderSettingsView>, String> {
    if id == "google" {
        return Err("Google Translate is a built-in system provider.".into());
    }
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    let removed_name = views(&settings)
        .into_iter()
        .find(|provider| provider.id == id)
        .map(|provider| provider.name)
        .ok_or_else(|| "Translation provider was not found.".to_string())?;
    if let Ok(credential) = entry(&id) {
        match credential.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut configs = load_configs(&settings);
    configs.remove(&id);
    let mut deleted = deleted_provider_ids(&settings);
    if !is_dynamic_provider_id(&id) {
        deleted.insert(id.clone());
    }
    settings.insert(
        SETTINGS_KEY.into(),
        serde_json::to_value(configs).map_err(|error| error.to_string())?,
    );
    settings.insert(
        DELETED_KEY.into(),
        serde_json::to_value(deleted).map_err(|error| error.to_string())?,
    );
    if let Some(fallback) = settings.get_mut(FALLBACK_KEY).and_then(Value::as_array_mut) {
        fallback.retain(|value| value.as_str() != Some(&id));
    }
    if settings.get("engine").and_then(Value::as_str) == Some(&removed_name) {
        settings.insert("engine".into(), Value::String("Google Translate".into()));
    }
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
        let registered = provider_definition(&config.id)
            .ok_or_else(|| "Unknown translation provider".to_string())?;
        config.timeout_seconds = config.timeout_seconds.clamp(5, 600);
        config.retries = config.retries.min(10);
        config.concurrency = config.concurrency.clamp(1, 32);
        let api_key = api_key
            .filter(|key| !key.trim().is_empty())
            .or_else(|| stored_api_key(&config.id, registered.id));
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
    use super::{connection, connection_from_config, defaults, views, SETTINGS_KEY};
    use serde_json::{Map, Value};
    use std::collections::BTreeMap;

    fn provider(id: &str) -> crate::engine::providers::ProviderInfo {
        crate::engine::providers::list()
            .into_iter()
            .find(|provider| provider.id == id)
            .expect("registered provider")
    }

    #[test]
    fn agent_cli_is_not_a_fixed_provider_without_a_saved_instance() {
        let settings = Map::new();
        assert!(!views(&settings)
            .iter()
            .any(|provider| provider.id == "agent_cli"));
    }

    #[test]
    fn saved_agent_cli_instances_are_listed_and_resolved_by_display_name() {
        let mut config = defaults("agent_cli:kiro");
        config.name = "Kiro Translator".into();
        config.enabled = true;
        config.model = "kiro-cli".into();
        config.base_url = "chat\n--no-interactive\n{prompt}".into();
        let mut configs = BTreeMap::new();
        configs.insert(config.id.clone(), config);
        let mut settings = Map::new();
        settings.insert(
            SETTINGS_KEY.into(),
            serde_json::to_value(configs).expect("serialize configs"),
        );

        let listed = views(&settings)
            .into_iter()
            .find(|provider| provider.id == "agent_cli:kiro")
            .expect("dynamic Agent CLI view");
        assert_eq!(listed.kind, "agent_cli");
        assert_eq!(listed.name, "Kiro Translator");

        let resolved = connection(&settings, "Kiro Translator").expect("dynamic connection");
        assert_eq!(resolved.id, "agent_cli");
        assert_eq!(resolved.name, "Kiro Translator");
        assert_eq!(resolved.model, "kiro-cli");
        assert!(matches!(settings.get(SETTINGS_KEY), Some(Value::Object(_))));
    }

    #[test]
    fn deleted_builtin_provider_is_not_listed() {
        let mut settings = Map::new();
        settings.insert(
            "translation_deleted_provider_ids".into(),
            serde_json::json!(["openai"]),
        );
        let listed = views(&settings);
        assert!(listed.iter().any(|provider| provider.id == "google"));
        assert!(!listed.iter().any(|provider| provider.id == "openai"));
    }

    #[test]
    fn custom_openai_compatible_instances_are_listed_and_resolved() {
        let mut config = defaults("custom:acme");
        config.name = "Acme Translate".into();
        config.enabled = true;
        config.model = "acme-model".into();
        config.base_url = "https://translate.example/v1".into();
        let mut configs = BTreeMap::new();
        configs.insert(config.id.clone(), config);
        let mut settings = Map::new();
        settings.insert(
            SETTINGS_KEY.into(),
            serde_json::to_value(configs).expect("serialize configs"),
        );

        let listed = views(&settings)
            .into_iter()
            .find(|provider| provider.id == "custom:acme")
            .expect("custom provider view");
        assert_eq!(listed.kind, "custom");
        assert_eq!(listed.name, "Acme Translate");

        let resolved = connection(&settings, "Acme Translate").expect("custom connection");
        assert_eq!(resolved.id, "local");
        assert_eq!(resolved.name, "Acme Translate");
        assert_eq!(resolved.model, "acme-model");
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
