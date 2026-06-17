use crate::db::{Database, Note, NoteSummary, NoteVersion};
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

pub struct AppPaths {
    pub attachments_dir: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct Attachment {
    pub path: String,
}

#[tauri::command]
pub fn create_note(db: State<Database>, content: String) -> Result<Note, String> {
    db.create_note(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note(db: State<Database>, id: String) -> Result<Option<Note>, String> {
    db.get_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notes(db: State<Database>) -> Result<Vec<NoteSummary>, String> {
    db.list_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_note(db: State<Database>, id: String, content: String) -> Result<Option<Note>, String> {
    db.update_note(&id, &content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(db: State<Database>, id: String) -> Result<bool, String> {
    db.delete_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_note(db: State<Database>, id: String) -> Result<bool, String> {
    db.restore_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn purge_note(db: State<Database>, id: String) -> Result<bool, String> {
    db.purge_note(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_deleted_notes(db: State<Database>) -> Result<Vec<NoteSummary>, String> {
    db.list_deleted_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_pin(db: State<Database>, id: String) -> Result<bool, String> {
    db.toggle_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_notes(db: State<Database>, query: String) -> Result<Vec<NoteSummary>, String> {
    db.search_notes(&query).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_note_versions(db: State<Database>, id: String) -> Result<Vec<NoteVersion>, String> {
    db.get_note_versions(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_note_version(
    db: State<Database>,
    note_id: String,
    version_id: i64,
) -> Result<Option<Note>, String> {
    db.restore_note_version(&note_id, version_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_version_pin(db: State<Database>, version_id: i64) -> Result<bool, String> {
    db.toggle_version_pin(version_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_attachment(
    paths: State<AppPaths>,
    data_url: String,
    filename: String,
) -> Result<Attachment, String> {
    let (_, payload) = data_url
        .split_once(',')
        .ok_or_else(|| "Invalid data URL".to_string())?;
    let bytes = general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("Invalid attachment payload: {e}"))?;

    let extension = infer_extension(&data_url, &filename);
    let id = uuid::Uuid::new_v4().to_string();
    let path = paths.attachments_dir.join(format!("{id}.{extension}"));
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to save attachment: {e}"))?;

    Ok(Attachment {
        path: path.to_string_lossy().to_string(),
    })
}

fn infer_extension(data_url: &str, filename: &str) -> String {
    let from_name = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp"));

    if let Some(ext) = from_name {
        return if ext == "jpeg" { "jpg".to_string() } else { ext };
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
