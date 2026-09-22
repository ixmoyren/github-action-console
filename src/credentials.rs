use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use thiserror::Error;

use crate::github::SecretToken;

const SERVICE: &str = "github-action-console";

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("keyring failure: {0}")]
    Keyring(#[from] keyring::Error),
}

/// In production tokens live in the OS keyring. Tests flip this to an
/// in-process map with `use_in_memory_backend`, because the keyring crates
/// offer no test backend that persists across `Entry` instances.
static USE_IN_MEMORY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn memory() -> &'static Mutex<HashMap<String, String>> {
    static MEMORY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn in_memory() -> bool {
    USE_IN_MEMORY.load(std::sync::atomic::Ordering::Relaxed)
}

/// Routes credential storage to an in-process map. Test support only.
#[doc(hidden)]
pub fn use_in_memory_backend() {
    USE_IN_MEMORY.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn store_token(account: &str, token: &SecretToken) -> Result<(), CredentialError> {
    if in_memory() {
        memory()
            .lock()
            .expect("credential memory poisoned")
            .insert(account.to_owned(), token.expose().to_owned());
        return Ok(());
    }

    keyring::Entry::new(SERVICE, account)?.set_password(token.expose())?;
    Ok(())
}

pub fn load_token(account: &str) -> Result<Option<SecretToken>, CredentialError> {
    if in_memory() {
        let found = memory()
            .lock()
            .expect("credential memory poisoned")
            .get(account)
            .cloned();
        return Ok(found.map(SecretToken::new));
    }

    match keyring::Entry::new(SERVICE, account)?.get_password() {
        Ok(value) => Ok(Some(SecretToken::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn delete_token(account: &str) -> Result<(), CredentialError> {
    if in_memory() {
        memory()
            .lock()
            .expect("credential memory poisoned")
            .remove(account);
        return Ok(());
    }

    match keyring::Entry::new(SERVICE, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
