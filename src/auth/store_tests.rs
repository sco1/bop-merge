use super::*;

/// A `KeychainStore` backed by its own in-memory mock store, so tests never share a token.
fn mock_store() -> KeychainStore {
    let store: Arc<CredentialStore> = keyring_core::mock::Store::new().unwrap();
    KeychainStore::in_store(&store, "test-client").unwrap()
}

fn secret(s: &str) -> SecretString {
    SecretString::from(s)
}

fn loaded(store: &KeychainStore) -> Option<String> {
    store
        .load()
        .unwrap()
        .map(|token| token.expose_secret().to_owned())
}

#[test]
fn load_is_none_when_nothing_saved() {
    assert_eq!(loaded(&mock_store()), None);
}

#[test]
fn save_then_load() {
    let store = mock_store();

    store.save(&secret("token-a")).unwrap();

    assert_eq!(loaded(&store).as_deref(), Some("token-a"));
}

#[test]
fn save_overwrites() {
    let store = mock_store();

    store.save(&secret("token-a")).unwrap();
    store.save(&secret("token-b")).unwrap();

    assert_eq!(loaded(&store).as_deref(), Some("token-b"));
}

#[test]
fn clear_removes_the_token() {
    let store = mock_store();
    store.save(&secret("token-a")).unwrap();

    store.clear().unwrap();

    assert_eq!(loaded(&store), None);
}

#[test]
fn clear_when_nothing_saved_is_ok() {
    let store = mock_store();

    store.clear().unwrap();
    store.clear().unwrap();
}

#[test]
fn separate_stores_do_not_share_tokens() {
    let first = mock_store();
    let second = mock_store();

    first.save(&secret("token-a")).unwrap();

    assert_eq!(loaded(&second), None);
}
