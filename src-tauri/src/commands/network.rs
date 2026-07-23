use crate::commands::settings::SettingsState;
use std::io::Write;
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
pub fn record_update_error(message: String) -> Result<(), String> {
    let message = message.replace(['\r', '\n'], " ");
    let message = message.chars().take(4_000).collect::<String>();
    let path = crate::get_data_dir().join("app.log");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("Unable to open {}: {error}", path.display()))?;
    writeln!(
        file,
        "{} [update] {message}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    )
    .map_err(|error| format!("Unable to write {}: {error}", path.display()))
}
