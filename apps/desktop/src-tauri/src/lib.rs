mod commands;
mod db;
mod sync;

use commands::{AppPaths, ClipboardCaptureState};
use db::Database;
use std::sync::Arc;
use sync::SyncService;
use tauri::Manager;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let new_note = MenuItem::with_id(app, "new_note", "新建快速便签", true, None::<&str>)?;
    let clipboard = MenuItem::with_id(app, "clipboard", "剪贴板历史", true, None::<&str>)?;
    let sync_now = MenuItem::with_id(app, "sync", "立即同步", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &new_note, &clipboard, &sync_now, &sep, &quit])?;

    // Clone Arcs for the menu event closure (owned, not borrowed from app state)
    let sync_svc: Arc<SyncService> = app.state::<Arc<SyncService>>().inner().clone();
    let db_arc: Arc<Database> = app.state::<Arc<Database>>().inner().clone();
    let paths_arc: Arc<AppPaths> = app.state::<Arc<AppPaths>>().inner().clone();

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("QuickNote")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |_app_handle, event| match event.id.as_ref() {
            "show" => show_main_window(_app_handle),
            "new_note" => open_popup(_app_handle, "quick-note"),
            "clipboard" => open_popup(_app_handle, "clipboard-popup"),
            "sync" => {
                let sync_svc = sync_svc.clone();
                let db = db_arc.clone();
                let paths = paths_arc.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = sync_svc.sync(&db, &paths.attachments_dir).await;
                });
            }
            "quit" => {
                _app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn open_popup(app: &tauri::AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let screen = monitor.size();
            let origin = monitor.position();
            let scale = monitor.scale_factor();
            let (win_w, win_h) = match label {
                "clipboard-popup" => (400.0, 600.0),
                "quick-note" => (500.0, 400.0),
                _ => (400.0, 500.0),
            };
            let x = (origin.x as f64 / scale) + (screen.width as f64 / scale) - win_w - 20.0;
            let y = (origin.y as f64 / scale) + (screen.height as f64 / scale) - win_h - 60.0;
            let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");

            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");

            let database = Database::new(app_dir.clone()).expect("failed to initialize database");
            let db_arc = Arc::new(database);
            app.manage(db_arc);

            let sync_service = Arc::new(SyncService::new(app_dir.join("sync.json")));
            app.manage(sync_service);

            app.manage(ClipboardCaptureState::default());

            let attachments_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir")
                .join("attachments");
            std::fs::create_dir_all(&attachments_dir).expect("failed to create attachments dir");
            app.manage(Arc::new(AppPaths { attachments_dir }));

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                // Setup system tray
                setup_tray(app).expect("failed to setup system tray");

                // Register global shortcuts
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
                };

                let shortcut_n =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyN);
                let shortcut_c =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyC);
                let shortcut_q =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyQ);

                app.handle()
                    .plugin(
                        tauri_plugin_global_shortcut::Builder::new()
                            .with_handler(move |app, shortcut, event| {
                                if event.state != ShortcutState::Pressed {
                                    return;
                                }
                                if shortcut == &shortcut_n {
                                    open_popup(app, "quick-note");
                                } else if shortcut == &shortcut_c {
                                    open_popup(app, "clipboard-popup");
                                } else if shortcut == &shortcut_q {
                                    open_popup(app, "quick-note");
                                }
                            })
                            .build(),
                    )
                    .expect("failed to register global shortcut plugin");

                for (name, shortcut) in [
                    ("Ctrl+Alt+N", shortcut_n),
                    ("Ctrl+Alt+C", shortcut_c),
                    ("Ctrl+Alt+Q", shortcut_q),
                ] {
                    if let Err(error) = app.global_shortcut().register(shortcut) {
                        eprintln!("failed to register {name}: {error}");
                    }
                }

                // Handle main window close = minimize to tray
                let main_window = app.get_webview_window("main").unwrap();
                let hide_handle = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide_handle.hide();
                    }
                });
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
            commands::delete_note_version,
            commands::clear_note_versions,
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
