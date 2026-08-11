use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::codex::{self, CredentialIdentity};

const INDEX_VERSION: u8 = 1;
const MAX_ALIAS_CHARS: usize = 32;
const MAX_INDEX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub alias: String,
    pub masked_email: Option<String>,
    account_fingerprint: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountVaultState {
    version: u8,
    profiles: Vec<AccountProfile>,
    active_profile_id: Option<String>,
}

impl Default for AccountVaultState {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            profiles: Vec::new(),
            active_profile_id: None,
        }
    }
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
    init_error: Option<String>,
    #[cfg(test)]
    fail_switch_commit: bool,
}

impl AccountVault {
    pub fn load(root: PathBuf) -> Self {
        let auth_path = codex::auth_path().unwrap_or_else(|| root.join("missing-auth.json"));
        Self::load_from_paths(root, auth_path)
    }

    fn load_from_paths(root: PathBuf, auth_path: PathBuf) -> Self {
        let index_path = root.join("index.json");
        let (state, init_error) = if !index_path.exists() {
            (AccountVaultState::default(), None)
        } else {
            match read_regular_file(&index_path, MAX_INDEX_BYTES).and_then(|raw| {
                serde_json::from_slice::<AccountVaultState>(&raw)
                    .map_err(|_| "Account metadata is invalid.".to_string())
            }) {
                Ok(state) if state.version == INDEX_VERSION => (state, None),
                Ok(_) => (
                    AccountVaultState::default(),
                    Some("Account metadata version is unsupported.".into()),
                ),
                Err(error) => (AccountVaultState::default(), Some(error)),
            }
        };
        Self {
            root,
            auth_path,
            state,
            init_error,
            #[cfg(test)]
            fail_switch_commit: false,
        }
    }

    pub fn view(&self) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        let current = self.current_identity().ok();
        let current_fingerprint = current.as_ref().map(fingerprint);
        let active_profile_id = current_fingerprint.as_ref().and_then(|value| {
            self.state
                .profiles
                .iter()
                .find(|profile| &profile.account_fingerprint == value)
                .map(|profile| profile.id.clone())
        });
        let profiles = self
            .state
            .profiles
            .iter()
            .map(|profile| AccountProfileView {
                id: profile.id.clone(),
                alias: profile.alias.clone(),
                masked_email: profile.masked_email.clone(),
                is_active: active_profile_id.as_deref() == Some(profile.id.as_str()),
                credential_status: if is_safe_regular(&self.credential_path(&profile.id)) {
                    "ready"
                } else {
                    "invalid"
                },
            })
            .collect();
        Ok(AccountVaultView {
            profiles,
            active_profile_id: active_profile_id.clone(),
            has_current_login: current.is_some(),
            current_login_saved: active_profile_id.is_some(),
        })
    }

    pub fn save_current(&mut self, alias: &str) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        let alias = validate_alias(alias)?;
        let raw = codex::read_auth_bytes(&self.auth_path).map_err(str::to_string)?;
        let identity = codex::credential_identity(&raw).map_err(str::to_string)?;
        let account_fingerprint = fingerprint(&identity);
        let now = Utc::now().to_rfc3339();
        let profile_id = if let Some(existing) = self
            .state
            .profiles
            .iter_mut()
            .find(|profile| profile.account_fingerprint == account_fingerprint)
        {
            existing.alias = alias;
            existing.masked_email = identity.email.as_deref().map(mask_email);
            existing.updated_at = now;
            existing.id.clone()
        } else {
            let id = Uuid::new_v4().to_string();
            self.state.profiles.push(AccountProfile {
                id: id.clone(),
                alias,
                masked_email: identity.email.as_deref().map(mask_email),
                account_fingerprint,
                created_at: now.clone(),
                updated_at: now,
            });
            id
        };
        self.write_credentials(&profile_id, &raw)?;
        self.state.active_profile_id = Some(profile_id);
        self.persist_state()?;
        self.view()
    }

    pub fn import_credentials(
        &mut self,
        alias: &str,
        raw: &[u8],
    ) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        let alias = validate_alias(alias)?;
        if raw.len() as u64 > codex::MAX_AUTH_BYTES {
            return Err("Codex login data is too large.".into());
        }
        let identity = codex::credential_identity(raw).map_err(str::to_string)?;
        let account_fingerprint = fingerprint(&identity);
        if self
            .state
            .profiles
            .iter()
            .any(|profile| profile.account_fingerprint == account_fingerprint)
        {
            return Err("This account is already saved.".into());
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.write_credentials(&id, raw)?;
        self.state.profiles.push(AccountProfile {
            id,
            alias,
            masked_email: identity.email.as_deref().map(mask_email),
            account_fingerprint,
            created_at: now.clone(),
            updated_at: now,
        });
        self.persist_state()?;
        self.view()
    }

    pub fn replace_credentials(
        &mut self,
        profile_id: &str,
        raw: &[u8],
    ) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        if raw.len() as u64 > codex::MAX_AUTH_BYTES {
            return Err("Codex login data is too large.".into());
        }
        let identity = codex::credential_identity(raw).map_err(str::to_string)?;
        let next_fingerprint = fingerprint(&identity);
        let profile = self
            .state
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Account was not found.".to_string())?;
        if profile.account_fingerprint != next_fingerprint {
            return Err("The new login belongs to a different account.".into());
        }
        profile.masked_email = identity.email.as_deref().map(mask_email);
        profile.updated_at = Utc::now().to_rfc3339();
        self.write_credentials(profile_id, raw)?;
        self.persist_state()?;
        self.view()
    }

    pub fn rename(&mut self, profile_id: &str, alias: &str) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        let alias = validate_alias(alias)?;
        let profile = self
            .state
            .profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Account was not found.".to_string())?;
        profile.alias = alias;
        profile.updated_at = Utc::now().to_rfc3339();
        self.persist_state()?;
        self.view()
    }

    pub fn delete(&mut self, profile_id: &str) -> Result<AccountVaultView, String> {
        self.ensure_ready()?;
        let current = self.view()?;
        if current.active_profile_id.as_deref() == Some(profile_id)
            && self.state.profiles.len() <= 1
        {
            return Err("The only recoverable current account cannot be deleted.".into());
        }
        let index = self
            .state
            .profiles
            .iter()
            .position(|profile| profile.id == profile_id)
            .ok_or_else(|| "Account was not found.".to_string())?;
        self.state.profiles.remove(index);
        if self.state.active_profile_id.as_deref() == Some(profile_id) {
            self.state.active_profile_id = None;
        }
        self.persist_state()?;
        remove_file_if_regular(&self.credential_path(profile_id))?;
        remove_file_if_regular(&self.recovery_path(profile_id))?;
        self.view()
    }

    pub fn switch_to(&mut self, profile_id: &str) -> Result<SwitchOutcome, String> {
        self.ensure_ready()?;
        let target = self
            .state
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "Account was not found.".to_string())?;
        let encrypted =
            read_regular_file(&self.credential_path(profile_id), codex::MAX_AUTH_BYTES * 2)?;
        let target_raw = unprotect(&encrypted)?;
        let target_identity = codex::credential_identity(&target_raw).map_err(str::to_string)?;
        if fingerprint(&target_identity) != target.account_fingerprint {
            return Err("Saved account credentials do not match their metadata.".into());
        }

        self.sync_current_credentials()?;
        let current_raw = codex::read_auth_bytes(&self.auth_path).map_err(str::to_string)?;
        if let Some(active_id) = self.active_profile_for_raw(&current_raw) {
            let encrypted_recovery = protect(&current_raw)?;
            atomic_write(&self.recovery_path(&active_id), &encrypted_recovery)?;
        }

        if let Err(error) = atomic_replace(&self.auth_path, &target_raw) {
            return Err(format!("Account switch failed before replacement: {error}"));
        }
        let verified = codex::read_auth_bytes(&self.auth_path)
            .and_then(|raw| codex::credential_identity(&raw))
            .map(|identity| fingerprint(&identity) == target.account_fingerprint)
            .unwrap_or(false);
        if !verified {
            let restore = atomic_replace(&self.auth_path, &current_raw);
            return Err(match restore {
                Ok(()) => {
                    "Account switch verification failed; the previous login was restored.".into()
                }
                Err(_) => {
                    "Account switch verification failed and automatic recovery also failed.".into()
                }
            });
        }

        let previous_active_profile_id = self.state.active_profile_id.clone();
        self.state.active_profile_id = Some(profile_id.to_string());
        #[cfg(test)]
        let persist_result = if std::mem::take(&mut self.fail_switch_commit) {
            Err("Injected account metadata failure.".to_string())
        } else {
            self.persist_state()
        };
        #[cfg(not(test))]
        let persist_result = self.persist_state();
        if let Err(error) = persist_result {
            self.state.active_profile_id = previous_active_profile_id;
            let restore = atomic_replace(&self.auth_path, &current_raw);
            return Err(match restore {
                Ok(()) => format!(
                    "Account metadata update failed; the previous login was restored: {error}"
                ),
                Err(_) => format!(
                    "Account metadata update failed and automatic recovery also failed: {error}"
                ),
            });
        }
        let profile = self
            .view()?
            .profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Switched account metadata is unavailable.".to_string())?;
        Ok(SwitchOutcome {
            profile,
            credentials_switched: true,
            restart_recommended: true,
        })
    }

    fn sync_current_credentials(&mut self) -> Result<(), String> {
        let raw = codex::read_auth_bytes(&self.auth_path).map_err(str::to_string)?;
        if let Some(profile_id) = self.active_profile_for_raw(&raw) {
            self.write_credentials(&profile_id, &raw)?;
            if let Some(profile) = self
                .state
                .profiles
                .iter_mut()
                .find(|profile| profile.id == profile_id)
            {
                profile.updated_at = Utc::now().to_rfc3339();
            }
            self.persist_state()?;
            return Ok(());
        }
        if self.state.active_profile_id.is_some() {
            return Err("The active Codex login no longer matches the saved account. Save or confirm it before switching.".into());
        }
        Ok(())
    }

    fn active_profile_for_raw(&self, raw: &[u8]) -> Option<String> {
        let identity = codex::credential_identity(raw).ok()?;
        let value = fingerprint(&identity);
        self.state
            .profiles
            .iter()
            .find(|profile| profile.account_fingerprint == value)
            .map(|profile| profile.id.clone())
    }

    fn current_identity(&self) -> Result<CredentialIdentity, String> {
        let raw = codex::read_auth_bytes(&self.auth_path).map_err(str::to_string)?;
        codex::credential_identity(&raw).map_err(str::to_string)
    }

    fn write_credentials(&self, profile_id: &str, raw: &[u8]) -> Result<(), String> {
        let encrypted = protect(raw)?;
        atomic_write(&self.credential_path(profile_id), &encrypted)
    }

    fn persist_state(&self) -> Result<(), String> {
        let raw = serde_json::to_vec_pretty(&self.state)
            .map_err(|_| "Account metadata could not be serialized.".to_string())?;
        atomic_write(&self.root.join("index.json"), &raw)
    }

    fn credential_path(&self, profile_id: &str) -> PathBuf {
        self.root.join(format!("{profile_id}.dpapi"))
    }

    fn recovery_path(&self, profile_id: &str) -> PathBuf {
        self.root
            .join("recovery")
            .join(format!("{profile_id}.dpapi"))
    }

    fn ensure_ready(&self) -> Result<(), String> {
        match &self.init_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

fn validate_alias(alias: &str) -> Result<String, String> {
    let alias = alias.trim();
    if alias.is_empty()
        || alias.chars().count() > MAX_ALIAS_CHARS
        || alias.chars().any(char::is_control)
    {
        return Err(format!(
            "Account name must contain 1-{MAX_ALIAS_CHARS} visible characters."
        ));
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
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "Account data is unavailable.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err("Account data failed the file safety check.".into());
    }
    fs::read(path).map_err(|_| "Account data could not be read.".into())
}

fn is_safe_regular(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
        .unwrap_or(false)
}

fn remove_file_if_regular(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "Account data could not be inspected.".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Refusing to remove unsafe account data path.".into());
    }
    fs::remove_file(path).map_err(|_| "Account data could not be removed.".into())
}

fn atomic_write(path: &Path, raw: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Account data path is invalid.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Account data directory could not be created.".to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("account"),
        Uuid::new_v4()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "Temporary account data could not be created.".to_string())?;
    if let Err(error) = file.write_all(raw).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Temporary account data could not be written: {error}"
        ));
    }
    drop(file);
    let result = if path.exists() {
        replace_existing(path, &temporary)
    } else {
        fs::rename(&temporary, path)
    };
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Account data could not be committed: {error}"));
    }
    Ok(())
}

fn atomic_replace(path: &Path, raw: &[u8]) -> Result<(), String> {
    atomic_write(path, raw)
}

#[cfg(windows)]
fn replace_existing(path: &Path, replacement: &Path) -> std::io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let replacement: Vec<u16> = replacement
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        ReplaceFileW(
            path.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            ptr::null(),
            ptr::null(),
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_existing(path: &Path, replacement: &Path) -> std::io::Result<()> {
    fs::remove_file(path)?;
    fs::rename(replacement, path)
}

#[cfg(windows)]
fn protect(raw: &[u8]) -> Result<Vec<u8>, String> {
    use std::{ptr, slice};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: raw
            .len()
            .try_into()
            .map_err(|_| "Account data is too large.".to_string())?,
        pbData: raw.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows could not encrypt the account credentials.".into());
    }
    let encrypted =
        unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as _) };
    Ok(encrypted)
}

#[cfg(windows)]
fn unprotect(raw: &[u8]) -> Result<Vec<u8>, String> {
    use std::{ptr, slice};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: raw
            .len()
            .try_into()
            .map_err(|_| "Encrypted account data is too large.".to_string())?,
        pbData: raw.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows could not decrypt the saved account.".into());
    }
    let decrypted =
        unsafe { slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as _) };
    Ok(decrypted)
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("Secure account storage is currently available on Windows only.".into())
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("Secure account storage is currently available on Windows only.".into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    fn fixture(account_id: &str, email: &str, marker: &str) -> Vec<u8> {
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "https://api.openai.com/auth.chatgpt_account_id": account_id,
                "email": email,
            })
            .to_string(),
        );
        serde_json::to_vec(&serde_json::json!({
            "tokens": {
                "access_token": format!("header.{payload}.{marker}"),
                "id_token": format!("header.{payload}.{marker}"),
                "refresh_token": format!("fictional-refresh-{marker}")
            }
        }))
        .unwrap()
    }

    fn test_vault() -> (PathBuf, PathBuf, AccountVault) {
        let root = std::env::temp_dir().join(format!("quota-beacon-vault-test-{}", Uuid::new_v4()));
        let auth = root.join("codex-home").join("auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        let vault = AccountVault::load_from_paths(root.join("accounts"), auth.clone());
        (root, auth, vault)
    }

    #[test]
    fn dpapi_ciphertext_does_not_contain_plaintext_token() {
        let raw = fixture("account-a", "alice@example.com", "secret-marker");
        let encrypted = protect(&raw).unwrap();
        assert!(!String::from_utf8_lossy(&encrypted).contains("secret-marker"));
        assert_eq!(unprotect(&encrypted).unwrap(), raw);
    }

    #[test]
    fn saves_detects_duplicate_and_switches_accounts() {
        let (root, auth, mut vault) = test_vault();
        let account_a = fixture("account-a", "alice@example.com", "a");
        let account_b = fixture("account-b", "bob@example.com", "b");
        atomic_write(&auth, &account_a).unwrap();
        let view_a = vault.save_current("Personal").unwrap();
        let a_id = view_a.active_profile_id.unwrap();
        atomic_replace(&auth, &account_b).unwrap();
        let view_b = vault.save_current("Work").unwrap();
        assert_eq!(view_b.profiles.len(), 2);
        vault.save_current("Work renamed").unwrap();
        assert_eq!(vault.view().unwrap().profiles.len(), 2);
        let outcome = vault.switch_to(&a_id).unwrap();
        assert!(outcome.credentials_switched);
        let restored = codex::credential_identity(&codex::read_auth_bytes(&auth).unwrap()).unwrap();
        assert_eq!(restored.account_id, "account-a");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_invalid_and_oversized_auth_files() {
        let (root, auth, mut vault) = test_vault();
        atomic_write(&auth, br#"{"tokens":{"access_token":"not-a-jwt"}}"#).unwrap();
        assert!(vault.save_current("Invalid").is_err());
        let oversized = vec![b'x'; codex::MAX_AUTH_BYTES as usize + 1];
        atomic_replace(&auth, &oversized).unwrap();
        assert!(vault.save_current("Oversized").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_delete_the_only_active_recovery() {
        let (root, auth, mut vault) = test_vault();
        atomic_write(&auth, &fixture("account-a", "alice@example.com", "a")).unwrap();
        let view = vault.save_current("Personal").unwrap();
        assert!(vault
            .delete(view.active_profile_id.as_deref().unwrap())
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restores_previous_login_when_switch_metadata_commit_fails() {
        let (root, auth, mut vault) = test_vault();
        let account_a = fixture("account-a", "alice@example.com", "a");
        let account_b = fixture("account-b", "bob@example.com", "b");
        atomic_write(&auth, &account_a).unwrap();
        let a_id = vault
            .save_current("Personal")
            .unwrap()
            .active_profile_id
            .unwrap();
        atomic_replace(&auth, &account_b).unwrap();
        let b_id = vault
            .save_current("Work")
            .unwrap()
            .active_profile_id
            .unwrap();

        vault.fail_switch_commit = true;
        let error = vault.switch_to(&a_id).unwrap_err();
        assert!(error.contains("previous login was restored"));
        let restored = codex::credential_identity(&codex::read_auth_bytes(&auth).unwrap()).unwrap();
        assert_eq!(restored.account_id, "account-b");
        assert_eq!(
            vault.view().unwrap().active_profile_id.as_deref(),
            Some(b_id.as_str())
        );

        let reloaded = AccountVault::load_from_paths(root.join("accounts"), auth.clone());
        assert_eq!(
            reloaded.view().unwrap().active_profile_id.as_deref(),
            Some(b_id.as_str())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
