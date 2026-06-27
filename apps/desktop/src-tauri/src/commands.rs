use crate::db::{ClipboardItem, Database, Note, NoteSummary, NoteVersion, TagSummary};
use crate::sync::{SyncConfig, SyncConfigInput, SyncReport, SyncService};
use base64::{engine::general_purpose, Engine as _};
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
#[cfg(target_os = "windows")]
use windows::{
    ApplicationModel::DataTransfer::{Clipboard, StandardDataFormats},
    Foundation::Uri,
};

#[derive(Clone)]
pub struct AppPaths {
    pub attachments_dir: PathBuf,
}

#[derive(Default)]
pub struct ClipboardCaptureState {
    fingerprint: Mutex<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct ClipboardSyncResult {
    pub captured: usize,
}

#[derive(Debug, Serialize)]
pub struct Attachment {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct AttachmentDataUrl {
    pub id: String,
    pub data_url: String,
}

#[tauri::command]
pub fn create_note(db: State<'_, Arc<Database>>, content: String) -> Result<Note, String> {
    db.create_note(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note(db: State<'_, Arc<Database>>, id: String) -> Result<Option<Note>, String> {
    db.get_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notes(db: State<'_, Arc<Database>>) -> Result<Vec<NoteSummary>, String> {
    db.list_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notes_by_tag(
    db: State<'_, Arc<Database>>,
    tag: String,
) -> Result<Vec<NoteSummary>, String> {
    db.list_notes_by_tag(&tag).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tags(db: State<'_, Arc<Database>>) -> Result<Vec<TagSummary>, String> {
    db.list_tags().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_note_tags(
    db: State<'_, Arc<Database>>,
    note_id: String,
    tags: Vec<String>,
) -> Result<Option<Note>, String> {
    db.set_note_tags(&note_id, &tags).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_note(
    db: State<'_, Arc<Database>>,
    id: String,
    content: String,
) -> Result<Option<Note>, String> {
    db.update_note(&id, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.delete_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_note(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.restore_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn purge_note(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.purge_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_deleted_notes(db: State<'_, Arc<Database>>) -> Result<Vec<NoteSummary>, String> {
    db.list_deleted_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_pin(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.toggle_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_notes(
    db: State<'_, Arc<Database>>,
    ids: Vec<String>,
    is_pinned: bool,
) -> Result<(), String> {
    db.reorder_notes(&ids, is_pinned).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_notes(
    db: State<'_, Arc<Database>>,
    query: String,
) -> Result<Vec<NoteSummary>, String> {
    db.search_notes(&query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_versions(
    db: State<'_, Arc<Database>>,
    id: String,
) -> Result<Vec<NoteVersion>, String> {
    db.get_note_versions(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_note_version(
    db: State<'_, Arc<Database>>,
    note_id: String,
    version_id: i64,
) -> Result<Option<Note>, String> {
    db.restore_note_version(&note_id, version_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_version_pin(db: State<'_, Arc<Database>>, version_id: i64) -> Result<bool, String> {
    db.toggle_version_pin(version_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note_version(db: State<'_, Arc<Database>>, version_id: i64) -> Result<bool, String> {
    db.delete_note_version(version_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_note_versions(db: State<'_, Arc<Database>>, note_id: String) -> Result<usize, String> {
    db.clear_note_versions(&note_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_attachment(
    db: State<'_, Arc<Database>>,
    paths: State<'_, Arc<AppPaths>>,
    data_url: String,
    filename: String,
) -> Result<Attachment, String> {
    let (_, payload) = data_url
        .split_once(',')
        .ok_or_else(|| "Invalid data URL".to_string())?;
    if payload.len() > 28 * 1024 * 1024 {
        return Err("Attachment is larger than the 20 MB limit".to_string());
    }
    let bytes = general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("Invalid attachment payload: {e}"))?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Attachment is larger than the 20 MB limit".to_string());
    }

    let extension = infer_extension(&data_url, &filename);
    let mime_type = infer_mime_type(&data_url, &extension);
    let id = format!("{:x}", Sha256::digest(&bytes));
    let relative_path = format!("{id}.{extension}");
    let path = paths.attachments_dir.join(&relative_path);
    if !path.exists() {
        std::fs::write(&path, &bytes).map_err(|e| format!("Failed to save attachment: {e}"))?;
    }
    let record = db
        .register_attachment(&id, &relative_path, &mime_type, bytes.len() as i64)
        .map_err(|e| e.to_string())?;
    let registered_path = paths.attachments_dir.join(record.relative_path);

    Ok(Attachment {
        id,
        path: registered_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn get_attachment(
    db: State<'_, Arc<Database>>,
    paths: State<'_, Arc<AppPaths>>,
    id: String,
) -> Result<Attachment, String> {
    let record = db
        .get_attachment(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Attachment not found".to_string())?;
    Ok(Attachment {
        id,
        path: paths
            .attachments_dir
            .join(record.relative_path)
            .to_string_lossy()
            .to_string(),
    })
}

#[tauri::command]
pub fn get_attachment_data_url(
    db: State<'_, Arc<Database>>,
    paths: State<'_, Arc<AppPaths>>,
    id: String,
) -> Result<AttachmentDataUrl, String> {
    let record = db
        .get_attachment(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Attachment not found".to_string())?;
    if !record.mime_type.starts_with("image/") {
        return Err("Attachment is not an image".to_string());
    }
    let path = paths.attachments_dir.join(&record.relative_path);
    let bytes = std::fs::read(&path).map_err(|e| format!("Failed to read attachment: {e}"))?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Attachment is larger than the 20 MB limit".to_string());
    }
    Ok(AttachmentDataUrl {
        id,
        data_url: format!(
            "data:{};base64,{}",
            record.mime_type,
            general_purpose::STANDARD.encode(bytes)
        ),
    })
}

#[tauri::command]
pub fn cleanup_attachments(
    db: State<'_, Arc<Database>>,
    paths: State<'_, Arc<AppPaths>>,
) -> Result<usize, String> {
    let orphans = db.orphan_attachments().map_err(|e| e.to_string())?;
    let mut removed = 0;
    for record in orphans {
        let path = paths.attachments_dir.join(&record.relative_path);
        if !path.exists() || std::fs::remove_file(&path).is_ok() {
            db.remove_attachment_record(&record.id)
                .map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    Ok(removed)
}

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
pub fn clipboard_auto_capture_supported() -> bool {
    cfg!(not(any(target_os = "android", target_os = "ios")))
}

#[tauri::command]
pub fn sync_clipboard_history(
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
) -> Result<ClipboardSyncResult, String> {
    #[cfg(target_os = "windows")]
    {
        let device_id = sync.get_config()?.device_id;
        let captured =
            tauri::async_runtime::block_on(capture_windows_clipboard_history(&db, &device_id))?;
        return Ok(ClipboardSyncResult { captured });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = db;
        let _ = sync;
        Ok(ClipboardSyncResult { captured: 0 })
    }
}

#[tauri::command]
pub fn capture_clipboard(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
    paths: State<'_, Arc<AppPaths>>,
    capture_state: State<'_, ClipboardCaptureState>,
) -> Result<Option<ClipboardItem>, String> {
    let content = match app.clipboard().read_text() {
        Ok(text) if !text.trim().is_empty() => text,
        _ => match read_clipboard_image_html(&app, &db, &paths) {
            Ok(Some(html)) => html,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        },
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{normalized}"))
    );
    if capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())?
        .as_ref()
        == Some(&fingerprint)
    {
        return Ok(None);
    }
    let device_id = sync.get_config()?.device_id;
    let item = db
        .capture_clipboard(&content, &device_id)
        .map(Some)
        .map_err(|e| e.to_string())?;
    *capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())? = Some(fingerprint);
    Ok(item)
}

#[tauri::command]
pub fn prime_clipboard_capture(
    app: AppHandle,
    capture_state: State<'_, ClipboardCaptureState>,
) -> Result<bool, String> {
    let content = match app.clipboard().read_text() {
        Ok(text) if !text.trim().is_empty() => text,
        _ => return Ok(false),
    };
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    *capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())? = Some(format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{normalized}"))
    ));
    Ok(true)
}

#[tauri::command]
pub fn list_clipboard_items(
    db: State<'_, Arc<Database>>,
    query: String,
) -> Result<Vec<ClipboardItem>, String> {
    db.list_clipboard_items(&query, 300)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_clipboard_item(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    capture_state: State<ClipboardCaptureState>,
    id: String,
) -> Result<bool, String> {
    let Some(item) = db.get_clipboard_item(&id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    app.clipboard()
        .write_text(clipboard_plain_text(&item))
        .map_err(|e| e.to_string())?;
    if item.kind == "rich" || item.kind == "image" {
        let _ = app
            .clipboard()
            .write_html(item.content.clone(), Some(clipboard_plain_text(&item)));
    }
    *capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())? = Some(format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{}", item.content))
    ));
    Ok(true)
}

#[tauri::command]
pub fn toggle_clipboard_pin(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.toggle_clipboard_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_clipboard_item(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.delete_clipboard_item(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_clipboard(db: State<'_, Arc<Database>>) -> Result<usize, String> {
    db.clear_clipboard_items().map_err(|e| e.to_string())
}

fn infer_extension(data_url: &str, filename: &str) -> String {
    let from_name = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp"));

    if let Some(ext) = from_name {
        return if ext == "jpeg" {
            "jpg".to_string()
        } else {
            ext
        };
    }

    if data_url.starts_with("data:image/png") {
        "png".to_string()
    } else if data_url.starts_with("data:image/jpeg") {
        "jpg".to_string()
    } else if data_url.starts_with("data:image/gif") {
        "gif".to_string()
    } else {
        "webp".to_string()
    }
}

fn read_clipboard_image_html(
    app: &AppHandle,
    db: &Database,
    paths: &AppPaths,
) -> Result<Option<String>, String> {
    let image = match app.clipboard().read_image() {
        Ok(image) => image,
        Err(_) => return Ok(None),
    };
    let bytes = image.rgba();
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Ok(None);
    }
    let mut png = Vec::new();
    PngEncoder::new(Cursor::new(&mut png))
        .write_image(
            bytes,
            image.width(),
            image.height(),
            ColorType::Rgba8.into(),
        )
        .map_err(|e| format!("Failed to encode clipboard image: {e}"))?;
    let id = save_attachment_bytes(db, paths, &png, "png", "image/png")?;
    Ok(Some(format!(
        r#"<img src="attachment://{id}" data-attachment-id="{id}" alt="剪贴板图片" title="剪贴板图片">"#
    )))
}

fn save_attachment_bytes(
    db: &Database,
    paths: &AppPaths,
    bytes: &[u8],
    extension: &str,
    mime_type: &str,
) -> Result<String, String> {
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Attachment is larger than the 20 MB limit".to_string());
    }
    let id = format!("{:x}", Sha256::digest(bytes));
    let relative_path = format!("{id}.{extension}");
    let path = paths.attachments_dir.join(&relative_path);
    if !path.exists() {
        std::fs::write(&path, bytes).map_err(|e| format!("Failed to save attachment: {e}"))?;
    }
    db.register_attachment(&id, &relative_path, mime_type, bytes.len() as i64)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

fn clipboard_plain_text(item: &ClipboardItem) -> String {
    if item.kind == "rich" || item.kind == "image" {
        strip_html_tags(&item.content)
    } else {
        item.content.clone()
    }
}

fn strip_html_tags(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut in_tag = false;
    for ch in content.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os = "windows")]
async fn capture_windows_clipboard_history(
    db: &Database,
    device_id: &str,
) -> Result<usize, String> {
    let result = Clipboard::GetHistoryItemsAsync()
        .map_err(|e| format!("Failed to read Windows clipboard history: {e}"))?
        .await
        .map_err(|e| format!("Failed to await Windows clipboard history: {e}"))?;

    let items = result
        .Items()
        .map_err(|e| format!("Failed to parse Windows clipboard history: {e}"))?;

    let size = items
        .Size()
        .map_err(|e| format!("Failed to inspect Windows clipboard history size: {e}"))?;

    let mut captured = 0usize;
    for index in (0..size).rev() {
        let item = items
            .GetAt(index)
            .map_err(|e| format!("Failed to read Windows clipboard history item: {e}"))?;
        let Some(content) = read_windows_history_item_content(&item).await? else {
            continue;
        };
        let captured_at = item
            .Timestamp()
            .map_err(|e| format!("Failed to read clipboard item timestamp: {e}"))?
            .universal_time_to_rfc3339()?;
        let record = db
            .capture_clipboard_at(&content, device_id, &captured_at)
            .map_err(|e| e.to_string())?;
        if record.last_copied_at == captured_at {
            captured += 1;
        }
    }
    Ok(captured)
}

#[cfg(target_os = "windows")]
async fn read_windows_history_item_content(
    item: &windows::ApplicationModel::DataTransfer::ClipboardHistoryItem,
) -> Result<Option<String>, String> {
    let package = item
        .Content()
        .map_err(|e| format!("Failed to read clipboard history package: {e}"))?;

    let html_format = StandardDataFormats::Html()
        .map_err(|e| format!("Failed to resolve HTML clipboard format: {e}"))?;
    if package
        .Contains(&html_format)
        .map_err(|e| format!("Failed to inspect HTML clipboard content: {e}"))?
    {
        let html = package
            .GetHtmlFormatAsync()
            .map_err(|e| format!("Failed to read HTML clipboard content: {e}"))?
            .await
            .map_err(|e| format!("Failed to await HTML clipboard content: {e}"))?;
        let normalized = normalize_windows_clipboard_html(&html.to_string());
        if looks_like_visual_clipboard_html(&normalized) {
            return Ok(Some(normalized));
        }
    }

    let text_format = StandardDataFormats::Text()
        .map_err(|e| format!("Failed to resolve text clipboard format: {e}"))?;
    if package
        .Contains(&text_format)
        .map_err(|e| format!("Failed to inspect text clipboard content: {e}"))?
    {
        let text = package
            .GetTextAsync()
            .map_err(|e| format!("Failed to read text clipboard content: {e}"))?
            .await
            .map_err(|e| format!("Failed to await text clipboard content: {e}"))?;
        let value = text.to_string();
        if !value.trim().is_empty() {
            return Ok(Some(value));
        }
    }

    let uri_format = StandardDataFormats::Uri()
        .map_err(|e| format!("Failed to resolve URI clipboard format: {e}"))?;
    if package
        .Contains(&uri_format)
        .map_err(|e| format!("Failed to inspect URI clipboard content: {e}"))?
    {
        let uri: Uri = package
            .GetUriAsync()
            .map_err(|e| format!("Failed to read URI clipboard content: {e}"))?
            .await
            .map_err(|e| format!("Failed to await URI clipboard content: {e}"))?;
        let value = uri
            .RawUri()
            .map_err(|e| format!("Failed to format URI clipboard content: {e}"))?
            .to_string();
        if !value.trim().is_empty() {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

#[cfg(target_os = "windows")]
fn normalize_windows_clipboard_html(content: &str) -> String {
    if let Some((_, fragment)) = content.split_once("<!--StartFragment-->") {
        if let Some((fragment, _)) = fragment.split_once("<!--EndFragment-->") {
            return fragment.trim().to_string();
        }
    }
    content.trim().to_string()
}

#[cfg(target_os = "windows")]
fn looks_like_visual_clipboard_html(content: &str) -> bool {
    let lowered = content.trim().to_ascii_lowercase();
    lowered.contains("<img ")
        || lowered.contains("<table")
        || lowered.contains("<ul")
        || lowered.contains("<ol")
        || lowered.contains("<li")
        || lowered.contains("<pre")
        || lowered.contains("<code")
        || lowered.contains("<blockquote")
        || lowered.contains("<figure")
        || lowered.contains("<br")
        || [
            "<p", "<div", "<section", "<article", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
        ]
        .iter()
        .map(|needle| lowered.matches(needle).count())
        .sum::<usize>()
            > 1
}

#[cfg(target_os = "windows")]
trait WindowsDateTimeExt {
    fn universal_time_to_rfc3339(self) -> Result<String, String>;
}

#[cfg(target_os = "windows")]
impl WindowsDateTimeExt for windows::Foundation::DateTime {
    fn universal_time_to_rfc3339(self) -> Result<String, String> {
        const WINDOWS_EPOCH_OFFSET_100NS: i64 = 116_444_736_000_000_000;
        let unix_100ns = self.UniversalTime - WINDOWS_EPOCH_OFFSET_100NS;
        let seconds = unix_100ns.div_euclid(10_000_000);
        let nanos = (unix_100ns.rem_euclid(10_000_000) * 100) as u32;
        chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
            .map(|value| value.to_rfc3339())
            .ok_or_else(|| "Failed to convert clipboard timestamp".to_string())
    }
}

fn infer_mime_type(data_url: &str, extension: &str) -> String {
    data_url
        .strip_prefix("data:")
        .and_then(|value| value.split_once(';').map(|(mime, _)| mime))
        .filter(|mime| mime.starts_with("image/"))
        .map(str::to_string)
        .unwrap_or_else(|| match extension {
            "png" => "image/png".to_string(),
            "jpg" => "image/jpeg".to_string(),
            "gif" => "image/gif".to_string(),
            _ => "image/webp".to_string(),
        })
}
