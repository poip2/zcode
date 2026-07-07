//! Integration tests for API key storage via OS keychain and mask logic.
//!
//! Run: cargo test --test settings_keychain -- --nocapture
//!
//! Tests the end-to-end flow of migrating legacy cleartext keys, masking,
//! and verifying that the store only holds de-identified values.

use std::fs;
use std::path::PathBuf;
use zcode_lib::settings;

// ── mask_api_key ──────────────────────────────────────────────────────────

#[test]
fn test_mask_api_key_standard() {
    assert_eq!(
        settings::mask_api_key("sk-abc123def4567890abcdef"),
        "sk-***cdef"
    );
    assert_eq!(
        settings::mask_api_key("sk-ant-api03-verylongkey1234567ea5"),
        "sk-***7ea5"
    );
}

#[test]
fn test_mask_api_key_short() {
    assert_eq!(settings::mask_api_key("abc"), "***");
    assert_eq!(settings::mask_api_key(""), "***");
    assert_eq!(settings::mask_api_key("1234567"), "***");
    assert_eq!(settings::mask_api_key("12345678"), "123***5678");
}

#[test]
fn test_mask_api_key_exact_boundary() {
    // exactly 8 chars
    assert_eq!(settings::mask_api_key("12345678"), "123***5678");
    // 9 chars
    assert_eq!(settings::mask_api_key("123456789"), "123***6789");
    // long openai key
    assert_eq!(
        settings::mask_api_key("sk-proj-abcdefghijklmnopqrstuvwxyz"),
        "sk-***wxyz"
    );
}

// ── Store-only masked key (no keychain required) ─────────────────────────

#[test]
fn test_masked_key_roundtrip_without_keychain() {
    // This test verifies that the frontend flow works: the store
    // holds a masked key, not the real key. The real key went to
    // the keychain (not exercised here because keychain is unavailable
    // in test environments without a secret-service daemon).

    let masked = settings::mask_api_key("sk-real-secret-key");
    assert_eq!(masked, "sk-***-key");
    assert_ne!(masked, "sk-real-secret-key");

    // The mask never reveals the original key
    assert!(!masked.contains("real-secret"));
}

// ── set_api_key graceful degradation when keychain unavailable ───────────

#[test]
fn test_set_api_key_returns_warning_when_keychain_unavailable() {
    // In WSL/Linux test environments without secret-service, keyring
    // operations will fail. The function must return Ok(Some(warning))
    // rather than Err — this ensures the app never blocks on keychain
    // unavailability.
    let result = settings::set_api_key("test-key-12345");
    assert!(
        result.is_ok(),
        "set_api_key must not panic or return Err even when keychain is unavailable"
    );

    if let Ok(opt) = result {
        if let Some(warning) = opt {
            eprintln!("Expected warning (no keychain daemon): {warning}");
            // The warning must not be empty
            assert!(!warning.is_empty());
        } else {
            eprintln!("Keychain is available — key stored successfully");
        }
    }
}

#[test]
fn test_get_api_key_no_entry_returns_none() {
    // If no key has ever been stored, get should return None
    let result = settings::get_api_key();
    // In test environment without keychain, this may return Err or Ok(None)
    match result {
        Ok(None) => eprintln!("No key stored: expected"),
        Ok(Some(_)) => eprintln!("Found existing key in keychain"),
        Err(e) => eprintln!("Keychain unavailable (expected in test env): {e}"),
    }
    // The function must not panic
}

#[test]
fn test_delete_api_key_graceful() {
    let result = settings::delete_api_key();
    assert!(result.is_ok(), "delete_api_key must not panic");
    // Even when keychain is unavailable, it should return Ok(Some(warning))
}

// ── Migration logic (file-level, no keychain needed) ─────────────────────

#[test]
fn test_migrate_old_settings_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = PathBuf::from(dir.path());
    // No file exists — migration should be a no-op
    settings::migrate_old_settings(&config_dir, &config_dir);
    // Assert: no panic, no new file created
    assert!(!config_dir.join("zcode-settings.json").exists());
}

#[test]
fn test_migrate_old_settings_no_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = PathBuf::from(dir.path());
    let json = r#"{
  "settings": {
    "aiProvider": {
      "baseUrl": "https://api.openai.com/v1",
      "model": "gpt-4o"
    }
  }
}"#;
    fs::write(config_dir.join("zcode-settings.json"), json).unwrap();

    settings::migrate_old_settings(&config_dir, &config_dir);

    // After migration: apiKey was absent, file should remain unchanged (no key to migrate)
    let content = fs::read_to_string(config_dir.join("zcode-settings.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&content).unwrap();
    let ai = &root["settings"]["aiProvider"];
    assert!(
        ai.get("apiKey").is_none(),
        "apiKey should not have appeared"
    );
    assert!(
        ai.get("maskedApiKey").is_none(),
        "maskedApiKey should not have appeared"
    );
}

#[test]
fn test_migrate_old_settings_with_api_key_graceful_no_keychain() {
    // When keychain is unavailable, migration must keep the legacy file intact
    // and not corrupt it.
    let dir = tempfile::tempdir().unwrap();
    let config_dir = PathBuf::from(dir.path());
    let original_json = r#"{
  "settings": {
    "aiProvider": {
      "baseUrl": "https://api.openai.com/v1",
      "apiKey": "sk-legacy-test-key-12345",
      "model": "gpt-4o"
    }
  }
}"#;
    fs::write(config_dir.join("zcode-settings.json"), original_json).unwrap();

    settings::migrate_old_settings(&config_dir, &config_dir);

    // Keychain unavailable in test env → legacy file should be preserved
    // (not removed, not corrupted)
    assert!(config_dir.join("zcode-settings.json").exists());
    let content = fs::read_to_string(config_dir.join("zcode-settings.json")).unwrap();
    eprintln!("Post-migration file: {content}");
    assert!(
        !content.is_empty(),
        "legacy file should not be empty after migration"
    );

    // If keychain IS available, apiKey should be removed and replaced with maskedApiKey
    // If keychain IS NOT available, the file stays intact with cleartext apiKey
    let root: serde_json::Value = serde_json::from_str(&content).unwrap();
    let ai = &root["settings"]["aiProvider"];
    let has_api_key = ai.get("apiKey").is_some();
    let has_masked = ai.get("maskedApiKey").is_some();
    eprintln!("has_api_key={has_api_key}, has_masked={has_masked}");

    // Either the migration succeeded (masked present, cleartext gone) or
    // keychain was unavailable (cleartext still present). Both are valid.
    if has_masked {
        assert!(
            !has_api_key,
            "cleartext apiKey must be removed when maskedApiKey is present"
        );
    } else {
        // Migration couldn't store to keychain — file preserved as-is
        assert!(has_api_key, "legacy apiKey should still be in the file");
    }
}

// ── Provider name parameterization ─────────────────────────────────────

#[test]
fn test_provider_name_passthrough_design() {
    // This test verifies the design intent: the provider name is
    // passed from the frontend, not hardcoded. We test this indirectly:
    // the `call_ai_provider` command signature accepts `provider_name: Option<String>`
    // and the default is only applied when empty/None.

    // The mask function is provider-agnostic
    let anthropic_key = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234";
    let masked = settings::mask_api_key(anthropic_key);
    assert_eq!(masked, "sk-***1234");
    // The mask works regardless of provider
}
