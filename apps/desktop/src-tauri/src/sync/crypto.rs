use super::*;

pub(super) fn derive_encryption_key(device_id: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update([0]);
    hasher.update(salt);
    hasher.finalize().into()
}

pub(super) fn encrypt_value(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Encryption init failed: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {e}"))?;
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(&combined))
}

pub(super) fn cloud_bootstrap_scope(config: &SyncConfig) -> String {
    format!("cloud:{}:{}", config.cloud_url, config.cloud_email)
}

pub(super) fn decrypt_value(encoded: &str, key: &[u8; 32]) -> Result<String, String> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;
    if combined.len() < 13 {
        return Err("Encrypted value is too short".to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("Decryption init failed: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed (key may have changed): {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("Decrypted value is not valid UTF-8: {e}"))
}

pub(super) fn ensure_salt(config: &mut SyncConfig) -> Vec<u8> {
    if let Some(ref salt_b64) = config.password_salt {
        if let Ok(salt) = BASE64.decode(salt_b64) {
            return salt;
        }
    }
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    config.password_salt = Some(BASE64.encode(salt));
    salt.to_vec()
}

pub(super) fn encryption_key_for(config: &SyncConfig) -> Result<[u8; 32], String> {
    let salt_b64 = config
        .password_salt
        .as_ref()
        .ok_or_else(|| "Encryption salt is missing".to_string())?;
    let salt_bytes = BASE64
        .decode(salt_b64)
        .map_err(|e| format!("Invalid salt: {e}"))?;
    Ok(derive_encryption_key(&config.device_id, &salt_bytes))
}

pub(super) fn store_webdav_password(config: &mut SyncConfig, password: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.webdav_password_encrypted = Some(encrypt_value(password, &key)?);
    Ok(())
}

pub(super) fn get_webdav_password(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .webdav_password_encrypted
        .as_ref()
        .ok_or_else(|| "WebDAV password is missing; save sync settings again".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

pub(super) fn store_cloud_password(config: &mut SyncConfig, password: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.cloud_password_encrypted = Some(encrypt_value(password, &key)?);
    Ok(())
}

pub(super) fn get_cloud_password(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .cloud_password_encrypted
        .as_ref()
        .ok_or_else(|| "Cloud password is missing; save cloud settings again".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

pub(super) fn store_cloud_token(config: &mut SyncConfig, token: &str) -> Result<(), String> {
    let salt = ensure_salt(config);
    let key = derive_encryption_key(&config.device_id, &salt);
    config.cloud_token_encrypted = Some(encrypt_value(token, &key)?);
    Ok(())
}

pub(super) fn get_cloud_token_from_config(config: &SyncConfig) -> Result<String, String> {
    let encrypted = config
        .cloud_token_encrypted
        .as_ref()
        .ok_or_else(|| "Cloud token is missing".to_string())?;
    let key = encryption_key_for(config)?;
    decrypt_value(encrypted, &key)
}

pub(super) fn delete_cloud_token(config: &mut SyncConfig) {
    config.cloud_token_encrypted = None;
}
