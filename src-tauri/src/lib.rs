mod account_quota;
mod account_vault;
mod codex;
mod models;
mod token_usage;

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use futures_util::{stream, StreamExt};
use models::{AccountWeeklyQuota, ProviderSnapshot, TokenUsageSummary, WidgetPreferences};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_window_state::Builder as WindowStateBuilder;

const HTTP_USER_AGENT: &str = concat!("Quota-Beacon/", env!("CARGO_PKG_VERSION"));

struct AppState {
    client: reqwest::Client,
    preferences: Mutex<WidgetPreferences>,
    preferences_path: PathBuf,
    fetch_lock: tokio::sync::Mutex<()>,
    snapshot_cache: Mutex<Option<(Instant, Vec<ProviderSnapshot>)>>,
    account_quota_state: Mutex<account_quota::AccountQuotaState>,
    account_window_generation: AtomicU64,
    account_generation: AtomicU64,
    token_usage_cache: Arc<Mutex<token_usage::TokenUsageCache>>,
    palette_generation: AtomicU64,
    account_vault: Mutex<account_vault::AccountVault>,
    account_switch_lock: tokio::sync::Mutex<()>,
    account_login_task: Mutex<Option<AccountLoginTask>>,
    account_login_results: Mutex<HashMap<String, (Instant, AccountLoginStatus)>>,
    account_login_root: PathBuf,
}

struct AccountLoginTask {
    id: String,
    alias: String,
    replace_profile_id: Option<String>,
    task_root: PathBuf,
    child: Child,
    started_at: Instant,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountLoginStatus {
    task_id: String,
    status: &'static str,
    message: Option<String>,
}

async fn fetch_current_account_snapshots(state: &AppState) -> Vec<ProviderSnapshot> {
    for _ in 0..3 {
        let generation = state.account_generation.load(Ordering::SeqCst);
        let values = vec![codex::fetch_snapshot(&state.client).await];
        if !account_response_is_current(generation, &state.account_generation) {
            continue;
        }
        if let Ok(mut cache) = state.snapshot_cache.lock() {
            *cache = Some((Instant::now(), values.clone()));
        }
        return values;
    }
    vec![ProviderSnapshot::failure(
        "unavailable",
        "Account changed repeatedly while quota was refreshing.",
    )]
}

fn account_response_is_current(generation: u64, current: &AtomicU64) -> bool {
    generation == current.load(Ordering::SeqCst)
}

async fn fetch_snapshots_uncached(state: &State<'_, AppState>) -> Vec<ProviderSnapshot> {
    let _guard = state.fetch_lock.lock().await;
    fetch_current_account_snapshots(state.inner()).await
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
    Ok(fetch_current_account_snapshots(state.inner()).await)
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
    *state
        .preferences
        .lock()
        .map_err(|_| "settings unavailable".to_string())? = next.clone();
    let _ = app.emit_to("widget", "preferences-changed", next.clone());
    Ok(next)
}

fn publish_account_vault(
    app: &AppHandle,
    state: &AppState,
) -> Result<account_vault::AccountVaultView, String> {
    let view = reconciled_account_vault(state)?;
    let _ = app.emit_to("widget", "account-vault-changed", view.clone());
    let _ = app.emit_to("account-switcher", "account-vault-changed", view.clone());
    let _ = refresh_tray_menu(app);
    Ok(view)
}

fn invalidate_reconciled_account(state: &AppState, view: &account_vault::AccountVaultView) {
    state.account_generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut cache) = state.snapshot_cache.lock() {
        *cache = None;
    }
    if let Ok(mut quota_state) = state.account_quota_state.lock() {
        for profile in &view.profiles {
            quota_state.invalidate(&profile.id);
        }
    }
}

fn reconciled_account_vault(state: &AppState) -> Result<account_vault::AccountVaultView, String> {
    let (view, changed) = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .reconcile_view()?;
    if changed {
        invalidate_reconciled_account(state, &view);
    }
    Ok(view)
}

#[tauri::command]
fn get_account_vault(
    state: State<'_, AppState>,
) -> Result<account_vault::AccountVaultView, String> {
    reconciled_account_vault(state.inner())
}

fn weekly_quota_from_snapshot(
    profile_id: String,
    snapshot: ProviderSnapshot,
) -> AccountWeeklyQuota {
    let remaining_percent = snapshot
        .weekly_window
        .as_ref()
        .map(|window| window.remaining_percent);
    let status = if snapshot.status == "ok" && remaining_percent.is_none() {
        "unavailable".to_string()
    } else {
        snapshot.status
    };
    let message = if status == "unavailable" && snapshot.message.is_none() {
        Some("Weekly quota is unavailable.".to_string())
    } else {
        snapshot.message
    };
    AccountWeeklyQuota {
        profile_id,
        remaining_percent,
        status,
        message,
    }
}

fn codex_snapshot(snapshots: &[ProviderSnapshot]) -> Option<ProviderSnapshot> {
    snapshots
        .iter()
        .find(|snapshot| snapshot.provider == "codex")
        .cloned()
}

#[tauri::command]
async fn get_account_weekly_quotas(
    state: State<'_, AppState>,
) -> Result<Vec<AccountWeeklyQuota>, String> {
    let window_generation = state.account_window_generation.load(Ordering::SeqCst);
    let vault_view = reconciled_account_vault(state.inner())?;
    let profile_ids = vault_view
        .profiles
        .iter()
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();
    let existing_profile_ids = profile_ids.iter().cloned().collect::<HashSet<_>>();
    if let Ok(mut quota_state) = state.account_quota_state.lock() {
        quota_state.prune(&existing_profile_ids);
    }

    let mut quotas = Vec::with_capacity(profile_ids.len());
    let mut requests = Vec::new();
    for profile_id in profile_ids {
        if vault_view.active_profile_id.as_deref() == Some(profile_id.as_str()) {
            let snapshot = state.snapshot_cache.lock().ok().and_then(|cache| {
                cache
                    .as_ref()
                    .and_then(|(_, values)| codex_snapshot(values))
            });
            quotas.push(match snapshot {
                Some(snapshot) => weekly_quota_from_snapshot(profile_id, snapshot),
                None => AccountWeeklyQuota {
                    profile_id,
                    remaining_percent: None,
                    status: "loading".into(),
                    message: None,
                },
            });
            continue;
        }

        let now = Instant::now();
        let cached = state
            .account_quota_state
            .lock()
            .ok()
            .and_then(|mut quota_state| quota_state.get_fresh(&profile_id, now));
        if let Some(cached) = cached {
            quotas.push(cached);
            continue;
        }

        let generation = state
            .account_quota_state
            .lock()
            .map_err(|_| "Account quota cache is busy.".to_string())?
            .generation(&profile_id);
        let credentials = state
            .account_vault
            .lock()
            .map_err(|_| "Account storage is busy.".to_string())?
            .read_profile_credentials(&profile_id);
        match credentials {
            Ok(raw) => requests.push((profile_id, generation, raw)),
            Err(message) => quotas.push(AccountWeeklyQuota {
                profile_id: profile_id.clone(),
                remaining_percent: None,
                status: "signed_out".into(),
                message: Some(message),
            }),
        }
    }

    let client = state.client.clone();
    let fetched = stream::iter(requests.into_iter().map(|(profile_id, generation, raw)| {
        let client = client.clone();
        async move {
            let snapshot = codex::fetch_weekly_snapshot_from_bytes(&client, &raw).await;
            (
                profile_id.clone(),
                generation,
                weekly_quota_from_snapshot(profile_id, snapshot),
            )
        }
    }))
    .buffer_unordered(3)
    .collect::<Vec<_>>()
    .await;

    for (profile_id, generation, quota) in fetched {
        if state.account_window_generation.load(Ordering::SeqCst) != window_generation {
            continue;
        }
        let inserted = state
            .account_quota_state
            .lock()
            .map(|mut quota_state| {
                quota_state.insert_if_current(
                    &profile_id,
                    generation,
                    quota.clone(),
                    Instant::now(),
                )
            })
            .unwrap_or(false);
        if inserted {
            quotas.push(quota);
        }
    }
    quotas.sort_by_key(|quota| {
        vault_view
            .profiles
            .iter()
            .position(|profile| profile.id == quota.profile_id)
            .unwrap_or(usize::MAX)
    });
    Ok(quotas)
}

#[tauri::command]
fn save_current_account(
    alias: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::AccountVaultView, String> {
    let view = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .save_current(&alias)?;
    if let Ok(mut quota_state) = state.account_quota_state.lock() {
        if let Some(profile_id) = &view.active_profile_id {
            quota_state.invalidate(profile_id);
        }
    }
    state.account_generation.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit_to("widget", "account-vault-changed", view.clone());
    let _ = app.emit_to("account-switcher", "account-vault-changed", view.clone());
    let _ = refresh_tray_menu(&app);
    Ok(view)
}

#[tauri::command]
fn rename_account(
    profile_id: String,
    alias: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::AccountVaultView, String> {
    let view = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .rename(&profile_id, &alias)?;
    let _ = app.emit_to("widget", "account-vault-changed", view.clone());
    let _ = app.emit_to("account-switcher", "account-vault-changed", view.clone());
    let _ = refresh_tray_menu(&app);
    Ok(view)
}

#[tauri::command]
fn delete_account(
    profile_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::AccountVaultView, String> {
    let view = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .delete(&profile_id)?;
    if let Ok(mut quota_state) = state.account_quota_state.lock() {
        quota_state.invalidate(&profile_id);
    }
    state.account_generation.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit_to("widget", "account-vault-changed", view.clone());
    let _ = app.emit_to("account-switcher", "account-vault-changed", view.clone());
    let _ = refresh_tray_menu(&app);
    Ok(view)
}

async fn switch_account_internal(
    profile_id: &str,
    app: &AppHandle,
    state: &AppState,
) -> Result<account_vault::SwitchOutcome, String> {
    let _guard = state
        .account_switch_lock
        .try_lock()
        .map_err(|_| "Another account switch is already running.".to_string())?;
    let outcome = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .switch_to(profile_id)?;
    if let Ok(mut quota_state) = state.account_quota_state.lock() {
        quota_state.invalidate(profile_id);
    }
    state.account_generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut cache) = state.snapshot_cache.lock() {
        *cache = None;
    }
    let _ = publish_account_vault(app, state);
    let _ = app.emit_to("widget", "account-switch-completed", outcome.clone());
    let _ = app.emit_to(
        "account-switcher",
        "account-switch-completed",
        outcome.clone(),
    );
    let _ = app.emit_to("widget", "refresh-requested", ());
    Ok(outcome)
}

#[tauri::command]
async fn switch_account(
    profile_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::SwitchOutcome, String> {
    switch_account_internal(&profile_id, &app, state.inner()).await
}

fn codex_cli_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        PathBuf::from("/Applications/Codex.app/Contents/MacOS/codex"),
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin/codex"));
    }
    candidates
}

fn locate_codex_cli() -> PathBuf {
    codex_cli_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("codex"))
}

fn cleanup_login_task(root: &Path, task_root: &Path) {
    if task_root.starts_with(root) && task_root != root {
        let _ = fs::remove_dir_all(task_root);
    }
}

fn cleanup_stale_login_tasks(root: &Path) {
    let Ok(metadata) = fs::symlink_metadata(root) else {
        return;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let safe_directory = entry
            .file_type()
            .map(|kind| kind.is_dir() && !kind.is_symlink())
            .unwrap_or(false);
        if safe_directory {
            cleanup_login_task(root, &path);
        }
    }
}

#[cfg(unix)]
fn secure_login_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "Isolated Codex login directory is unavailable.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Isolated Codex login directory is not safe.".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "Isolated Codex login directory permissions could not be secured.".to_string())
}

#[cfg(not(unix))]
fn secure_login_directory(_: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn secure_login_auth_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "Isolated Codex login data is unavailable.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Isolated Codex login data is not safe.".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "Isolated Codex login data permissions could not be secured.".to_string())
}

#[cfg(not(unix))]
fn secure_login_auth_file(_: &Path) -> Result<(), String> {
    Ok(())
}

fn completed_login_status(
    results: &Mutex<HashMap<String, (Instant, AccountLoginStatus)>>,
    task_id: &str,
) -> Option<AccountLoginStatus> {
    let now = Instant::now();
    results.lock().ok().and_then(|mut results| {
        results.retain(|_, (completed_at, _)| {
            now.saturating_duration_since(*completed_at) < Duration::from_secs(10 * 60)
        });
        results.get(task_id).map(|(_, status)| status.clone())
    })
}

fn remember_login_status(
    results: &Mutex<HashMap<String, (Instant, AccountLoginStatus)>>,
    status: &AccountLoginStatus,
) {
    if status.status == "running" {
        return;
    }
    if let Ok(mut results) = results.lock() {
        results.insert(status.task_id.clone(), (Instant::now(), status.clone()));
    }
}

#[tauri::command]
fn begin_account_login(
    alias: String,
    replace_profile_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<AccountLoginStatus, String> {
    let alias = alias.trim().to_string();
    if alias.is_empty() || alias.chars().count() > 32 || alias.chars().any(char::is_control) {
        return Err("Account name must contain 1-32 visible characters.".into());
    }
    let mut slot = state
        .account_login_task
        .lock()
        .map_err(|_| "Account login state is busy.".to_string())?;
    if slot.is_some() {
        return Err("Another account login is already running.".into());
    }
    fs::create_dir_all(&state.account_login_root)
        .map_err(|_| "Account login directory could not be created.".to_string())?;
    let root_metadata = fs::symlink_metadata(&state.account_login_root)
        .map_err(|_| "Account login directory is unavailable.".to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("Account login directory is not safe.".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let task_root = state.account_login_root.join(&id);
    let codex_home = task_root.join("codex-home");
    fs::create_dir_all(&codex_home)
        .map_err(|_| "Isolated Codex login directory could not be created.".to_string())?;
    if let Err(message) = secure_login_directory(&codex_home) {
        cleanup_login_task(&state.account_login_root, &task_root);
        return Err(message);
    }
    let child = match Command::new(locate_codex_cli())
        .arg("login")
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            cleanup_login_task(&state.account_login_root, &task_root);
            return Err("Codex CLI was not found or login could not be started.".into());
        }
    };
    *slot = Some(AccountLoginTask {
        id: id.clone(),
        alias,
        replace_profile_id,
        task_root,
        child,
        started_at: Instant::now(),
    });
    Ok(AccountLoginStatus {
        task_id: id,
        status: "running",
        message: None,
    })
}

#[tauri::command]
fn poll_account_login(
    task_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AccountLoginStatus, String> {
    if let Some(status) = completed_login_status(&state.account_login_results, &task_id) {
        return Ok(status);
    }
    let mut slot = state
        .account_login_task
        .lock()
        .map_err(|_| "Account login state is busy.".to_string())?;
    let task = slot
        .as_mut()
        .ok_or_else(|| "No account login is running.".to_string())?;
    if task.id != task_id {
        return Err("Account login task does not match.".into());
    }
    if task.started_at.elapsed() > Duration::from_secs(10 * 60) {
        let mut task = slot.take().expect("login task was checked");
        let _ = task.child.kill();
        let _ = task.child.wait();
        cleanup_login_task(&state.account_login_root, &task.task_root);
        let status = AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some("Codex login timed out after 10 minutes.".into()),
        };
        remember_login_status(&state.account_login_results, &status);
        return Ok(status);
    }
    let Some(exit) = task
        .child
        .try_wait()
        .map_err(|_| "Account login status could not be read.".to_string())?
    else {
        return Ok(AccountLoginStatus {
            task_id,
            status: "running",
            message: None,
        });
    };
    let task = slot.take().expect("login task was checked");
    if !exit.success() {
        cleanup_login_task(&state.account_login_root, &task.task_root);
        let status = AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some("Codex login was cancelled or did not complete.".into()),
        };
        remember_login_status(&state.account_login_results, &status);
        return Ok(status);
    }
    let auth_path = task.task_root.join("codex-home/auth.json");
    if let Err(message) = secure_login_auth_file(&auth_path) {
        cleanup_login_task(&state.account_login_root, &task.task_root);
        return Ok(AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some(message),
        });
    }
    let raw = match codex::read_auth_bytes(&auth_path) {
        Ok(value) => value,
        Err(message) => {
            cleanup_login_task(&state.account_login_root, &task.task_root);
            let status = AccountLoginStatus {
                task_id,
                status: "failed",
                message: Some(message.into()),
            };
            remember_login_status(&state.account_login_results, &status);
            return Ok(status);
        }
    };
    let result = {
        let mut vault = state
            .account_vault
            .lock()
            .map_err(|_| "Account storage is busy.".to_string())?;
        match task.replace_profile_id.as_deref() {
            Some(profile_id) => vault
                .replace_credentials(profile_id, &raw)
                .map(|outcome| (outcome.view, outcome.current_login_replaced)),
            None => vault
                .import_credentials(&task.alias, &raw)
                .map(|view| (view, false)),
        }
    };
    cleanup_login_task(&state.account_login_root, &task.task_root);
    let status = match result {
        Ok((view, current_login_replaced)) => {
            if let Some(profile_id) = task.replace_profile_id.as_deref() {
                if let Ok(mut quota_state) = state.account_quota_state.lock() {
                    quota_state.invalidate(profile_id);
                }
                state.account_generation.fetch_add(1, Ordering::SeqCst);
                if current_login_replaced {
                    if let Ok(mut cache) = state.snapshot_cache.lock() {
                        *cache = None;
                    }
                    let _ = app.emit_to("widget", "refresh-requested", ());
                }
            }
            let _ = app.emit_to("widget", "account-vault-changed", view.clone());
            let _ = app.emit_to("account-switcher", "account-vault-changed", view);
            let _ = refresh_tray_menu(&app);
            AccountLoginStatus {
                task_id,
                status: "completed",
                message: None,
            }
        }
        Err(message) => AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some(message),
        },
    };
    remember_login_status(&state.account_login_results, &status);
    Ok(status)
}

#[tauri::command]
fn cancel_account_login(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = state
        .account_login_task
        .lock()
        .map_err(|_| "Account login state is busy.".to_string())?;
    let Some(mut task) = slot.take() else {
        return Ok(());
    };
    if task.id != task_id {
        *slot = Some(task);
        return Err("Account login task does not match.".into());
    }
    let _ = task.child.kill();
    let _ = task.child.wait();
    cleanup_login_task(&state.account_login_root, &task.task_root);
    remember_login_status(
        &state.account_login_results,
        &AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some("Codex login was cancelled.".into()),
        },
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn clamp_axis(value: i32, start: i32, span: u32, item_span: u32) -> i32 {
    let end = start.saturating_add(span as i32);
    let max = end.saturating_sub(item_span as i32).max(start);
    value.clamp(start, max)
}

fn account_window_position(
    widget: PhysicalRect,
    account_width: u32,
    account_height: u32,
    work_area: PhysicalRect,
    gap: i32,
) -> (i32, i32) {
    let x = clamp_axis(widget.x, work_area.x, work_area.width, account_width);
    let below = widget
        .y
        .saturating_add(widget.height as i32)
        .saturating_add(gap);
    let above = widget
        .y
        .saturating_sub(account_height as i32)
        .saturating_sub(gap);
    let work_bottom = work_area.y.saturating_add(work_area.height as i32);
    let y = if below.saturating_add(account_height as i32) <= work_bottom {
        below
    } else if above >= work_area.y {
        above
    } else {
        clamp_axis(widget.y, work_area.y, work_area.height, account_height)
    };
    (x, y)
}

fn position_account_window(app: &AppHandle) -> Result<(), String> {
    let widget = app
        .get_webview_window("widget")
        .ok_or_else(|| "widget window missing".to_string())?;
    let account = app
        .get_webview_window("account-switcher")
        .ok_or_else(|| "account window missing".to_string())?;
    if !account.is_visible().unwrap_or(false) {
        return Ok(());
    }
    let widget_position = widget.outer_position().map_err(|error| error.to_string())?;
    let widget_size = widget.outer_size().map_err(|error| error.to_string())?;
    let account_size = account.outer_size().map_err(|error| error.to_string())?;
    let scale = widget.scale_factor().unwrap_or(1.0);
    let gap = (8.0 * scale).round() as i32;
    let work_area = widget
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| *monitor.work_area());
    let work_area = work_area
        .map(|area| PhysicalRect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        })
        .unwrap_or(PhysicalRect {
            x: widget_position.x,
            y: i32::MIN / 4,
            width: account_size.width,
            height: u32::MAX / 2,
        });
    let (x, y) = account_window_position(
        PhysicalRect {
            x: widget_position.x,
            y: widget_position.y,
            width: widget_size.width,
            height: widget_size.height,
        },
        account_size.width,
        account_size.height,
        work_area,
        gap,
    );
    account
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|error| format!("failed to position account window: {error}"))
}

fn hide_account_switcher(app: &AppHandle) -> Result<(), String> {
    let account = app
        .get_webview_window("account-switcher")
        .ok_or_else(|| "account window missing".to_string())?;
    if !account.is_visible().unwrap_or(false) {
        return Ok(());
    }
    account
        .hide()
        .map_err(|error| format!("failed to hide account window: {error}"))?;
    if let Some(state) = app.try_state::<AppState>() {
        state
            .account_window_generation
            .fetch_add(1, Ordering::SeqCst);
    }
    let _ = app.emit_to("account-switcher", "account-switcher-closed", ());
    let _ = app.emit_to("widget", "account-switcher-closed", ());
    Ok(())
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountWindowTheme {
    percent: Option<u8>,
    colors: Vec<String>,
}

impl AccountWindowTheme {
    fn normalized(mut self) -> Self {
        self.percent = self.percent.map(|percent| percent.min(100));
        if models::valid_palette_colors(&self.colors) {
            self.colors = self
                .colors
                .into_iter()
                .map(|color| color.to_ascii_lowercase())
                .collect();
        } else {
            self.colors = WidgetPreferences::default().palette_colors;
        }
        self
    }
}

fn account_window_theme_from_state(state: &AppState) -> AccountWindowTheme {
    let colors = state
        .preferences
        .lock()
        .map(|preferences| preferences.palette_colors.clone())
        .unwrap_or_else(|_| WidgetPreferences::default().palette_colors);
    let percent = state
        .snapshot_cache
        .lock()
        .ok()
        .and_then(|cache| cache.as_ref().cloned())
        .and_then(|(_, snapshots)| {
            snapshots
                .into_iter()
                .find(|snapshot| snapshot.provider == "codex")
        })
        .and_then(|snapshot| snapshot.short_window)
        .map(|window| window.remaining_percent.round().clamp(0.0, 100.0) as u8);
    AccountWindowTheme { percent, colors }
}

#[tauri::command]
fn open_account_switcher(
    theme: AccountWindowTheme,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::AccountVaultView, String> {
    let theme = theme.normalized();
    finish_palette_preview(&app);
    let account = app
        .get_webview_window("account-switcher")
        .ok_or_else(|| "account window missing".to_string())?;
    account
        .show()
        .map_err(|error| format!("failed to show account window: {error}"))?;
    let _ = account.set_always_on_top(true);
    let opened = (|| {
        position_account_window(&app)?;
        publish_account_vault(&app, state.inner())
    })();
    let view = match opened {
        Ok(view) => view,
        Err(error) => {
            let _ = hide_account_switcher(&app);
            return Err(error);
        }
    };
    state
        .account_window_generation
        .fetch_add(1, Ordering::SeqCst);
    let _ = app.emit_to("account-switcher", "account-switcher-opened", theme);
    let _ = app.emit_to("widget", "account-switcher-opened", ());
    let _ = account.set_focus();
    Ok(view)
}

#[tauri::command]
fn update_account_switcher_theme(app: AppHandle, theme: AccountWindowTheme) {
    let _ = app.emit_to(
        "account-switcher",
        "account-switcher-theme-changed",
        theme.normalized(),
    );
}

#[tauri::command]
fn close_account_switcher(app: AppHandle) -> Result<(), String> {
    hide_account_switcher(&app)
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let chinese = app
        .try_state::<AppState>()
        .and_then(|state| {
            state
                .preferences
                .lock()
                .ok()
                .map(|preferences| preferences.language != "en")
        })
        .unwrap_or(true);
    let text = |zh: &'static str, en: &'static str| if chinese { zh } else { en };
    let show = MenuItem::with_id(
        app,
        "show",
        text("显示 / 隐藏", "Show / Hide"),
        true,
        None::<&str>,
    )?;
    let refresh = MenuItem::with_id(
        app,
        "refresh",
        text("立即刷新", "Refresh now"),
        true,
        None::<&str>,
    )?;
    let unlock = MenuItem::with_id(
        app,
        "unlock",
        text("解锁悬浮窗", "Unlock widget"),
        true,
        None::<&str>,
    )?;
    let pin = MenuItem::with_id(
        app,
        "pin",
        text("固定 / 取消固定 Codex", "Pin / Unpin Codex"),
        true,
        None::<&str>,
    )?;
    let language = MenuItem::with_id(
        app,
        "language",
        text("切换语言 / Switch Language", "Switch Language / 切换语言"),
        true,
        None::<&str>,
    )?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        text("登录时启动", "Start at login"),
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let accounts = Submenu::new(app, text("Codex 账号", "Codex Accounts"), true)?;
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(view) = reconciled_account_vault(state.inner()) {
            for profile in view.profiles {
                let label = if profile.is_active {
                    format!("✓ {}", profile.alias)
                } else {
                    profile.alias
                };
                let item = MenuItem::with_id(
                    app,
                    format!("account-switch:{}", profile.id),
                    label,
                    !profile.is_active && profile.credential_status == "ready",
                    None::<&str>,
                )?;
                accounts.append(&item)?;
            }
        }
    }
    let manage_accounts = MenuItem::with_id(
        app,
        "account-manage",
        text("管理账号…", "Manage accounts…"),
        true,
        None::<&str>,
    )?;
    accounts.append(&manage_accounts)?;
    let quit = MenuItem::with_id(app, "quit", text("退出", "Quit"), true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &show, &refresh, &accounts, &unlock, &pin, &language, &autostart, &quit,
        ],
    )
}

fn refresh_tray_menu(app: &AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(build_tray_menu(app)?))?;
    }
    Ok(())
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = build_tray_menu(app.handle())?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Quota Beacon");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(move |app, event| {
            if let Some(profile_id) = event.id.as_ref().strip_prefix("account-switch:") {
                let app = app.clone();
                let profile_id = profile_id.to_string();
                tauri::async_runtime::spawn(async move {
                    let result = if let Some(state) = app.try_state::<AppState>() {
                        switch_account_internal(&profile_id, &app, state.inner()).await
                    } else {
                        Err("Account state is unavailable.".into())
                    };
                    if let Err(message) = result {
                        let _ = app.emit_to("widget", "account-operation-error", message.clone());
                        let _ = app.emit_to("account-switcher", "account-operation-error", message);
                    }
                });
                return;
            }
            match event.id.as_ref() {
                "account-manage" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let theme = account_window_theme_from_state(state.inner());
                        let _ = open_account_switcher(theme, app.clone(), state);
                    }
                }
                "show" => {
                    if let Some(window) = app.get_webview_window("widget") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                            finish_palette_preview(app);
                            let _ = hide_account_switcher(app);
                        } else {
                            let _ = window.show();
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
                    let _ = refresh_tray_menu(app);
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
                            let _ = refresh_tray_menu(app);
                        }
                        Err(_) => eprintln!("autostart update failed"),
                    }
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            finish_palette_preview(app);
            let _ = hide_account_switcher(app);
            if let Some(window) = app.get_webview_window("widget") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            WindowStateBuilder::default()
                .with_denylist(&["palette", "palette-editor", "account-switcher"])
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_config_dir()?;
            let preferences_path = data_dir.join("preferences.json");
            let accounts_root = data_dir.join("accounts");
            let account_login_root = accounts_root.join("login-tasks");
            cleanup_stale_login_tasks(&account_login_root);
            let preferences = load_preferences(&preferences_path);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(HTTP_USER_AGENT)
                .build()
                .expect("static HTTP client configuration must be valid");
            let token_usage_cache = Arc::new(Mutex::new(token_usage::TokenUsageCache::default()));
            app.manage(AppState {
                client,
                preferences: Mutex::new(preferences.clone()),
                preferences_path,
                fetch_lock: tokio::sync::Mutex::new(()),
                snapshot_cache: Mutex::new(None),
                account_quota_state: Mutex::new(account_quota::AccountQuotaState::default()),
                account_window_generation: AtomicU64::new(0),
                account_generation: AtomicU64::new(0),
                token_usage_cache,
                palette_generation: AtomicU64::new(0),
                account_vault: Mutex::new(account_vault::AccountVault::load(accounts_root)),
                account_switch_lock: tokio::sync::Mutex::new(()),
                account_login_task: Mutex::new(None),
                account_login_results: Mutex::new(HashMap::new()),
                account_login_root,
            });
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
            open_palette_preview,
            update_palette_preview,
            update_palette_colors,
            save_palette_colors,
            close_palette_preview,
            get_account_vault,
            get_account_weekly_quotas,
            save_current_account,
            rename_account,
            delete_account,
            switch_account,
            begin_account_login,
            poll_account_login,
            cancel_account_login,
            open_account_switcher,
            update_account_switcher_theme,
            close_account_switcher
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
                    let _ = window.set_focus();
                }
            }
        })
        .on_window_event(|window, event| {
            if window.label() == "widget"
                && matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_))
            {
                let _ = position_palette_windows(window.app_handle());
                let _ = position_account_window(window.app_handle());
            }
            if window.label() == "account-switcher" && matches!(event, WindowEvent::Resized(_)) {
                let _ = position_account_window(window.app_handle());
            }
            if ["widget", "palette", "palette-editor", "account-switcher"].contains(&window.label())
                && matches!(event, WindowEvent::Focused(false))
            {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let app_focused = ["widget", "palette", "palette-editor", "account-switcher"]
                        .iter()
                        .any(|label| {
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
                if window.label() == "account-switcher" {
                    let _ = hide_account_switcher(window.app_handle());
                } else {
                    let _ = window.hide();
                }
                if window.label() == "palette" || window.label() == "palette-editor" {
                    finish_palette_preview(window.app_handle());
                } else if window.label() == "widget" {
                    finish_palette_preview(window.app_handle());
                    let _ = hide_account_switcher(window.app_handle());
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
            if let Some(state) = app_handle.try_state::<AppState>() {
                if let Ok(mut slot) = state.account_login_task.lock() {
                    if let Some(mut task) = slot.take() {
                        let _ = task.child.kill();
                        let _ = task.child.wait();
                        cleanup_login_task(&state.account_login_root, &task.task_root);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod account_generation_tests {
    use super::*;

    #[test]
    fn rejects_a_response_started_before_an_account_switch() {
        let generation = AtomicU64::new(7);
        assert!(account_response_is_current(7, &generation));
        generation.fetch_add(1, Ordering::SeqCst);
        assert!(!account_response_is_current(7, &generation));
        assert!(account_response_is_current(8, &generation));
    }
}

#[cfg(test)]
mod account_window_tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect {
        PhysicalRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn clamps_account_window_to_right_and_left_monitor_edges() {
        let work = rect(0, 24, 1440, 876);
        assert_eq!(
            account_window_position(rect(1380, 100, 100, 100), 320, 240, work, 8),
            (1120, 208)
        );
        assert_eq!(
            account_window_position(rect(-80, 100, 100, 100), 320, 240, work, 8),
            (0, 208)
        );
    }

    #[test]
    fn supports_negative_coordinate_monitors_and_chooses_above_near_the_dock() {
        let work = rect(-1920, 24, 1920, 1056);
        assert_eq!(
            account_window_position(rect(-1900, 900, 100, 100), 320, 240, work, 16),
            (-1900, 644)
        );
    }

    #[test]
    fn clamps_when_neither_above_nor_below_has_enough_space() {
        let work = rect(0, 24, 800, 300);
        assert_eq!(
            account_window_position(rect(200, 80, 100, 100), 320, 280, work, 8),
            (200, 44)
        );
    }

    #[test]
    fn completed_login_status_is_idempotent_for_repeated_polls() {
        let results = Mutex::new(HashMap::new());
        let status = AccountLoginStatus {
            task_id: "task".into(),
            status: "completed",
            message: None,
        };
        remember_login_status(&results, &status);
        assert_eq!(
            completed_login_status(&results, "task")
                .expect("completed status")
                .status,
            "completed"
        );
        assert_eq!(
            completed_login_status(&results, "task")
                .expect("same completed status")
                .status,
            "completed"
        );
    }
}

#[cfg(test)]
mod account_quota_tests {
    use super::*;

    #[test]
    fn weekly_quota_uses_only_the_weekly_window() {
        let mut snapshot = ProviderSnapshot::failure("unavailable", "temporary");
        snapshot.status = "ok".into();
        snapshot.message = None;
        snapshot.short_window = Some(models::UsageWindow {
            remaining_percent: 91.0,
            resets_at: None,
            window_seconds: 18_000,
        });
        let quota = weekly_quota_from_snapshot("profile".into(), snapshot);
        assert_eq!(quota.remaining_percent, None);
        assert_eq!(quota.status, "unavailable");
    }

    #[test]
    fn current_quota_finds_codex_when_snapshot_order_changes() {
        let mut other = ProviderSnapshot::failure("unavailable", "other");
        other.provider = "other".into();
        let codex = ProviderSnapshot::failure("signed_out", "codex");
        assert_eq!(
            codex_snapshot(&[other, codex])
                .expect("codex snapshot")
                .provider,
            "codex"
        );
    }

    #[test]
    fn account_window_theme_clamps_percent_and_rejects_invalid_palettes() {
        let normalized = AccountWindowTheme {
            percent: Some(128),
            colors: vec!["not-a-color".into()],
        }
        .normalized();
        assert_eq!(normalized.percent, Some(100));
        assert_eq!(normalized.colors, WidgetPreferences::default().palette_colors);

        let custom = AccountWindowTheme {
            percent: None,
            colors: vec![
                "#EB5B58".into(),
                "#F1A06F".into(),
                "#F5D98F".into(),
                "#E3F4B8".into(),
                "#B9E4C9".into(),
            ],
        }
        .normalized();
        assert_eq!(custom.percent, None);
        assert_eq!(custom.colors[0], "#eb5b58");
    }
}
