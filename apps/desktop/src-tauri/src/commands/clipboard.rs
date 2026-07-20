use super::*;

#[tauri::command]
pub fn clipboard_auto_capture_supported() -> bool {
    cfg!(not(any(target_os = "android", target_os = "ios")))
}

#[tauri::command]
pub fn set_clipboard_auto_capture_enabled(
    app: AppHandle,
    capture_state: State<'_, ClipboardCaptureState>,
    enabled: bool,
) -> Result<bool, String> {
    if !capture_state.initialized.swap(true, Ordering::AcqRel) {
        if let Ok(content) = app.clipboard().read_text() {
            if !content.trim().is_empty() {
                let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
                *capture_state
                    .fingerprint
                    .lock()
                    .map_err(|_| "clipboard capture state is unavailable".to_string())? =
                    Some(clipboard_fingerprint(&normalized));
            }
        }
    }
    capture_state.enabled.store(enabled, Ordering::Release);
    Ok(enabled)
}

#[tauri::command]
pub fn sync_clipboard_history(
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
) -> Result<ClipboardSyncResult, String> {
    #[cfg(target_os = "windows")]
    {
        let device_id = sync.get_config()?.device_id;
        let captured =
            tauri::async_runtime::block_on(capture_windows_clipboard_history(&db, &device_id))?;
        Ok(ClipboardSyncResult { captured })
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = db;
        let _ = sync;
        Ok(ClipboardSyncResult { captured: 0 })
    }
}

#[tauri::command]
pub fn capture_clipboard(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    sync: State<'_, Arc<SyncService>>,
    paths: State<'_, Arc<AppPaths>>,
    capture_state: State<'_, ClipboardCaptureState>,
) -> Result<Option<ClipboardItem>, String> {
    let content = match app.clipboard().read_text() {
        Ok(text) if !text.trim().is_empty() => text,
        _ => match read_clipboard_image_html(&app, &db, &paths) {
            Ok(Some(html)) => html,
            Ok(None) => return Ok(None),
            Err(error) => return Err(error),
        },
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    let device_id = sync.get_config()?.device_id;
    let item = capture_content_if_new(&db, &device_id, &capture_state, &content, None)?;
    if let Some(ref item) = item {
        let _ = app.emit("clipboard-captured", item);
    }
    Ok(item)
}

#[tauri::command]
pub fn prime_clipboard_capture(
    app: AppHandle,
    capture_state: State<'_, ClipboardCaptureState>,
) -> Result<bool, String> {
    let content = match app.clipboard().read_text() {
        Ok(text) if !text.trim().is_empty() => text,
        _ => return Ok(false),
    };
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    *capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())? = Some(format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{normalized}"))
    ));
    Ok(true)
}

#[tauri::command]
pub fn list_clipboard_items(
    db: State<'_, Arc<Database>>,
    query: String,
) -> Result<Vec<ClipboardItem>, String> {
    db.list_clipboard_items(&query, 500)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_clipboard_item(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    capture_state: State<ClipboardCaptureState>,
    id: String,
) -> Result<bool, String> {
    let Some(item) = db.get_clipboard_item(&id).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    capture_state.suppress_events.store(true, Ordering::Release);
    let result = (|| {
        let plain_text = clipboard_plain_text(&item);
        *capture_state
            .fingerprint
            .lock()
            .map_err(|_| "clipboard capture state is unavailable".to_string())? =
            Some(clipboard_fingerprint(&plain_text));
        app.clipboard()
            .write_text(plain_text.clone())
            .map_err(|e| e.to_string())?;
        if item.kind == "rich" || item.kind == "image" {
            *capture_state
                .fingerprint
                .lock()
                .map_err(|_| "clipboard capture state is unavailable".to_string())? =
                Some(clipboard_fingerprint(&item.content));
            let _ = app
                .clipboard()
                .write_html(item.content.clone(), Some(plain_text));
        }
        Ok(true)
    })();
    capture_state
        .suppress_events
        .store(false, Ordering::Release);
    result
}

pub fn start_clipboard_monitor(
    app: AppHandle,
    db: Arc<Database>,
    device_id: String,
    capture_state: ClipboardCaptureState,
) {
    #[cfg(not(target_os = "windows"))]
    {
        let poll_app = app.clone();
        let poll_db = db.clone();
        let poll_device_id = device_id.clone();
        let poll_state = capture_state.clone();
        tauri::async_runtime::spawn(async move {
            let mut timer = tokio::time::interval(std::time::Duration::from_millis(400));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                if !poll_state.enabled.load(Ordering::Acquire)
                    || poll_state.suppress_events.load(Ordering::Acquire)
                {
                    continue;
                }
                let Ok(content) = poll_app.clipboard().read_text() else {
                    continue;
                };
                if content.trim().is_empty() {
                    continue;
                }
                if let Ok(Some(item)) =
                    capture_content_if_new(&poll_db, &poll_device_id, &poll_state, &content, None)
                {
                    let _ = poll_app.emit("clipboard-captured", item);
                }
            }
        });
    }

    #[cfg(target_os = "windows")]
    {
        let (sender, receiver) = std::sync::mpsc::channel::<DataPackageView>();
        let worker_app = app.clone();
        let worker_db = db;
        let worker_state = capture_state.clone();
        let worker_device_id = device_id;
        std::thread::spawn(move || {
            while let Ok(package) = receiver.recv() {
                if !worker_state.enabled.load(Ordering::Acquire)
                    || worker_state.suppress_events.load(Ordering::Acquire)
                {
                    continue;
                }
                let Ok(Some(content)) =
                    tauri::async_runtime::block_on(read_windows_data_package_content(&package))
                else {
                    continue;
                };
                if let Ok(Some(item)) = capture_content_if_new(
                    &worker_db,
                    &worker_device_id,
                    &worker_state,
                    &content,
                    None,
                ) {
                    let _ = worker_app.emit("clipboard-captured", item);
                }
            }
        });
        let event_state = capture_state;
        let handler = EventHandler::<IInspectable>::new(move |_, _| {
            if !event_state.enabled.load(Ordering::Acquire)
                || event_state.suppress_events.load(Ordering::Acquire)
            {
                return Ok(());
            }
            // GetContent returns a snapshot. Taking it inside the event callback preserves
            // intermediate values even when the user copies several items very quickly.
            if let Ok(package) = Clipboard::GetContent() {
                let _ = sender.send(package);
            }
            Ok(())
        });
        if let Err(error) = Clipboard::ContentChanged(&handler) {
            eprintln!("failed to subscribe to clipboard changes: {error}");
        }
    }
}

fn capture_content_if_new(
    db: &Database,
    device_id: &str,
    capture_state: &ClipboardCaptureState,
    content: &str,
    captured_at: Option<&str>,
) -> Result<Option<ClipboardItem>, String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return Ok(None);
    }
    let fingerprint = clipboard_fingerprint(&normalized);
    let mut current = capture_state
        .fingerprint
        .lock()
        .map_err(|_| "clipboard capture state is unavailable".to_string())?;
    if current.as_ref() == Some(&fingerprint) {
        return Ok(None);
    }
    let item = match captured_at {
        Some(timestamp) => db.capture_clipboard_at(content, device_id, timestamp),
        None => db.capture_clipboard(content, device_id),
    }
    .map_err(|e| e.to_string())?;
    *current = Some(fingerprint);
    Ok(Some(item))
}

fn clipboard_fingerprint(normalized: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("clipboard:text:{normalized}"))
    )
}

#[tauri::command]
pub fn toggle_clipboard_pin(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.toggle_clipboard_pin(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_clipboard_item(db: State<'_, Arc<Database>>, id: String) -> Result<bool, String> {
    db.delete_clipboard_item(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_clipboard(db: State<'_, Arc<Database>>) -> Result<usize, String> {
    db.clear_clipboard_items().map_err(|e| e.to_string())
}
