//! Small boundary around the operating system credential store.
//!
//! Product code receives secrets only after explicitly asking this boundary.
//! Tests inject an in-memory implementation and never require a desktop
//! keyring service.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    Unavailable,
}

pub trait CredentialStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, CredentialError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError>;
    fn delete(&self, account: &str) -> Result<(), CredentialError>;
}

#[derive(Default)]
pub struct OsCredentialStore;

impl OsCredentialStore {
    const SERVICE: &'static str = "Consensus Arena";

    fn entry(account: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(Self::SERVICE, account).map_err(|_| CredentialError::Unavailable)
    }
}

impl CredentialStore for OsCredentialStore {
    fn get(&self, account: &str) -> Result<Option<String>, CredentialError> {
        let entry = Self::entry(account)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CredentialError::Unavailable),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|_| CredentialError::Unavailable)
    }

    fn delete(&self, account: &str) -> Result<(), CredentialError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError::Unavailable),
        }
    }
}

pub fn secure_storage_help() -> &'static str {
    "Secure credential storage is unavailable. Unlock the system keyring or Credential Manager, then retry. Existing saved credentials have been kept."
}

#[cfg(test)]
mod tests {
    use super::{CredentialStore, OsCredentialStore};

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an available Linux Secret Service session"]
    fn native_linux_credential_store_round_trip() {
        native_store_round_trip();
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_credential_store_round_trip() {
        native_store_round_trip();
    }

    #[cfg(any(target_os = "linux", windows))]
    fn native_store_round_trip() {
        let store = OsCredentialStore;
        let account = format!(
            "qualification.{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        );
        let sentinel = format!("synthetic-credential-{}", uuid::Uuid::new_v4());
        assert!(store.set(&account, &sentinel).is_ok());
        let read_back = store.get(&account);
        let matched = matches!(read_back, Ok(Some(value)) if value == sentinel);
        let deleted = store.delete(&account).is_ok();
        let absent = matches!(store.get(&account), Ok(None));
        assert!(matched && deleted && absent);
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MemoryCredentialStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl CredentialStore for MemoryCredentialStore {
    fn get(&self, account: &str) -> Result<Option<String>, CredentialError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        Ok(entries.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), CredentialError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        entries.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), CredentialError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        entries.remove(account);
        Ok(())
    }
}
