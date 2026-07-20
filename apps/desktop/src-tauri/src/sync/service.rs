use super::*;

impl SyncService {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            sync_lock: Mutex::new(()),
        }
    }

    pub fn get_config(&self) -> Result<SyncConfig, String> {
        if !self.config_path.exists() {
            let config = SyncConfig::default();
            let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
            std::fs::write(&self.config_path, data)
                .map_err(|e| format!("Failed to initialize sync config: {e}"))?;
            return Ok(config);
        }
        let data = std::fs::read(&self.config_path)
            .map_err(|e| format!("Failed to read sync config: {e}"))?;
        serde_json::from_slice(&data).map_err(|e| format!("Invalid sync config: {e}"))
    }

    pub fn set_config(&self, input: SyncConfigInput) -> Result<SyncConfig, String> {
        if input.provider != PROVIDER_NAME && !input.provider.is_empty() {
            return Err(format!("Unsupported sync provider: {}", input.provider));
        }
        let endpoint = input.endpoint.trim().trim_end_matches('/').to_string();
        let username = input.username.trim().to_string();
        let cloud_url = input.cloud_url.trim().trim_end_matches('/').to_string();
        let cloud_email = input.cloud_email.trim().to_string();
        if input.enabled
            && (!endpoint.starts_with("https://") && !endpoint.starts_with("http://")
                || username.is_empty())
        {
            return Err("WebDAV sync requires an HTTP(S) endpoint and username".to_string());
        }
        if input.cloud_enabled && input.enabled {
            return Err("Cloud mode and direct WebDAV mode are mutually exclusive".to_string());
        }
        if input.cloud_enabled
            && (!cloud_url.starts_with("https://") && !cloud_url.starts_with("http://")
                || cloud_email.is_empty())
        {
            return Err("Cloud sync requires an HTTP(S) URL and email".to_string());
        }

        let existing = self.get_config().unwrap_or_default();
        let webdav_identity_changed =
            existing.endpoint != endpoint || existing.username != username;
        let cloud_identity_changed =
            existing.cloud_url != cloud_url || existing.cloud_email != cloud_email;
        let mut config = SyncConfig {
            enabled: input.enabled,
            provider: input.provider,
            endpoint,
            username,
            device_id: existing.device_id.clone(),
            cloud_enabled: input.cloud_enabled,
            cloud_url,
            cloud_email,
            cloud_cursor_seq: if cloud_identity_changed {
                0
            } else {
                existing.cloud_cursor_seq
            },
            cloud_token_created_at: if cloud_identity_changed {
                0
            } else {
                existing.cloud_token_created_at
            },
            password_salt: existing.password_salt.clone(),
            webdav_password_encrypted: existing.webdav_password_encrypted.clone(),
            cloud_password_encrypted: existing.cloud_password_encrypted.clone(),
            cloud_token_encrypted: existing.cloud_token_encrypted.clone(),
        };
        if let Some(password) = input.password.filter(|value| !value.is_empty()) {
            store_webdav_password(&mut config, &password)?;
        } else if config.enabled {
            if webdav_identity_changed {
                if let Ok(old_password) = get_webdav_password(&existing) {
                    store_webdav_password(&mut config, &old_password)?;
                } else {
                    return Err(
                        "WebDAV endpoint or username changed; please re-enter the password"
                            .to_string(),
                    );
                }
            } else {
                get_webdav_password(&config)?;
            }
        }
        let has_new_cloud_password = input
            .cloud_password
            .as_deref()
            .is_some_and(|value| !value.is_empty());
        if let Some(cloud_pw) = input
            .cloud_password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            if !config.cloud_url.is_empty() && !config.cloud_email.is_empty() {
                store_cloud_password(&mut config, cloud_pw)?;
                delete_cloud_token(&mut config);
            }
        } else if config.cloud_enabled {
            if cloud_identity_changed {
                if let Ok(old_password) = get_cloud_password(&existing) {
                    store_cloud_password(&mut config, &old_password)?;
                } else {
                    return Err(
                        "Cloud account changed; please re-enter the cloud password".to_string()
                    );
                }
            } else {
                get_cloud_password(&config)?;
            }
        }
        if cloud_identity_changed && !has_new_cloud_password {
            delete_cloud_token(&mut config);
        }
        let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to write sync config: {e}"))?;
        Ok(config)
    }

    pub async fn sync(&self, db: &Database, attachments_dir: &Path) -> Result<SyncReport, String> {
        let _guard = self.sync_lock.lock().await;
        let mut config = self.get_config()?;

        let mut total_pushed = 0;
        let mut total_pulled = 0;
        let mut total_conflicts = 0;

        // Cloud sync path
        if config.cloud_enabled && !config.cloud_url.is_empty() {
            let cloud_token = self.get_cloud_token(&mut config).await?;
            let cloud = cloud::CloudProvider::new(&config.cloud_url, &cloud_token)?;
            db.ensure_sync_bootstrap(&cloud_bootstrap_scope(&config))
                .map_err(|e| e.to_string())?;

            // Pull from cloud
            let (envelopes, server_seq) = cloud.pull(config.cloud_cursor_seq).await?;
            for envelope in &envelopes {
                let (changed, conflict) =
                    apply_envelope(&cloud, db, attachments_dir, envelope, &config.device_id)
                        .await?;
                if changed {
                    total_pulled += 1;
                }
                if conflict {
                    total_conflicts += 1;
                }
            }
            config.cloud_cursor_seq = server_seq;

            // Push local changes to cloud
            let changes = db.list_pending_changes(500).map_err(|e| e.to_string())?;
            let mut cloud_envelopes = Vec::new();
            for change in &changes {
                if let Ok(envelope) = build_envelope(db, change, &config.device_id) {
                    if let Some(attachment) = &envelope.attachment {
                        let bytes = std::fs::read(attachments_dir.join(&attachment.relative_path))
                            .map_err(|error| {
                                format!("Failed to read attachment {}: {error}", attachment.id)
                            })?;
                        cloud
                            .put(
                                &format!("attachments/{}", attachment.id),
                                bytes,
                                &attachment.mime_type,
                            )
                            .await?;
                        db.mark_change_synced(change.seq)
                            .map_err(|error| error.to_string())?;
                        total_pushed += 1;
                    } else {
                        cloud_envelopes.push(envelope);
                    }
                }
            }
            if !cloud_envelopes.is_empty() {
                let response = cloud.push(&cloud_envelopes).await?;
                total_pushed += response.accepted;
                total_conflicts += response.conflicts;
                for sequence in response.acknowledged_sequences {
                    db.mark_change_synced(sequence)
                        .map_err(|error| error.to_string())?;
                }
            }

            // Save updated cursor
            self.save_config(&config)?;
        } else if config.enabled {
            // WebDAV-only sync (existing logic)
            let (wp, (pulled, conflicts)) = self.webdav_sync(db, attachments_dir, &config).await?;
            total_pushed = wp;
            total_pulled = pulled;
            total_conflicts = conflicts;
        } else {
            return Err("Sync is not enabled".to_string());
        }

        Ok(SyncReport {
            pushed: total_pushed,
            pulled: total_pulled,
            conflicts: total_conflicts,
        })
    }

    async fn webdav_sync(
        &self,
        db: &Database,
        attachments_dir: &Path,
        config: &SyncConfig,
    ) -> Result<(usize, (usize, usize)), String> {
        let password = get_webdav_password(config)?;
        let provider = WebDavProvider::new(&config.endpoint, &config.username, &password)?;
        provider.prepare(&config.device_id).await?;
        db.ensure_sync_bootstrap(&format!("{}:{}", config.provider, config.endpoint))
            .map_err(|e| e.to_string())?;

        let pulled_conflicts = pull_state(&provider, db, attachments_dir, config).await?;
        let pushed = push_state(&provider, db, attachments_dir, config).await?;
        Ok((pushed, pulled_conflicts))
    }

    async fn get_cloud_token(&self, config: &mut SyncConfig) -> Result<String, String> {
        let now = chrono::Utc::now().timestamp();
        if now - config.cloud_token_created_at < 6 * 24 * 60 * 60 {
            if let Ok(token) = get_cloud_token_from_config(config) {
                return Ok(token);
            }
        }
        let password = get_cloud_password(config)?;
        let login =
            cloud::CloudProvider::login(&config.cloud_url, &config.cloud_email, &password).await?;
        store_cloud_token(config, &login.token)?;
        config.cloud_token_created_at = now;
        Ok(login.token)
    }

    fn save_config(&self, config: &SyncConfig) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        std::fs::write(&self.config_path, data)
            .map_err(|e| format!("Failed to save sync config: {e}"))?;
        Ok(())
    }

    /// Test the stored WebDAV connection using the saved encrypted password.
    pub async fn test_stored_webdav(&self) -> Result<(), String> {
        let config = self.get_config()?;
        if !config.enabled {
            return Err("WebDAV sync is not enabled".to_string());
        }
        let password = get_webdav_password(&config)?;
        test_webdav_connection(&config.endpoint, &config.username, &password).await
    }
}
