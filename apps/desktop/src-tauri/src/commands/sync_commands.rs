use super::*;

#[tauri::command]
pub fn get_sync_config(sync: State<'_, Arc<SyncService>>) -> Result<SyncConfig, String> {
    sync.get_config()
}

#[tauri::command]
pub fn set_sync_config(
    sync: State<'_, Arc<SyncService>>,
    config: SyncConfigInput,
) -> Result<SyncConfig, String> {
    sync.set_config(config)
}

#[tauri::command]
pub async fn test_webdav_connection(
    sync: State<'_, Arc<SyncService>>,
    endpoint: String,
    username: String,
    password: String,
) -> Result<(), String> {
    if password.is_empty() {
        // Use stored encrypted password when user didn't re-enter
        sync.test_stored_webdav().await
    } else {
        crate::sync::test_webdav_connection(&endpoint, &username, &password).await
    }
}

#[tauri::command]
pub async fn test_cloud_connection(
    cloud_url: String,
    cloud_email: String,
    cloud_password: String,
) -> Result<(), String> {
    crate::sync::test_cloud_connection(&cloud_url, &cloud_email, &cloud_password).await
}

#[tauri::command]
pub async fn sync_now(
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
    paths: State<'_, Arc<AppPaths>>,
) -> Result<SyncReport, String> {
    sync.sync(&db, &paths.attachments_dir).await
}

#[tauri::command]
pub fn has_pending_sync_changes(db: State<'_, Arc<Database>>) -> Result<bool, String> {
    db.list_pending_changes(1)
        .map(|changes| !changes.is_empty())
        .map_err(|error| error.to_string())
}
