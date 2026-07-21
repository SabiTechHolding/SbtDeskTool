#![allow(clippy::items_after_test_module)]

mod commands;
mod engine;
mod models;

#[allow(unused_imports)]
use commands::{
    diff::compute_diff,
    network::{get_network_strategy, resolve_system_proxy},
    notes::{delete_note, flush_notes, list_notes, save_note},
    settings::{get_settings, save_setting, save_settings_flush, SettingsState},
    translate::{translate, translate_units},
    window::{
        exit_app, restart_app, set_always_on_top, set_window_effect, set_window_size,
        toggle_compact,
    },
};

use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub(crate) fn get_data_dir() -> PathBuf {
    if let Ok(forced) = std::env::var("SBTDESKTOOL_DATA_DIR") {
        let path = PathBuf::from(forced);
        if std::fs::create_dir_all(&path).is_ok() {
            return path;
        }
    }
    for name in ["LOCALAPPDATA", "APPDATA"] {
        if let Ok(root) = std::env::var(name) {
            let path = PathBuf::from(root).join("SbtDeskTool");
            if std::fs::create_dir_all(&path).is_ok() {
                return path;
            }
        }
    }
    let path = std::env::temp_dir().join("SbtDeskTool");
    let _ = std::fs::create_dir_all(&path);
    path
}

fn default_settings() -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    map.insert("theme".into(), Value::String("dark".into()));
    map.insert("layout".into(), Value::String("horizontal".into()));
    map.insert("always_on_top".into(), Value::Bool(false));
    map.insert("compact_mode".into(), Value::Bool(false));
    map.insert("engine".into(), Value::String("Google Translate".into()));
    map.insert("src_lang".into(), Value::String("Auto Detect".into()));
    map.insert("dest_lang".into(), Value::String("English".into()));
    map.insert("font_size_tran".into(), Value::Number(10.into()));
    map.insert("font_size_diff".into(), Value::Number(10.into()));
    map.insert("font_size_note".into(), Value::Number(10.into()));
    map.insert("active_tab".into(), Value::String("diff".into()));
    map.insert("diff_word_diff".into(), Value::Bool(true));
    map.insert("diff_ignore_whitespace".into(), Value::Bool(false));
    map.insert("diff_show_whitespace".into(), Value::Bool(false));
    map.insert("diff_algorithm".into(), Value::String("legacy".into()));
    map.insert("note_auto_save".into(), Value::Bool(true));
    map.insert("word_wrap".into(), Value::Bool(true));
    map.insert("word_wrap_tran".into(), Value::Bool(true));
    map.insert("word_wrap_diff".into(), Value::Bool(true));
    map.insert("word_wrap_note".into(), Value::Bool(true));
    map.insert("window_effect".into(), Value::String("blur".into()));
    map.insert("window_width".into(), Value::Number(980.into()));
    map.insert("window_height".into(), Value::Number(640.into()));
    map.insert("network_strategy".into(), Value::Number(0.into()));
    map.insert("sash_pos_tran".into(), Value::Number(50.into()));
    map.insert("compact_width".into(), Value::Number(500.into()));
    map.insert("compact_height".into(), Value::Number(240.into()));
    map.insert("compact_diff_height".into(), Value::Number(280.into()));
    for prefix in ["tran", "diff", "note"] {
        map.insert(format!("{prefix}_find_case"), Value::Bool(false));
        map.insert(format!("{prefix}_find_word"), Value::Bool(false));
        map.insert(format!("{prefix}_find_regex"), Value::Bool(false));
    }
    map.insert(
        "diff_left_ratio".into(),
        Value::Number(serde_json::Number::from_f64(0.5).expect("valid ratio")),
    );
    map.insert("note_sidebar_width".into(), Value::Number(220.into()));
    map
}

fn load_settings_from_disk() -> serde_json::Map<String, Value> {
    let path = get_data_dir().join("settings.json");
    if let Ok(content) = std::fs::read_to_string(&path) {
        let content = content.trim_start_matches('\u{feff}');
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(content) {
            let settings = merge_settings(map);
            save_settings_to_disk(&settings);
            return settings;
        }
    }
    default_settings()
}

fn merge_settings(map: serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    let legacy_font_size = map.get("font_size").cloned();
    let legacy_effect = map.get("opacity_mode").cloned();
    let legacy_left_width = map.get("diff_left_width").and_then(Value::as_f64);
    let legacy_window_width = map.get("window_width").and_then(Value::as_f64);
    let missing_tab_fonts = ["font_size_tran", "font_size_diff", "font_size_note"]
        .map(|key| (key, !map.contains_key(key)));
    let missing_window_effect = !map.contains_key("window_effect");
    let missing_diff_ratio = !map.contains_key("diff_left_ratio");
    let legacy_word_wrap = map
        .get("word_wrap")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let missing_tab_wraps = ["word_wrap_tran", "word_wrap_diff", "word_wrap_note"]
        .map(|key| (key, !map.contains_key(key)));

    let mut settings = default_settings();
    for (key, value) in map {
        settings.insert(key, value);
    }
    if let Some(font_size) = legacy_font_size {
        for (key, missing) in missing_tab_fonts {
            if missing {
                settings.insert(key.into(), font_size.clone());
            }
        }
    }
    if missing_window_effect {
        if let Some(Value::String(effect)) = legacy_effect {
            settings.insert("window_effect".into(), Value::String(effect));
        }
    }
    if missing_diff_ratio {
        if let (Some(left_width), Some(window_width)) = (legacy_left_width, legacy_window_width) {
            if window_width > 0.0 {
                let ratio = (left_width / window_width).clamp(0.2, 0.8);
                if let Some(number) = serde_json::Number::from_f64(ratio) {
                    settings.insert("diff_left_ratio".into(), Value::Number(number));
                }
            }
        }
    }
    for (key, missing) in missing_tab_wraps {
        if missing {
            settings.insert(key.into(), Value::Bool(legacy_word_wrap));
        }
    }
    normalize_window_dimensions(&mut settings);
    settings
}

fn normalize_window_dimensions(settings: &mut serde_json::Map<String, Value>) -> bool {
    let dimensions = [
        ("window_width", 340, 980),
        ("window_height", 120, 640),
        ("compact_width", 280, 500),
        ("compact_height", 120, 240),
        ("compact_diff_height", 120, 280),
    ];
    let mut changed = false;
    for (key, minimum, fallback) in dimensions {
        let valid = settings
            .get(key)
            .and_then(Value::as_u64)
            .is_some_and(|value| value >= minimum);
        if !valid {
            settings.insert(key.into(), Value::Number(fallback.into()));
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn repairs_zero_window_dimensions() {
        let mut settings = serde_json::Map::new();
        settings.insert("window_width".into(), Value::Number(0.into()));
        settings.insert("window_height".into(), Value::Number(0.into()));
        let settings = merge_settings(settings);
        assert_eq!(settings["window_width"], Value::Number(980.into()));
        assert_eq!(settings["window_height"], Value::Number(640.into()));
        assert_eq!(settings["diff_algorithm"], Value::String("legacy".into()));
    }

    #[test]
    fn normalizes_older_settings() {
        let mut settings = serde_json::Map::new();
        settings.insert("font_size".into(), Value::Number(13.into()));
        settings.insert("font_size_diff".into(), Value::Number(15.into()));
        settings.insert("word_wrap".into(), Value::Bool(false));
        settings.insert("opacity_mode".into(), Value::String("ghost".into()));
        settings.insert("window_width".into(), Value::Number(1000.into()));
        settings.insert("diff_left_width".into(), Value::Number(600.into()));

        let settings = merge_settings(settings);
        assert_eq!(settings["font_size_tran"], Value::Number(13.into()));
        assert_eq!(settings["font_size_diff"], Value::Number(15.into()));
        assert_eq!(settings["font_size_note"], Value::Number(13.into()));
        assert_eq!(settings["word_wrap_tran"], Value::Bool(false));
        assert_eq!(settings["word_wrap_diff"], Value::Bool(false));
        assert_eq!(settings["word_wrap_note"], Value::Bool(false));
        assert_eq!(settings["window_effect"], Value::String("ghost".into()));
        assert_eq!(settings["diff_left_ratio"].as_f64(), Some(0.6));
    }
}

pub(crate) fn save_settings_to_disk(map: &serde_json::Map<String, Value>) {
    let path = get_data_dir().join("settings.json");
    if let Ok(content) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(path, content);
    }
}

#[cfg(target_os = "windows")]
fn set_windows_app_user_model_id() {
    use std::{iter, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let app_id: Vec<u16> = std::ffi::OsStr::new("com.sabitech.sbtdesktool")
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    // SAFETY: app_id is a valid null-terminated UTF-16 string for this call.
    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
    if result < 0 {
        eprintln!("Unable to set Windows AppUserModelID: HRESULT {result:#x}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = load_settings_from_disk();

    #[cfg(target_os = "windows")]
    set_windows_app_user_model_id();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(SettingsState(Mutex::new(settings)))
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_setting,
            save_settings_flush,
            translate,
            translate_units,
            compute_diff,
            list_notes,
            save_note,
            delete_note,
            flush_notes,
            set_window_effect,
            set_always_on_top,
            set_window_size,
            toggle_compact,
            restart_app,
            exit_app,
            get_network_strategy,
            resolve_system_proxy,
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Resized(size) => {
                let scale = window.scale_factor().unwrap_or(1.0);
                let _ = window.emit(
                    "window-resized",
                    serde_json::json!({
                        "width": (size.width as f64 / scale).round() as u32,
                        "height": (size.height as f64 / scale).round() as u32
                    }),
                );
            }
            tauri::WindowEvent::CloseRequested { .. } => {
                let app_handle = window.app_handle();
                let state = app_handle.state::<SettingsState>();
                let map = state.0.lock().ok();
                if let Some(m) = map {
                    save_settings_to_disk(&m);
                }
            }
            _ => {}
        })
        .setup(|app| {
            if cfg!(debug_assertions) {
                if let Err(error) = app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                ) {
                    eprintln!("Unable to initialize debug logging: {error}");
                }
            }

            #[cfg(target_os = "windows")]
            if let (Some(window), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                window.set_icon(icon.clone())?;
                window.set_skip_taskbar(false)?;
            }

            let tray_result = (|| -> tauri::Result<()> {
                let show_item =
                    tauri::menu::MenuItemBuilder::with_id("show", "Show/Hide").build(app)?;
                let quit_item = tauri::menu::MenuItemBuilder::with_id("quit", "Quit").build(app)?;
                let menu = tauri::menu::MenuBuilder::new(app)
                    .item(&show_item)
                    .item(&quit_item)
                    .build()?;

                tauri::tray::TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        "quit" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.emit("app-quit-requested", ());
                            } else {
                                app.exit(0);
                            }
                        }
                        _ => {}
                    })
                    .build(app)?;
                Ok(())
            })();
            if let Err(error) = tray_result {
                log::warn!("Unable to create tray icon: {error}");
            }

            let handle = app.handle().clone();
            let shortcut_plugin = handle.plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed
                            && shortcut.matches(
                                tauri_plugin_global_shortcut::Modifiers::ALT
                                    | tauri_plugin_global_shortcut::Modifiers::CONTROL,
                                tauri_plugin_global_shortcut::Code::KeyT,
                            )
                        {
                            if let Some(window) = _app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    })
                    .build(),
            );

            if let Err(error) = shortcut_plugin {
                log::warn!("Unable to initialize global shortcuts: {error}");
            } else {
                let shortcut = tauri_plugin_global_shortcut::Shortcut {
                    mods: tauri_plugin_global_shortcut::Modifiers::ALT
                        | tauri_plugin_global_shortcut::Modifiers::CONTROL,
                    key: tauri_plugin_global_shortcut::Code::KeyT,
                    id: 0,
                };
                if let Err(error) = handle.global_shortcut().register(shortcut) {
                    log::warn!("Unable to register Ctrl+Alt+T global shortcut: {error}");
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
