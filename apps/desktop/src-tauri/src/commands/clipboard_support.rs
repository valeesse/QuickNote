use super::*;

pub(super) fn read_clipboard_image_html(
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

pub(super) fn clipboard_plain_text(item: &ClipboardItem) -> String {
    if item.kind == "rich" || item.kind == "image" {
        strip_html_tags(&item.content)
    } else {
        item.content.clone()
    }
}

pub(super) fn strip_html_tags(content: &str) -> String {
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
pub(super) async fn capture_windows_clipboard_history(
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
pub(super) async fn read_windows_history_item_content(
    item: &windows::ApplicationModel::DataTransfer::ClipboardHistoryItem,
) -> Result<Option<String>, String> {
    let package = item
        .Content()
        .map_err(|e| format!("Failed to read clipboard history package: {e}"))?;
    read_windows_data_package_content(&package).await
}

#[cfg(target_os = "windows")]
pub(super) async fn read_windows_data_package_content(
    package: &DataPackageView,
) -> Result<Option<String>, String> {
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
pub(super) fn normalize_windows_clipboard_html(content: &str) -> String {
    if let Some((_, fragment)) = content.split_once("<!--StartFragment-->") {
        if let Some((fragment, _)) = fragment.split_once("<!--EndFragment-->") {
            return fragment.trim().to_string();
        }
    }
    content.trim().to_string()
}

#[cfg(target_os = "windows")]
pub(super) fn looks_like_visual_clipboard_html(content: &str) -> bool {
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
