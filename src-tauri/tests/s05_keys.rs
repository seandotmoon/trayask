//! S05 tester checks: `InMemoryKeyStore` behavior (no real keychain writes in CI).
use trayask::keys::{InMemoryKeyStore, KeyStore};

#[test]
fn set_then_get_round_trip() {
    let store = InMemoryKeyStore::new();
    store.set_key("openai", "k1").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), Some("k1".to_string()));
}

#[test]
fn get_missing_returns_none() {
    let store = InMemoryKeyStore::new();
    assert_eq!(store.get_key("openai").unwrap(), None);
}

#[test]
fn set_overwrites_previous_value() {
    let store = InMemoryKeyStore::new();
    store.set_key("openai", "old").unwrap();
    store.set_key("openai", "new").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), Some("new".to_string()));
}

#[test]
fn delete_removes_key_and_missing_delete_is_ok() {
    let store = InMemoryKeyStore::new();
    store.set_key("openai", "k1").unwrap();
    store.delete_key("openai").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), None);
    // Deleting a provider with no stored key must not error.
    store.delete_key("openai").unwrap();
    store.delete_key("anthropic").unwrap();
}

#[test]
fn providers_are_isolated() {
    let store = InMemoryKeyStore::new();
    store.set_key("openai", "k-openai").unwrap();
    store.set_key("anthropic", "k-anthropic").unwrap();
    assert_eq!(
        store.get_key("openai").unwrap(),
        Some("k-openai".to_string())
    );
    assert_eq!(
        store.get_key("anthropic").unwrap(),
        Some("k-anthropic".to_string())
    );
    store.delete_key("openai").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), None);
    assert_eq!(
        store.get_key("anthropic").unwrap(),
        Some("k-anthropic".to_string())
    );
}
