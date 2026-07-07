//! API key storage via OS keychain.
//!
//! The real apiKey lives exclusively in the system keychain.
//! The frontend store (`zcode-settings.json`) only holds a masked
//! version (e.g. "sk-5d70d***5c60") that is safe to persist as plaintext.
//!
//! # Keyring availability
//! - macOS:   Security framework (Keychain Access) — always available
//! - Windows: Windows Credential Manager — always available
//! - Linux:   `secret-service` D-Bus API (requires gnome-keyring, kwallet, or
//!            similar daemon). In WSL without a daemon, operations will fail
//!            gracefully with a descriptive error.

// ============================================================================
// Constants
// ============================================================================

/// Keychain service name.
const KEYRING_SERVICE: &str = "com.poip3.zcode.ai_provider";

/// Keychain account name.
const KEYRING_ACCOUNT: &str = "api_key";

// ============================================================================
// Keyring helpers
// ============================================================================

fn keyring_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
}

/// Store (or overwrite) the API key in the OS keychain.
/// Returns `Ok(None)` on success, `Ok(Some(warning))` if keychain unavailable.
pub fn set_api_key(api_key: &str) -> Result<Option<String>, String> {
    let entry = match keyring_entry() {
        Ok(e) => e,
        Err(e) => {
            return Ok(Some(format!(
                "API key could not be stored in keychain.\n\
                 On Linux/WSL, make sure a secret-service daemon is running\n\
                 (gnome-keyring, kwallet, etc.).\n\
                 Details: {e}"
            )));
        }
    };
    match entry.set_password(api_key) {
        Ok(()) => Ok(None),
        Err(e) => Ok(Some(format!(
            "API key could not be stored in keychain.\n\
             On Linux/WSL, make sure a secret-service daemon is running.\n\
             Details: {e}"
        ))),
    }
}

/// Retrieve the API key from the OS keychain.
pub fn get_api_key() -> Result<Option<String>, String> {
    let entry = keyring_entry().map_err(|e| format!("Keychain error: {e}"))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read API key from keychain: {e}")),
    }
}

/// Remove the API key from the OS keychain.
pub fn delete_api_key() -> Result<Option<String>, String> {
    let entry = match keyring_entry() {
        Ok(e) => e,
        Err(e) => {
            return Ok(Some(format!(
                "Could not access keychain to delete API key. Details: {e}"
            )));
        }
    };
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Ok(Some(format!("Failed to delete API key: {e}"))),
    }
}


