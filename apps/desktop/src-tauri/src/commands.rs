use crate::db::{ClipboardItem, Database, Note, NoteSummary, NoteVersion};
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

#[derive(Clone)]
pub struct AppPaths {
    pub attachments_dir: PathBuf,
}

#[derive(Default)]
pub struct ClipboardCaptureState {
    fingerprint: Mutex<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct Attachment {
    pub id: String,
    pub path: String,
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
pub fn search_notes(db: State<'_, Arc<Database>>, query: String) -> Result<Vec<NoteSummary>, String> {
    db.search_notes(&query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_versions(db: State<'_, Arc<Database>>, id: String) -> Result<Vec<NoteVersion>, String> {
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
    db.delete_note_version(version_id).map_err(|e| e.to_string())
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
pub fn cleanup_attachments(db: State<'_, Arc<Database>>, paths: State<'_, Arc<AppPaths>>) -> Result<usize, String> {
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
pub fn capture_clipboard(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
    capture_state: State<ClipboardCaptureState>,
) -> Result<Option<ClipboardItem>, String> {
    let content = match app.clipboard().read_text() {
        Ok(text) if !text.trim().is_empty() => text,
        _ => match read_clipboard_image_html(&app) {
            Ok(Some(html)) => html,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        },
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let fingerprint = format!("{:x}", Sha256::digest(format!("clipboard:text:{normalized}")));
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
        .map_err(|_| "clipboard capture state is unavailable".to_string())? =
        Some(format!("{:x}", Sha256::digest(format!("clipboard:text:{}", item.content))));
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

fn read_clipboard_image_html(app: &AppHandle) -> Result<Option<String>, String> {
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
        .write_image(bytes, image.width(), image.height(), ColorType::Rgba8.into())
        .map_err(|e| format!("Failed to encode clipboard image: {e}"))?;
    let data_url = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(png));
    Ok(Some(format!(
        r#"<img src="{data_url}" alt="剪贴板图片" title="剪贴板图片">"#
    )))
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
