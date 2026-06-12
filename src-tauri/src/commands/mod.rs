// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

use crate::models::{
    AppSettings, ChecksumResult, Download, ExecutorSummary, PreflightResult, StartDownloadRequest,
    UpdateCheckResult, UpdateDownloadOptionsRequest, UpdateSettingsRequest,
};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    request: StartDownloadRequest,
) -> Result<Download, String> {
    state
        .manager
        .start_download(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, id: String) -> Result<Download, String> {
    state
        .manager
        .pause_download(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, id: String) -> Result<Download, String> {
    state
        .manager
        .resume_download(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, id: String) -> Result<Download, String> {
    state
        .manager
        .cancel_download(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> Result<Vec<Download>, String> {
    state
        .manager
        .list_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_download(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state
        .manager
        .delete_download(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .manager
        .app_settings()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    request: UpdateSettingsRequest,
) -> Result<AppSettings, String> {
    state
        .manager
        .update_settings(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pause_all_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .pause_all_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_all_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .resume_all_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retry_failed_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .retry_failed_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_completed_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .clear_completed_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_cancelled_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .clear_cancelled_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_failed_downloads(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .clear_failed_downloads()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reorder_download(
    state: State<'_, AppState>,
    id: String,
    position: u32,
) -> Result<Download, String> {
    state
        .manager
        .reorder_download(&id, position)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_download_options(
    state: State<'_, AppState>,
    request: UpdateDownloadOptionsRequest,
) -> Result<Download, String> {
    state
        .manager
        .update_download_options(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_executor_summary(state: State<'_, AppState>) -> Result<ExecutorSummary, String> {
    state
        .manager
        .get_executor_summary()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn preflight_check(
    state: State<'_, AppState>,
    url: String,
) -> Result<PreflightResult, String> {
    state
        .manager
        .preflight_check(url)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_file(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .manager
        .open_file(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn reveal_in_explorer(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .manager
        .reveal_in_explorer(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn verify_download_checksum(
    state: State<'_, AppState>,
    id: String,
    expected_sha256: Option<String>,
) -> Result<ChecksumResult, String> {
    state
        .manager
        .verify_download_checksum(&id, expected_sha256)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_for_updates(state: State<'_, AppState>) -> Result<UpdateCheckResult, String> {
    state
        .manager
        .check_for_updates()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cleanup_history(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .manager
        .cleanup_history()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn move_download(
    state: State<'_, AppState>,
    id: String,
    directory: String,
) -> Result<Download, String> {
    state
        .manager
        .move_download(&id, &directory)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn rename_download(
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<Download, String> {
    state
        .manager
        .rename_download(&id, &new_name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<std::path::PathBuf>>();
    app.dialog().file().pick_folder(move |path| {
        let _ = tx.send(path.and_then(|fp| match fp {
            tauri_plugin_dialog::FilePath::Path(p) => Some(p),
            _ => None,
        }));
    });
    Ok(rx
        .await
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().to_string()))
}
