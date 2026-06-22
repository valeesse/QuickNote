mod commands;
mod db;
mod sync;

use commands::{AppPaths, ClipboardCaptureState};
use db::Database;
use sync::SyncService;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Get app data directory for SQLite database
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");

            // Create directory if it doesn't exist
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");

            // Initialize database
            let database = Database::new(app_dir.clone()).expect("failed to initialize database");
            app.manage(database);
            app.manage(SyncService::new(app_dir.join("sync.json")));
            app.manage(ClipboardCaptureState::default());

            let attachments_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir")
                .join("attachments");
            std::fs::create_dir_all(&attachments_dir).expect("failed to create attachments dir");
            app.manage(AppPaths { attachments_dir });

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                // Register global shortcut: Ctrl+Alt+N to show/focus window
                use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};
                let handle = app.handle().clone();
                app.handle()
                    .plugin(
                        ShortcutBuilder::new()
                            .with_shortcuts(["CmdOrCtrl+Alt+N"])
                            .expect("failed to register shortcut")
                            .with_handler(move |_app, _shortcut, event| {
                                if event.state == ShortcutState::Pressed {
                                    if let Some(window) = handle.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                            })
                            .build(),
                    )
                    .expect("failed to register global shortcut plugin");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_note,
            commands::get_note,
            commands::list_notes,
            commands::update_note,
            commands::delete_note,
            commands::restore_note,
            commands::purge_note,
            commands::list_deleted_notes,
            commands::toggle_pin,
            commands::search_notes,
            commands::get_note_versions,
            commands::restore_note_version,
            commands::toggle_version_pin,
            commands::save_attachment,
            commands::get_attachment,
            commands::cleanup_attachments,
            commands::get_sync_config,
            commands::set_sync_config,
            commands::sync_now,
            commands::clipboard_auto_capture_supported,
            commands::capture_clipboard,
            commands::list_clipboard_items,
            commands::copy_clipboard_item,
            commands::toggle_clipboard_pin,
            commands::delete_clipboard_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
