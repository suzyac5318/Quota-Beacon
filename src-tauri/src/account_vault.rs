use std::{
    fs,
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
    secrets: Arc<dyn SecretStore>,
}

impl AccountVault {
    pub fn load(root: PathBuf) -> Self {
        let auth_path = codex::auth_path().unwrap_or_else(|| root.join("missing-auth.json"));
        Self::load_with_store(root, auth_path, Arc::new(PlatformSecretStore))
    }

    fn load_with_store(root: PathBuf, auth_path: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        let state_path = root.join(STATE_FILE);
        let state = read_regular_file(&state_path, codex::MAX_AUTH_BYTES)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self {
            root,
            auth_path,
            state,
            secrets,
        }
    }

    pub fn view(&self) -> Result<AccountVaultView, String> {
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
            if let Some(secret) = previous_secret {
                let _ = self.secrets.set(&touched_id, &secret);
            } else {
                let _ = self.secrets.delete(&touched_id);
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
                let _ = self.secrets.delete(&profile.id);
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
            if let Some(secret) = previous_secret {
                let _ = self.secrets.set(profile_id, &secret);
            }
            return Err(error);
        }
        self.view()
    }

    pub fn rename(&mut self, profile_id: &str, alias: &str) -> Result<AccountVaultView, String> {
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
            let _ = self.secrets.set(profile_id, &old_secret);
            return Err(error);
        }
        self.view()
    }

    pub fn switch_to(&mut self, profile_id: &str) -> Result<SwitchOutcome, String> {
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
            if let Some(previous) = previous_raw {
                let _ = atomic_replace(&self.auth_path, &previous);
            }
            return Err(
                "Credential switch verification failed; the previous login was restored.".into(),
            );
        }
        self.state.active_profile_id = Some(profile_id.to_string());
        if let Err(error) = self.persist_state() {
            self.state = previous_state;
            if let Some(previous) = previous_raw {
                let _ = atomic_replace(&self.auth_path, &previous);
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
        let raw = serde_json::to_vec_pretty(&self.state)
            .map_err(|_| "Account metadata could not be encoded.".to_string())?;
        atomic_write(&self.root.join(STATE_FILE), &raw)
    }
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

fn atomic_write(path: &Path, raw: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Storage path is invalid.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Account storage directory could not be created.".to_string())?;
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)
        .map_err(|_| "Temporary account metadata could not be created.".to_string())?;
    file.write_all(raw)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Account metadata could not be written.".to_string())?;
    if path.exists() {
        let backup = path.with_extension("bak");
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|_| "Account metadata backup failed.".to_string())?;
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            return Err(format!("Account metadata commit failed: {error}"));
        }
    } else {
        fs::rename(&temporary, path).map_err(|_| "Account metadata commit failed.".to_string())?;
    }
    Ok(())
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
    let temporary = parent.join(".auth.json.quotabeacon.tmp");
    let mut file = fs::File::create(&temporary)
        .map_err(|_| "Temporary Codex login could not be created.".to_string())?;
    file.write_all(raw)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Temporary Codex login could not be written.".to_string())?;
    #[cfg(unix)]
    {
        fs::rename(&temporary, path)
            .map_err(|_| "Codex login could not be replaced atomically.".to_string())?;
    }
    #[cfg(not(unix))]
    {
        let backup = parent.join(".auth.json.quotabeacon.bak");
        let _ = fs::remove_file(&backup);
        if path.exists() {
            fs::rename(path, &backup).map_err(|_| "Codex login backup failed.".to_string())?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::rename(&backup, path);
            return Err(format!("Codex login replacement failed: {error}"));
        }
        let _ = fs::remove_file(backup);
    }
    Ok(())
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
