mod account_vault;
mod codex;
mod codex_overlay;
mod models;
mod token_usage;
mod window_material;

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use models::{AccountWeeklyQuota, ProviderSnapshot, TokenUsageSummary, WidgetPreferences};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
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
    persisted_snapshots: Mutex<Vec<ProviderSnapshot>>,
    snapshot_cache_path: PathBuf,
    account_quota_cache: Mutex<HashMap<String, (u64, Instant, AccountWeeklyQuota)>>,
    account_generation: AtomicU64,
    token_usage_cache: Arc<Mutex<token_usage::TokenUsageCache>>,
    palette_generation: AtomicU64,
    account_vault: Mutex<account_vault::AccountVault>,
    account_switch_lock: tokio::sync::Mutex<()>,
    account_login_task: Mutex<Option<AccountLoginTask>>,
    account_login_root: PathBuf,
}

const INACTIVE_ACCOUNT_QUOTA_TTL: Duration = Duration::from_secs(5 * 60);
const SNAPSHOT_CACHE_SCHEMA_VERSION: u8 = 1;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSnapshotCache {
    schema_version: u8,
    account_id: String,
    snapshots: Vec<ProviderSnapshot>,
}

fn snapshots_for_startup(values: &[ProviderSnapshot]) -> Vec<ProviderSnapshot> {
    values
        .iter()
        .filter(|snapshot| snapshot.status == "ok" && (snapshot.short_window.is_some() || snapshot.weekly_window.is_some()))
        .cloned()
        .map(|mut snapshot| {
            snapshot.status = "stale".into();
            snapshot.message = Some("Updating latest quota.".into());
            snapshot
        })
        .collect()
}

fn decode_persisted_snapshots(raw: &[u8], account_id: &str) -> Vec<ProviderSnapshot> {
    let Ok(cache) = serde_json::from_slice::<PersistedSnapshotCache>(raw) else {
        return Vec::new();
    };
    if cache.schema_version != SNAPSHOT_CACHE_SCHEMA_VERSION || cache.account_id != account_id {
        return Vec::new();
    }
    snapshots_for_startup(&cache.snapshots)
}

fn load_persisted_snapshots(path: &PathBuf) -> Vec<ProviderSnapshot> {
    let Ok(identity) = codex::current_credential_identity() else {
        return Vec::new();
    };
    fs::read(path)
        .ok()
        .map(|raw| decode_persisted_snapshots(&raw, &identity.account_id))
        .unwrap_or_default()
}

fn persist_successful_snapshots(path: &PathBuf, values: &[ProviderSnapshot]) -> Result<(), String> {
    let successful = values.iter().filter(|snapshot| snapshot.status == "ok").cloned().collect::<Vec<_>>();
    if successful.is_empty() {
        return Ok(());
    }
    let identity = codex::current_credential_identity().map_err(str::to_string)?;
    let cache = PersistedSnapshotCache {
        schema_version: SNAPSHOT_CACHE_SCHEMA_VERSION,
        account_id: identity.account_id,
        snapshots: successful,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "failed to create snapshot cache directory".to_string())?;
    }
    let serialized = serde_json::to_vec(&cache).map_err(|_| "failed to serialize snapshot cache".to_string())?;
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = fs::File::create(&temporary).map_err(|_| "failed to create temporary snapshot cache".to_string())?;
    file.write_all(&serialized)
        .and_then(|_| file.sync_all())
        .map_err(|_| "failed to write snapshot cache".to_string())?;
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|_| "failed to back up snapshot cache".to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(format!("failed to commit snapshot cache: {error}"));
    }
    Ok(())
}

fn clear_persisted_snapshots(state: &AppState) {
    if let Ok(mut values) = state.persisted_snapshots.lock() {
        values.clear();
    }
    match fs::remove_file(&state.snapshot_cache_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!("failed to clear snapshot cache: {error}"),
    }
    let _ = fs::remove_file(state.snapshot_cache_path.with_extension("json.bak"));
}

fn inactive_account_quota_is_fresh(updated_at: Instant) -> bool {
    updated_at.elapsed() < INACTIVE_ACCOUNT_QUOTA_TTL
}

fn inactive_account_quota_cache_is_current(
    cached_generation: u64,
    current_generation: u64,
    updated_at: Instant,
) -> bool {
    cached_generation == current_generation && inactive_account_quota_is_fresh(updated_at)
}

struct AccountLoginTask {
    id: String,
    alias: String,
    task_root: PathBuf,
    codex_home: PathBuf,
    child: Child,
    started_at: Instant,
    replace_profile_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountLoginStatus {
    task_id: String,
    status: &'static str,
    message: Option<String>,
}

async fn fetch_current_account_snapshots(state: &AppState) -> Vec<ProviderSnapshot> {
    const MAX_ACCOUNT_CHANGES_DURING_FETCH: usize = 4;
    for _ in 0..MAX_ACCOUNT_CHANGES_DURING_FETCH {
        let generation = state.account_generation.load(Ordering::SeqCst);
        let values = vec![codex::fetch_snapshot(&state.client).await];
        if generation != state.account_generation.load(Ordering::SeqCst) {
            continue;
        }
        if let Ok(mut cache) = state.snapshot_cache.lock() {
            *cache = Some((Instant::now(), values.clone()));
        }
        if generation == state.account_generation.load(Ordering::SeqCst) {
            if values.iter().any(|snapshot| snapshot.status == "ok") {
                if let Err(error) = persist_successful_snapshots(&state.snapshot_cache_path, &values) {
                    eprintln!("{error}");
                }
                if let Ok(mut cached) = state.persisted_snapshots.lock() {
                    *cached = snapshots_for_startup(&values);
                }
            }
            return values;
        }
    }
    vec![ProviderSnapshot::failure(
        "unavailable",
        "Account changed repeatedly while quota was refreshing.",
    )]
}

async fn fetch_snapshots_uncached(state: &State<'_, AppState>) -> Vec<ProviderSnapshot> {
    let _guard = match state.fetch_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            if let Ok(cache) = state.snapshot_cache.lock() {
                if let Some((_, values)) = &*cache {
                    return values.clone();
                }
            }
            return vec![ProviderSnapshot::failure(
                "unavailable",
                "Quota refresh is already running.",
            )];
        }
    };
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
fn get_cached_snapshots(state: State<'_, AppState>) -> Vec<ProviderSnapshot> {
    state.persisted_snapshots.lock().map(|values| values.clone()).unwrap_or_default()
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

fn publish_account_vault(app: &AppHandle, state: &AppState) -> Result<account_vault::AccountVaultView, String> {
    let view = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .view()?;
    let _ = app.emit_to("widget", "account-vault-changed", view.clone());
    let _ = app.emit_to("account-switcher", "account-vault-changed", view.clone());
    let _ = refresh_tray_menu(&app);
    Ok(view)
}

#[tauri::command]
fn get_account_vault(state: State<'_, AppState>) -> Result<account_vault::AccountVaultView, String> {
    state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .view()
}

fn weekly_quota_from_snapshot(profile_id: String, snapshot: ProviderSnapshot) -> AccountWeeklyQuota {
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

#[tauri::command]
async fn get_account_weekly_quotas(
    state: State<'_, AppState>,
) -> Result<Vec<AccountWeeklyQuota>, String> {
    let vault_view = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .view()?;
    let profile_ids = vault_view
        .profiles
        .iter()
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();
    if let Ok(mut cache) = state.account_quota_cache.lock() {
        cache.retain(|profile_id, _| profile_ids.contains(profile_id));
    }

    let mut quotas = Vec::with_capacity(profile_ids.len());
    for profile_id in profile_ids {
        if vault_view.active_profile_id.as_deref() == Some(profile_id.as_str()) {
            let snapshot = state
                .snapshot_cache
                .lock()
                .ok()
                .and_then(|cache| cache.as_ref().and_then(|(_, values)| values.first().cloned()));
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

        let current_generation = state.account_generation.load(Ordering::SeqCst);
        let cached = state
            .account_quota_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&profile_id).cloned())
            .filter(|(generation, updated_at, _)| {
                inactive_account_quota_cache_is_current(
                    *generation,
                    current_generation,
                    *updated_at,
                )
            })
            .map(|(_, _, quota)| quota);
        if let Some(cached) = cached {
            quotas.push(cached);
            continue;
        }

        const MAX_CREDENTIAL_CHANGES_DURING_FETCH: usize = 3;
        let mut refreshed = None;
        for _ in 0..MAX_CREDENTIAL_CHANGES_DURING_FETCH {
            let generation = state.account_generation.load(Ordering::SeqCst);
            let credentials = state
                .account_vault
                .lock()
                .map_err(|_| "Account storage is busy.".to_string())?
                .read_profile_credentials(&profile_id);
            let quota = match credentials {
                Ok(raw) => weekly_quota_from_snapshot(
                    profile_id.clone(),
                    codex::fetch_snapshot_from_bytes(&state.client, &raw).await,
                ),
                Err(message) => AccountWeeklyQuota {
                    profile_id: profile_id.clone(),
                    remaining_percent: None,
                    status: "signed_out".into(),
                    message: Some(message),
                },
            };
            if generation != state.account_generation.load(Ordering::SeqCst) {
                continue;
            }
            if let Ok(mut cache) = state.account_quota_cache.lock() {
                cache.insert(
                    profile_id.clone(),
                    (generation, Instant::now(), quota.clone()),
                );
            }
            refreshed = Some(quota);
            break;
        }
        quotas.push(refreshed.unwrap_or_else(|| AccountWeeklyQuota {
            profile_id,
            remaining_percent: None,
            status: "unavailable".into(),
            message: Some("Account changed repeatedly while quota was refreshing.".into()),
        }));
    }
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
    let _switch_guard = state
        .account_switch_lock
        .try_lock()
        .map_err(|_| "Another account switch is already running.".to_string())?;
    let outcome = state
        .account_vault
        .lock()
        .map_err(|_| "Account storage is busy.".to_string())?
        .switch_to(profile_id)?;
    state.account_generation.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut cache) = state.snapshot_cache.lock() {
        *cache = None;
    }
    clear_persisted_snapshots(state);
    let _ = publish_account_vault(app, state);
    let _ = app.emit_to("widget", "account-switch-completed", outcome.clone());
    let _ = app.emit_to("account-switcher", "account-switch-completed", outcome.clone());
    let _ = refresh_tray_menu(app);
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

#[cfg(windows)]
fn schedule_codex_restart() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const SCRIPT: &str = r#"Start-Sleep -Milliseconds 900; Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like '*\WindowsApps\OpenAI.Codex_*\app\ChatGPT.exe' -or $_.ExecutablePath -like '*\WindowsApps\OpenAI.Codex_*\app\resources\codex.exe' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }; Start-Sleep -Milliseconds 700; Start-Process explorer.exe 'shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App'"#;
    Command::new("powershell.exe")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", SCRIPT])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Codex restart could not be scheduled.".to_string())
}

#[cfg(not(windows))]
fn schedule_codex_restart() -> Result<(), String> {
    Err("Codex restart is currently available on Windows only.".into())
}

#[tauri::command]
async fn switch_account_and_restart_codex(
    profile_id: String,
    confirmed: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<account_vault::SwitchOutcome, String> {
    if !confirmed {
        return Err("Restart confirmation is required.".into());
    }
    let outcome = switch_account_internal(&profile_id, &app, state.inner()).await?;
    schedule_codex_restart()?;
    Ok(outcome)
}

fn locate_codex_cli() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_CLI_PATH").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("OpenAI").join("Codex").join("bin").join("codex.exe"))
        .filter(|path| path.is_file())
}

fn cleanup_login_task(state: &AppState, task_root: &PathBuf) {
    if task_root.starts_with(&state.account_login_root) && task_root != &state.account_login_root {
        let _ = fs::remove_dir_all(task_root);
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
    let cli = locate_codex_cli().ok_or_else(|| "Codex CLI was not found.".to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let task_root = state.account_login_root.join(&id);
    let codex_home = task_root.join("codex-home");
    fs::create_dir_all(&codex_home).map_err(|_| "Temporary login directory could not be created.".to_string())?;
    let child = Command::new(cli)
        .arg("login")
        .arg("-c")
        .arg("cli_auth_credentials_store=\"file\"")
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Codex login could not be started.".to_string())?;
    *slot = Some(AccountLoginTask {
        id: id.clone(),
        alias,
        task_root,
        codex_home,
        child,
        started_at: Instant::now(),
        replace_profile_id,
    });
    Ok(AccountLoginStatus { task_id: id, status: "running", message: None })
}

#[tauri::command]
fn poll_account_login(
    task_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AccountLoginStatus, String> {
    let mut slot = state
        .account_login_task
        .lock()
        .map_err(|_| "Account login state is busy.".to_string())?;
    let task = slot.as_mut().ok_or_else(|| "No account login is running.".to_string())?;
    if task.id != task_id {
        return Err("Account login task does not match.".into());
    }
    if task.started_at.elapsed() > Duration::from_secs(10 * 60) {
        let mut task = slot.take().expect("checked account login task must exist");
        let _ = task.child.kill();
        let _ = task.child.wait();
        cleanup_login_task(state.inner(), &task.task_root);
        return Ok(AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some("Codex login timed out after 10 minutes.".into()),
        });
    }
    let Some(exit) = task.child.try_wait().map_err(|_| "Account login status could not be read.".to_string())? else {
        return Ok(AccountLoginStatus { task_id, status: "running", message: None });
    };
    let task = slot.take().expect("checked account login task must exist");
    if !exit.success() {
        cleanup_login_task(state.inner(), &task.task_root);
        return Ok(AccountLoginStatus {
            task_id,
            status: "failed",
            message: Some("Codex login was cancelled or did not complete.".into()),
        });
    }
    let auth_path = task.codex_home.join("auth.json");
    let raw = match codex::read_auth_bytes(&auth_path) {
        Ok(raw) => raw,
        Err(message) => {
            cleanup_login_task(state.inner(), &task.task_root);
            return Ok(AccountLoginStatus { task_id, status: "failed", message: Some(message.into()) });
        }
    };
    let import = {
        let mut vault = state
            .account_vault
            .lock()
            .map_err(|_| "Account storage is busy.".to_string())?;
        match task.replace_profile_id.as_deref() {
            Some(profile_id) => vault.replace_credentials(profile_id, &raw),
            None => vault.import_credentials(&task.alias, &raw),
        }
    };
    cleanup_login_task(state.inner(), &task.task_root);
    match import {
        Ok(view) => {
            if let Some(profile_id) = task.replace_profile_id.as_deref() {
                state.account_generation.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut cache) = state.account_quota_cache.lock() {
                    cache.remove(profile_id);
                }
                if view.active_profile_id.as_deref() == Some(profile_id) {
                    if let Ok(mut cache) = state.snapshot_cache.lock() {
                        *cache = None;
                    }
                    let _ = app.emit_to("widget", "refresh-requested", "account-relogin");
                }
            }
            let _ = app.emit_to("widget", "account-vault-changed", view.clone());
            let _ = app.emit_to("account-switcher", "account-vault-changed", view);
            let _ = refresh_tray_menu(&app);
            Ok(AccountLoginStatus { task_id, status: "completed", message: None })
        }
        Err(message) => Ok(AccountLoginStatus { task_id, status: "failed", message: Some(message) }),
    }
}

#[tauri::command]
fn cancel_account_login(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut slot = state
        .account_login_task
        .lock()
        .map_err(|_| "Account login state is busy.".to_string())?;
    let Some(mut task) = slot.take() else { return Ok(()); };
    if task.id != task_id {
        *slot = Some(task);
        return Err("Account login task does not match.".into());
    }
    let _ = task.child.kill();
    let _ = task.child.wait();
    cleanup_login_task(state.inner(), &task.task_root);
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
    let work_area = widget.current_monitor().ok().flatten().map(|monitor| *monitor.work_area());
    let monitor_top = work_area.map(|area| area.position.y).unwrap_or(i32::MIN);
    let monitor_bottom = work_area.map(|area| area.position.y + area.size.height as i32).unwrap_or(i32::MAX);
    let below = widget_position.y + widget_size.height as i32 + gap;
    let y = if below + account_size.height as i32 <= monitor_bottom {
        below
    } else {
        (widget_position.y - account_size.height as i32 - gap).max(monitor_top)
    };
    account
        .set_position(tauri::PhysicalPosition::new(widget_position.x, y))
        .map_err(|error| format!("failed to position account window: {error}"))
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
        .set_size(tauri::LogicalSize::new(320.0, 190.0))
        .map_err(|error| format!("failed to reset account window size: {error}"))?;
    let _ = app.emit_to("account-switcher", "account-switcher-opened", theme);
    account.show().map_err(|error| format!("failed to show account window: {error}"))?;
    let _ = account.set_always_on_top(true);
    position_account_window(&app)?;
    window_material::sync_window_material(&app);
    let view = publish_account_vault(&app, state.inner())?;
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
    let account = app
        .get_webview_window("account-switcher")
        .ok_or_else(|| "account window missing".to_string())?;
    account.hide().map_err(|error| format!("failed to hide account window: {error}"))?;
    window_material::sync_window_material(&app);
    let _ = app.emit_to("widget", "account-switcher-closed", ());
    Ok(())
}

#[tauri::command]
fn open_palette_preview(
    percent: u8,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(account) = app.get_webview_window("account-switcher") {
        if account.is_visible().unwrap_or(false) {
            let _ = account.hide();
            let _ = app.emit_to("widget", "account-switcher-closed", ());
        }
    }
    state.palette_generation.fetch_add(1, Ordering::SeqCst);
    window_material::reset_palette_material();
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
    window_material::sync_window_material(&app);
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
    window_material::animate_palette_material(app.clone(), true);
    let _ = palette.set_focus();
    Ok(())
}

fn finish_palette_preview(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.palette_generation.fetch_add(1, Ordering::SeqCst);
    }
    window_material::reset_palette_material();
    if let Some(palette) = app.get_webview_window("palette") {
        let _ = palette.hide();
    }
    if let Some(editor) = app.get_webview_window("palette-editor") {
        let _ = editor.hide();
    }
    window_material::sync_window_material(app);
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
    window_material::animate_palette_material(app.clone(), false);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
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
    let menu = build_tray_menu(app.handle())?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Quota Beacon");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            if let Some(profile_id) = id.strip_prefix("account-switch:") {
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
                    let _ = refresh_tray_menu(&app);
                });
                return;
            }
            match id {
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
                    let _ = app.emit_to("widget", "refresh-requested", "manual");
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
                            prefs.pinned_provider = if prefs.pinned_provider.is_some() { None } else { Some("codex".into()) };
                            let _ = persist_preferences(&state.preferences_path, &prefs);
                            let _ = app.emit_to("widget", "preferences-changed", prefs.clone());
                        }
                    }
                }
                "language" => {
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut prefs) = state.preferences.lock() {
                            prefs.language = if prefs.language == "en" { "zh-CN".into() } else { "en".into() };
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
                    let result = if enabled { manager.disable() } else { manager.enable() };
                    if result.is_err() {
                        eprintln!("autostart update failed");
                    }
                    let _ = refresh_tray_menu(app);
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

fn build_tray_menu<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
    let unlock = MenuItem::with_id(app, "unlock", "解锁小组件", true, None::<&str>)?;
    let pin = MenuItem::with_id(app, "pin", "置顶 / 取消置顶 Codex", true, None::<&str>)?;
    let language = MenuItem::with_id(
        app,
        "language",
        "切换语言",
        true,
        None::<&str>,
    )?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let accounts = Submenu::new(app, "Codex 账号", true)?;
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(vault) = state.account_vault.lock() {
            if let Ok(view) = vault.view() {
                for profile in view.profiles {
                    let label = if profile.is_active { format!("✓ {}", profile.alias) } else { profile.alias };
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
    }
    let manage_accounts = MenuItem::with_id(app, "account-manage", "管理账号…", true, None::<&str>)?;
    accounts.append(&manage_accounts)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &refresh, &accounts, &unlock, &pin, &language, &autostart, &quit],
    )?;
    Ok(menu)
}

fn refresh_tray_menu<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }
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
                .with_denylist(&["palette", "palette-editor", "account-switcher"])
                .build(),
        )
        .setup(|app| {
            let data_dir = app.path().app_config_dir()?;
            let preferences_path = data_dir.join("preferences.json");
            let snapshot_cache_path = data_dir.join("snapshot-cache.json");
            let accounts_root = data_dir.join("accounts");
            let account_login_root = accounts_root.join("login-tasks");
            let preferences = load_preferences(&preferences_path);
            let persisted_snapshots = load_persisted_snapshots(&snapshot_cache_path);
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
                persisted_snapshots: Mutex::new(persisted_snapshots),
                snapshot_cache_path,
                account_quota_cache: Mutex::new(HashMap::new()),
                account_generation: AtomicU64::new(0),
                token_usage_cache: Arc::clone(&token_usage_cache),
                palette_generation: AtomicU64::new(0),
                account_vault: Mutex::new(account_vault::AccountVault::load(accounts_root)),
                account_switch_lock: tokio::sync::Mutex::new(()),
                account_login_task: Mutex::new(None),
                account_login_root,
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
            get_cached_snapshots,
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
            close_palette_preview,
            get_account_vault,
            get_account_weekly_quotas,
            save_current_account,
            rename_account,
            delete_account,
            switch_account,
            switch_account_and_restart_codex,
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
                    window_material::sync_window_material(app);
                    let _ = window.set_focus();
                }
            }
        })
        .on_window_event(|window, event| {
            let material_window =
                ["widget", "palette", "palette-editor", "account-switcher"].contains(&window.label());
            let material_geometry_changed = matches!(
                event,
                WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
            );
            if material_window && material_geometry_changed {
                window_material::sync_window_material(window.app_handle());
            }
            if window.label() == "widget" && material_geometry_changed {
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
                    let app_focused = ["widget", "palette", "palette-editor", "account-switcher"].iter().any(|label| {
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
                } else if window.label() == "account-switcher" {
                    let _ = window.app_handle().emit_to("widget", "account-switcher-closed", ());
                } else if window.label() == "widget" {
                    finish_palette_preview(window.app_handle());
                    if let Some(account) = window.app_handle().get_webview_window("account-switcher") {
                        let _ = account.hide();
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build Quota Beacon");
    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Resumed) {
            let _ = app_handle.emit_to("widget", "refresh-requested", "auto");
        }
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                if let Ok(mut slot) = state.account_login_task.lock() {
                    if let Some(mut task) = slot.take() {
                        let _ = task.child.kill();
                        let _ = task.child.wait();
                        cleanup_login_task(state.inner(), &task.task_root);
                    }
                }
            }
            window_material::destroy_window_materials();
        }
    });
}

#[cfg(test)]
mod account_quota_tests {
    use super::*;

    fn successful_snapshot() -> ProviderSnapshot {
        ProviderSnapshot {
            provider: "codex".into(),
            display_name: "CODEX".into(),
            plan: Some("PRO".into()),
            short_window: Some(models::UsageWindow {
                remaining_percent: 74.0,
                resets_at: None,
                window_seconds: 18_000,
            }),
            weekly_window: None,
            reset_credits: None,
            reset_credit_expires_at: Vec::new(),
            updated_at: "2026-08-23T00:00:00Z".into(),
            status: "ok".into(),
            message: None,
        }
    }

    #[test]
    fn persisted_snapshots_are_account_bound_and_loaded_as_stale() {
        let cache = PersistedSnapshotCache {
            schema_version: SNAPSHOT_CACHE_SCHEMA_VERSION,
            account_id: "account-a".into(),
            snapshots: vec![successful_snapshot()],
        };
        let raw = serde_json::to_vec(&cache).unwrap();

        let loaded = decode_persisted_snapshots(&raw, "account-a");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].status, "stale");
        assert_eq!(loaded[0].short_window.as_ref().unwrap().remaining_percent, 74.0);
        assert!(decode_persisted_snapshots(&raw, "account-b").is_empty());
    }

    #[test]
    fn failed_snapshots_are_not_offered_as_startup_cache() {
        assert!(snapshots_for_startup(&[ProviderSnapshot::failure(
            "unavailable",
            "temporary failure",
        )]).is_empty());
    }

    #[test]
    fn inactive_account_quota_cache_expires_at_five_minutes() {
        assert!(inactive_account_quota_is_fresh(
            Instant::now() - Duration::from_secs(299)
        ));
        assert!(!inactive_account_quota_is_fresh(
            Instant::now() - Duration::from_secs(300)
        ));
        assert!(inactive_account_quota_cache_is_current(
            7,
            7,
            Instant::now()
        ));
        assert!(!inactive_account_quota_cache_is_current(
            6,
            7,
            Instant::now()
        ));
    }
}
