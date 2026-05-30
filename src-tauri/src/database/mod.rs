use crate::downloader::{DownloadError, Result};
use crate::models::{AppSettings, Download, DownloadRow, DownloadStatus};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const KEY_DEFAULT_DOWNLOAD_DIRECTORY: &str = "default_download_directory";
const KEY_DEFAULT_SPEED_LIMIT_BPS: &str = "default_speed_limit_bps";

pub async fn connect(app: &AppHandle) -> Result<SqlitePool> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| DownloadError::Config(error.to_string()))?;

    tokio::fs::create_dir_all(&data_dir).await?;

    let db_path = data_dir.join("orangedl.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS downloads (
            id TEXT PRIMARY KEY NOT NULL,
            url TEXT NOT NULL,
            file_name TEXT NOT NULL,
            destination TEXT NOT NULL,
            temp_path TEXT NOT NULL,
            total_bytes INTEGER,
            downloaded_bytes INTEGER NOT NULL DEFAULT 0,
            status TEXT NOT NULL,
            speed_bps REAL NOT NULL DEFAULT 0,
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            speed_limit_bps INTEGER
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn reset_interrupted(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE downloads
        SET status = 'paused',
            speed_bps = 0,
            updated_at = ?
        WHERE status IN ('queued', 'downloading')
        "#,
    )
    .bind(now())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn insert_download(pool: &SqlitePool, download: &Download) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO downloads (
            id, url, file_name, destination, temp_path, total_bytes,
            downloaded_bytes, status, speed_bps, error, created_at, updated_at, speed_limit_bps
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&download.id)
    .bind(&download.url)
    .bind(&download.file_name)
    .bind(&download.destination)
    .bind(&download.temp_path)
    .bind(download.total_bytes.map(to_i64))
    .bind(to_i64(download.downloaded_bytes))
    .bind(download.status.as_str())
    .bind(download.speed_bps)
    .bind(&download.error)
    .bind(&download.created_at)
    .bind(&download.updated_at)
    .bind(download.speed_limit_bps.map(to_i64))
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_downloads(pool: &SqlitePool) -> Result<Vec<Download>> {
    let rows = sqlx::query(
        r#"
        SELECT id, url, file_name, destination, temp_path, total_bytes, downloaded_bytes,
               status, speed_bps, error, created_at, updated_at, speed_limit_bps
        FROM downloads
        ORDER BY created_at DESC
        "#,
    )
    .try_map(download_from_row)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn get_download(pool: &SqlitePool, id: &str) -> Result<Option<Download>> {
    let row = sqlx::query(
        r#"
        SELECT id, url, file_name, destination, temp_path, total_bytes, downloaded_bytes,
               status, speed_bps, error, created_at, updated_at, speed_limit_bps
        FROM downloads
        WHERE id = ?
        "#,
    )
    .bind(id)
    .try_map(download_from_row)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn update_progress(
    pool: &SqlitePool,
    id: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    speed_bps: f64,
) -> Result<Option<Download>> {
    sqlx::query(
        r#"
        UPDATE downloads
        SET downloaded_bytes = ?,
            total_bytes = COALESCE(?, total_bytes),
            speed_bps = ?,
            status = 'downloading',
            error = NULL,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(to_i64(downloaded_bytes))
    .bind(total_bytes.map(to_i64))
    .bind(speed_bps.max(0.0))
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;

    get_download(pool, id).await
}

pub async fn set_status(
    pool: &SqlitePool,
    id: &str,
    status: DownloadStatus,
    error: Option<&str>,
) -> Result<Option<Download>> {
    sqlx::query(
        r#"
        UPDATE downloads
        SET status = ?,
            speed_bps = CASE WHEN ? IN ('paused', 'completed', 'failed', 'cancelled') THEN 0 ELSE speed_bps END,
            error = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(status.as_str())
    .bind(status.as_str())
    .bind(error)
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;

    get_download(pool, id).await
}

pub async fn set_total_bytes(pool: &SqlitePool, id: &str, total_bytes: Option<u64>) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE downloads
        SET total_bytes = COALESCE(?, total_bytes),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(total_bytes.map(to_i64))
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_download(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM downloads WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_app_settings(pool: &SqlitePool, app: &AppHandle) -> Result<AppSettings> {
    let default_download_directory = match get_setting(pool, KEY_DEFAULT_DOWNLOAD_DIRECTORY).await?
    {
        Some(value) if !value.trim().is_empty() => value,
        _ => system_download_dir(app)?.to_string_lossy().to_string(),
    };

    let default_speed_limit_bps = get_setting(pool, KEY_DEFAULT_SPEED_LIMIT_BPS)
        .await?
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0);

    Ok(AppSettings {
        default_download_directory,
        default_speed_limit_bps,
    })
}

pub async fn set_default_download_directory(pool: &SqlitePool, directory: &str) -> Result<()> {
    set_setting(pool, KEY_DEFAULT_DOWNLOAD_DIRECTORY, directory).await
}

pub async fn set_default_speed_limit(
    pool: &SqlitePool,
    speed_limit_bps: Option<u64>,
) -> Result<()> {
    match speed_limit_bps.filter(|value| *value > 0) {
        Some(value) => set_setting(pool, KEY_DEFAULT_SPEED_LIMIT_BPS, &value.to_string()).await,
        None => delete_setting(pool, KEY_DEFAULT_SPEED_LIMIT_BPS).await,
    }
}

pub fn system_download_dir(app: &AppHandle) -> Result<PathBuf> {
    match app.path().download_dir() {
        Ok(path) => Ok(path),
        Err(_) => app
            .path()
            .app_data_dir()
            .map(|path| path.join("downloads"))
            .map_err(|error| DownloadError::Config(error.to_string())),
    }
}

async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    let row = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;

    Ok(row)
}

async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(now())
    .execute(pool)
    .await?;

    Ok(())
}

async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;

    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn download_from_row(row: SqliteRow) -> std::result::Result<Download, sqlx::Error> {
    Ok(Download::from(DownloadRow {
        id: row.try_get("id")?,
        url: row.try_get("url")?,
        file_name: row.try_get("file_name")?,
        destination: row.try_get("destination")?,
        temp_path: row.try_get("temp_path")?,
        total_bytes: row.try_get("total_bytes")?,
        downloaded_bytes: row.try_get("downloaded_bytes")?,
        status: row.try_get("status")?,
        speed_bps: row.try_get("speed_bps")?,
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        speed_limit_bps: row.try_get("speed_limit_bps")?,
    }))
}
