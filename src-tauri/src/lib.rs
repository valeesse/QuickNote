mod db;
mod commands;

use commands::AppPaths;
use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Get app data directory for SQLite database
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");

            // Create directory if it doesn't exist
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");

            // Initialize database
            let database = Database::new(app_dir).expect("failed to initialize database");
            app.manage(database);

            let attachments_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir")
                .join("attachments");
            std::fs::create_dir_all(&attachments_dir).expect("failed to create attachments dir");
            app.manage(AppPaths { attachments_dir });

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
