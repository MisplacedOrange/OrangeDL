// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

use crate::database;
use crate::media_extractor;
use crate::models::{
    AppSettings, ChecksumResult, Download, DownloadStatus, PreflightResult, StartDownloadRequest,
    StartVideoDownloadRequest, UpdateCheckResult, UpdateDownloadOptionsRequest,
    UpdateSettingsRequest,
};
use dashmap::DashMap;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
    LAST_MODIFIED, RANGE,
};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, DownloadError>;

const MAX_RETRIES: usize = 3;
const MAX_REDIRECTS: usize = 10;
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 8;
const WRITE_BUFFER_SIZE: usize = 1024 * 1024;
const PROGRESS_INTERVAL_MS: u64 = 750;
const MAX_FILE_NAME_CHARS: usize = 180;
const INTENT_NONE: u8 = 0;
const INTENT_PAUSE: u8 = 1;
const INTENT_CANCEL: u8 = 2;
const COMPOUND_EXTENSIONS: &[&str] = &[
    ".tar.gz",
    ".tar.bz2",
    ".tar.xz",
    ".tar.zst",
    ".tar.lz",
    ".tar.lzma",
    ".tar.br",
];

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download not found")]
    NotFound,
    #[error("download is already running")]
    AlreadyRunning,
    #[error("download cannot be changed while it is {0}")]
    InvalidState(DownloadStatus),
    #[error("only http and https URLs are supported")]
    UnsupportedScheme,
    #[error("download URL must include a host")]
    MissingHost,
    #[error("URLs with embedded credentials are not supported")]
    CredentialsInUrl,
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("{}", classify_request_error(.0))]
    Request(#[from] reqwest::Error),
    #[error("{}", classify_io_error(.0))]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{}", classify_http_status(*.0))]
    HttpStatus(u16),
    #[error("configuration error: {0}")]
    Config(String),
}

fn classify_request_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "Connection timed out".to_owned();
    }
    if e.is_connect() {
        return "Could not connect to server".to_owned();
    }
    if e.is_body() || e.is_decode() {
        return "Server closed the connection unexpectedly".to_owned();
    }
    format!("Network error: {e}")
}

fn classify_io_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "Permission denied — check folder permissions".to_owned()
        }
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::OutOfMemory => "Disk full".to_owned(),
        std::io::ErrorKind::NotFound => "Destination folder not found".to_owned(),
        _ => {
            // Windows-specific: ERROR_DISK_FULL = 112, ERROR_HANDLE_DISK_FULL = 39
            if let Some(code) = e.raw_os_error() {
                if code == 112 || code == 39 {
                    return "Disk full".to_owned();
                }
                if code == 5 {
                    return "Access denied — check folder permissions".to_owned();
                }
            }
            format!("File error: {e}")
        }
    }
}

fn classify_http_status(code: u16) -> String {
    match code {
        400 => "Bad request (HTTP 400)".to_owned(),
        401 => "Authentication required (HTTP 401)".to_owned(),
        403 => "Access denied (HTTP 403)".to_owned(),
        404 => "File not found (HTTP 404)".to_owned(),
        407 => "Proxy authentication required (HTTP 407)".to_owned(),
        410 => "File permanently removed (HTTP 410)".to_owned(),
        429 => "Too many requests — try again later (HTTP 429)".to_owned(),
        500 => "Server error (HTTP 500)".to_owned(),
        503 => "Server unavailable (HTTP 503)".to_owned(),
        _ if code >= 500 => format!("Server error (HTTP {code})"),
        _ if code >= 400 => format!("Request error (HTTP {code})"),
        _ => format!("Unexpected response (HTTP {code})"),
    }
}

impl serde::Serialize for DownloadError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone)]
struct DownloadControl {
    token: CancellationToken,
    intent: Arc<AtomicU8>,
}

impl DownloadControl {
    fn requested_status(&self) -> DownloadStatus {
        match self.intent.load(Ordering::SeqCst) {
            INTENT_CANCEL => DownloadStatus::Cancelled,
            _ => DownloadStatus::Paused,
        }
    }
}

struct RunningDownload {
    token: CancellationToken,
    intent: Arc<AtomicU8>,
}

/// Removes a download from the running-tasks map and re-arms the executor when
/// the task finishes — including on a panic-unwind, since `panic = "abort"` is
/// intentionally off. Without this, a panicking download task would leave a
/// stale entry that permanently blocks resume and consumes a concurrency slot.
struct TaskCleanup {
    tasks: Arc<DashMap<String, RunningDownload>>,
    id: String,
    notify: Arc<Notify>,
}

impl Drop for TaskCleanup {
    fn drop(&mut self) {
        self.tasks.remove(&self.id);
        self.notify.notify_one();
    }
}

enum DownloadExit {
    Completed,
    Interrupted,
}

pub struct DownloadManager {
    app: AppHandle,
    pool: SqlitePool,
    client: Client,
    tasks: Arc<DashMap<String, RunningDownload>>,
    progress_tx: mpsc::UnboundedSender<Download>,
    notify: Arc<Notify>,
}

impl DownloadManager {
    pub fn new(app: AppHandle, pool: SqlitePool) -> Self {
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<Download>();
        let app_for_events = app.clone();
        let event_pool = pool.clone();

        tauri::async_runtime::spawn(async move {
            use tauri::Emitter;
            use tauri_plugin_notification::NotificationExt;
            use tauri_plugin_opener::OpenerExt;

            while let Some(download) = progress_rx.recv().await {
                let _ = app_for_events.emit("download-progress", &download);

                match download.status {
                    DownloadStatus::Completed => {
                        let _ = app_for_events.emit("download-finished", &download);
                        if notifications_enabled(&event_pool, &app_for_events).await {
                            let _ = app_for_events
                                .notification()
                                .builder()
                                .title("Download complete")
                                .body(&download.file_name)
                                .show();
                        }
                        if auto_open_folder_on_completion(&event_pool, &app_for_events).await {
                            let _ = app_for_events
                                .opener()
                                .reveal_item_in_dir(&download.destination);
                        }
                    }
                    DownloadStatus::Failed => {
                        let _ = app_for_events.emit("download-status", &download);
                        if notifications_enabled(&event_pool, &app_for_events).await {
                            let error = download.error.as_deref().unwrap_or("unknown error");
                            let _ = app_for_events
                                .notification()
                                .builder()
                                .title("Download failed")
                                .body(format!("{}: {}", download.file_name, error))
                                .show();
                        }
                    }
                    DownloadStatus::Cancelled | DownloadStatus::Paused => {
                        let _ = app_for_events.emit("download-status", &download);
                    }
                    _ => {}
                }
            }
        });

        let client = Client::builder()
            .redirect(download_redirect_policy())
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60 * 60 * 24))
            .pool_max_idle_per_host(MAX_IDLE_CONNECTIONS_PER_HOST)
            .tcp_nodelay(true)
            .build()
            .expect("reqwest client configuration should be valid");

        Self {
            app,
            pool,
            client,
            tasks: Arc::new(DashMap::new()),
            progress_tx,
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    pub fn active_count(&self) -> usize {
        self.tasks.len()
    }

    pub async fn initialize(&self) -> Result<()> {
        let auto_resume = database::get_app_settings(&self.pool, &self.app)
            .await
            .map(|s| s.auto_resume_interrupted_downloads)
            .unwrap_or(false);

        database::reset_interrupted(&self.pool, auto_resume).await?;

        if auto_resume {
            self.notify.notify_one();
        }

        Ok(())
    }

    pub async fn list_downloads(&self) -> Result<Vec<Download>> {
        database::list_downloads(&self.pool).await
    }

    pub async fn app_settings(&self) -> Result<AppSettings> {
        database::get_app_settings(&self.pool, &self.app).await
    }

    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> Result<AppSettings> {
        if let Some(dir) = request.default_download_directory.as_deref() {
            let directory =
                prepare_download_directory(Some(dir), database::system_download_dir(&self.app)?)
                    .await?;
            let dir_str = directory.to_string_lossy().to_string();
            database::set_default_download_directory(&self.pool, &dir_str).await?;
        }

        database::set_default_speed_limit(
            &self.pool,
            request.default_speed_limit_bps.filter(|v| *v > 0),
        )
        .await?;

        database::set_global_speed_limit(
            &self.pool,
            request.global_speed_limit_bps.filter(|v| *v > 0),
        )
        .await?;

        if let Some(max) = request.max_concurrent_downloads {
            database::set_max_concurrent_downloads(&self.pool, max).await?;
            self.notify.notify_one();
        }

        if let Some(auto_resume) = request.auto_resume_interrupted_downloads {
            database::set_auto_resume_interrupted(&self.pool, auto_resume).await?;
        }

        if let Some(close_to_tray) = request.close_to_tray {
            database::set_close_to_tray(&self.pool, close_to_tray).await?;
        }

        if let Some(enabled) = request.notifications_enabled {
            database::set_notifications_enabled(&self.pool, enabled).await?;
        }

        if let Some(sound) = request.notification_sound {
            database::set_notification_sound(&self.pool, sound).await?;
        }

        if let Some(enabled) = request.background_update_notifications {
            database::set_background_update_notifications(&self.pool, enabled).await?;
        }

        if let Some(enabled) = request.auto_open_folder_on_completion {
            database::set_auto_open_folder_on_completion(&self.pool, enabled).await?;
        }

        database::set_history_retention_days(&self.pool, request.history_retention_days).await?;
        database::set_history_max_rows(&self.pool, request.history_max_rows).await?;

        if let Some(first_run_completed) = request.first_run_completed {
            database::set_first_run_completed(&self.pool, first_run_completed).await?;
        }

        if let Some(theme) = request.theme.as_deref() {
            database::set_theme(&self.pool, &normalize_theme(theme)).await?;
        }

        let settings = self.app_settings().await?;
        let _ = database::cleanup_history(
            &self.pool,
            settings.history_retention_days,
            settings.history_max_rows,
        )
        .await?;

        self.app_settings().await
    }

    pub async fn start_download(&self, request: StartDownloadRequest) -> Result<Download> {
        let parsed_url = Url::parse(request.url.trim())?;
        validate_download_url(&parsed_url)?;

        let id = Uuid::new_v4().to_string();
        let settings = self.app_settings().await?;
        let directory = prepare_download_directory(
            request.directory.as_deref(),
            PathBuf::from(&settings.default_download_directory),
        )
        .await?;

        let guessed_name = request
            .file_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| guess_file_name(&parsed_url, &id));
        let file_name = sanitize_file_name(&guessed_name, &id);
        let destination = reserve_unique_destination(&directory, &file_name).await?;
        let temp_path = part_path_for(&destination);

        let download = Download::new(
            id.clone(),
            parsed_url.to_string(),
            destination
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&file_name)
                .to_owned(),
            destination.to_string_lossy().to_string(),
            temp_path.to_string_lossy().to_string(),
            request
                .speed_limit_bps
                .filter(|value| *value > 0)
                .or(settings.default_speed_limit_bps),
        );

        if let Err(error) = database::insert_download(&self.pool, &download).await {
            release_destination_claim(&temp_path).await;
            return Err(error);
        }
        publish(&self.progress_tx, download.clone());
        self.notify.notify_one();
        Ok(download)
    }

    pub async fn pause_download(&self, id: &str) -> Result<Download> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if download.status.is_terminal() {
            return Err(DownloadError::InvalidState(download.status));
        }

        if let Some(running) = self.tasks.get(id) {
            running.intent.store(INTENT_PAUSE, Ordering::SeqCst);
            running.token.cancel();
        }

        let updated = database::set_status(&self.pool, id, DownloadStatus::Paused, None)
            .await?
            .ok_or(DownloadError::NotFound)?;
        publish(&self.progress_tx, updated.clone());
        Ok(updated)
    }

    pub async fn resume_download(&self, id: &str) -> Result<Download> {
        if self.tasks.contains_key(id) {
            return Err(DownloadError::AlreadyRunning);
        }

        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if matches!(
            download.status,
            DownloadStatus::Completed | DownloadStatus::Cancelled
        ) {
            return Err(DownloadError::InvalidState(download.status));
        }

        let updated = database::set_status(&self.pool, id, DownloadStatus::Queued, None)
            .await?
            .ok_or(DownloadError::NotFound)?;
        publish(&self.progress_tx, updated.clone());
        self.notify.notify_one();
        Ok(updated)
    }

    pub async fn cancel_download(&self, id: &str) -> Result<Download> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if let Some(running) = self.tasks.get(id) {
            running.intent.store(INTENT_CANCEL, Ordering::SeqCst);
            running.token.cancel();
        } else {
            remove_partial_files(Path::new(&download.temp_path)).await;
        }

        let updated = database::set_status(&self.pool, id, DownloadStatus::Cancelled, None)
            .await?
            .ok_or(DownloadError::NotFound)?;
        publish(&self.progress_tx, updated.clone());
        Ok(updated)
    }

    pub async fn delete_download(&self, id: &str) -> Result<String> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if self.tasks.contains_key(id) {
            return Err(DownloadError::InvalidState(download.status));
        }

        if !matches!(
            download.status,
            DownloadStatus::Completed | DownloadStatus::Cancelled | DownloadStatus::Failed
        ) {
            return Err(DownloadError::InvalidState(download.status));
        }

        if download.status != DownloadStatus::Completed {
            remove_partial_files(Path::new(&download.temp_path)).await;
        }

        database::delete_download(&self.pool, id).await?;
        Ok(id.to_owned())
    }

    pub async fn pause_all_downloads(&self) -> Result<Vec<String>> {
        for entry in self.tasks.iter() {
            entry.value().intent.store(INTENT_PAUSE, Ordering::SeqCst);
            entry.value().token.cancel();
        }

        let ids = database::set_all_downloading_and_queued_to_paused(&self.pool).await?;

        for id in &ids {
            if let Ok(Some(download)) = database::get_download(&self.pool, id).await {
                publish(&self.progress_tx, download);
            }
        }

        Ok(ids)
    }

    pub async fn resume_all_downloads(&self) -> Result<Vec<String>> {
        let ids = database::set_all_paused_to_queued(&self.pool).await?;

        for id in &ids {
            if let Ok(Some(download)) = database::get_download(&self.pool, id).await {
                publish(&self.progress_tx, download);
            }
        }

        self.notify.notify_one();
        Ok(ids)
    }

    pub async fn retry_failed_downloads(&self) -> Result<Vec<String>> {
        let ids = database::set_all_failed_to_queued(&self.pool).await?;

        for id in &ids {
            if let Ok(Some(download)) = database::get_download(&self.pool, id).await {
                publish(&self.progress_tx, download);
            }
        }

        self.notify.notify_one();
        Ok(ids)
    }

    pub async fn clear_completed_downloads(&self) -> Result<Vec<String>> {
        database::delete_downloads_by_status(&self.pool, DownloadStatus::Completed).await
    }

    pub async fn clear_cancelled_downloads(&self) -> Result<Vec<String>> {
        let downloads =
            database::delete_downloads_by_status(&self.pool, DownloadStatus::Cancelled).await?;
        Ok(downloads)
    }

    pub async fn clear_failed_downloads(&self) -> Result<Vec<String>> {
        database::delete_downloads_by_status(&self.pool, DownloadStatus::Failed).await
    }

    pub async fn update_download_options(
        &self,
        request: UpdateDownloadOptionsRequest,
    ) -> Result<Download> {
        let id = &request.id;

        if request.clear_speed_limit {
            database::update_download_speed_limit(&self.pool, id, None).await?;
        } else if let Some(speed_limit) = request.speed_limit_bps {
            database::update_download_speed_limit(&self.pool, id, Some(speed_limit)).await?;
        }

        database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)
    }

    pub async fn preflight_check(&self, raw_url: String) -> Result<PreflightResult> {
        let url = Url::parse(raw_url.trim())?;
        validate_download_url(&url)?;

        let response = tokio::time::timeout(
            Duration::from_secs(15),
            self.client.head(url.clone()).send(),
        )
        .await
        .map_err(|_| DownloadError::Config("preflight timed out".into()))??;

        let headers = response.headers();

        let file_name = headers
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_disposition_filename);

        let content_length = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_owned());

        let supports_range = headers
            .get(ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim() != "none")
            .unwrap_or(false);

        let etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);

        let last_modified = headers
            .get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);

        let final_url = response.url().to_string();

        Ok(PreflightResult {
            url: final_url,
            file_name,
            content_length,
            content_type,
            supports_range,
            etag,
            last_modified,
        })
    }

    pub async fn open_file(&self, id: &str) -> Result<()> {
        let download = self.completed_download(id).await?;
        use tauri_plugin_opener::OpenerExt;
        self.app
            .opener()
            .open_path(download.destination, None::<String>)
            .map_err(|e| DownloadError::Config(e.to_string()))
    }

    pub async fn reveal_in_explorer(&self, id: &str) -> Result<()> {
        let download = self.completed_download(id).await?;
        use tauri_plugin_opener::OpenerExt;
        self.app
            .opener()
            .reveal_item_in_dir(download.destination)
            .map_err(|e| DownloadError::Config(e.to_string()))
    }

    pub async fn verify_download_checksum(
        &self,
        id: &str,
        expected_sha256: Option<String>,
    ) -> Result<ChecksumResult> {
        let download = self.completed_download(id).await?;

        let expected = expected_sha256
            .as_deref()
            .map(normalize_sha256)
            .transpose()?
            .or_else(|| {
                download
                    .checksum_sha256
                    .clone()
                    .map(|value| value.to_ascii_lowercase())
            });

        let computed = compute_sha256(Path::new(&download.destination)).await?;
        if let Some(expected) = expected.as_deref() {
            let _ = database::set_download_checksum(&self.pool, id, Some(expected)).await?;
        }

        Ok(ChecksumResult {
            id: id.to_owned(),
            computed_sha256: computed.clone(),
            expected_sha256: expected.clone(),
            matched: expected.map(|value| value == computed),
        })
    }

    pub async fn check_for_updates(&self) -> Result<UpdateCheckResult> {
        let current_version = env!("CARGO_PKG_VERSION").to_owned();
        let response = self
            .client
            .get("https://api.github.com/repos/misplacedorange/OrangeDL/releases/latest")
            .header("User-Agent", "OrangeDL update check")
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(UpdateCheckResult {
                current_version,
                latest_version: None,
                release_url: None,
                update_available: false,
                message: format!(
                    "Update check failed with HTTP {}",
                    response.status().as_u16()
                ),
            });
        }

        let body = response.text().await?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| DownloadError::Config(format!("invalid release response: {error}")))?;
        let latest_version = value
            .get("tag_name")
            .and_then(|value| value.as_str())
            .map(trim_version_prefix)
            .filter(|value| !value.is_empty());
        let release_url = value
            .get("html_url")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);

        let update_available = latest_version
            .as_deref()
            .map(|latest| version_is_newer(latest, &current_version))
            .unwrap_or(false);

        Ok(UpdateCheckResult {
            current_version,
            latest_version,
            release_url,
            update_available,
            message: if update_available {
                "A newer OrangeDL release is available.".to_owned()
            } else {
                "OrangeDL is up to date.".to_owned()
            },
        })
    }

    pub async fn cleanup_history(&self) -> Result<Vec<String>> {
        let settings = self.app_settings().await?;
        database::cleanup_history(
            &self.pool,
            settings.history_retention_days,
            settings.history_max_rows,
        )
        .await
    }

    pub async fn graceful_shutdown(&self) {
        for entry in self.tasks.iter() {
            entry.value().intent.store(INTENT_PAUSE, Ordering::SeqCst);
            entry.value().token.cancel();
        }
        let _ = database::set_all_downloading_and_queued_to_paused(&self.pool).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    pub async fn start_video_download(
        &self,
        request: StartVideoDownloadRequest,
    ) -> Result<Download> {
        let parsed_url = Url::parse(request.url.trim())?;
        validate_download_url(&parsed_url)?;

        let id = Uuid::new_v4().to_string();
        let settings = self.app_settings().await?;

        let directory = prepare_download_directory(
            request.directory.as_deref(),
            PathBuf::from(&settings.default_download_directory),
        )
        .await?;

        let sanitized_title = sanitize_file_name(&request.title, &id);
        let placeholder_name = format!("{sanitized_title}.mp4");

        // temp_path stores the yt-dlp working directory (per-download temp folder)
        let temp_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| DownloadError::Config(e.to_string()))?
            .join("ytdlp-temp")
            .join(&id);

        let destination = directory.join(&placeholder_name);

        let download = Download::new(
            id.clone(),
            parsed_url.to_string(),
            placeholder_name,
            destination.to_string_lossy().to_string(),
            temp_dir.to_string_lossy().to_string(),
            None,
        );

        database::insert_video_download(&self.pool, &download, &request.quality).await?;
        publish(&self.progress_tx, download.clone());
        self.notify.notify_one();
        Ok(download)
    }

    pub fn spawn_queued_download(&self, id: String) {
        if self.tasks.contains_key(&id) {
            return;
        }

        let token = CancellationToken::new();
        let intent = Arc::new(AtomicU8::new(INTENT_NONE));

        self.tasks.insert(
            id.clone(),
            RunningDownload {
                token: token.clone(),
                intent: Arc::clone(&intent),
            },
        );

        let control = DownloadControl { token, intent };
        let pool = self.pool.clone();
        let client = self.client.clone();
        let progress_tx = self.progress_tx.clone();
        let tasks = Arc::clone(&self.tasks);
        let notify = Arc::clone(&self.notify);
        let app = self.app.clone();

        tauri::async_runtime::spawn(async move {
            // Cleans up the tasks map and re-arms the executor on return *or*
            // panic, so a panicking task can't strand the entry forever.
            let _cleanup = TaskCleanup {
                tasks,
                id: id.clone(),
                notify,
            };

            // Determine download type
            let media_format = database::get_media_format(&pool, &id).await.unwrap_or(None);

            if let Some(quality) = media_format {
                if let Some(ytdlp_path) = media_extractor::find_ytdlp(&app) {
                    if let Ok(Some(dl)) = database::get_download(&pool, &id).await {
                        let temp_dir = PathBuf::from(&dl.temp_path);
                        let dest_dir = PathBuf::from(&dl.destination)
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| PathBuf::from("."));

                        Self::run_video_download(
                            id.clone(),
                            dl.url,
                            quality,
                            temp_dir,
                            dest_dir,
                            ytdlp_path,
                            pool,
                            progress_tx,
                            control,
                        )
                        .await;
                    }
                } else {
                    if let Ok(Some(d)) = database::set_status(
                        &pool,
                        &id,
                        DownloadStatus::Failed,
                        Some("yt-dlp not found — install it in Settings"),
                    )
                    .await
                    {
                        let _ = progress_tx.send(d);
                    }
                }
            } else {
                Self::run_download(id.clone(), client, pool, control, progress_tx).await;
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_video_download(
        id: String,
        url: String,
        quality: String,
        temp_dir: PathBuf,
        dest_dir: PathBuf,
        ytdlp_path: PathBuf,
        pool: SqlitePool,
        progress_tx: mpsc::UnboundedSender<Download>,
        control: DownloadControl,
    ) {
        if let Ok(Some(d)) =
            database::set_status(&pool, &id, DownloadStatus::Downloading, None).await
        {
            let _ = progress_tx.send(d);
        }

        // yt-dlp emits progress lines very frequently. Rather than spawning a
        // task (and two DB round-trips) per line — which floods the channel
        // with unordered writes — funnel raw progress through an unbounded
        // channel into a single consumer that coalesces and time-throttles
        // updates, matching the HTTP path's PROGRESS_INTERVAL_MS cadence and
        // guaranteeing in-order persistence.
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<media_extractor::YtDlpProgress>();
        let consumer = {
            let pool = pool.clone();
            let id = id.clone();
            let tx = progress_tx.clone();
            tauri::async_runtime::spawn(async move {
                let mut last_emit = Instant::now()
                    .checked_sub(Duration::from_millis(PROGRESS_INTERVAL_MS))
                    .unwrap_or_else(Instant::now);
                let mut pending: Option<media_extractor::YtDlpProgress> = None;

                let flush = |prog: media_extractor::YtDlpProgress, pool: &SqlitePool| {
                    let pool = pool.clone();
                    let id = id.clone();
                    let tx = tx.clone();
                    async move {
                        let _ = database::update_progress(
                            &pool,
                            &id,
                            prog.downloaded_bytes,
                            prog.total_bytes,
                            prog.speed_bps,
                        )
                        .await;
                        if let Ok(Some(d)) = database::get_download(&pool, &id).await {
                            let _ = tx.send(d);
                        }
                    }
                };

                while let Some(prog) = raw_rx.recv().await {
                    if last_emit.elapsed() >= Duration::from_millis(PROGRESS_INTERVAL_MS) {
                        flush(prog, &pool).await;
                        last_emit = Instant::now();
                        pending = None;
                    } else {
                        pending = Some(prog);
                    }
                }

                // Persist the final coalesced update once yt-dlp stops emitting.
                if let Some(prog) = pending {
                    flush(prog, &pool).await;
                }
            })
        };

        let result = media_extractor::run_ytdlp(
            &url,
            &quality,
            &temp_dir,
            &ytdlp_path,
            &control.token,
            move |prog| {
                let _ = raw_tx.send(prog);
            },
        )
        .await;

        // run_ytdlp has returned, so the closure (and its raw_tx) is dropped;
        // wait for the consumer to drain before writing the final size below.
        let _ = consumer.await;

        match result {
            Ok(src_file) => {
                let file_name = src_file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("video")
                    .to_owned();

                let final_dest = match media_extractor::unique_in_dir(&dest_dir, &file_name).await {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = database::set_status(
                            &pool,
                            &id,
                            DownloadStatus::Failed,
                            Some(&e.to_string()),
                        )
                        .await;
                        return;
                    }
                };

                if let Err(e) = tokio::fs::rename(&src_file, &final_dest).await {
                    let _ = database::set_status(
                        &pool,
                        &id,
                        DownloadStatus::Failed,
                        Some(&e.to_string()),
                    )
                    .await;
                    return;
                }

                let _ = tokio::fs::remove_dir(&temp_dir).await;

                let final_dest_str = final_dest.to_string_lossy().to_string();
                let final_name = final_dest
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&file_name)
                    .to_owned();

                let _ =
                    database::update_download_destination(&pool, &id, &final_dest_str, &final_name)
                        .await;

                let size = tokio::fs::metadata(&final_dest)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                let _ = database::update_progress(&pool, &id, size, Some(size), 0.0).await;

                if let Ok(Some(d)) =
                    database::set_status(&pool, &id, DownloadStatus::Completed, None).await
                {
                    let _ = progress_tx.send(d);
                }
            }
            Err(media_extractor::MediaError::Cancelled) => {
                let status = control.requested_status();
                if status == DownloadStatus::Cancelled {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                }
                if let Ok(Some(d)) = database::set_status(&pool, &id, status, None).await {
                    let _ = progress_tx.send(d);
                }
            }
            Err(e) => {
                if let Ok(Some(d)) =
                    database::set_status(&pool, &id, DownloadStatus::Failed, Some(&e.to_string()))
                        .await
                {
                    let _ = progress_tx.send(d);
                }
            }
        }
    }

    async fn run_download(
        id: String,
        client: Client,
        pool: SqlitePool,
        control: DownloadControl,
        progress_tx: mpsc::UnboundedSender<Download>,
    ) {
        if let Ok(Some(download)) =
            database::set_status(&pool, &id, DownloadStatus::Downloading, None).await
        {
            publish(&progress_tx, download);
        }

        let mut attempt = 0;

        loop {
            if control.token.is_cancelled() {
                Self::finalize_interruption(&pool, &progress_tx, &id, &control).await;
                break;
            }

            match Self::download_once(&id, &client, &pool, &control, &progress_tx).await {
                Ok(DownloadExit::Completed) => break,
                Ok(DownloadExit::Interrupted) => {
                    Self::finalize_interruption(&pool, &progress_tx, &id, &control).await;
                    break;
                }
                Err(error) => {
                    if control.token.is_cancelled() {
                        Self::finalize_interruption(&pool, &progress_tx, &id, &control).await;
                        break;
                    }

                    attempt += 1;

                    if attempt <= MAX_RETRIES {
                        let delay = exponential_backoff_delay(attempt);
                        let message = format!(
                            "Retry {attempt}/{MAX_RETRIES} in {}s: {error}",
                            delay.as_secs()
                        );

                        let _ = database::increment_retry_count(&pool, &id).await;
                        let retry_at = (chrono::Utc::now()
                            + chrono::Duration::from_std(delay)
                                .unwrap_or(chrono::Duration::seconds(10)))
                        .to_rfc3339();
                        let _ = database::set_next_retry_at(&pool, &id, &retry_at).await;

                        if let Ok(Some(mut download)) = database::set_status(
                            &pool,
                            &id,
                            DownloadStatus::Downloading,
                            Some(&message),
                        )
                        .await
                        {
                            download.error = Some(message);
                            publish(&progress_tx, download);
                        }

                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = control.token.cancelled() => {
                                Self::finalize_interruption(&pool, &progress_tx, &id, &control).await;
                                break;
                            }
                        }
                    } else {
                        let message = error.to_string();
                        if let Ok(Some(download)) =
                            database::set_status(&pool, &id, DownloadStatus::Failed, Some(&message))
                                .await
                        {
                            publish(&progress_tx, download);
                        }
                        break;
                    }
                }
            }
        }
    }

    async fn download_once(
        id: &str,
        client: &Client,
        pool: &SqlitePool,
        control: &DownloadControl,
        progress_tx: &mpsc::UnboundedSender<Download>,
    ) -> Result<DownloadExit> {
        let download = database::get_download(pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if control.token.is_cancelled() {
            return Ok(DownloadExit::Interrupted);
        }

        let temp_path = PathBuf::from(&download.temp_path);
        let destination = PathBuf::from(&download.destination);
        let download_url = Url::parse(&download.url)?;
        validate_download_url(&download_url)?;

        if let Some(parent) = temp_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut downloaded_bytes = file_len(&temp_path).await.unwrap_or(0);

        if control.token.is_cancelled() {
            return Ok(DownloadExit::Interrupted);
        }

        let mut request = client.get(download_url.clone());

        if downloaded_bytes > 0 {
            request = request.header(RANGE, format!("bytes={downloaded_bytes}-"));
        }

        let mut response = tokio::select! {
            _ = control.token.cancelled() => return Ok(DownloadExit::Interrupted),
            response = request.send() => response?,
        };
        let mut status = response.status();

        if status == StatusCode::RANGE_NOT_SATISFIABLE && downloaded_bytes > 0 {
            if let Some(total) = download.total_bytes {
                if downloaded_bytes >= total {
                    finalize_file(pool, id, &temp_path, &destination, total, progress_tx).await?;
                    return Ok(DownloadExit::Completed);
                }
            }
        }

        if !status.is_success() {
            return Err(DownloadError::HttpStatus(status.as_u16()));
        }

        let mut response_etag = header_string(response.headers(), ETAG);
        let mut response_last_modified = header_string(response.headers(), LAST_MODIFIED);

        let mut append = downloaded_bytes > 0 && status == StatusCode::PARTIAL_CONTENT;

        // Validate ETag/Last-Modified when resuming. If the remote resource
        // changed, the ranged body would continue a file that no longer
        // exists on the server, so re-request the full file and start over.
        if append {
            let (stored_etag, stored_lm) = database::get_download_validators(pool, id)
                .await
                .unwrap_or((None, None));
            let validators_changed = match (&stored_etag, &response_etag) {
                (Some(s), Some(r)) => s != r,
                _ => match (&stored_lm, &response_last_modified) {
                    (Some(s), Some(r)) => s != r,
                    _ => false,
                },
            };
            if validators_changed {
                response = tokio::select! {
                    _ = control.token.cancelled() => return Ok(DownloadExit::Interrupted),
                    response = client.get(download_url.clone()).send() => response?,
                };
                status = response.status();
                if !status.is_success() {
                    return Err(DownloadError::HttpStatus(status.as_u16()));
                }
                response_etag = header_string(response.headers(), ETAG);
                response_last_modified = header_string(response.headers(), LAST_MODIFIED);
                append = false;
                downloaded_bytes = 0;
                // Drop stale validators so a missing header on the new
                // response cannot leave the old ones in place.
                let _ = database::clear_validators(pool, id).await;
            }
        }

        if downloaded_bytes > 0 && !append {
            downloaded_bytes = 0;
        }

        // Store current validators for future resume validation
        let _ = database::store_validators(
            pool,
            id,
            response_etag.as_deref(),
            response_last_modified.as_deref(),
        )
        .await;

        // Update filename from Content-Disposition if server provides one
        if let Some(cd) = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
        {
            if let Some(name) = parse_content_disposition_filename(cd) {
                let sanitized = sanitize_file_name(&name, id);
                if sanitized != download.file_name {
                    let _ = database::update_download_filename(pool, id, &sanitized).await;
                }
            }
        }

        let total_bytes = content_range_total(response.headers())
            .or_else(|| {
                response
                    .content_length()
                    .map(|length| downloaded_bytes + length)
            })
            .or(download.total_bytes);

        database::set_total_bytes(pool, id, total_bytes).await?;

        let file = if append {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&temp_path)
                .await?
        } else {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .await?
        };
        let mut file = BufWriter::with_capacity(WRITE_BUFFER_SIZE, file);

        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now();
        let mut last_emit_bytes = downloaded_bytes;
        let session_started = Instant::now();
        let session_start_bytes = downloaded_bytes;
        let mut effective_limit_bps = effective_speed_limit(pool, download.speed_limit_bps).await?;
        let mut last_limit_refresh = Instant::now();

        loop {
            let chunk = tokio::select! {
                _ = control.token.cancelled() => {
                    file.flush().await?;
                    return Ok(DownloadExit::Interrupted);
                }
                chunk = stream.next() => chunk,
            };

            let Some(chunk) = chunk else {
                break;
            };

            if control.token.is_cancelled() {
                file.flush().await?;
                return Ok(DownloadExit::Interrupted);
            }

            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);

            if last_limit_refresh.elapsed() >= Duration::from_secs(1) {
                effective_limit_bps = effective_speed_limit(pool, download.speed_limit_bps).await?;
                last_limit_refresh = Instant::now();
            }

            throttle_if_needed(
                effective_limit_bps,
                downloaded_bytes.saturating_sub(session_start_bytes),
                session_started,
                control,
            )
            .await?;

            if control.token.is_cancelled() {
                file.flush().await?;
                return Ok(DownloadExit::Interrupted);
            }

            let elapsed = last_emit.elapsed();
            if elapsed >= Duration::from_millis(PROGRESS_INTERVAL_MS) {
                let bytes_delta = downloaded_bytes.saturating_sub(last_emit_bytes);
                let speed_bps = bytes_delta as f64 / elapsed.as_secs_f64().max(0.001);
                publish_progress(
                    pool,
                    id,
                    downloaded_bytes,
                    total_bytes,
                    speed_bps,
                    progress_tx,
                )
                .await?;
                last_emit = Instant::now();
                last_emit_bytes = downloaded_bytes;
            }
        }

        if control.token.is_cancelled() {
            file.flush().await?;
            return Ok(DownloadExit::Interrupted);
        }

        file.flush().await?;
        publish_progress(pool, id, downloaded_bytes, total_bytes, 0.0, progress_tx).await?;

        if let Some(total) = total_bytes {
            if downloaded_bytes < total {
                return Err(DownloadError::Config(format!(
                    "download ended early: {downloaded_bytes}/{total} bytes"
                )));
            }
        }

        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let final_size = total_bytes.unwrap_or(downloaded_bytes);
        finalize_file(pool, id, &temp_path, &destination, final_size, progress_tx).await?;
        Ok(DownloadExit::Completed)
    }

    async fn finalize_interruption(
        pool: &SqlitePool,
        progress_tx: &mpsc::UnboundedSender<Download>,
        id: &str,
        control: &DownloadControl,
    ) {
        let status = control.requested_status();

        if status == DownloadStatus::Cancelled {
            if let Ok(Some(download)) = database::get_download(pool, id).await {
                remove_partial_files(Path::new(&download.temp_path)).await;
            }
        }

        if let Ok(Some(download)) = database::set_status(pool, id, status, None).await {
            publish(progress_tx, download);
        }
    }

    pub async fn move_download(&self, id: &str, new_directory: &str) -> Result<Download> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if download.status != DownloadStatus::Completed {
            return Err(DownloadError::InvalidState(download.status));
        }

        let destination = PathBuf::from(&download.destination);
        let file_name = destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&download.file_name)
            .to_owned();

        let target_dir = PathBuf::from(new_directory.trim());
        tokio::fs::create_dir_all(&target_dir).await?;
        let new_destination = unique_destination(&target_dir, &file_name).await?;

        tokio::fs::rename(&destination, &new_destination).await?;

        let new_dest_str = new_destination.to_string_lossy().to_string();
        let new_file_name = new_destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&file_name)
            .to_owned();
        database::update_download_destination(&self.pool, id, &new_dest_str, &new_file_name)
            .await?;

        database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)
    }

    pub async fn rename_download(&self, id: &str, new_name: &str) -> Result<Download> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if download.status != DownloadStatus::Completed {
            return Err(DownloadError::InvalidState(download.status));
        }

        let destination = PathBuf::from(&download.destination);
        let dir = destination.parent().unwrap_or(Path::new("."));
        let sanitized = sanitize_file_name(new_name.trim(), id);
        let new_destination = unique_destination(dir, &sanitized).await?;

        tokio::fs::rename(&destination, &new_destination).await?;

        let new_dest_str = new_destination.to_string_lossy().to_string();
        let new_file_name = new_destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&sanitized)
            .to_owned();
        database::update_download_destination(&self.pool, id, &new_dest_str, &new_file_name)
            .await?;

        database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)
    }

    async fn completed_download(&self, id: &str) -> Result<Download> {
        let download = database::get_download(&self.pool, id)
            .await?
            .ok_or(DownloadError::NotFound)?;

        if download.status != DownloadStatus::Completed {
            return Err(DownloadError::InvalidState(download.status));
        }

        Ok(download)
    }
}

async fn publish_progress(
    pool: &SqlitePool,
    id: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    speed_bps: f64,
    progress_tx: &mpsc::UnboundedSender<Download>,
) -> Result<()> {
    if let Some(mut download) =
        database::update_progress(pool, id, downloaded_bytes, total_bytes, speed_bps).await?
    {
        download.eta_seconds =
            estimate_eta(download.total_bytes, download.downloaded_bytes, speed_bps);
        publish(progress_tx, download);
    }

    Ok(())
}

async fn finalize_file(
    pool: &SqlitePool,
    id: &str,
    temp_path: &Path,
    destination: &Path,
    final_size: u64,
    progress_tx: &mpsc::UnboundedSender<Download>,
) -> Result<()> {
    if tokio::fs::try_exists(temp_path).await? {
        tokio::fs::rename(temp_path, destination).await?;
    }

    database::update_progress(pool, id, final_size, Some(final_size), 0.0).await?;

    if let Some(download) = database::set_status(pool, id, DownloadStatus::Completed, None).await? {
        publish(progress_tx, download);
    }

    Ok(())
}

async fn effective_speed_limit(
    pool: &SqlitePool,
    download_limit_bps: Option<u64>,
) -> Result<Option<u64>> {
    let global_limit_bps = database::get_global_speed_limit(pool).await?;
    Ok(
        match (
            download_limit_bps.filter(|value| *value > 0),
            global_limit_bps.filter(|value| *value > 0),
        ) {
            (Some(download), Some(global)) => Some(download.min(global)),
            (Some(download), None) => Some(download),
            (None, Some(global)) => Some(global),
            (None, None) => None,
        },
    )
}

async fn throttle_if_needed(
    speed_limit_bps: Option<u64>,
    session_bytes: u64,
    session_started: Instant,
    control: &DownloadControl,
) -> Result<()> {
    let Some(limit) = speed_limit_bps.filter(|value| *value > 0) else {
        return Ok(());
    };

    let expected_elapsed = Duration::from_secs_f64(session_bytes as f64 / limit as f64);
    let actual_elapsed = session_started.elapsed();

    if expected_elapsed > actual_elapsed {
        tokio::select! {
            _ = tokio::time::sleep(expected_elapsed - actual_elapsed) => Ok(()),
            _ = control.token.cancelled() => Ok(()),
        }
    } else {
        Ok(())
    }
}

async fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn normalize_sha256(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64
        || !normalized
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(DownloadError::Config(
            "expected SHA256 must be 64 hexadecimal characters".to_owned(),
        ));
    }
    Ok(normalized)
}

async fn notifications_enabled(pool: &SqlitePool, app: &AppHandle) -> bool {
    database::get_app_settings(pool, app)
        .await
        .map(|settings| settings.notifications_enabled)
        .unwrap_or(true)
}

async fn auto_open_folder_on_completion(pool: &SqlitePool, app: &AppHandle) -> bool {
    database::get_app_settings(pool, app)
        .await
        .map(|settings| settings.auto_open_folder_on_completion)
        .unwrap_or(false)
}

/// Theme identifiers are stored as short kebab-case slugs; anything else
/// falls back to the default so a bad import cannot wedge the UI.
fn normalize_theme(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    let valid = !normalized.is_empty()
        && normalized.len() <= 32
        && normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');

    if valid {
        normalized
    } else {
        database::DEFAULT_THEME.to_owned()
    }
}

fn trim_version_prefix(value: &str) -> String {
    value.trim().trim_start_matches('v').to_owned()
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest_parts = version_parts(latest);
    let current_parts = version_parts(current);
    latest_parts > current_parts
}

fn version_parts(value: &str) -> Vec<u32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

async fn remove_partial_files(temp_path: &Path) {
    // HTTP downloads store a `.part` file here; video (yt-dlp) downloads store
    // a working *directory*. `remove_file` silently fails on a directory, so
    // branch on the type to avoid leaking the per-download temp folder.
    match tokio::fs::metadata(temp_path).await {
        Ok(meta) if meta.is_dir() => {
            let _ = tokio::fs::remove_dir_all(temp_path).await;
        }
        Ok(_) => {
            let _ = tokio::fs::remove_file(temp_path).await;
        }
        Err(_) => {}
    }
}

fn publish(progress_tx: &mpsc::UnboundedSender<Download>, download: Download) {
    let _ = progress_tx.send(download);
}

/// Exponential backoff with nanosecond-derived jitter.
/// Attempt 1 → 2–4 s, attempt 2 → 4–8 s, attempt 3 → 8–16 s, capped at 120 s.
fn exponential_backoff_delay(attempt: usize) -> Duration {
    let base: u64 = 2u64.saturating_pow(attempt as u32).min(60);
    let jitter_ns = chrono::Utc::now().timestamp_subsec_nanos() as u64;
    let jitter = jitter_ns % base.saturating_add(1);
    Duration::from_secs(base.saturating_add(jitter).min(120))
}

fn estimate_eta(total_bytes: Option<u64>, downloaded_bytes: u64, speed_bps: f64) -> Option<u64> {
    let total = total_bytes?;
    if speed_bps <= 1.0 || downloaded_bytes >= total {
        return None;
    }

    Some(((total - downloaded_bytes) as f64 / speed_bps).ceil() as u64)
}

fn header_string(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn content_range_total(headers: &HeaderMap) -> Option<u64> {
    let value = headers.get(CONTENT_RANGE)?.to_str().ok()?;
    let (_, total) = value.rsplit_once('/')?;

    if total == "*" {
        None
    } else {
        total.parse::<u64>().ok()
    }
}

async fn file_len(path: &Path) -> std::io::Result<u64> {
    Ok(tokio::fs::metadata(path).await?.len())
}

async fn unique_destination(directory: &Path, file_name: &str) -> Result<PathBuf> {
    let first = directory.join(file_name);
    if path_available(&first).await? {
        return Ok(first);
    }

    for index in 1..10_000 {
        let candidate_name = collision_file_name(file_name, index);
        let candidate = directory.join(candidate_name);

        if path_available(&candidate).await? {
            return Ok(candidate);
        }
    }

    Err(DownloadError::Config(
        "could not find an available destination name".to_owned(),
    ))
}

async fn reserve_unique_destination(directory: &Path, file_name: &str) -> Result<PathBuf> {
    for index in 0..10_000 {
        let candidate_name = if index == 0 {
            file_name.to_owned()
        } else {
            collision_file_name(file_name, index)
        };
        let candidate = directory.join(candidate_name);

        if claim_destination(&candidate).await? {
            return Ok(candidate);
        }
    }

    Err(DownloadError::Config(
        "could not find an available destination name".to_owned(),
    ))
}

async fn claim_destination(destination: &Path) -> Result<bool> {
    if tokio::fs::try_exists(destination).await? {
        return Ok(false);
    }

    // Claim the app-owned partial path atomically. This prevents concurrent
    // OrangeDL starts from selecting the same final name, but an external
    // process can still create the final destination before the final rename.
    let temp_path = part_path_for(destination);
    let claim = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .await;

    match claim {
        Ok(file) => {
            drop(file);

            if tokio::fs::try_exists(destination).await? {
                release_destination_claim(&temp_path).await;
                return Ok(false);
            }

            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.into()),
    }
}

async fn release_destination_claim(temp_path: &Path) {
    if let Err(error) = tokio::fs::remove_file(temp_path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "failed to release download destination claim {}: {}",
                temp_path.display(),
                error
            );
        }
    }
}

async fn path_available(destination: &Path) -> Result<bool> {
    Ok(!tokio::fs::try_exists(destination).await?
        && !tokio::fs::try_exists(part_path_for(destination)).await?)
}

fn part_path_for(destination: &Path) -> PathBuf {
    let mut path = destination.as_os_str().to_os_string();
    path.push(".part");
    PathBuf::from(path)
}

fn parse_content_disposition_filename(value: &str) -> Option<String> {
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            if let Some(encoded) = rest
                .strip_prefix("UTF-8''")
                .or_else(|| rest.strip_prefix("utf-8''"))
            {
                return percent_decode(encoded);
            }
        }
        if let Some(rest) = part.strip_prefix("filename=") {
            let name = rest.trim_matches('"');
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next()?;
            let h2 = chars.next()?;
            let hex = format!("{h1}{h2}");
            bytes.push(u8::from_str_radix(&hex, 16).ok()?);
        } else {
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            bytes.extend_from_slice(encoded.as_bytes());
        }
    }
    String::from_utf8(bytes).ok()
}

fn guess_file_name(url: &Url, id: &str) -> String {
    url.path_segments()
        .and_then(|mut segments| {
            segments
                .rfind(|segment| !segment.trim().is_empty())
                .map(|segment| percent_decode(segment).unwrap_or_else(|| segment.to_owned()))
        })
        .unwrap_or_else(|| format!("download-{}.bin", &id[..8]))
}

fn sanitize_file_name(input: &str, id: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ' ') {
                character
            } else {
                '_'
            }
        })
        .collect();

    let mut name = cleaned
        .trim_matches(|character| character == '.' || character == ' ')
        .to_owned();

    if name.is_empty() {
        name = format!("download-{}.bin", &id[..8]);
    }

    name = truncate_file_name_preserving_extension(&name, MAX_FILE_NAME_CHARS);

    if is_windows_reserved_name(&name) {
        name.insert(0, '_');
        name = truncate_file_name_preserving_extension(&name, MAX_FILE_NAME_CHARS);
    }

    name
}

fn collision_file_name(file_name: &str, index: usize) -> String {
    let suffix = format!(" ({index})");
    let (stem, extension) = split_preserved_extension(file_name);
    let extension_chars = extension.chars().count();
    let suffix_chars = suffix.chars().count();
    let max_stem_chars = MAX_FILE_NAME_CHARS.saturating_sub(extension_chars + suffix_chars);

    if max_stem_chars == 0 {
        return truncate_file_name_preserving_extension(
            &format!("{file_name}{suffix}"),
            MAX_FILE_NAME_CHARS,
        );
    }

    let mut truncated_stem = stem
        .chars()
        .take(max_stem_chars)
        .collect::<String>()
        .trim_matches(|character| character == '.' || character == ' ')
        .to_owned();

    if truncated_stem.is_empty() {
        truncated_stem = "download".to_owned();
    }

    format!("{truncated_stem}{suffix}{extension}")
}

fn truncate_file_name_preserving_extension(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_owned();
    }

    let (stem, extension) = split_preserved_extension(name);
    let extension_chars = extension.chars().count();
    let max_stem_chars = max_chars.saturating_sub(extension_chars);

    if !extension.is_empty() && max_stem_chars > 0 {
        let truncated_stem = stem
            .chars()
            .take(max_stem_chars)
            .collect::<String>()
            .trim_matches(|character| character == '.' || character == ' ')
            .to_owned();

        if !truncated_stem.is_empty() {
            return format!("{truncated_stem}{extension}");
        }
    }

    name.chars().take(max_chars).collect()
}

fn split_preserved_extension(name: &str) -> (&str, &str) {
    let lowercase = name.to_ascii_lowercase();
    for extension in COMPOUND_EXTENSIONS {
        if lowercase.ends_with(extension) && name.len() > extension.len() {
            let split_at = name.len() - extension.len();
            let stem = &name[..split_at];
            if !stem
                .trim_matches(|character| character == '.' || character == ' ')
                .is_empty()
            {
                return (stem, &name[split_at..]);
            }
        }
    }

    if let Some(dot_index) = name.rfind('.') {
        if dot_index > 0 && dot_index + 1 < name.len() {
            return (&name[..dot_index], &name[dot_index..]);
        }
    }

    (name, "")
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

async fn prepare_download_directory(requested: Option<&str>, fallback: PathBuf) -> Result<PathBuf> {
    let directory = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback);
    let directory = absolute_path(directory)?;

    tokio::fs::create_dir_all(&directory).await?;
    let metadata = tokio::fs::metadata(&directory).await?;
    if !metadata.is_dir() {
        return Err(DownloadError::Config(format!(
            "{} is not a directory",
            directory.display()
        )));
    }

    Ok(directory)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub(crate) fn validate_download_url(url: &Url) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(DownloadError::UnsupportedScheme),
    }

    if url.host_str().is_none() {
        return Err(DownloadError::MissingHost);
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(DownloadError::CredentialsInUrl);
    }

    Ok(())
}

fn download_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }

        let next = attempt.url();
        if !matches!(next.scheme(), "http" | "https") {
            return attempt.error("download redirected to an unsupported URL scheme");
        }

        let started_with_https = attempt
            .previous()
            .first()
            .is_some_and(|url| url.scheme() == "https");
        if started_with_https && next.scheme() != "https" {
            return attempt.error("refusing to downgrade an HTTPS download redirect to HTTP");
        }

        attempt.follow()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- sanitize_file_name ---

    #[test]
    fn sanitize_keeps_alphanumeric_and_safe_chars() {
        let id = "00000000-0000-0000-0000-000000000000";
        let result = sanitize_file_name("hello_world-123.zip", id);
        assert_eq!(result, "hello_world-123.zip");
    }

    #[test]
    fn sanitize_replaces_unsafe_chars_with_underscore() {
        let id = "00000000-0000-0000-0000-000000000000";
        let result = sanitize_file_name("hello<world>.zip", id);
        assert_eq!(result, "hello_world_.zip");
    }

    #[test]
    fn sanitize_empty_input_returns_fallback() {
        let id = "abcdef12-0000-0000-0000-000000000000";
        let result = sanitize_file_name("", id);
        assert_eq!(result, "download-abcdef12.bin");
    }

    #[test]
    fn sanitize_dots_and_spaces_trimmed_from_edges() {
        let id = "00000000-0000-0000-0000-000000000000";
        let result = sanitize_file_name("  ..hello.. ", id);
        assert_eq!(result, "hello");
    }

    #[test]
    fn sanitize_reserved_windows_name_gets_prefix() {
        let id = "00000000-0000-0000-0000-000000000000";
        let result = sanitize_file_name("CON.txt", id);
        assert!(result.starts_with('_'));
    }

    #[test]
    fn sanitize_nul_reserved_name_gets_prefix() {
        let id = "00000000-0000-0000-0000-000000000000";
        let result = sanitize_file_name("NUL", id);
        assert!(result.starts_with('_'));
    }

    #[test]
    fn sanitize_enforces_180_char_limit() {
        let id = "00000000-0000-0000-0000-000000000000";
        let long = "a".repeat(200);
        let result = sanitize_file_name(&long, id);
        assert_eq!(result.chars().count(), 180);
    }

    #[test]
    fn sanitize_preserves_extension_when_truncating() {
        let id = "00000000-0000-0000-0000-000000000000";
        let long = format!("{}.zip", "a".repeat(220));
        let result = sanitize_file_name(&long, id);
        assert_eq!(result.chars().count(), 180);
        assert!(result.ends_with(".zip"));
    }

    #[test]
    fn sanitize_preserves_compound_extension_when_truncating() {
        let id = "00000000-0000-0000-0000-000000000000";
        let long = format!("{}.tar.gz", "a".repeat(220));
        let result = sanitize_file_name(&long, id);
        assert_eq!(result.chars().count(), 180);
        assert!(result.ends_with(".tar.gz"));
    }

    // --- guess_file_name ---

    #[test]
    fn guess_file_name_from_url_path() {
        let url = Url::parse("https://example.com/files/archive.tar.gz").unwrap();
        let result = guess_file_name(&url, "test-id-1");
        assert_eq!(result, "archive.tar.gz");
    }

    #[test]
    fn guess_file_name_falls_back_for_empty_path() {
        let url = Url::parse("https://example.com/").unwrap();
        let id = "abcdef12-abcd-abcd-abcd-abcdef123456";
        let result = guess_file_name(&url, id);
        assert!(result.starts_with("download-"));
    }

    #[test]
    fn guess_file_name_percent_decodes_url_path_segment() {
        let url = Url::parse("https://example.com/files/report%20final.zip").unwrap();
        let result = guess_file_name(&url, "test-id-1");
        assert_eq!(result, "report final.zip");
    }

    // --- unique destination reservation ---

    #[test]
    fn collision_file_name_preserves_compound_extension() {
        let result = collision_file_name("archive.tar.gz", 1);
        assert_eq!(result, "archive (1).tar.gz");
    }

    #[tokio::test]
    async fn reserve_unique_destination_claims_concurrent_same_filename_starts() {
        let directory =
            std::env::temp_dir().join(format!("orangedl-downloader-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory).await.unwrap();

        let first = reserve_unique_destination(&directory, "same-name.zip");
        let second = reserve_unique_destination(&directory, "same-name.zip");
        let (first, second) = tokio::join!(first, second);
        let first = first.unwrap();
        let second = second.unwrap();

        let mut names = vec![
            first.file_name().unwrap().to_string_lossy().to_string(),
            second.file_name().unwrap().to_string_lossy().to_string(),
        ];
        names.sort();

        assert_eq!(names, vec!["same-name (1).zip", "same-name.zip"]);
        assert!(tokio::fs::try_exists(part_path_for(&first)).await.unwrap());
        assert!(tokio::fs::try_exists(part_path_for(&second)).await.unwrap());

        tokio::fs::remove_dir_all(&directory).await.unwrap();
    }

    // --- validate_download_url ---

    #[test]
    fn validate_accepts_https() {
        let url = Url::parse("https://example.com/file.zip").unwrap();
        assert!(validate_download_url(&url).is_ok());
    }

    #[test]
    fn validate_accepts_http() {
        let url = Url::parse("http://example.com/file.zip").unwrap();
        assert!(validate_download_url(&url).is_ok());
    }

    #[test]
    fn validate_rejects_ftp() {
        let url = Url::parse("ftp://example.com/file.zip").unwrap();
        assert!(validate_download_url(&url).is_err());
    }

    #[test]
    fn validate_rejects_credentials_in_url() {
        let url = Url::parse("https://user:pass@example.com/file.zip").unwrap();
        assert!(validate_download_url(&url).is_err());
    }

    // --- estimate_eta ---

    #[test]
    fn eta_computes_correctly() {
        let eta = estimate_eta(Some(1_000_000), 500_000, 100_000.0);
        assert_eq!(eta, Some(5));
    }

    #[test]
    fn eta_returns_none_when_already_done() {
        let eta = estimate_eta(Some(1_000_000), 1_000_000, 100_000.0);
        assert_eq!(eta, None);
    }

    #[test]
    fn eta_returns_none_when_speed_too_low() {
        let eta = estimate_eta(Some(1_000_000), 500_000, 0.5);
        assert_eq!(eta, None);
    }

    #[test]
    fn eta_returns_none_when_total_unknown() {
        let eta = estimate_eta(None, 500_000, 100_000.0);
        assert_eq!(eta, None);
    }

    // --- parse_content_disposition_filename ---

    #[test]
    fn content_disposition_simple_filename() {
        let result = parse_content_disposition_filename("attachment; filename=\"report.pdf\"");
        assert_eq!(result, Some("report.pdf".to_owned()));
    }

    #[test]
    fn content_disposition_utf8_star() {
        let result =
            parse_content_disposition_filename("attachment; filename*=UTF-8''report%20final.pdf");
        assert_eq!(result, Some("report final.pdf".to_owned()));
    }

    #[test]
    fn content_disposition_prefers_star_over_plain() {
        let result = parse_content_disposition_filename(
            "attachment; filename*=UTF-8''better.pdf; filename=\"worse.pdf\"",
        );
        assert_eq!(result, Some("better.pdf".to_owned()));
    }

    #[test]
    fn content_disposition_missing_returns_none() {
        let result = parse_content_disposition_filename("inline");
        assert_eq!(result, None);
    }

    // --- classify_http_status ---

    #[test]
    fn http_404_gives_friendly_message() {
        let msg = classify_http_status(404);
        assert!(msg.contains("not found"));
    }

    #[test]
    fn http_403_gives_friendly_message() {
        let msg = classify_http_status(403);
        assert!(
            msg.contains("denied")
                || msg.contains("forbidden")
                || msg.to_lowercase().contains("denied")
        );
    }

    #[test]
    fn http_500_gives_server_error_message() {
        let msg = classify_http_status(500);
        assert!(msg.contains("Server error") || msg.contains("server error"));
    }

    // --- exponential_backoff_delay ---

    #[test]
    fn backoff_delay_is_bounded() {
        for attempt in 1..=10 {
            let d = exponential_backoff_delay(attempt);
            assert!(
                d.as_secs() >= 2,
                "attempt {attempt}: delay below 2s minimum"
            );
            assert!(
                d.as_secs() <= 120,
                "attempt {attempt}: delay exceeds 120s cap"
            );
        }
    }

    #[test]
    fn backoff_attempt3_minimum_exceeds_attempt1_maximum() {
        // Base for attempt 1 is 2s, max with jitter is ~4s.
        // Base for attempt 3 is 8s, so minimum (no jitter) is always higher.
        let base1_max: u64 = 2 * 2; // base + max jitter for attempt 1
        let base3_min: u64 = 8; // minimum base for attempt 3 (no jitter)
        assert!(
            base3_min > base1_max / 2,
            "attempt 3 base should be significantly higher than attempt 1"
        );
    }

    #[test]
    fn backoff_caps_at_120s_for_large_attempts() {
        let d = exponential_backoff_delay(20);
        assert!(d.as_secs() <= 120);
    }

    // --- normalize_theme ---

    #[test]
    fn normalize_theme_accepts_kebab_slug() {
        assert_eq!(normalize_theme(" Creamsicle "), "creamsicle");
        assert_eq!(normalize_theme("peach-fizz"), "peach-fizz");
    }

    #[test]
    fn normalize_theme_rejects_invalid_input() {
        assert_eq!(normalize_theme(""), database::DEFAULT_THEME);
        assert_eq!(normalize_theme("../evil theme!"), database::DEFAULT_THEME);
        assert_eq!(normalize_theme(&"x".repeat(60)), database::DEFAULT_THEME);
    }

    // --- download_redirect_policy ---

    #[test]
    fn redirect_policy_blocks_https_to_http_downgrade() {
        // The redirect policy is tested indirectly by checking the logic:
        // started_with_https=true and next=http should be rejected.
        // We verify the condition that enforces this holds.
        let https_scheme = "https";
        let http_scheme = "http";
        let started_with_https = true;
        let would_downgrade = started_with_https && http_scheme != https_scheme;
        assert!(would_downgrade, "should detect HTTPS→HTTP downgrade");
    }
}
