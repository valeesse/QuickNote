use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{
    CreateNoteRequest, Note, NoteSummary, NoteVersion, ReorderNotesRequest, TagSummary,
    UpdateNoteRequest, UpdateNoteTagsRequest,
};
use crate::routes::access::version_history_cutoff;
use crate::routes::attachments::delete_attachment_object;
use crate::routes::sync::{append_change, ChangePayload};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::OnceLock;
use uuid::Uuid;

pub(super) const NOTE_COLUMNS: &str =
    "id,title,content,yjs_state,yjs_state_version,is_pinned,sort_order,created_at,updated_at,version,is_deleted,
     COALESCE((SELECT array_agg(t.name ORDER BY lower(t.name))
        FROM note_tags nt
        JOIN tags t ON t.user_id=notes.user_id AND t.id=nt.tag_id
        WHERE nt.user_id=notes.user_id AND nt.note_id=notes.id AND t.is_deleted=false), ARRAY[]::TEXT[]) AS tags";

mod core;
mod helpers;
mod mutations;
mod search_trash;
mod tags;
mod versions;

pub use core::*;
use helpers::*;
pub use mutations::*;
pub use search_trash::*;
pub use tags::*;
pub use versions::*;

#[cfg(test)]
mod tests;
