//! Persistence of the Spotify refresh token between runs using the OS' keyring implementation.

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

use std::sync::Arc;

use anyhow::{Context, Result};
use keyring_core::{CredentialStore, Entry};
use secrecy::{ExposeSecret, SecretString};

/// Service name of the credential entry. The account name is the Spotify client ID.
const SERVICE: &str = "bop-merge";
const OPEN_FAILED: &str = "could not open the OS credential store";

/// Opens the platform's native credential store.
///
/// Uses Windows Credential Manager, the macOS Keychain, or the Secret Service on Linux. Other
/// platforms are not supported.
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

/// Storage for a single Spotify refresh token.
pub trait TokenStore {
    /// Returns the saved refresh token, or `None` if nothing is saved.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store cannot be read.
    fn load(&self) -> Result<Option<SecretString>>;

    /// Saves `token`, replacing any previously saved token.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store cannot be written.
    fn save(&self, token: &SecretString) -> Result<()>;

    /// Deletes the saved token. Succeeds if nothing is saved.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store cannot be modified.
    fn clear(&self) -> Result<()>;
}

/// A [`TokenStore`] backed by the OS credential store.
///
/// The token is stored under the service `bop-merge`, with the Spotify client ID as the account
/// name, so each client ID has its own saved login.
pub struct KeychainStore {
    entry: Entry,
}

impl KeychainStore {
    /// Opens the credential entry for `client_id` in the OS credential store.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform is not supported, the credential store cannot be
    /// opened, or the entry cannot be created.
    pub fn new(client_id: &str) -> Result<Self> {
        Self::in_store(&os_store()?, client_id)
    }

    /// Open the credential entry for `client_id` in `store`.
    fn in_store(store: &Arc<CredentialStore>, client_id: &str) -> Result<Self> {
        let entry = Entry::new_in_store(store, SERVICE, client_id)
            .context("could not create credential entry")?;

        Ok(Self { entry })
    }
}

impl TokenStore for KeychainStore {
    fn load(&self) -> Result<Option<SecretString>> {
        match self.entry.get_password() {
            Ok(token) => Ok(Some(SecretString::from(token))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context("could not read saved login"),
        }
    }

    fn save(&self, token: &SecretString) -> Result<()> {
        self.entry
            .set_password(token.expose_secret())
            .context("could not save login")
    }

    fn clear(&self) -> Result<()> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).context("could not delete saved login"),
        }
    }
}
