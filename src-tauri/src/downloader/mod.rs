use crate::database;
use crate::models::{
    AppSettings, Download, DownloadStatus, StartDownloadRequest, UpdateSettingsRequest,
};
use dashmap::DashMap;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, DownloadError>;

const MAX_RETRIES: usize = 3;
const MAX_REDIRECTS: usize = 10;
const MAX_IDLE_CONNECTIONS_PER_HOST: usize = 8;
const WRITE_BUFFER_SIZE: usize = 1024 * 1024;
const PROGRESS_INTERVAL_MS: u64 = 750;
const INTENT_NONE: u8 = 0;
const INTENT_PAUSE: u8 = 1;
const INTENT_CANCEL: u8 = 2;

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
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("server returned HTTP {0}")]
    HttpStatus(u16),
    #[error("configuration error: {0}")]
    Config(String),
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
            INTENT_PAUSE | INTENT_NONE => DownloadStatus::Paused,
            _ => DownloadStatus::Paused,
        }
    }
}

struct RunningDownload {
    token: CancellationToken,
    intent: Arc<AtomicU8>,
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
}

impl DownloadManager {
    pub fn new(app: AppHandle, pool: SqlitePool) -> Self {
        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<Download>();
        let app_for_events = app.clone();

        tauri::async_runtime::spawn(async move {
            use tauri::Emitter;

            while let Some(download) = progress_rx.recv().await {
                let _ = app_for_events.emit("download-progress", download.clone());

                match download.status {
                    DownloadStatus::Completed => {
                        let _ = app_for_events.emit("download-finished", download.clone());
                    }
                    DownloadStatus::Failed | DownloadStatus::Cancelled | DownloadStatus::Paused => {
                        let _ = app_for_events.emit("download-status", download.clone());
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
        }
    }

    pub async fn initialize(&self) -> Result<()> {
        database::reset_interrupted(&self.pool).await
    }

    pub async fn list_downloads(&self) -> Result<Vec<Download>> {
        database::list_downloads(&self.pool).await
    }

    pub async fn app_settings(&self) -> Result<AppSettings> {
        database::get_app_settings(&self.pool, &self.app).await
    }

    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> Result<AppSettings> {
        let directory = prepare_download_directory(
            request.default_download_directory.as_deref(),
            database::system_download_dir(&self.app)?,
        )
        .await?;

        let directory_string = directory.to_string_lossy().to_string();
        database::set_default_download_directory(&self.pool, &directory_string).await?;
        database::set_default_speed_limit(
            &self.pool,
            request.default_speed_limit_bps.filter(|value| *value > 0),
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
        let destination = unique_destination(&directory, &file_name).await?;
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

        database::insert_download(&self.pool, &download).await?;
        publish(&self.progress_tx, download.clone());
        self.spawn_download(id);
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
        self.spawn_download(id.to_owned());
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

    fn spawn_download(&self, id: String) {
        let token = CancellationToken::new();
        let intent = Arc::new(AtomicU8::new(INTENT_NONE));

        if let Some(running) = self.tasks.insert(
            id.clone(),
            RunningDownload {
                token: token.clone(),
                intent: Arc::clone(&intent),
            },
        ) {
            running.intent.store(INTENT_PAUSE, Ordering::SeqCst);
            running.token.cancel();
        }

        let control = DownloadControl { token, intent };
        let pool = self.pool.clone();
        let client = self.client.clone();
        let progress_tx = self.progress_tx.clone();
        let tasks = Arc::clone(&self.tasks);

        tauri::async_runtime::spawn(async move {
            Self::run_download(id.clone(), client, pool, control, progress_tx).await;
            tasks.remove(&id);
        });
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
                        let delay = Duration::from_secs((attempt * 2) as u64);
                        let message = format!(
                            "Retry {attempt}/{MAX_RETRIES} in {}s: {error}",
                            delay.as_secs()
                        );

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

        let mut request = client.get(download_url);

        if downloaded_bytes > 0 {
            request = request.header(RANGE, format!("bytes={downloaded_bytes}-"));
        }

        let response = tokio::select! {
            _ = control.token.cancelled() => return Ok(DownloadExit::Interrupted),
            response = request.send() => response?,
        };
        let status = response.status();

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

        let append = downloaded_bytes > 0 && status == StatusCode::PARTIAL_CONTENT;
        if downloaded_bytes > 0 && !append {
            downloaded_bytes = 0;
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

            throttle_if_needed(
                download.speed_limit_bps,
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

async fn remove_partial_files(temp_path: &Path) {
    let _ = tokio::fs::remove_file(temp_path).await;
}

fn publish(progress_tx: &mpsc::UnboundedSender<Download>, download: Download) {
    let _ = progress_tx.send(download);
}

fn estimate_eta(total_bytes: Option<u64>, downloaded_bytes: u64, speed_bps: f64) -> Option<u64> {
    let total = total_bytes?;
    if speed_bps <= 1.0 || downloaded_bytes >= total {
        return None;
    }

    Some(((total - downloaded_bytes) as f64 / speed_bps).ceil() as u64)
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

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 1..10_000 {
        let candidate_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} ({index}).{extension}"),
            _ => format!("{stem} ({index})"),
        };
        let candidate = directory.join(candidate_name);

        if path_available(&candidate).await? {
            return Ok(candidate);
        }
    }

    Err(DownloadError::Config(
        "could not find an available destination name".to_owned(),
    ))
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

fn guess_file_name(url: &Url, id: &str) -> String {
    url.path_segments()
        .and_then(|mut segments| {
            segments
                .rfind(|segment| !segment.trim().is_empty())
                .map(ToOwned::to_owned)
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
        .chars()
        .take(180)
        .collect::<String>();

    if name.is_empty() {
        name = format!("download-{}.bin", &id[..8]);
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();

    if reserved.contains(&stem.as_str()) {
        name.insert(0, '_');
    }

    name
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

fn validate_download_url(url: &Url) -> Result<()> {
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
