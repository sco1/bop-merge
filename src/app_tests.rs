use std::cell::RefCell;

use anyhow::anyhow;
use secrecy::{ExposeSecret, SecretString};

use super::*;

/// An in-memory `TokenStore`; `load_error` makes `load` fail like a locked keychain.
#[derive(Default)]
struct MemoryStore {
    token: RefCell<Option<String>>,
    load_error: bool,
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Result<Option<SecretString>> {
        if self.load_error {
            return Err(anyhow!("keychain is locked"));
        }
        Ok(self.token.borrow().clone().map(SecretString::from))
    }

    fn save(&self, token: &SecretString) -> Result<()> {
        *self.token.borrow_mut() = Some(token.expose_secret().to_owned());
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        *self.token.borrow_mut() = None;
        Ok(())
    }
}

#[test]
fn connect_without_saved_login_asks_to_log_in() {
    // Returns before any network request, so no server is needed.
    let err = connect("test-client", &MemoryStore::default())
        .err()
        .unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("not logged in"), "{msg}");
    assert!(msg.contains("bop-merge login"), "{msg}");
}

#[test]
fn connect_passes_on_store_errors() {
    // A store that can't be read is not the same as "not logged in".
    let store = MemoryStore {
        load_error: true,
        ..Default::default()
    };

    let err = connect("test-client", &store).err().unwrap();

    let msg = format!("{err:#}");
    assert!(msg.contains("keychain is locked"), "{msg}");
    assert!(!msg.contains("not logged in"), "{msg}");
}
