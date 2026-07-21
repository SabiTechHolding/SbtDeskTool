use crate::commands::settings::SettingsState;
use tauri::State;

#[tauri::command]
pub async fn get_network_strategy(state: State<'_, SettingsState>) -> Result<u8, String> {
    let map = state.0.lock().map_err(|e| e.to_string())?;
    Ok(map
        .get("network_strategy")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8)
}

#[tauri::command]
pub async fn resolve_system_proxy(url: String) -> Result<Option<String>, String> {
    crate::engine::network::resolve_system_proxy(&url).await
}
