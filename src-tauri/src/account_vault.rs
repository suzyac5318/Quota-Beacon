use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::codex::{self, CredentialIdentity};

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "app.quotabeacon.desktop.accounts";
const STATE_FILE: &str = "vault.json";

#[derive(Clone, Debug, PartialEq, Eq)]
enum SecretError {
    Unavailable,
}

trait SecretStore: Send + Sync {
    fn set(&self, account: &str, secret: &[u8]) -> Result<(), SecretError>;
    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError>;
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

#[cfg(target_os = "macos")]
struct PlatformSecretStore;

#[cfg(target_os = "macos")]
impl SecretStore for PlatformSecretStore {
    fn set(&self, account: &str, secret: &[u8]) -> Result<(), SecretError> {
        security_framework::passwords::set_generic_password(KEYCHAIN_SERVICE, account, secret)
            .map_err(|_| SecretError::Unavailable)
    }

    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account)
            .map_err(|_| SecretError::Unavailable)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account)
            .map_err(|_| SecretError::Unavailable)
    }
}

#[cfg(not(target_os = "macos"))]
struct PlatformSecretStore;

#[cfg(not(target_os = "macos"))]
impl SecretStore for PlatformSecretStore {
    fn set(&self, _: &str, _: &[u8]) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }

    fn get(&self, _: &str) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::Unavailable)
    }

    fn delete(&self, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountProfile {
    id: String,
    alias: String,
    masked_email: Option<String>,
    fingerprint: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountVaultState {
    profiles: Vec<AccountProfile>,
    active_profile_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfileView {
    pub id: String,
    pub alias: String,
    pub masked_email: Option<String>,
    pub is_active: bool,
    pub credential_status: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountVaultView {
    pub profiles: Vec<AccountProfileView>,
    pub active_profile_id: Option<String>,
    pub has_current_login: bool,
    pub current_login_saved: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    pub profile: AccountProfileView,
    pub credentials_switched: bool,
    pub restart_recommended: bool,
}

pub struct AccountVault {
    root: PathBuf,
    auth_path: PathBuf,
    state: AccountVaultState,
    state_error: Option<String>,
    secrets: Arc<dyn SecretStore>,
}

impl AccountVault {
    pub fn load(root: PathBuf) -> Self {
        let auth_path = codex::auth_path().unwrap_or_else(|| root.join("missing-auth.json"));
        Self::load_with_store(root, auth_path, Arc::new(PlatformSecretStore))
    }

    fn load_with_store(root: PathBuf, auth_path: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        let state_path = root.join(STATE_FILE);
        let (state, state_error) = load_state_with_recovery(&state_path);
        Self {
            root,
            auth_path,
            state,
            state_error,
            secrets,
        }
    }

    fn ensure_state_ready(&self) -> Result<(), String> {
        match &self.state_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub fn view(&self) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        let current = self.current_identity().ok();
        let current_fingerprint = current.as_ref().map(fingerprint);
        let profiles = self
            .state
            .profiles
            .iter()
            .map(|profile| self.profile_view(profile))
            .collect();
        Ok(AccountVaultView {
            profiles,
            active_profile_id: self.state.active_profile_id.clone(),
            has_current_login: current.is_some(),
            current_login_saved: current_fingerprint.is_some_and(|value| {
                self.state
                    .profiles
                    .iter()
                    .any(|profile| profile.fingerprint == value)
            }),
        })
    }

    pub fn save_current(&mut self, alias: &str) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        let alias = validate_alias(alias)?;
        let raw = read_regular_file(&self.auth_path, codex::MAX_AUTH_BYTES)
            .map_err(|_| "Current Codex login is unavailable.".to_string())?;
        let identity = codex::credential_identity(&raw).map_err(str::to_string)?;
        let identity_fingerprint = fingerprint(&identity);
        let previous_state = self.state.clone();
        let mut previous_secret = None;
        let touched_id;
        if let Some(index) = self
            .state
            .profiles
            .iter()
            .position(|profile| profile.fingerprint == identity_fingerprint)
        {
            let id = self.state.profiles[index].id.clone();
            previous_secret = self.secrets.get(&id).ok();
            self.secrets
                .set(&id, &raw)
                .map_err(|_| "macOS Keychain could not save this account.".to_string())?;
            self.state.profiles[index].alias = alias;
            self.state.profiles[index].masked_email = identity.email.as_deref().map(mask_email);
            self.state.active_profile_id = Some(id);
            touched_id = self.state.profiles[index].id.clone();
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            self.secrets
                .set(&id, &raw)
                .map_err(|_| "macOS Keychain could not save this account.".to_string())?;
            self.state.profiles.push(AccountProfile {
                id: id.clone(),
                alias,
                masked_email: identity.email.as_deref().map(mask_email),
                fingerprint: identity_fingerprint,
            });
            self.state.active_profile_id = Some(id);
            touched_id = self
                .state
                .active_profile_id
                .clone()
                .expect("new account has active id");
        }
        if let Err(error) = self.persist_state() {
            self.state = previous_state;
            let restored = if let Some(secret) = previous_secret {
                self.secrets.set(&touched_id, &secret).is_ok()
            } else {
                self.secrets.delete(&touched_id).is_ok()
            };
            if !restored {
                return Err(format!(
                    "{error} Keychain rollback also failed; account recovery requires attention."
                ));
            }
            return Err(error);
        }
        self.view()
    }

    pub fn import_credentials(
        &mut self,
        alias: &str,
        raw: &[u8],
    ) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        let alias = validate_alias(alias)?;
        if raw.len() as u64 > codex::MAX_AUTH_BYTES {
            return Err("Codex login data is too large.".into());
        }
        let identity = codex::credential_identity(raw).map_err(str::to_string)?;
        let identity_fingerprint = fingerprint(&identity);
        if self
            .state
            .profiles
            .iter()
            .any(|profile| profile.fingerprint == identity_fingerprint)
        {
            return Err("This Codex account is already saved.".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.secrets
            .set(&id, raw)
            .map_err(|_| "macOS Keychain could not save this account.".to_string())?;
        self.state.profiles.push(AccountProfile {
            id,
            alias,
            masked_email: identity.email.as_deref().map(mask_email),
            fingerprint: identity_fingerprint,
        });
        if let Err(error) = self.persist_state() {
            if let Some(profile) = self.state.profiles.pop() {
                if self.secrets.delete(&profile.id).is_err() {
                    return Err(format!(
                        "{error} Keychain cleanup also failed; account recovery requires attention."
                    ));
                }
            }
            return Err(error);
        }
        self.view()
    }

    pub fn replace_credentials(
        &mut self,
        profile_id: &str,
        raw: &[u8],
    ) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        if raw.len() as u64 > codex::MAX_AUTH_BYTES {
            return Err("Codex login data is too large.".into());
        }
        let identity = codex::credential_identity(raw).map_err(str::to_string)?;
        let identity_fingerprint = fingerprint(&identity);
        if self
            .state
            .profiles
            .iter()
            .any(|profile| profile.id != profile_id && profile.fingerprint == identity_fingerprint)
        {
            return Err("This Codex account is already saved.".into());
        }
        let previous_state = self.state.clone();
        let previous_secret = self.secrets.get(profile_id).ok();
        let profile = self
            .state
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Saved account was not found.".to_string())?;
        self.secrets
            .set(profile_id, raw)
            .map_err(|_| "macOS Keychain could not update this account.".to_string())?;
        profile.masked_email = identity.email.as_deref().map(mask_email);
        profile.fingerprint = identity_fingerprint;
        if let Err(error) = self.persist_state() {
            self.state = previous_state;
            let restored = if let Some(secret) = previous_secret {
                self.secrets.set(profile_id, &secret).is_ok()
            } else {
                self.secrets.delete(profile_id).is_ok()
            };
            if !restored {
                return Err(format!(
                    "{error} Keychain rollback also failed; account recovery requires attention."
                ));
            }
            return Err(error);
        }
        self.view()
    }

    pub fn rename(&mut self, profile_id: &str, alias: &str) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        let alias = validate_alias(alias)?;
        let previous_state = self.state.clone();
        let profile = self
            .state
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Saved account was not found.".to_string())?;
        profile.alias = alias;
        if let Err(error) = self.persist_state() {
            self.state = previous_state;
            return Err(error);
        }
        self.view()
    }

    pub fn delete(&mut self, profile_id: &str) -> Result<AccountVaultView, String> {
        self.ensure_state_ready()?;
        if self.state.active_profile_id.as_deref() == Some(profile_id) {
            return Err(
                "Switch to another saved account before deleting the active account.".into(),
            );
        }
        let index = self
            .state
            .profiles
            .iter()
            .position(|profile| profile.id == profile_id)
            .ok_or_else(|| "Saved account was not found.".to_string())?;
        let old_secret = self
            .secrets
            .get(profile_id)
            .map_err(|_| "macOS Keychain could not open this account.".to_string())?;
        self.secrets
            .delete(profile_id)
            .map_err(|_| "macOS Keychain could not delete this account.".to_string())?;
        let profile = self.state.profiles.remove(index);
        if let Err(error) = self.persist_state() {
            self.state.profiles.insert(index, profile);
            if self.secrets.set(profile_id, &old_secret).is_err() {
                return Err(format!(
                    "{error} Keychain rollback also failed; account recovery requires attention."
                ));
            }
            return Err(error);
        }
        self.view()
    }

    pub fn switch_to(&mut self, profile_id: &str) -> Result<SwitchOutcome, String> {
        self.ensure_state_ready()?;
        self.sync_current_credentials()?;
        let target = self
            .state
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "Saved account was not found.".to_string())?;
        let raw = self
            .secrets
            .get(profile_id)
            .map_err(|_| "Saved credentials are unavailable. Sign in again.".to_string())?;
        let identity = codex::credential_identity(&raw).map_err(|_| {
            "Saved credentials are invalid. Sign in again before switching.".to_string()
        })?;
        if fingerprint(&identity) != target.fingerprint {
            return Err("Saved account identity does not match its metadata.".into());
        }
        let previous_raw = read_regular_file(&self.auth_path, codex::MAX_AUTH_BYTES).ok();
        let previous_state = self.state.clone();
        atomic_replace(&self.auth_path, &raw)?;
        let committed = read_regular_file(&self.auth_path, codex::MAX_AUTH_BYTES)
            .ok()
            .and_then(|bytes| codex::credential_identity(&bytes).ok())
            .is_some_and(|value| fingerprint(&value) == target.fingerprint);
        if !committed {
            if !restore_login(&self.auth_path, previous_raw.as_deref()) {
                return Err("Credential switch verification failed and the previous login could not be restored; account recovery requires attention.".into());
            }
            return Err(
                "Credential switch verification failed; the previous login was restored.".into(),
            );
        }
        self.state.active_profile_id = Some(profile_id.to_string());
        if let Err(error) = self.persist_state() {
            self.state = previous_state;
            if !restore_login(&self.auth_path, previous_raw.as_deref()) {
                return Err(format!(
                    "{error} The previous login could not be restored; account recovery requires attention."
                ));
            }
            return Err(format!("{error} The previous login was restored."));
        }
        Ok(SwitchOutcome {
            profile: self.profile_view(&target),
            credentials_switched: true,
            restart_recommended: false,
        })
    }

    pub fn read_profile_credentials(&self, profile_id: &str) -> Result<Vec<u8>, String> {
        self.ensure_state_ready()?;
        let profile = self
            .state
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Saved account was not found.".to_string())?;
        let raw = self
            .secrets
            .get(profile_id)
            .map_err(|_| "Saved credentials are unavailable. Sign in again.".to_string())?;
        if raw.len() as u64 > codex::MAX_AUTH_BYTES {
            return Err("Saved credentials are too large. Sign in again.".into());
        }
        let identity = codex::credential_identity(&raw)
            .map_err(|_| "Saved credentials are invalid. Sign in again.".to_string())?;
        if fingerprint(&identity) != profile.fingerprint {
            return Err("Saved account identity does not match its metadata.".into());
        }
        Ok(raw)
    }

    fn profile_view(&self, profile: &AccountProfile) -> AccountProfileView {
        let credential_status = match self.secrets.get(&profile.id) {
            Ok(raw)
                if codex::credential_identity(&raw)
                    .is_ok_and(|identity| fingerprint(&identity) == profile.fingerprint) =>
            {
                "ready"
            }
            _ => "invalid",
        };
        AccountProfileView {
            id: profile.id.clone(),
            alias: profile.alias.clone(),
            masked_email: profile.masked_email.clone(),
            is_active: self.state.active_profile_id.as_deref() == Some(&profile.id),
            credential_status,
        }
    }

    fn current_identity(&self) -> Result<CredentialIdentity, String> {
        let raw = read_regular_file(&self.auth_path, codex::MAX_AUTH_BYTES)
            .map_err(|_| "Current Codex login is unavailable.".to_string())?;
        codex::credential_identity(&raw).map_err(str::to_string)
    }

    fn sync_current_credentials(&mut self) -> Result<(), String> {
        let raw = match read_regular_file(&self.auth_path, codex::MAX_AUTH_BYTES) {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };
        let identity = match codex::credential_identity(&raw) {
            Ok(value) => value,
            Err(_) => return Ok(()),
        };
        let identity_fingerprint = fingerprint(&identity);
        if let Some(profile) = self
            .state
            .profiles
            .iter()
            .find(|profile| profile.fingerprint == identity_fingerprint)
        {
            self.secrets.set(&profile.id, &raw).map_err(|_| {
                "macOS Keychain could not synchronize the active account.".to_string()
            })?;
            self.state.active_profile_id = Some(profile.id.clone());
        }
        Ok(())
    }

    fn persist_state(&self) -> Result<(), String> {
        if let Some(error) = &self.state_error {
            return Err(error.clone());
        }
        validate_state(&self.state)?;
        let raw = serde_json::to_vec_pretty(&self.state)
            .map_err(|_| "Account metadata could not be encoded.".to_string())?;
        atomic_write(&self.root.join(STATE_FILE), &raw)
    }
}

fn validate_state(state: &AccountVaultState) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    let mut fingerprints = std::collections::HashSet::new();
    for profile in &state.profiles {
        if !ids.insert(profile.id.as_str()) {
            return Err("Account metadata contains duplicate profile IDs.".into());
        }
        if !fingerprints.insert(profile.fingerprint.as_str()) {
            return Err("Account metadata contains duplicate account identities.".into());
        }
    }
    if state
        .active_profile_id
        .as_ref()
        .is_some_and(|active| !ids.contains(active.as_str()))
    {
        return Err("Account metadata points to a missing active profile.".into());
    }
    Ok(())
}

fn decode_state(raw: &[u8]) -> Result<AccountVaultState, String> {
    let state: AccountVaultState =
        serde_json::from_slice(raw).map_err(|_| "Account metadata is damaged.".to_string())?;
    validate_state(&state)?;
    Ok(state)
}

fn load_state_with_recovery(path: &Path) -> (AccountVaultState, Option<String>) {
    let backup = path.with_extension("bak");
    let primary_exists = path.exists();
    if primary_exists {
        if let Ok(raw) = read_regular_file(path, codex::MAX_AUTH_BYTES) {
            if let Ok(state) = decode_state(&raw) {
                return (state, None);
            }
        }
    }

    let backup_exists = backup.exists();
    if backup_exists {
        if let Ok(raw) = read_regular_file(&backup, codex::MAX_AUTH_BYTES) {
            if let Ok(state) = decode_state(&raw) {
                match write_atomic_file(path, &raw, FileKind::Metadata) {
                    Ok(()) => {
                        eprintln!("account metadata recovered from the local backup");
                        return (state, None);
                    }
                    Err(_) => {
                        return (
                            state,
                            Some(
                                "Account metadata backup is valid but could not be restored."
                                    .into(),
                            ),
                        );
                    }
                }
            }
        }
    }

    if primary_exists || backup_exists {
        return (
            AccountVaultState::default(),
            Some("Account metadata is damaged and no valid backup is available.".into()),
        );
    }
    (AccountVaultState::default(), None)
}

fn validate_alias(alias: &str) -> Result<String, String> {
    let alias = alias.trim();
    if alias.is_empty() || alias.chars().count() > 32 || alias.chars().any(char::is_control) {
        return Err("Account name must contain 1-32 visible characters.".into());
    }
    Ok(alias.to_string())
}

fn fingerprint(identity: &CredentialIdentity) -> String {
    let digest = Sha256::digest(identity.account_id.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn mask_email(email: &str) -> String {
    let Some((local, domain)) = email.split_once('@') else {
        return "***".into();
    };
    let first = local.chars().next().unwrap_or('*');
    format!("{first}***@{domain}")
}

fn read_regular_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "File is unavailable.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err("File is not a safe regular file.".into());
    }
    fs::read(path).map_err(|_| "File could not be read.".into())
}

#[derive(Clone, Copy)]
enum FileKind {
    Metadata,
    Credentials,
}

fn unique_temporary_path(parent: &Path, label: &str) -> PathBuf {
    parent.join(format!(".{label}.quotabeacon.{}.tmp", uuid::Uuid::new_v4()))
}

fn write_atomic_file(path: &Path, raw: &[u8], kind: FileKind) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Storage path is invalid.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Account storage directory could not be created.".to_string())?;
    let label = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let temporary = unique_temporary_path(parent, label);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Temporary account metadata could not be created.".to_string())?;
    if file.write_all(raw).and_then(|_| file.sync_all()).is_err() {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(match kind {
            FileKind::Metadata => "Account metadata could not be written.".into(),
            FileKind::Credentials => "Temporary Codex login could not be written.".into(),
        });
    }
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("Temporary file permissions could not be secured.".into());
        }
    }
    if let Err(error) = replace_file(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn replace_file(temporary: &Path, path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        fs::rename(temporary, path).map_err(|error| format!("Atomic file commit failed: {error}"))
    }
    #[cfg(not(unix))]
    {
        let backup = unique_temporary_path(
            path.parent()
                .ok_or_else(|| "Storage path is invalid.".to_string())?,
            "replace-backup",
        );
        if path.exists() {
            fs::rename(path, &backup)
                .map_err(|error| format!("File replacement backup failed: {error}"))?;
        }
        if let Err(error) = fs::rename(temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(format!("Atomic file commit failed: {error}"));
        }
        if backup.exists() {
            let _ = fs::remove_file(backup);
        }
        Ok(())
    }
}

fn atomic_write(path: &Path, raw: &[u8]) -> Result<(), String> {
    let backup = path.with_extension("bak");
    if path.exists() {
        let current = read_regular_file(path, codex::MAX_AUTH_BYTES)
            .map_err(|_| "Existing account metadata is unsafe or unreadable.".to_string())?;
        write_atomic_file(&backup, &current, FileKind::Metadata)
            .map_err(|_| "Account metadata backup failed.".to_string())?;
    }
    write_atomic_file(path, raw, FileKind::Metadata)
}

fn atomic_replace(path: &Path, raw: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Codex login path is invalid.".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "Codex login directory is unavailable.".to_string())?;
    if path.exists() {
        let metadata =
            fs::symlink_metadata(path).map_err(|_| "Codex login is unavailable.".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("Codex login path is not a safe regular file.".into());
        }
    }
    write_atomic_file(path, raw, FileKind::Credentials)
}

fn restore_login(path: &Path, previous: Option<&[u8]>) -> bool {
    match previous {
        Some(raw) => {
            if atomic_replace(path, raw).is_err() {
                return false;
            }
            read_regular_file(path, codex::MAX_AUTH_BYTES).is_ok_and(|value| value == raw)
        }
        None => {
            if !path.exists() {
                return true;
            }
            fs::symlink_metadata(path)
                .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
                && fs::remove_file(path).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    struct MemorySecrets(Mutex<HashMap<String, Vec<u8>>>);

    impl SecretStore for MemorySecrets {
        fn set(&self, account: &str, secret: &[u8]) -> Result<(), SecretError> {
            self.0.lock().unwrap().insert(account.into(), secret.into());
            Ok(())
        }
        fn get(&self, account: &str) -> Result<Vec<u8>, SecretError> {
            self.0
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(SecretError::Unavailable)
        }
        fn delete(&self, account: &str) -> Result<(), SecretError> {
            self.0
                .lock()
                .unwrap()
                .remove(account)
                .map(|_| ())
                .ok_or(SecretError::Unavailable)
        }
    }

    fn fixture(account_id: &str, email: &str, marker: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "tokens": { "access_token": marker, "account_id": account_id, "email": email }
        }))
        .unwrap()
    }

    fn test_vault() -> (PathBuf, PathBuf, Arc<MemorySecrets>, AccountVault) {
        let root =
            std::env::temp_dir().join(format!("quota-beacon-mac-vault-{}", uuid::Uuid::new_v4()));
        let auth = root.join("codex-home").join("auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        let secrets = Arc::new(MemorySecrets::default());
        let vault = AccountVault::load_with_store(root.clone(), auth.clone(), secrets.clone());
        (root, auth, secrets, vault)
    }

    fn state_fixture(active_profile_id: Option<&str>) -> AccountVaultState {
        AccountVaultState {
            profiles: vec![AccountProfile {
                id: "profile-a".into(),
                alias: "A".into(),
                masked_email: Some("a***@example.com".into()),
                fingerprint: "fingerprint-a".into(),
            }],
            active_profile_id: active_profile_id.map(str::to_string),
        }
    }

    fn write_state(path: &Path, state: &AccountVaultState) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(state).unwrap()).unwrap();
    }

    #[test]
    fn loads_valid_primary_metadata_without_rewriting_it() {
        let (root, auth, secrets, _) = test_vault();
        let state_path = root.join(STATE_FILE);
        let expected = state_fixture(Some("profile-a"));
        write_state(&state_path, &expected);
        let before = fs::read(&state_path).unwrap();

        let vault = AccountVault::load_with_store(root.clone(), auth, secrets);

        assert_eq!(vault.state.profiles[0].id, "profile-a");
        assert!(vault.state_error.is_none());
        assert_eq!(fs::read(&state_path).unwrap(), before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restores_valid_backup_when_primary_is_missing_or_damaged() {
        for damaged_primary in [false, true] {
            let (root, auth, secrets, _) = test_vault();
            let state_path = root.join(STATE_FILE);
            let backup_path = state_path.with_extension("bak");
            let expected = state_fixture(Some("profile-a"));
            write_state(&backup_path, &expected);
            if damaged_primary {
                fs::write(&state_path, b"damaged metadata").unwrap();
            }

            let vault = AccountVault::load_with_store(root.clone(), auth, secrets);

            assert!(vault.state_error.is_none());
            assert_eq!(vault.state.profiles[0].id, "profile-a");
            assert_eq!(
                decode_state(&fs::read(&state_path).unwrap())
                    .unwrap()
                    .profiles
                    .len(),
                1
            );
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn damaged_primary_and_backup_are_preserved_and_block_mutations() {
        let (root, auth, secrets, _) = test_vault();
        let state_path = root.join(STATE_FILE);
        let backup_path = state_path.with_extension("bak");
        fs::write(&state_path, b"damaged primary").unwrap();
        fs::write(&backup_path, b"damaged backup").unwrap();
        let primary_before = fs::read(&state_path).unwrap();
        let backup_before = fs::read(&backup_path).unwrap();

        let mut vault = AccountVault::load_with_store(root.clone(), auth, secrets);

        assert!(vault.view().is_err());
        assert!(vault.rename("missing", "new name").is_err());
        assert_eq!(fs::read(&state_path).unwrap(), primary_before);
        assert_eq!(fs::read(&backup_path).unwrap(), backup_before);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_duplicate_ids_fingerprints_and_dangling_active_profile() {
        let valid = state_fixture(Some("profile-a"));
        let mut duplicate_id = valid.clone();
        duplicate_id.profiles.push(AccountProfile {
            id: "profile-a".into(),
            alias: "B".into(),
            masked_email: None,
            fingerprint: "fingerprint-b".into(),
        });
        assert!(validate_state(&duplicate_id).is_err());

        let mut duplicate_fingerprint = valid.clone();
        duplicate_fingerprint.profiles.push(AccountProfile {
            id: "profile-b".into(),
            alias: "B".into(),
            masked_email: None,
            fingerprint: "fingerprint-a".into(),
        });
        assert!(validate_state(&duplicate_fingerprint).is_err());

        let mut dangling = valid;
        dangling.active_profile_id = Some("missing".into());
        assert!(validate_state(&dangling).is_err());
    }

    #[test]
    fn unique_temporary_files_do_not_overwrite_preexisting_paths() {
        let (root, _, _, _) = test_vault();
        let target = root.join("atomic.json");
        let reserved = unique_temporary_path(&root, "atomic.json");
        fs::write(&reserved, b"reserved").unwrap();

        write_atomic_file(&target, b"new value", FileKind::Metadata).unwrap();

        assert_eq!(fs::read(&reserved).unwrap(), b"reserved");
        assert_eq!(fs::read(&target).unwrap(), b"new value");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn credential_and_metadata_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (root, auth, _, _) = test_vault();
        let state_path = root.join(STATE_FILE);
        atomic_write(&state_path, b"{}").unwrap();
        atomic_replace(&auth, b"secret").unwrap();

        assert_eq!(
            fs::metadata(&state_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&auth).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_never_contains_credentials_and_duplicate_accounts_are_rejected() {
        let (root, auth, _, mut vault) = test_vault();
        let personal = fixture("acct-personal", "person@example.com", "secret-marker");
        fs::write(&auth, &personal).unwrap();
        vault.save_current("个人").unwrap();
        let metadata = fs::read_to_string(root.join(STATE_FILE)).unwrap();
        assert!(!metadata.contains("secret-marker"));
        assert!(!metadata.contains("acct-personal"));
        assert!(vault.import_credentials("重复", &personal).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn saves_switches_and_protects_the_active_account() {
        let (root, auth, _, mut vault) = test_vault();
        fs::write(&auth, fixture("acct-a", "a@example.com", "token-a")).unwrap();
        let first = vault.save_current("A").unwrap();
        let first_id = first.active_profile_id.unwrap();
        let second = fixture("acct-b", "b@example.com", "token-b");
        let view = vault.import_credentials("B", &second).unwrap();
        let second_id = view
            .profiles
            .iter()
            .find(|item| item.alias == "B")
            .unwrap()
            .id
            .clone();
        assert!(vault.delete(&first_id).is_err());
        let outcome = vault.switch_to(&second_id).unwrap();
        assert_eq!(outcome.profile.alias, "B");
        assert_eq!(fs::read(&auth).unwrap(), second);
        assert!(vault.delete(&first_id).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_one_profile_from_the_secret_store_without_switching_auth() {
        let (root, auth, _, mut vault) = test_vault();
        let current = fixture("acct-a", "a@example.com", "token-a");
        let inactive = fixture("acct-b", "b@example.com", "token-b");
        fs::write(&auth, &current).unwrap();
        vault.save_current("A").unwrap();
        let view = vault.import_credentials("B", &inactive).unwrap();
        let inactive_id = view
            .profiles
            .iter()
            .find(|item| item.alias == "B")
            .unwrap()
            .id
            .clone();

        assert_eq!(
            vault.read_profile_credentials(&inactive_id).unwrap(),
            inactive
        );
        assert_eq!(fs::read(&auth).unwrap(), current);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_invalid_oversized_and_symlink_like_inputs() {
        let (root, _, _, mut vault) = test_vault();
        assert!(vault.import_credentials("bad", b"not-json").is_err());
        let oversized = vec![b'x'; codex::MAX_AUTH_BYTES as usize + 1];
        assert!(vault.import_credentials("big", &oversized).is_err());
        assert!(validate_alias("\n").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restores_previous_login_when_metadata_commit_fails() {
        let (root, auth, _, mut vault) = test_vault();
        let first = fixture("acct-a", "a@example.com", "token-a");
        fs::write(&auth, &first).unwrap();
        vault.save_current("A").unwrap();
        let second = fixture("acct-b", "b@example.com", "token-b");
        let view = vault.import_credentials("B", &second).unwrap();
        let second_id = view
            .profiles
            .iter()
            .find(|item| item.alias == "B")
            .unwrap()
            .id
            .clone();
        let blocked_root = root.join("blocked-root");
        fs::write(&blocked_root, b"not a directory").unwrap();
        vault.root = blocked_root;
        assert!(vault.switch_to(&second_id).is_err());
        assert_eq!(fs::read(&auth).unwrap(), first);
        assert_ne!(
            vault.state.active_profile_id.as_deref(),
            Some(second_id.as_str())
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_auth_files() {
        use std::os::unix::fs::symlink;

        let (root, auth, _, _) = test_vault();
        let target = root.join("real-auth.json");
        fs::write(&target, fixture("acct-a", "a@example.com", "token-a")).unwrap();
        let _ = fs::remove_file(&auth);
        symlink(&target, &auth).unwrap();
        assert!(read_regular_file(&auth, codex::MAX_AUTH_BYTES).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
