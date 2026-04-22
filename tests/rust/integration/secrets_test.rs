/// Verify vault can be created and re-opened.
#[test]
fn test_secrets_vault_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test_secrets.enc");

    // Create vault
    let mut vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault.set("llm.openai.api_key", "sk-test-123").unwrap();
    vault.set("telegram.bot_token", "bot:token").unwrap();
    vault.save().unwrap();

    assert!(path.exists(), "Vault file should be created");

    // Re-open and verify
    let vault2 = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    assert_eq!(
        vault2.get("llm.openai.api_key").as_deref(),
        Some("sk-test-123")
    );
    assert_eq!(
        vault2.get("telegram.bot_token").as_deref(),
        Some("bot:token")
    );
}

/// Verify delete removes a key from the vault.
#[test]
fn test_secrets_vault_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test_delete.enc");

    let mut vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault.set("llm.openai.api_key", "sk-delete-me").unwrap();
    vault.set("telegram.bot_token", "bot:delete").unwrap();
    vault.save().unwrap();

    // Re-open, delete, save
    let mut vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault.delete("llm.openai.api_key").unwrap();
    vault.save().unwrap();

    // Verify
    let vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    assert!(vault.get("llm.openai.api_key").is_none());
    assert_eq!(
        vault.get("telegram.bot_token").as_deref(),
        Some("bot:delete")
    );
}

/// Verify the encrypted file is not plaintext readable.
#[test]
fn test_secrets_vault_encrypted() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test_encrypted.enc");

    let mut vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault
        .set("llm.openai.api_key", "super-secret-value-12345")
        .unwrap();
    vault.save().unwrap();

    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        !raw.contains("super-secret-value-12345"),
        "Secret value should NOT appear in plaintext in the vault file"
    );
}

/// Verify get for non-existent key returns None.
#[test]
fn test_secrets_vault_missing_key() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test_missing.enc");

    let mut vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault.set("llm.openai.api_key", "sk-exists").unwrap();
    vault.save().unwrap();

    let vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    assert!(vault.get("github.access_token").is_none());
}

/// Verify has_llm_configured works correctly.
#[test]
fn test_secrets_has_llm_configured() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test_llm.enc");

    let vault = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    assert!(!vault.has_llm_configured());

    let mut vault2 = pylot::secrets::SecretsVault::open(&path, None).unwrap();
    vault2.set("llm.openai.api_key", "sk-test").unwrap();
    assert!(vault2.has_llm_configured());
}
