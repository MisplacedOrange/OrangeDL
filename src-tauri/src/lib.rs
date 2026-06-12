// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

mod commands;
mod database;
mod downloader;
mod executor;
mod models;
mod tray;

use downloader::DownloadManager;
use executor::DownloadExecutor;
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub manager: Arc<DownloadManager>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let pool = tauri::async_runtime::block_on(database::connect(&app_handle))?;
            let manager = Arc::new(DownloadManager::new(app_handle.clone(), pool.clone()));
            let notify = manager.notify();
            let init_manager = Arc::clone(&manager);

            app.manage(AppState {
                manager: Arc::clone(&manager),
            });

            tauri::async_runtime::spawn(async move {
                if let Err(error) = init_manager.initialize().await {
                    eprintln!("OrangeDL startup cleanup failed: {error}");
                }
            });

            let executor = DownloadExecutor::new(app_handle.clone(), pool, manager, notify);
            executor.start();

            // Handle orangedl:// deep links
            use tauri_plugin_deep_link::DeepLinkExt;
            let deep_link_app = app_handle.clone();
            app.deep_link().on_open_url(move |event| {
                use tauri::Emitter;
                for url in event.urls() {
                    let _ = deep_link_app.emit("deep-link-url", url.to_string());
                }
                if let Some(window) = deep_link_app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
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
                let close_to_tray = window
                    .try_state::<AppState>()
                    .and_then(|state| {
                        tauri::async_runtime::block_on(state.manager.app_settings()).ok()
                    })
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(true);

                api.prevent_close();
                if close_to_tray {
                    let _ = window.hide();
                } else if let Some(state) = window.try_state::<AppState>() {
                    let app = window.app_handle().clone();
                    let manager = Arc::clone(&state.manager);
                    tauri::async_runtime::spawn(async move {
                        manager.graceful_shutdown().await;
                        app.exit(0);
                    });
                } else {
                    window.app_handle().exit(0);
                }
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
            commands::pause_all_downloads,
            commands::resume_all_downloads,
            commands::retry_failed_downloads,
            commands::clear_completed_downloads,
            commands::clear_cancelled_downloads,
            commands::clear_failed_downloads,
            commands::reorder_download,
            commands::update_download_options,
            commands::get_executor_summary,
            commands::preflight_check,
            commands::open_file,
            commands::reveal_in_explorer,
            commands::verify_download_checksum,
            commands::move_download,
            commands::rename_download,
            commands::check_for_updates,
            commands::cleanup_history,
            commands::pick_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OrangeDL");
}
