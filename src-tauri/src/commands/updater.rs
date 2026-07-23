use serde::Serialize;
use std::time::Duration;
use tauri::{Manager, ResourceId, Webview};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

#[tauri::command]
pub async fn check_for_update(
    webview: Webview,
    timeout: Option<u64>,
) -> Result<Option<UpdateMetadata>, String> {
    let mut builder = webview.updater_builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(Duration::from_millis(timeout));
    }

    #[cfg(target_os = "windows")]
    {
        builder = builder.configure_client(|client| {
            // PAC rules can differ between the metadata host and the release
            // asset host reached after redirects. Resolve every request URL
            // with the current user's Windows settings.
            let proxy = reqwest_updater::Proxy::custom(|url| {
                crate::engine::network::resolve_system_proxy_blocking(url.as_str())
                    .ok()
                    .flatten()
            });
            client.no_proxy().proxy(proxy)
        });
    }

    let updater = builder.build().map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };

    let metadata = UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.date.map(|date| date.to_string()),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        rid: webview.resources_table().add(update),
    };
    Ok(Some(metadata))
}
