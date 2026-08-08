mod codex;
mod codex_overlay;
mod models;
mod token_usage;
mod window_material;

use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use models::{ProviderSnapshot, TokenUsageSummary, WidgetPreferences};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_window_state::Builder as WindowStateBuilder;

struct AppState {
    client: reqwest::Client,
    preferences: Mutex<WidgetPreferences>,
    preferences_path: PathBuf,
    fetch_lock: tokio::sync::Mutex<()>,
    snapshot_cache: Mutex<Option<(Instant, Vec<ProviderSnapshot>)>>,
    token_usage_cache: Arc<Mutex<token_usage::TokenUsageCache>>,
    palette_generation: AtomicU64,
}

async fn fetch_snapshots_uncached(state: &State<'_, AppState>) -> Vec<ProviderSnapshot> {
    let _guard = state.fetch_lock.lock().await;
    let values = vec![codex::fetch_snapshot(&state.client).await];
    if let Ok(mut cache) = state.snapshot_cache.lock() {
        *cache = Some((Instant::now(), values.clone()));
    }
    values
}

fn load_preferences(path: &PathBuf) -> WidgetPreferences {
    let parse = |candidate: &PathBuf| {
        fs::read_to_string(candidate)
            .ok()
            .and_then(|raw| serde_json::from_str::<WidgetPreferences>(&raw).ok())
    };
    if let Some(value) = parse(path) {
        return value.normalized();
    }
    let backup = path.with_extension("json.bak");
    if let Some(value) = parse(&backup) {
        eprintln!("preferences recovered from backup");
        return value.normalized();
    }
    WidgetPreferences::default()
}

fn persist_preferences(path: &PathBuf, value: &WidgetPreferences) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "failed to create settings directory".to_string())?;
    }
    let serialized =
        serde_json::to_vec_pretty(value).map_err(|_| "failed to serialize settings".to_string())?;
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = fs::File::create(&temporary)
        .map_err(|_| "failed to create temporary settings file".to_string())?;
    file.write_all(&serialized)
        .and_then(|_| file.sync_all())
        .map_err(|_| "failed to write settings".to_string())?;
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|_| "failed to back up settings".to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(format!("failed to commit settings: {error}"));
    }
    Ok(())
}

#[tauri::command]
async fn get_snapshots(state: State<'_, AppState>) -> Result<Vec<ProviderSnapshot>, String> {
    const CACHE_TTL: Duration = Duration::from_secs(30);
    if let Ok(cache) = state.snapshot_cache.lock() {
        if let Some((time, values)) = &*cache {
            if time.elapsed() < CACHE_TTL {
                return Ok(values.clone());
            }
        }
    }
    let _guard = match state.fetch_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            if let Ok(cache) = state.snapshot_cache.lock() {
                if let Some((_, values)) = &*cache {
                    return Ok(values.clone());
                }
            }
            return Ok(vec![ProviderSnapshot::failure(
                "unavailable",
                "Quota refresh is already running.",
            )]);
        }
    };
    if let Ok(cache) = state.snapshot_cache.lock() {
        if let Some((time, values)) = &*cache {
            if time.elapsed() < CACHE_TTL {
                return Ok(values.clone());
            }
        }
    }
    let values = vec![codex::fetch_snapshot(&state.client).await];
    if let Ok(mut cache) = state.snapshot_cache.lock() {
        *cache = Some((Instant::now(), values.clone()));
    }
    Ok(values)
}

#[tauri::command]
async fn refresh_snapshots(state: State<'_, AppState>) -> Result<Vec<ProviderSnapshot>, String> {
    Ok(fetch_snapshots_uncached(&state).await)
}

#[tauri::command]
async fn get_token_usage(state: State<'_, AppState>) -> Result<TokenUsageSummary, String> {
    let cache = Arc::clone(&state.token_usage_cache);
    tauri::async_runtime::spawn_blocking(move || token_usage::scan(&cache))
        .await
        .map_err(|error| format!("Token usage scan failed: {error}"))?
}

#[tauri::command]
fn get_preferences(state: State<'_, AppState>) -> Result<WidgetPreferences, String> {
    state
        .preferences
        .lock()
        .map(|value| value.clone())
        .map_err(|_| "settings unavailable".into())
}

#[tauri::command]
fn set_preferences(
    preferences: WidgetPreferences,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let preferences = preferences.normalized();
    persist_preferences(&state.preferences_path, &preferences)?;
    *state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())? = preferences;
    Ok(())
}

fn apply_lock(app: &AppHandle, locked: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("widget")
        .ok_or_else(|| "widget window missing".to_string())?;
    window
        .set_ignore_cursor_events(locked)
        .map_err(|_| "failed to toggle click-through".to_string())
}

fn position_palette_windows(app: &AppHandle) -> Result<(), String> {
    let widget = app
        .get_webview_window("widget")
        .ok_or_else(|| "widget window missing".to_string())?;
    let palette = app
        .get_webview_window("palette")
        .ok_or_else(|| "palette window missing".to_string())?;
    let editor = app
        .get_webview_window("palette-editor")
        .ok_or_else(|| "palette editor window missing".to_string())?;
    if !palette.is_visible().unwrap_or(false) || !editor.is_visible().unwrap_or(false) {
        return Ok(());
    }

    let widget_position = widget.outer_position().map_err(|error| error.to_string())?;
    let widget_size = widget.outer_size().map_err(|error| error.to_string())?;
    let palette_size = palette.outer_size().map_err(|error| error.to_string())?;
    let editor_size = editor.outer_size().map_err(|error| error.to_string())?;
    let scale = widget.scale_factor().unwrap_or(1.0);
    let gap = (8.0 * scale).round() as i32;
    let below = widget_position.y + widget_size.height as i32 + gap;
    let group_height = palette_size.height as i32 + gap + editor_size.height as i32;
    let work_area = widget
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| *monitor.work_area());
    let monitor_top = work_area.map(|area| area.position.y).unwrap_or(i32::MIN);
    let monitor_bottom = work_area
        .map(|area| area.position.y + area.size.height as i32)
        .unwrap_or(i32::MAX);
    let palette_y = if below + group_height <= monitor_bottom {
        below
    } else {
        (widget_position.y - group_height - gap).max(monitor_top)
    };
    palette
        .set_position(tauri::PhysicalPosition::new(widget_position.x, palette_y))
        .map_err(|error| format!("failed to position palette window: {error}"))?;
    editor
        .set_position(tauri::PhysicalPosition::new(
            widget_position.x,
            palette_y + palette_size.height as i32 + gap,
        ))
        .map_err(|error| format!("failed to position palette editor window: {error}"))
}

#[tauri::command]
fn open_palette_preview(
    percent: u8,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.palette_generation.fetch_add(1, Ordering::SeqCst);
    let palette = app
        .get_webview_window("palette")
        .ok_or_else(|| "palette window missing".to_string())?;
    palette
        .show()
        .map_err(|error| format!("failed to show palette window: {error}"))?;
    let editor = app
        .get_webview_window("palette-editor")
        .ok_or_else(|| "palette editor window missing".to_string())?;
    editor
        .show()
        .map_err(|error| format!("failed to show palette editor window: {error}"))?;
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = widget.set_always_on_top(true);
        window_material::sync_window_material(&app);
    }
    let _ = palette.set_always_on_top(true);
    let _ = editor.set_always_on_top(true);
    position_palette_windows(&app)?;
    let colors = state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())?
        .palette_colors
        .clone();
    let payload = serde_json::json!({ "percent": percent.min(100), "colors": colors });
    app.emit_to("palette", "palette-preview-opened", payload.clone())
        .map_err(|error| format!("failed to initialize palette preview: {error}"))?;
    app.emit_to("palette-editor", "palette-preview-opened", payload)
        .map_err(|error| format!("failed to initialize palette editor: {error}"))?;
    let _ = palette.set_focus();
    Ok(())
}

fn finish_palette_preview(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.palette_generation.fetch_add(1, Ordering::SeqCst);
    }
    if let Some(palette) = app.get_webview_window("palette") {
        let _ = palette.hide();
    }
    if let Some(editor) = app.get_webview_window("palette-editor") {
        let _ = editor.hide();
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(preferences) = state.preferences.lock() {
            if let Some(widget) = app.get_webview_window("widget") {
                let _ = widget.set_always_on_top(preferences.always_on_top);
                window_material::sync_window_material(app);
            }
        }
    }
    let _ = app.emit_to("widget", "palette-preview-closed", ());
}

#[tauri::command]
fn update_palette_preview(percent: u8, app: AppHandle) -> Result<(), String> {
    let percent = percent.min(100);
    app.emit_to("widget", "palette-preview-changed", percent)
        .map_err(|error| format!("failed to update palette preview: {error}"))?;
    app.emit_to("palette-editor", "palette-preview-changed", percent)
        .map_err(|error| format!("failed to update palette editor: {error}"))
}

#[tauri::command]
fn update_palette_colors(colors: Vec<String>, app: AppHandle) -> Result<(), String> {
    if !models::valid_palette_colors(&colors) {
        return Err("invalid palette colors".to_string());
    }
    app.emit_to("widget", "palette-colors-changed", colors.clone())
        .map_err(|error| format!("failed to update widget colors: {error}"))?;
    app.emit_to("palette", "palette-colors-changed", colors)
        .map_err(|error| format!("failed to update palette colors: {error}"))
}

#[tauri::command]
fn save_palette_colors(
    colors: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WidgetPreferences, String> {
    if !models::valid_palette_colors(&colors) {
        return Err("invalid palette colors".to_string());
    }
    let mut preferences = state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())?
        .clone();
    preferences.palette_colors = colors;
    preferences = preferences.normalized();
    persist_preferences(&state.preferences_path, &preferences)?;
    *state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())? = preferences.clone();
    app.emit_to("widget", "preferences-changed", preferences.clone())
        .map_err(|error| format!("failed to publish palette settings: {error}"))?;
    Ok(preferences)
}

#[tauri::command]
fn close_palette_preview(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let generation = state.palette_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit_to("palette", "palette-preview-closing", ());
    let _ = app.emit_to("palette-editor", "palette-preview-closing", ());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(220)).await;
        let should_finish = app
            .try_state::<AppState>()
            .map(|state| state.palette_generation.load(Ordering::SeqCst) == generation)
            .unwrap_or(false);
        if should_finish {
            finish_palette_preview(&app);
        }
    });
    Ok(())
}

#[tauri::command]
fn set_widget_locked(
    locked: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WidgetPreferences, String> {
    let previous = state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())?
        .clone();
    let mut next = previous.clone();
    next.locked = locked;
    persist_preferences(&state.preferences_path, &next)?;
    if let Err(error) = apply_lock(&app, locked) {
        let _ = persist_preferences(&state.preferences_path, &previous);
        return Err(error);
    }
    *state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())? = next.clone();
    Ok(next)
}

#[tauri::command]
fn set_widget_always_on_top(
    always_on_top: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WidgetPreferences, String> {
    let previous = state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())?
        .clone();
    let mut next = previous.clone();
    next.always_on_top = always_on_top;
    persist_preferences(&state.preferences_path, &next)?;
    let window = app
        .get_webview_window("widget")
        .ok_or_else(|| "widget window missing".to_string())?;
    if let Err(error) = window.set_always_on_top(always_on_top) {
        let _ = persist_preferences(&state.preferences_path, &previous);
        return Err(format!("failed to toggle always-on-top: {error}"));
    }
    window_material::sync_window_material(&app);
    *state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())? = next.clone();
    let _ = app.emit_to("widget", "preferences-changed", next.clone());
    Ok(next)
}

#[tauri::command]
fn set_widget_clip(expanded: bool, app: AppHandle) {
    window_material::animate_widget_region(app, expanded);
}

#[tauri::command]
fn set_widget_css_scale(scale: f64, app: AppHandle) {
    window_material::set_widget_css_scale(&app, scale as f32);
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show / Hide", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let unlock = MenuItem::with_id(app, "unlock", "Unlock widget", true, None::<&str>)?;
    let pin = MenuItem::with_id(app, "pin", "Pin / Unpin Codex", true, None::<&str>)?;
    let language = MenuItem::with_id(
        app,
        "language",
        "Switch Language / 切换语言",
        true,
        None::<&str>,
    )?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &refresh, &unlock, &pin, &language, &autostart, &quit],
    )?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Quota Beacon");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let autostart_menu = autostart.clone();
    builder
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("widget") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        window_material::hide_window_material();
                        finish_palette_preview(app);
                    } else {
                        let _ = window.show();
                        window_material::sync_window_material(app);
                        let _ = window.set_focus();
                    }
                }
            }
            "refresh" => {
                let _ = app.emit_to("widget", "refresh-requested", ());
            }
            "unlock" => {
                let _ = apply_lock(app, false);
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut prefs) = state.preferences.lock() {
                        prefs.locked = false;
                        let _ = persist_preferences(&state.preferences_path, &prefs);
                        let _ = app.emit_to("widget", "preferences-changed", prefs.clone());
                    }
                }
            }
            "pin" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut prefs) = state.preferences.lock() {
                        prefs.pinned_provider = if prefs.pinned_provider.is_some() {
                            None
                        } else {
                            Some("codex".into())
                        };
                        let _ = persist_preferences(&state.preferences_path, &prefs);
                        let _ = app.emit_to("widget", "preferences-changed", prefs.clone());
                    }
                }
            }
            "language" => {
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(mut prefs) = state.preferences.lock() {
                        prefs.language = if prefs.language == "en" {
                            "zh-CN".into()
                        } else {
                            "en".into()
                        };
                        let normalized = prefs.clone().normalized();
                        *prefs = normalized.clone();
                        let _ = persist_preferences(&state.preferences_path, &normalized);
                        let _ = app.emit_to("widget", "preferences-changed", normalized);
                    }
                }
            }
            "autostart" => {
                let manager = app.autolaunch();
                let enabled = manager.is_enabled().unwrap_or(false);
                let result = if enabled {
                    manager.disable()
                } else {
                    manager.enable()
                };
                match result {
                    Ok(()) => {
                        let _ = autostart_menu.set_checked(!enabled);
                    }
                    Err(_) => eprintln!("autostart update failed"),
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("widget") {
                let _ = window.show();
                window_material::sync_window_material(app);
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            WindowStateBuilder::default()
                .with_denylist(&["palette", "palette-editor"])
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_config_dir()?;
            let preferences_path = data_dir.join("preferences.json");
            let preferences = load_preferences(&preferences_path);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("QuotaFloat/0.1")
                .build()
                .expect("static HTTP client configuration must be valid");
            let token_usage_cache = Arc::new(Mutex::new(token_usage::TokenUsageCache::default()));
            app.manage(AppState {
                client,
                preferences: Mutex::new(preferences.clone()),
                preferences_path,
                fetch_lock: tokio::sync::Mutex::new(()),
                snapshot_cache: Mutex::new(None),
                token_usage_cache: Arc::clone(&token_usage_cache),
                palette_generation: AtomicU64::new(0),
            });
            let material_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                if let Some(window) = material_app.get_webview_window("widget") {
                    let apply_app = material_app.clone();
                    let _ = window.run_on_main_thread(move || {
                        window_material::apply_window_materials(&apply_app);
                    });
                }
            });
            codex_overlay::start(app.handle().clone(), token_usage_cache);
            if setup_tray(app).is_err() {
                eprintln!("tray setup failed; enabling taskbar fallback");
                if let Some(window) = app.get_webview_window("widget") {
                    let _ = window.set_skip_taskbar(false);
                }
            }
            if preferences.locked {
                let _ = apply_lock(app.handle(), true);
            }
            if let Some(window) = app.get_webview_window("widget") {
                let _ = window.set_always_on_top(preferences.always_on_top);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshots,
            refresh_snapshots,
            get_token_usage,
            get_preferences,
            set_preferences,
            set_widget_locked,
            set_widget_always_on_top,
            set_widget_clip,
            set_widget_css_scale,
            open_palette_preview,
            update_palette_preview,
            update_palette_colors,
            save_palette_colors,
            close_palette_preview
        ])
        .on_tray_icon_event(|app, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = app.get_webview_window("widget") {
                    let _ = window.show();
                    window_material::sync_window_material(app);
                    let _ = window.set_focus();
                }
            }
        })
        .on_window_event(|window, event| {
            if window.label() == "widget"
                && matches!(
                    event,
                    WindowEvent::Moved(_)
                        | WindowEvent::Resized(_)
                        | WindowEvent::ScaleFactorChanged { .. }
                )
            {
                window_material::sync_window_material(window.app_handle());
                let _ = position_palette_windows(window.app_handle());
            }
            if ["widget", "palette", "palette-editor"].contains(&window.label())
                && matches!(event, WindowEvent::Focused(false))
            {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let app_focused = ["widget", "palette", "palette-editor"].iter().any(|label| {
                        app.get_webview_window(label)
                            .and_then(|window| window.is_focused().ok())
                            .unwrap_or(false)
                    });
                    if !app_focused {
                        let _ = app.emit_to("widget", "widget-focus-lost", ());
                    }
                });
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                if window.label() == "widget" {
                    window_material::hide_window_material();
                }
                if window.label() == "palette" || window.label() == "palette-editor" {
                    finish_palette_preview(window.app_handle());
                } else if window.label() == "widget" {
                    finish_palette_preview(window.app_handle());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Quota Beacon");
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Resumed) {
            let _ = app_handle.emit_to("widget", "refresh-requested", ());
        }
        if matches!(event, tauri::RunEvent::Exit) {
            window_material::destroy_window_materials();
        }
    });
}
