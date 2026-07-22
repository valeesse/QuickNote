mod commands;
mod db;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod shortcuts;
mod sync;

use commands::{AppPaths, ClipboardCaptureState};
use db::{Database, DatabaseState};
use std::sync::Arc;
use sync::SyncService;
use tauri::Manager;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_autostart::ManagerExt;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
const AUTOSTART_ARG: &str = "--autostart";

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::{
        menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let new_note = MenuItem::with_id(app, "new_note", "新建快速便签", true, None::<&str>)?;
    let clipboard = MenuItem::with_id(app, "clipboard", "剪贴板历史", true, None::<&str>)?;
    let sync_now = MenuItem::with_id(app, "sync", "立即同步", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自启",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show, &new_note, &clipboard, &sync_now, &autostart, &sep, &quit,
        ],
    )?;

    // Clone Arcs for the menu event closure (owned, not borrowed from app state)
    let sync_svc: Arc<SyncService> = app.state::<Arc<SyncService>>().inner().clone();
    let db_arc = app.state::<DatabaseState>().arc();
    let paths_arc: Arc<AppPaths> = app.state::<Arc<AppPaths>>().inner().clone();
    let autostart_item = autostart.clone();

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
            "autostart" => {
                let target_enabled = !_app_handle.autolaunch().is_enabled().unwrap_or(false);
                let result = if target_enabled {
                    _app_handle.autolaunch().enable()
                } else {
                    _app_handle.autolaunch().disable()
                };

                if let Err(error) = result {
                    eprintln!("failed to update autostart state: {error}");
                }

                let actual_enabled = _app_handle.autolaunch().is_enabled().unwrap_or(false);
                let _ = autostart_item.set_checked(actual_enabled);
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
pub(crate) fn open_popup(app: &tauri::AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        // Toggle: if already visible, hide it; otherwise show and focus.
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            return;
        }
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
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let shortcut_runtime = Arc::new(shortcuts::ShortcutRuntime::default());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let launched_from_autostart = std::env::args().any(|arg| arg == AUTOSTART_ARG);

    let managed_state_plugin = tauri::plugin::Builder::<tauri::Wry, ()>::new("managed-state")
        .setup(|app, _api| {
            let app_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_dir)?;

            let database = Arc::new(Database::new(app_dir.clone())?);
            if !app.state::<DatabaseState>().initialize(database) {
                return Err("database state was initialized more than once".into());
            }

            app.manage(Arc::new(SyncService::new(app_dir.join("sync.json"))));
            app.manage(ClipboardCaptureState::default());

            let attachments_dir = app_dir.join("attachments");
            std::fs::create_dir_all(&attachments_dir)?;
            app.manage(Arc::new(AppPaths { attachments_dir }));
            Ok(())
        })
        .build();

    let builder = tauri::Builder::default()
        .manage(DatabaseState::default())
        // Plugin setup runs before configured windows are created. State must be
        // ready before their frontend can invoke commands such as `list_notes`.
        .plugin(managed_state_plugin)
        // Pass a marker to distinguish an OS login launch from a normal launch.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_ARG)
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = {
        let handler_runtime = shortcut_runtime.clone();
        builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    shortcuts::handle_shortcut(app, &handler_runtime, shortcut, event);
                })
                .build(),
        )
    };

    builder
        .setup(move |app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            let db_arc = app.state::<DatabaseState>().arc();
            let sync_service: Arc<SyncService> = app.state::<Arc<SyncService>>().inner().clone();
            let clipboard_capture_state = app.state::<ClipboardCaptureState>().inner().clone();
            let app_paths: Arc<AppPaths> = app.state::<Arc<AppPaths>>().inner().clone();

            let device_id = sync_service
                .get_config()
                .expect("failed to initialize sync config")
                .device_id;
            commands::start_clipboard_monitor(
                app.handle().clone(),
                db_arc.clone(),
                app_paths,
                device_id,
                clipboard_capture_state,
            );

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                // Setup system tray
                setup_tray(app).expect("failed to setup system tray");

                let shortcut_service = Arc::new(shortcuts::ShortcutService::new(
                    app_dir.join("shortcuts.json"),
                    shortcut_runtime.clone(),
                ));
                let shortcut_config = shortcut_service.get_config();
                if let Err(error) =
                    shortcut_service.apply_config(app.handle(), &shortcut_config, false)
                {
                    eprintln!("failed to register startup shortcuts: {error}");
                }
                app.manage(shortcut_service);

                // Handle main window close = minimize to tray
                let main_window = app.get_webview_window("main").unwrap();
                let hide_handle = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hide_handle.hide();
                    }
                });
                // Keep the UI hidden when the operating system starts the app at login.
                // The tray icon and global shortcuts remain available, and the user can
                // explicitly reveal the window from the tray menu or its left-click action.
                if !launched_from_autostart {
                    let _ = main_window.show();
                    let _ = main_window.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_note,
            commands::get_note,
            commands::list_notes,
            commands::list_notes_by_tag,
            commands::list_tags,
            commands::set_note_tags,
            commands::update_note,
            commands::delete_note,
            commands::restore_note,
            commands::purge_note,
            commands::list_deleted_notes,
            commands::toggle_pin,
            commands::reorder_notes,
            commands::search_notes,
            commands::get_note_versions,
            commands::restore_note_version,
            commands::toggle_version_pin,
            commands::delete_note_version,
            commands::clear_note_versions,
            commands::save_attachment,
            commands::get_attachment,
            commands::get_attachment_data_url,
            commands::cleanup_attachments,
            commands::get_sync_config,
            commands::set_sync_config,
            commands::test_webdav_connection,
            commands::test_cloud_connection,
            commands::has_pending_sync_changes,
            commands::pending_sync_change_count,
            commands::has_sync_changes,
            commands::get_webdav_storage_status,
            commands::run_webdav_gc,
            commands::sync_now,
            commands::clipboard_auto_capture_supported,
            commands::set_clipboard_auto_capture_enabled,
            commands::sync_clipboard_history,
            commands::capture_clipboard,
            commands::prime_clipboard_capture,
            commands::list_clipboard_items,
            commands::copy_clipboard_item,
            commands::toggle_clipboard_pin,
            commands::delete_clipboard_item,
            commands::clear_clipboard,
            shortcuts::get_shortcut_config,
            shortcuts::set_shortcut_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
