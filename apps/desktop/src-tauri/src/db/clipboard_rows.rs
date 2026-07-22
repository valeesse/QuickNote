use super::*;

pub(super) fn attachment_from_row(row: &rusqlite::Row<'_>) -> Result<AttachmentRecord> {
    Ok(AttachmentRecord {
        id: row.get(0)?,
        relative_path: row.get(1)?,
        mime_type: row.get(2)?,
        size: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub(super) fn clipboard_item_from_row(row: &rusqlite::Row<'_>) -> Result<ClipboardItem> {
    Ok(ClipboardItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        content: row.get(2)?,
        preview: row.get(3)?,
        source_device: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        last_copied_at: row.get(7)?,
        capture_count: row.get(8)?,
        is_pinned: row.get(9)?,
        is_deleted: row.get(10)?,
    })
}

pub(super) fn get_clipboard_item_locked(
    conn: &Connection,
    id: &str,
    include_deleted: bool,
) -> Result<Option<ClipboardItem>> {
    conn.query_row(
        "SELECT id, kind, content, preview, source_device, created_at, updated_at,
                last_copied_at, capture_count, is_pinned, is_deleted
         FROM clipboard_items WHERE id = ?1 AND (?2 = 1 OR is_deleted = 0)",
        params![id, include_deleted],
        clipboard_item_from_row,
    )
    .optional()
}

pub(super) fn upsert_remote_clipboard_locked(
    conn: &Connection,
    item: &ClipboardItem,
) -> Result<()> {
    conn.execute(
        "INSERT INTO clipboard_items(
            id, kind, content, preview, source_device, created_at, updated_at,
            last_copied_at, capture_count, is_pinned, is_deleted
         ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            kind = excluded.kind,
            content = excluded.content,
            preview = excluded.preview,
            source_device = excluded.source_device,
            updated_at = excluded.updated_at,
            last_copied_at = excluded.last_copied_at,
            capture_count = excluded.capture_count,
            is_pinned = excluded.is_pinned,
            is_deleted = excluded.is_deleted",
        params![
            item.id,
            item.kind,
            item.content,
            item.preview,
            item.source_device,
            item.created_at,
            item.updated_at,
            item.last_copied_at,
            item.capture_count,
            item.is_pinned,
            item.is_deleted,
        ],
    )?;
    Ok(())
}

pub(super) fn normalize_clipboard_content(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

pub(super) fn classify_clipboard_content(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.starts_with("data:image/")
        || (trimmed.starts_with("<img ") && strip_html_tags(trimmed).is_empty())
    {
        "image".to_string()
    } else if looks_like_rich_clipboard(content) {
        "rich".to_string()
    } else if (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
        && !trimmed.chars().any(char::is_whitespace)
    {
        "link".to_string()
    } else if content.contains('\n')
        && ["{", "};", "=>", "def ", "fn ", "class ", "import "]
            .iter()
            .any(|needle| content.contains(needle))
    {
        "code".to_string()
    } else {
        "text".to_string()
    }
}

pub(super) fn looks_like_rich_clipboard(content: &str) -> bool {
    let lowered = content.to_ascii_lowercase();
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
        || count_html_block_tags(&lowered) > 1
}

pub(super) fn count_html_block_tags(lowered: &str) -> usize {
    [
        "<p", "<div", "<section", "<article", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6",
    ]
    .iter()
    .map(|needle| lowered.matches(needle).count())
    .sum()
}

pub(super) fn clipboard_preview(content: &str, kind: &str) -> String {
    if kind == "image" {
        return "图片".to_string();
    }
    let preview = if kind == "rich" {
        strip_html_tags(content)
    } else {
        content.replace('\n', " ")
    };
    truncate_chars(&preview, 240)
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
