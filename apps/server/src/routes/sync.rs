use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{
    AttachmentRecord, CausalRelation, CausalVersion, ClipboardItem, CloudChange, Note, NoteTag,
    PullRequest, PullResponse, PushRequest, PushResponse, SyncEnvelope, SyncEvent, Tag,
};
use crate::routes::access::ensure_device_allowed;
use crate::AppState;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream::Stream;
use sqlx::{Postgres, Transaction};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

mod apply;
mod changes;
mod events;
mod transfer;
mod validation;

use apply::*;
pub use changes::*;
pub use events::*;
pub use transfer::*;
use validation::*;

#[cfg(test)]
mod tests;
