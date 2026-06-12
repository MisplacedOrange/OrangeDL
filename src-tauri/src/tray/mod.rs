// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

use crate::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show OrangeDL", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let add = MenuItem::with_id(app, "add_download", "Add download…", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let pause_all = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
    let resume_all = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit OrangeDL", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &sep1,
            &add,
            &sep2,
            &pause_all,
            &resume_all,
            &sep3,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .tooltip("OrangeDL")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "add_download" => {
                show_main_window(app);
                let _ = app.emit("tray-add-download", ());
            }
            "pause_all" => {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_clone.state::<AppState>();
                    let _ = state.manager.pause_all_downloads().await;
                });
            }
            "resume_all" => {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_clone.state::<AppState>();
                    let _ = state.manager.resume_all_downloads().await;
                });
            }
            "quit" => {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_clone.state::<AppState>();
                    state.manager.graceful_shutdown().await;
                    app_clone.exit(0);
                });
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

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
