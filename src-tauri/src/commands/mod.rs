use crate::models::{AppSettings, Download, StartDownloadRequest, UpdateSettingsRequest};
use crate::AppState;
use tauri::State;

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
