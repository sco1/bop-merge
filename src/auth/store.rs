#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

use std::sync::Arc;

use anyhow::{Context, Result};
use keyring_core::{CredentialStore, Entry};
use secrecy::{ExposeSecret, SecretString};

const SERVICE: &str = "bop-merge";
const OPEN_FAILED: &str = "could not open the OS credential store";

fn os_store() -> Result<Arc<CredentialStore>> {
    let store: Arc<CredentialStore> = cfg_select! {
        windows => windows_native_keyring_store::Store::new().context(OPEN_FAILED)?,
        target_os = "macos" => {
            apple_native_keyring_store::keychain::Store::new().context(OPEN_FAILED)?
        }
        target_os = "linux" => {
            zbus_secret_service_keyring_store::Store::new().context(OPEN_FAILED)?
        }
        _ => {
            anyhow::bail!("login token caching is not supported on this OS")
        }
    };

    Ok(store)
}

pub trait TokenStore {
    fn load(&self) -> Result<Option<SecretString>>;
    fn save(&self, token: &SecretString) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub struct KeychainStore {
    entry: Entry,
}

impl KeychainStore {
    pub fn new(client_id: &str) -> Result<Self> {
        Self::in_store(&os_store()?, client_id)
    }

    fn in_store(store: &Arc<CredentialStore>, client_id: &str) -> Result<Self> {
        let entry = Entry::new_in_store(store, SERVICE, client_id)
            .context("could not create credential entry")?;

        Ok(Self { entry })
    }
}

impl TokenStore for KeychainStore {
    /// Return the saved refresh token, or `None`` if nothing is saved.
    fn load(&self) -> Result<Option<SecretString>> {
        match self.entry.get_password() {
            Ok(token) => Ok(Some(SecretString::from(token))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("could not read saved login"),
        }
    }

    /// Save the refresh token, replacing any saved one.
    fn save(&self, token: &SecretString) -> Result<()> {
        self.entry
            .set_password(token.expose_secret())
            .context("could not save login")
    }

    /// Delete the saved refresh token; also succeeds if nothing is saved.
    fn clear(&self) -> Result<()> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("could not delete saved login"),
        }
    }
}
