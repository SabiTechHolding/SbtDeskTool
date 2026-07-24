use crate::commands::settings::SettingsState;
use crate::engine::translator::TranslateResult;
use tauri::State;

#[tauri::command]
pub fn get_translation_providers() -> Vec<crate::engine::providers::ProviderInfo> {
    crate::engine::providers::list()
}

#[tauri::command]
pub async fn translate(
    text: String,
    src: String,
    dest: String,
    engine: String,
    state: State<'_, SettingsState>,
) -> Result<TranslateResult, String> {
    let (strategy, providers, policy) = resolve(&state, &engine)?;
    let result = crate::engine::translation_manager::translate_with_fallback(
        &text, &src, &dest, providers, strategy, policy,
    )
    .await?;
    persist_strategy(&state, result.strategy)?;
    Ok(result)
}

/// Translate multiple text units sequentially.
/// Returns results in the same order as input.
#[tauri::command]
pub async fn translate_units(
    texts: Vec<String>,
    src: String,
    dest: String,
    engine: String,
    state: State<'_, SettingsState>,
) -> Result<Vec<TranslateResult>, String> {
    let (strategy, providers, policy) = resolve(&state, &engine)?;
    let mut results = Vec::with_capacity(texts.len());
    let mut working_strategy = strategy;
    for text in &texts {
        let r = crate::engine::translation_manager::translate_with_fallback(
            text,
            &src,
            &dest,
            providers.clone(),
            working_strategy,
            policy.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;
        working_strategy = r.strategy;
        results.push(r);
    }
    persist_strategy(&state, working_strategy)?;
    Ok(results)
}

fn resolve(
    state: &State<'_, SettingsState>,
    engine: &str,
) -> Result<
    (
        u8,
        Vec<crate::commands::providers::ProviderConnection>,
        crate::commands::providers::TranslationPolicy,
    ),
    String,
> {
    let settings = state.0.lock().map_err(|error| error.to_string())?;
    let strategy = settings
        .get("network_strategy")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;
    let primary = crate::commands::providers::connection(&settings, engine)?;
    let mut providers = vec![primary];
    providers.extend(crate::commands::providers::fallback_connections(
        &settings, engine,
    ));
    let policy = crate::commands::providers::translation_policy(&settings);
    Ok((strategy, providers, policy))
}

fn persist_strategy(state: &State<'_, SettingsState>, strategy: u8) -> Result<(), String> {
    let mut settings = state.0.lock().map_err(|error| error.to_string())?;
    settings.insert(
        "network_strategy".into(),
        serde_json::Value::Number(strategy.into()),
    );
    crate::save_settings_to_disk(&settings);
    Ok(())
}
