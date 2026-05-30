mod commands;
mod database;
mod downloader;
mod models;
mod tray;

use downloader::DownloadManager;
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub manager: Arc<DownloadManager>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(database::connect(&app_handle))?;
            let manager = Arc::new(DownloadManager::new(app_handle, pool));
            let init_manager = Arc::clone(&manager);

            app.manage(AppState { manager });

            tauri::async_runtime::spawn(async move {
                if let Err(error) = init_manager.initialize().await {
                    eprintln!("OrangeDL startup cleanup failed: {error}");
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }

            if let Err(error) = tray::create_tray(app.handle()) {
                eprintln!("OrangeDL tray unavailable: {error}");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::list_downloads,
            commands::delete_download,
            commands::get_settings,
            commands::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OrangeDL");
}
