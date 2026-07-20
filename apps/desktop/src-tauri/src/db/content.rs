use super::*;

pub(super) fn extract_title(content: &str) -> String {
    let plain_text = html_to_text(content);
    let title = plain_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim_start_matches('#')
        .trim();

    if title.is_empty() {
        "无标题".to_string()
    } else {
        truncate_chars(title, 100)
    }
}

pub(super) fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;

    if !columns.iter().any(|name| name == column) {
        conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
            [],
        )?;
    }

    Ok(())
}

pub(super) fn make_preview_without_title(content: &str) -> String {
    let text = html_to_text(content);
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let _title = lines.next();
    let body = lines.collect::<Vec<_>>().join(" ");

    if body.is_empty() {
        String::new()
    } else {
        truncate_chars(&body, 200)
    }
}

pub(super) fn prune_unpinned_versions(conn: &Connection, note_id: &str, limit: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM note_versions
         WHERE note_id = ?1
           AND is_pinned = 0
           AND id NOT IN (
             SELECT id FROM note_versions
             WHERE note_id = ?1 AND is_pinned = 0
             ORDER BY created_at DESC, id DESC
             LIMIT ?2
           )",
        params![note_id, limit],
    )?;
    Ok(())
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub(super) fn normalize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|term| {
            let cleaned = term.trim_matches(|c: char| c.is_ascii_punctuation() && c != '_');
            if cleaned.is_empty() {
                None
            } else {
                Some(format!("\"{}\"*", cleaned.replace('"', "\"\"")))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn html_to_text(content: &str) -> String {
    let mut text = String::with_capacity(content.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    let mut tag_buf = String::new();

    for ch in content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buf.clear();
                push_space(&mut text, &mut last_was_space);
            }
            '>' if in_tag => {
                in_tag = false;
                let tag = tag_buf.trim().to_ascii_lowercase();
                if tag.starts_with("img") {
                    push_word(&mut text, &mut last_was_space, "[图片]");
                } else if tag.starts_with("br")
                    || tag.starts_with("/p")
                    || tag.starts_with("/h")
                    || tag.starts_with("/li")
                    || tag.starts_with("/div")
                {
                    push_line_break(&mut text, &mut last_was_space);
                }
            }
            _ if in_tag => tag_buf.push(ch),
            c if c.is_whitespace() => push_space(&mut text, &mut last_was_space),
            c => {
                text.push(c);
                last_was_space = false;
            }
        }
    }

    decode_basic_entities(text.trim()).to_string()
}

pub(super) fn push_space(text: &mut String, last_was_space: &mut bool) {
    if !*last_was_space && !text.is_empty() {
        text.push(' ');
        *last_was_space = true;
    }
}

pub(super) fn push_word(text: &mut String, last_was_space: &mut bool, word: &str) {
    push_space(text, last_was_space);
    text.push_str(word);
    *last_was_space = false;
}

pub(super) fn push_line_break(text: &mut String, last_was_space: &mut bool) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
        *last_was_space = true;
    }
}

pub(super) fn decode_basic_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

pub(super) fn merge_collaborative_notes(local: &Note, remote: &Note) -> Result<Option<Note>> {
    use yrs::updates::decoder::Decode;
    use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

    let (Some(local_state), Some(remote_state)) = (&local.yjs_state, &remote.yjs_state) else {
        return Ok(None);
    };
    let doc = Doc::new();
    for bytes in [local_state, remote_state] {
        let update = Update::decode_v1(bytes).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!("invalid Yjs state: {error}"))
        })?;
        doc.transact_mut().apply_update(update).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!("cannot merge Yjs state: {error}"))
        })?;
    }
    let mut merged = remote.clone();
    merged.yjs_state = Some(
        doc.transact()
            .encode_state_as_update_v1(&StateVector::default()),
    );
    merged.yjs_state_version = local.yjs_state_version.max(remote.yjs_state_version) + 1;
    Ok(Some(merged))
}
