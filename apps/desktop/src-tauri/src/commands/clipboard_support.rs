use super::*;

const MAX_CLIPBOARD_IMAGE_RGBA_BYTES: usize = 64 * 1024 * 1024;

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
    if !clipboard_image_buffer_is_supported(image.width(), image.height(), bytes.len()) {
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

fn clipboard_image_buffer_is_supported(width: u32, height: u32, byte_len: usize) -> bool {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    width > 0
        && height > 0
        && expected_len == Some(byte_len)
        && byte_len <= MAX_CLIPBOARD_IMAGE_RGBA_BYTES
}

#[cfg(target_os = "windows")]
pub(super) fn read_windows_clipboard_image_html_with_retry(
    app: &AppHandle,
    db: &Database,
    paths: &AppPaths,
) -> Result<Option<String>, String> {
    const ATTEMPTS: usize = 6;
    for attempt in 0..ATTEMPTS {
        if let Some(content) = read_windows_native_png_html(db, paths)? {
            return Ok(Some(content));
        }
        match read_clipboard_image_html(app, db, paths) {
            Ok(Some(content)) => return Ok(Some(content)),
            Err(error) if attempt + 1 == ATTEMPTS => return Err(error),
            _ if attempt + 1 < ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => {}
        }
    }
    Ok(None)
}

#[cfg(target_os = "windows")]
fn read_windows_native_png_html(db: &Database, paths: &AppPaths) -> Result<Option<String>, String> {
    use clipboard_win::{formats::RawData, register_format, Clipboard as WinClipboard, Getter};

    let Some(format) = register_format("PNG") else {
        return Ok(None);
    };
    let Ok(_clipboard) = WinClipboard::new_attempts(5) else {
        return Ok(None);
    };
    let mut png = Vec::new();
    if RawData(format.get()).read_clipboard(&mut png).is_err() {
        return Ok(None);
    }
    if png.len() < 8 || png.len() > 20 * 1024 * 1024 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Ok(None);
    }
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

pub(super) fn clipboard_html_for_system(
    item: &ClipboardItem,
    db: &Database,
    paths: &AppPaths,
) -> Result<String, String> {
    let mut html = item.content.clone();
    for id in crate::db::clipboard_attachment_ids(&item.content) {
        let record = db
            .get_attachment(&id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Clipboard attachment {id} was not found"))?;
        let bytes = std::fs::read(paths.attachments_dir.join(record.relative_path))
            .map_err(|e| format!("Failed to read rich clipboard attachment: {e}"))?;
        let data_url = format!(
            "data:{};base64,{}",
            record.mime_type,
            general_purpose::STANDARD.encode(bytes)
        );
        html = html.replace(&format!("attachment://{id}"), &data_url);
    }
    Ok(html)
}

pub(super) fn write_clipboard_image(
    app: &AppHandle,
    db: &Database,
    paths: &AppPaths,
    content: &str,
) -> Result<bool, String> {
    let Some(id) = attachment_id_from_image_html(content) else {
        return Ok(false);
    };
    let record = db
        .get_attachment(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Clipboard image attachment was not found".to_string())?;
    if !record.mime_type.starts_with("image/") {
        return Err("Clipboard attachment is not an image".to_string());
    }
    let bytes = std::fs::read(paths.attachments_dir.join(record.relative_path))
        .map_err(|e| format!("Failed to read clipboard image: {e}"))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode clipboard image: {e}"))?
        .into_rgba8();
    let (width, height) = decoded.dimensions();
    let image = tauri::image::Image::new_owned(decoded.into_raw(), width, height);
    app.clipboard()
        .write_image(&image)
        .map_err(|e| format!("Failed to write clipboard image: {e}"))?;
    Ok(true)
}

fn attachment_id_from_image_html(content: &str) -> Option<&str> {
    let start = content.find("attachment://")? + "attachment://".len();
    let id = content[start..]
        .split(|ch: char| !ch.is_ascii_hexdigit())
        .next()?;
    (id.len() == 64).then_some(id)
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
pub(super) fn windows_data_package_contains_bitmap(package: &DataPackageView) -> bool {
    StandardDataFormats::Bitmap()
        .ok()
        .and_then(|format| package.Contains(&format).ok())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
pub(super) fn read_preferred_windows_package_content(
    package: &DataPackageView,
    app: &AppHandle,
    db: &Database,
    paths: &AppPaths,
) -> Result<Option<String>, String> {
    let packaged = tauri::async_runtime::block_on(read_windows_data_package_content(package))?;
    if !windows_data_package_contains_bitmap(package) {
        // Some screenshot tools publish only native DIB/PNG clipboard formats.
        // Windows clipboard history can display those images, but WinRT does not
        // necessarily expose them as StandardDataFormats::Bitmap. If the package
        // has no usable HTML/text/URI, probe the native clipboard image anyway.
        return match packaged {
            Some(content) => Ok(Some(content)),
            None => read_windows_clipboard_image_html_with_retry(app, db, paths),
        };
    }

    if packaged
        .as_deref()
        .is_some_and(clipboard_html_has_meaningful_text)
    {
        return Ok(packaged);
    }

    match read_windows_clipboard_image_html_with_retry(app, db, paths)? {
        Some(image) => Ok(Some(image)),
        None => Ok(packaged),
    }
}

#[cfg(target_os = "windows")]
fn clipboard_html_has_meaningful_text(content: &str) -> bool {
    if !looks_like_visual_clipboard_html(content) {
        return false;
    }
    let text = strip_html_tags(content);
    let without_spacing_entities = text
        .replace("&nbsp;", "")
        .replace("&#32;", "")
        .replace("&#160;", "")
        .replace("&#x20;", "")
        .replace("&#xa0;", "");
    !without_spacing_entities.trim().is_empty()
}

#[cfg(target_os = "windows")]
pub(super) fn normalize_windows_clipboard_html(content: &str) -> String {
    if let Some((_, fragment)) = content.split_once("<!--StartFragment-->") {
        if let Some((fragment, _)) = fragment.split_once("<!--EndFragment-->") {
            return fragment.trim().to_string();
        }
    }

    if let Some(fragment) = clipboard_html_range(content, "StartFragment:", "EndFragment:") {
        return fragment.trim().to_string();
    }
    if let Some(html) = clipboard_html_range(content, "StartHTML:", "EndHTML:") {
        return html.trim().to_string();
    }
    content.trim().to_string()
}

#[cfg(target_os = "windows")]
fn clipboard_html_range<'a>(content: &'a str, start_key: &str, end_key: &str) -> Option<&'a str> {
    let offset = |key: &str| {
        content
            .lines()
            .find_map(|line| line.trim().strip_prefix(key))
            .and_then(|value| value.trim().parse::<usize>().ok())
    };
    let start = offset(start_key)?;
    let end = offset(end_key)?;
    if start > end || end > content.len() {
        return None;
    }
    content.get(start..end)
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
        || lowered.contains("<strong")
        || lowered.contains("<span")
        || lowered.contains("<em")
        || lowered.contains("<b>")
        || lowered.contains("<i>")
        || lowered.contains("<u>")
        || lowered.contains("<s>")
        || lowered.contains("<mark")
        || lowered.contains("<sub")
        || lowered.contains("<sup")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_attachment_id_from_clipboard_image_html() {
        let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let html = format!(r#"<img src="attachment://{id}" data-attachment-id="{id}">"#);
        assert_eq!(attachment_id_from_image_html(&html), Some(id));
    }

    #[test]
    fn accepts_full_screen_clipboard_images_but_rejects_oversized_buffers() {
        let full_screen_len = 2400 * 1599 * 4;
        assert!(clipboard_image_buffer_is_supported(
            2400,
            1599,
            full_screen_len
        ));
        assert!(!clipboard_image_buffer_is_supported(
            5000,
            5000,
            5000 * 5000 * 4
        ));
        assert!(!clipboard_image_buffer_is_supported(2400, 1599, 12));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn strips_cf_html_header_using_fragment_byte_offsets() {
        let fragment = "<pre>mkdir&#32;-p&#32;data/postgres</pre>";
        let header_template = concat!(
            "Version:1.0\r\n",
            "StartHTML:{start:010}\r\n",
            "EndHTML:{end:010}\r\n",
            "StartFragment:{start:010}\r\n",
            "EndFragment:{end:010}\r\n",
            "SourceURL:about:blank\r\n"
        );
        let provisional = header_template
            .replace("{start:010}", "0000000000")
            .replace("{end:010}", "0000000000");
        let start = provisional.len();
        let end = start + fragment.len();
        let header = header_template
            .replace("{start:010}", &format!("{start:010}"))
            .replace("{end:010}", &format!("{end:010}"));
        let cf_html = format!("{header}{fragment}");

        assert_eq!(normalize_windows_clipboard_html(&cf_html), fragment);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn distinguishes_mixed_rich_content_from_an_image_wrapper() {
        assert!(clipboard_html_has_meaningful_text(
            "<p>说明文字</p><img src=\"https://example.com/image.png\">"
        ));
        assert!(!clipboard_html_has_meaningful_text(
            "<div>&nbsp;</div><img src=\"https://example.com/image.png\">"
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn preserves_single_inline_formatted_html_from_word_or_a_browser() {
        assert!(looks_like_visual_clipboard_html(
            "<p><span style=\"font-weight:bold;color:red\">格式文本</span></p>"
        ));
        assert!(looks_like_visual_clipboard_html("<strong>粗体</strong>"));
    }
}
