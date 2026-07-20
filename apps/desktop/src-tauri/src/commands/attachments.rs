use super::*;

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
pub(super) fn infer_extension(data_url: &str, filename: &str) -> String {
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

pub(super) fn save_attachment_bytes(
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

pub(super) fn infer_mime_type(data_url: &str, extension: &str) -> String {
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
