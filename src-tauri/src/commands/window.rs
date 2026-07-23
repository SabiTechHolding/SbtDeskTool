use tauri::Manager;

async fn resize_centered_in_work_area(
    window: &tauri::WebviewWindow,
    logical_width: u32,
    logical_height: u32,
) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let start_position = window.outer_position().map_err(|error| error.to_string())?;
    let start_outer = window.outer_size().map_err(|error| error.to_string())?;
    let start_inner = window.inner_size().map_err(|error| error.to_string())?;
    let frame_width = start_outer.width.saturating_sub(start_inner.width);
    let frame_height = start_outer.height.saturating_sub(start_inner.height);
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or("No monitor available for main window")?;
    let work_area = monitor.work_area();

    let requested_inner_width = (logical_width as f64 * scale).round() as u32;
    let requested_inner_height = (logical_height as f64 * scale).round() as u32;
    let target_outer_width = requested_inner_width
        .saturating_add(frame_width)
        .min(work_area.size.width);
    let target_outer_height = requested_inner_height
        .saturating_add(frame_height)
        .min(work_area.size.height);
    let target_inner_width = target_outer_width.saturating_sub(frame_width).max(1);
    let target_inner_height = target_outer_height.saturating_sub(frame_height).max(1);

    let center_x = start_position.x as i64 + start_outer.width as i64 / 2;
    let center_y = start_position.y as i64 + start_outer.height as i64 / 2;
    let work_left = work_area.position.x as i64;
    let work_top = work_area.position.y as i64;
    let work_right = work_left + work_area.size.width as i64;
    let work_bottom = work_top + work_area.size.height as i64;

    for step in 1..=12 {
        let progress = step as f64 / 12.0;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let outer_width = (start_outer.width as f64
            + (target_outer_width as f64 - start_outer.width as f64) * eased)
            .round() as u32;
        let outer_height = (start_outer.height as f64
            + (target_outer_height as f64 - start_outer.height as f64) * eased)
            .round() as u32;
        let inner_width = outer_width.saturating_sub(frame_width).max(1);
        let inner_height = outer_height.saturating_sub(frame_height).max(1);
        let x = (center_x - outer_width as i64 / 2)
            .clamp(work_left, (work_right - outer_width as i64).max(work_left));
        let y = (center_y - outer_height as i64 / 2)
            .clamp(work_top, (work_bottom - outer_height as i64).max(work_top));

        window
            .set_size(tauri::PhysicalSize::new(inner_width, inner_height))
            .map_err(|error| error.to_string())?;
        window
            .set_position(tauri::PhysicalPosition::new(x as i32, y as i32))
            .map_err(|error| error.to_string())?;
        tokio::time::sleep(std::time::Duration::from_millis(12)).await;
    }

    // Finish on the exact target in case integer interpolation rounded a frame.
    window
        .set_size(tauri::PhysicalSize::new(
            target_inner_width,
            target_inner_height,
        ))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn toggle_compact(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::commands::settings::SettingsState>,
    compact: bool,
    tab: String,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    let (width, height, minimum_width) = {
        let map = state.0.lock().map_err(|e| e.to_string())?;
        if compact {
            let width = map
                .get("compact_width")
                .and_then(|v| v.as_u64())
                .unwrap_or(500) as u32;
            let height_key = if tab == "diff" {
                "compact_diff_height"
            } else {
                "compact_height"
            };
            let height = map
                .get(height_key)
                .and_then(|v| v.as_u64())
                .unwrap_or(if tab == "diff" { 280 } else { 240 }) as u32;
            (width, height, 280)
        } else {
            let width = map
                .get("window_width")
                .and_then(|v| v.as_u64())
                .unwrap_or(980) as u32;
            let height = map
                .get("window_height")
                .and_then(|v| v.as_u64())
                .unwrap_or(640) as u32;
            (width, height, 340)
        }
    };
    window
        .set_min_size(Some(tauri::LogicalSize::new(minimum_width as f64, 120.0)))
        .map_err(|e| e.to_string())?;
    resize_centered_in_work_area(&window, width.max(minimum_width), height.max(120)).await
}

#[tauri::command]
pub async fn set_window_size(
    app: tauri::AppHandle,
    width: u32,
    height: u32,
    compact: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    let (minimum_width, minimum_height) = if compact { (280, 120) } else { (340, 120) };
    window
        .set_min_size(Some(tauri::LogicalSize::new(
            minimum_width as f64,
            minimum_height as f64,
        )))
        .map_err(|e| e.to_string())?;
    resize_centered_in_work_area(
        &window,
        width.max(minimum_width),
        height.max(minimum_height),
    )
    .await
}

/// Available window effects:
/// (name, alpha, use_blur)
const WINDOW_EFFECTS: &[(&str, f64, bool)] = &[
    ("solid", 1.00, false),
    ("blur", 0.98, true),
    ("frosted", 0.92, true),
    ("transp", 0.85, false),
    ("dim", 0.88, false),
    ("ghost", 0.80, false),
    ("clear", 0.45, false),
];

fn apply_effects(window: &tauri::WebviewWindow, effect: &str) -> Result<(), String> {
    let (_, alpha, use_blur) = WINDOW_EFFECTS
        .iter()
        .find(|(name, _, _)| *name == effect)
        .unwrap_or(&("solid", 1.0, false));

    #[cfg(target_os = "windows")]
    {
        set_window_alpha(window, *alpha)?;
        let _ = window_vibrancy::clear_blur(window);
        let _ = window_vibrancy::clear_acrylic(window);
        if *use_blur {
            // window-vibrancy expects RGBA. Keep the fallback surface dark while
            // the webview is loading or repainting during a resize.
            window_vibrancy::apply_blur(window, Some((0x1A, 0x1A, 0x1A, 0xCC)))
                .map_err(|e| format!("Blur error: {e}"))?;
        } else {
            let _ = window.set_effects(None);
        }
    }

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};
        let _ = alpha;
        let _ = window_vibrancy::clear_vibrancy(window);
        if *use_blur {
            let material = if effect == "frosted" {
                NSVisualEffectMaterial::HudWindow
            } else {
                NSVisualEffectMaterial::WindowBackground
            };
            window_vibrancy::apply_vibrancy(
                window,
                material,
                Some(NSVisualEffectState::FollowsWindowActiveState),
                None,
            )
            .map_err(|error| format!("Window effect error: {error}"))?;
        } else {
            let _ = window.set_effects(None);
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (alpha, use_blur, effect);
        let _ = window.set_effects(None);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_window_alpha(window: &tauri::WebviewWindow, alpha: f64) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW, GWL_EXSTYLE, LWA_ALPHA,
        WS_EX_LAYERED,
    };
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0;
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as i32);
        if SetLayeredWindowAttributes(hwnd, 0, (alpha * 255.0).round() as u8, LWA_ALPHA) == 0 {
            return Err("Unable to set window opacity".into());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn set_window_effect(app: tauri::AppHandle, effect: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    apply_effects(&window, &effect)
}

#[tauri::command]
pub async fn set_always_on_top(app: tauri::AppHandle, on_top: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window not found")?;
    window.set_always_on_top(on_top).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart()
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    app.exit(0)
}
