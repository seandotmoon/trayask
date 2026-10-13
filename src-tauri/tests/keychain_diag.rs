//! Manual diagnostic for OS-keychain issues (e.g. "saved but read-back says
//! no key" on macOS dev builds). Ignored by default so CI never touches a
//! real keychain. Run on the affected machine with:
//!   cargo test --test keychain_diag -- --ignored --nocapture
//! Uses a unique `trayask-diag-*` entry and deletes it afterwards; never
//! touches real provider keys. Prints each step result, never key material.
use trayask::keys::{KeyStore, KeychainStore};

#[test]
#[ignore = "manual diagnostic: touches the real OS keychain"]
fn real_keychain_round_trip() {
    let provider = format!("diag-{}", std::process::id());
    let store = KeychainStore::new();
    // Clean slate in case a previous interrupted run left the entry behind.
    let _ = store.delete_key(&provider);
    eprintln!("[diag] entry: trayask-{provider} / api-key");

    match store.set_key(&provider, "diag-secret-value") {
        Ok(()) => eprintln!("[diag] set: ok"),
        Err(e) => {
            eprintln!("[diag] set: FAILED: {e}");
            panic!("set failed: {e}");
        }
    }
    match store.get_key(&provider) {
        Ok(Some(v)) if v == "diag-secret-value" => {
            eprintln!("[diag] get: ok (round-trip match)")
        }
        Ok(Some(_)) => {
            eprintln!("[diag] get: MISMATCH (a different value came back)");
            panic!("value mismatch");
        }
        Ok(None) => {
            eprintln!("[diag] get: MISSING (set reported ok)");
            panic!("read-back missing");
        }
        Err(e) => {
            eprintln!("[diag] get: FAILED: {e}");
            panic!("get failed: {e}");
        }
    }
    match store.delete_key(&provider) {
        Ok(()) => eprintln!("[diag] delete: ok"),
        Err(e) => eprintln!("[diag] delete: FAILED: {e}"),
    }
    match store.get_key(&provider) {
        Ok(None) => eprintln!("[diag] post-delete get: ok (none, as expected)"),
        Ok(Some(_)) => {
            eprintln!("[diag] post-delete get: UNEXPECTED (still present)");
            panic!("post-delete state unexpected");
        }
        Err(e) => {
            eprintln!("[diag] post-delete get: FAILED: {e}");
            panic!("post-delete get failed: {e}");
        }
    }
}
