use crate::db::{
    ClipboardItem, Database, DatabaseState, Note, NoteSummary, NoteVersion, TagSummary,
};
use crate::sync::{
    SyncConfig, SyncConfigInput, SyncReport, SyncService, WebDavGcReport, WebDavStorageStatus,
};
use base64::{engine::general_purpose, Engine as _};
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicU32;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
#[cfg(target_os = "windows")]
use windows::{
    core::IInspectable,
    ApplicationModel::DataTransfer::{Clipboard, DataPackageView, StandardDataFormats},
    Foundation::{EventHandler, Uri},
};

#[derive(Clone)]
pub struct AppPaths {
    pub attachments_dir: PathBuf,
}

#[derive(Clone, Default)]
pub struct ClipboardCaptureState {
    fingerprint: Arc<Mutex<Option<String>>>,
    accept_next_duplicate: Arc<AtomicBool>,
    enabled: Arc<AtomicBool>,
    initialized: Arc<AtomicBool>,
    suppress_events: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
pub struct ClipboardSyncResult {
    pub captured: usize,
}

#[derive(Debug, Serialize)]
pub struct Attachment {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct AttachmentDataUrl {
    pub id: String,
    pub data_url: String,
}

mod attachments;
mod clipboard;
mod clipboard_support;
mod notes;
mod sync_commands;

pub use attachments::*;
pub use clipboard::*;
use clipboard_support::*;
pub use notes::*;
pub use sync_commands::*;
