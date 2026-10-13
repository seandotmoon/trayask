use std::collections::HashMap;
use std::sync::Mutex;

/// Abstract key storage. The test mock is [`InMemoryKeyStore`];
/// the real OS-keychain backend arrives in S05.
pub trait KeyStore: Send + Sync {
    fn set_key(&self, provider: &str, key: &str) -> Result<(), String>;
    fn get_key(&self, provider: &str) -> Result<Option<String>, String>;
    fn delete_key(&self, provider: &str) -> Result<(), String>;
}

/// In-memory [`KeyStore`] for tests. Not for production secrets.
pub struct InMemoryKeyStore {
    inner: Mutex<HashMap<String, String>>,
}

impl InMemoryKeyStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for InMemoryKeyStore {
    fn set_key(&self, provider: &str, key: &str) -> Result<(), String> {
        self.inner
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .insert(provider.to_string(), key.to_string());
        Ok(())
    }

    fn get_key(&self, provider: &str) -> Result<Option<String>, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .get(provider)
            .cloned())
    }

    fn delete_key(&self, provider: &str) -> Result<(), String> {
        self.inner
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .remove(provider);
        Ok(())
    }
}

/// Real OS-keychain [`KeyStore`]. Same API on all OSes; the
/// `keyring` crate picks the native backend underneath.
/// One entry per provider: service `trayask-<provider>`, user `api-key`.
pub struct KeychainStore;

impl KeychainStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(provider: &str) -> Result<keyring::Entry, String> {
        let service = format!("trayask-{provider}");
        keyring::Entry::new(&service, "api-key").map_err(|e| format!("keychain error: {e}"))
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for KeychainStore {
    fn set_key(&self, provider: &str, key: &str) -> Result<(), String> {
        Self::entry(provider)?
            .set_password(key)
            .map_err(|e| format!("keychain error: {e}"))
    }

    fn get_key(&self, provider: &str) -> Result<Option<String>, String> {
        let entry = Self::entry(provider)?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("keychain error: {e}")),
        }
    }

    fn delete_key(&self, provider: &str) -> Result<(), String> {
        let entry = Self::entry(provider)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("keychain error: {e}")),
        }
    }
}
