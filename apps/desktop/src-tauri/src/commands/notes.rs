use super::*;

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
    yjs_state: Option<Vec<u8>>,
) -> Result<Option<Note>, String> {
    db.update_note_with_yjs(&id, &content, yjs_state.as_deref())
        .map_err(|e| e.to_string())
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
