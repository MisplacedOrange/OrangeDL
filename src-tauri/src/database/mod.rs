// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

use crate::downloader::{DownloadError, Result};
use crate::models::{AppSettings, Download, DownloadRow, DownloadStatus, ExecutorSummary};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous,
};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const KEY_DEFAULT_DOWNLOAD_DIRECTORY: &str = "default_download_directory";
const KEY_DEFAULT_SPEED_LIMIT_BPS: &str = "default_speed_limit_bps";
const KEY_GLOBAL_SPEED_LIMIT_BPS: &str = "global_speed_limit_bps";
const KEY_MAX_CONCURRENT_DOWNLOADS: &str = "max_concurrent_downloads";
const KEY_AUTO_RESUME_INTERRUPTED: &str = "auto_resume_interrupted_downloads";
const KEY_CLOSE_TO_TRAY: &str = "close_to_tray";
const KEY_NOTIFICATIONS_ENABLED: &str = "notifications_enabled";
const KEY_NOTIFICATION_SOUND: &str = "notification_sound";
const KEY_BACKGROUND_UPDATE_NOTIFICATIONS: &str = "background_update_notifications";
const KEY_AUTO_OPEN_FOLDER_ON_COMPLETION: &str = "auto_open_folder_on_completion";
const KEY_HISTORY_RETENTION_DAYS: &str = "history_retention_days";
const KEY_HISTORY_MAX_ROWS: &str = "history_max_rows";
const KEY_FIRST_RUN_COMPLETED: &str = "first_run_completed";
const KEY_THEME: &str = "theme";

pub const DEFAULT_THEME: &str = "creamsicle";

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
    // Schema version table — tracks applied migrations so each version runs exactly once.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;

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

    // v2: add executor columns
    let executor_columns = [
        "ALTER TABLE downloads ADD COLUMN queued_at TEXT",
        "ALTER TABLE downloads ADD COLUMN started_at TEXT",
        "ALTER TABLE downloads ADD COLUMN completed_at TEXT",
        "ALTER TABLE downloads ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3",
        "ALTER TABLE downloads ADD COLUMN next_retry_at TEXT",
        "ALTER TABLE downloads ADD COLUMN last_error_kind TEXT",
        "ALTER TABLE downloads ADD COLUMN source_host TEXT",
        "ALTER TABLE downloads ADD COLUMN etag TEXT",
        "ALTER TABLE downloads ADD COLUMN last_modified TEXT",
        "ALTER TABLE downloads ADD COLUMN checksum_sha256 TEXT",
    ];

    for sql in &executor_columns {
        add_column_if_missing(pool, sql).await?;
    }

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_downloads_queue ON downloads(status, created_at ASC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_downloads_retry ON downloads(next_retry_at) WHERE next_retry_at IS NOT NULL",
    )
    .execute(pool)
    .await?;

    // Record schema version 2 as applied (idempotent).
    let _ =
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (2, ?)")
            .bind(now())
            .execute(pool)
            .await;

    // v3: media/video download support
    add_column_if_missing(pool, "ALTER TABLE downloads ADD COLUMN media_format TEXT").await?;

    let _ =
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (3, ?)")
            .bind(now())
            .execute(pool)
            .await;

    Ok(())
}

async fn add_column_if_missing(pool: &SqlitePool, sql: &str) -> Result<()> {
    match sqlx::query(sql).execute(pool).await {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(e)) if e.message().contains("duplicate column") => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub async fn reset_interrupted(pool: &SqlitePool, auto_resume: bool) -> Result<()> {
    let target_status = if auto_resume { "queued" } else { "paused" };

    sqlx::query(
        r#"
        UPDATE downloads
        SET status = ?,
            speed_bps = 0,
            updated_at = ?
        WHERE status IN ('queued', 'downloading')
        "#,
    )
    .bind(target_status)
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
            downloaded_bytes, status, speed_bps, error, created_at, updated_at,
            speed_limit_bps, retry_count, max_retries,
            queued_at, checksum_sha256
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(download.retry_count as i64)
    .bind(download.max_retries as i64)
    .bind(now())
    .bind(download.checksum_sha256.as_deref())
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_downloads(pool: &SqlitePool) -> Result<Vec<Download>> {
    let rows = sqlx::query(
        r#"
        SELECT id, url, file_name, destination, temp_path, total_bytes, downloaded_bytes,
               status, speed_bps, error, created_at, updated_at, speed_limit_bps,
               COALESCE(retry_count, 0) as retry_count,
               COALESCE(max_retries, 3) as max_retries,
               checksum_sha256
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
               status, speed_bps, error, created_at, updated_at, speed_limit_bps,
               COALESCE(retry_count, 0) as retry_count,
               COALESCE(max_retries, 3) as max_retries,
               checksum_sha256
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

pub async fn get_next_queued(pool: &SqlitePool, limit: u32) -> Result<Vec<Download>> {
    let rows = sqlx::query(
        r#"
        SELECT id, url, file_name, destination, temp_path, total_bytes, downloaded_bytes,
               status, speed_bps, error, created_at, updated_at, speed_limit_bps,
               COALESCE(retry_count, 0) as retry_count,
               COALESCE(max_retries, 3) as max_retries,
               checksum_sha256
        FROM downloads
        WHERE status = 'queued'
        ORDER BY created_at ASC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .try_map(download_from_row)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn get_executor_summary(pool: &SqlitePool) -> Result<ExecutorSummary> {
    let counts = get_executor_counts(pool).await?;
    let total_speed = get_total_speed_bps(pool).await?;
    Ok(ExecutorSummary {
        active: *counts.get("downloading").unwrap_or(&0),
        queued: *counts.get("queued").unwrap_or(&0),
        paused: *counts.get("paused").unwrap_or(&0),
        completed: *counts.get("completed").unwrap_or(&0),
        failed: *counts.get("failed").unwrap_or(&0),
        cancelled: *counts.get("cancelled").unwrap_or(&0),
        max_concurrent: get_max_concurrent_downloads(pool).await?,
        total_speed_bps: total_speed,
    })
}

pub async fn get_executor_counts(pool: &SqlitePool) -> Result<HashMap<String, i64>> {
    let rows = sqlx::query("SELECT status, COUNT(*) as cnt FROM downloads GROUP BY status")
        .fetch_all(pool)
        .await?;

    let mut map = HashMap::new();
    for row in rows {
        let status: String = row.try_get("status").unwrap_or_default();
        let cnt: i64 = row.try_get("cnt").unwrap_or(0);
        map.insert(status, cnt);
    }

    Ok(map)
}

pub async fn get_total_speed_bps(pool: &SqlitePool) -> Result<f64> {
    let speed: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(speed_bps), 0.0) FROM downloads WHERE status = 'downloading'",
    )
    .fetch_one(pool)
    .await?;

    Ok(speed.max(0.0))
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
        WHERE id = ? AND status = 'downloading'
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
    let completed_at = if status == DownloadStatus::Completed {
        Some(now())
    } else {
        None
    };

    sqlx::query(
        r#"
        UPDATE downloads
        SET status = ?,
            speed_bps = CASE WHEN ? IN ('paused', 'completed', 'failed', 'cancelled') THEN 0 ELSE speed_bps END,
            error = ?,
            completed_at = COALESCE(?, completed_at),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(status.as_str())
    .bind(status.as_str())
    .bind(error)
    .bind(completed_at)
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

pub async fn increment_retry_count(pool: &SqlitePool, id: &str) -> Result<u32> {
    sqlx::query("UPDATE downloads SET retry_count = retry_count + 1, updated_at = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;

    let count: i64 =
        sqlx::query_scalar("SELECT COALESCE(retry_count, 0) FROM downloads WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;

    Ok(count.max(0) as u32)
}

pub async fn delete_download(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM downloads WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn set_all_downloading_and_queued_to_paused(pool: &SqlitePool) -> Result<Vec<String>> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM downloads WHERE status IN ('downloading', 'queued')")
            .fetch_all(pool)
            .await?;

    if !ids.is_empty() {
        sqlx::query(
            "UPDATE downloads SET status = 'paused', speed_bps = 0, updated_at = ? WHERE status IN ('downloading', 'queued')",
        )
        .bind(now())
        .execute(pool)
        .await?;
    }

    Ok(ids)
}

pub async fn set_all_paused_to_queued(pool: &SqlitePool) -> Result<Vec<String>> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM downloads WHERE status = 'paused'")
        .fetch_all(pool)
        .await?;

    if !ids.is_empty() {
        sqlx::query(
            "UPDATE downloads SET status = 'queued', updated_at = ? WHERE status = 'paused'",
        )
        .bind(now())
        .execute(pool)
        .await?;
    }

    Ok(ids)
}

pub async fn set_all_failed_to_queued(pool: &SqlitePool) -> Result<Vec<String>> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM downloads WHERE status = 'failed'")
        .fetch_all(pool)
        .await?;

    if !ids.is_empty() {
        sqlx::query(
            "UPDATE downloads SET status = 'queued', error = NULL, retry_count = 0, next_retry_at = NULL, updated_at = ? WHERE status = 'failed'",
        )
        .bind(now())
        .execute(pool)
        .await?;
    }

    Ok(ids)
}

pub async fn delete_downloads_by_status(
    pool: &SqlitePool,
    status: DownloadStatus,
) -> Result<Vec<String>> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM downloads WHERE status = ?")
        .bind(status.as_str())
        .fetch_all(pool)
        .await?;

    if !ids.is_empty() {
        sqlx::query("DELETE FROM downloads WHERE status = ?")
            .bind(status.as_str())
            .execute(pool)
            .await?;
    }

    Ok(ids)
}

pub async fn update_download_speed_limit(
    pool: &SqlitePool,
    id: &str,
    speed_limit_bps: Option<u64>,
) -> Result<Option<Download>> {
    sqlx::query("UPDATE downloads SET speed_limit_bps = ?, updated_at = ? WHERE id = ?")
        .bind(speed_limit_bps.map(to_i64))
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;

    get_download(pool, id).await
}

pub async fn get_app_settings(pool: &SqlitePool, app: &AppHandle) -> Result<AppSettings> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    let values: HashMap<String, String> = rows.into_iter().collect();

    let get = |key: &str| values.get(key).map(String::as_str);
    let get_bool = |key: &str, fallback: bool| {
        get(key)
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(fallback)
    };
    let get_u32 = |key: &str| {
        get(key)
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
    };
    let get_u64 = |key: &str| {
        get(key)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
    };

    let default_download_directory = match get(KEY_DEFAULT_DOWNLOAD_DIRECTORY) {
        Some(value) if !value.trim().is_empty() => value.to_owned(),
        _ => system_download_dir(app)?.to_string_lossy().to_string(),
    };

    Ok(AppSettings {
        default_download_directory,
        default_speed_limit_bps: get_u64(KEY_DEFAULT_SPEED_LIMIT_BPS),
        global_speed_limit_bps: get_u64(KEY_GLOBAL_SPEED_LIMIT_BPS),
        max_concurrent_downloads: get_u32(KEY_MAX_CONCURRENT_DOWNLOADS).unwrap_or(3),
        auto_resume_interrupted_downloads: get_bool(KEY_AUTO_RESUME_INTERRUPTED, false),
        close_to_tray: get_bool(KEY_CLOSE_TO_TRAY, true),
        notifications_enabled: get_bool(KEY_NOTIFICATIONS_ENABLED, true),
        notification_sound: get_bool(KEY_NOTIFICATION_SOUND, false),
        background_update_notifications: get_bool(KEY_BACKGROUND_UPDATE_NOTIFICATIONS, false),
        auto_open_folder_on_completion: get_bool(KEY_AUTO_OPEN_FOLDER_ON_COMPLETION, false),
        history_retention_days: get_u32(KEY_HISTORY_RETENTION_DAYS),
        history_max_rows: get_u32(KEY_HISTORY_MAX_ROWS),
        first_run_completed: get_bool(KEY_FIRST_RUN_COMPLETED, false),
        theme: get(KEY_THEME)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| DEFAULT_THEME.to_owned()),
    })
}

pub async fn get_max_concurrent_downloads(pool: &SqlitePool) -> Result<u32> {
    let value = get_setting(pool, KEY_MAX_CONCURRENT_DOWNLOADS)
        .await?
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(3);

    Ok(value)
}

pub async fn get_global_speed_limit(pool: &SqlitePool) -> Result<Option<u64>> {
    Ok(get_setting(pool, KEY_GLOBAL_SPEED_LIMIT_BPS)
        .await?
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0))
}

pub async fn set_default_download_directory(pool: &SqlitePool, directory: &str) -> Result<()> {
    set_setting(pool, KEY_DEFAULT_DOWNLOAD_DIRECTORY, directory).await
}

pub async fn set_default_speed_limit(
    pool: &SqlitePool,
    speed_limit_bps: Option<u64>,
) -> Result<()> {
    set_optional_u64(pool, KEY_DEFAULT_SPEED_LIMIT_BPS, speed_limit_bps).await
}

pub async fn set_global_speed_limit(pool: &SqlitePool, speed_limit_bps: Option<u64>) -> Result<()> {
    set_optional_u64(pool, KEY_GLOBAL_SPEED_LIMIT_BPS, speed_limit_bps).await
}

pub async fn set_max_concurrent_downloads(pool: &SqlitePool, value: u32) -> Result<()> {
    set_setting(
        pool,
        KEY_MAX_CONCURRENT_DOWNLOADS,
        &value.max(1).to_string(),
    )
    .await
}

pub async fn set_auto_resume_interrupted(pool: &SqlitePool, value: bool) -> Result<()> {
    set_setting(
        pool,
        KEY_AUTO_RESUME_INTERRUPTED,
        if value { "true" } else { "false" },
    )
    .await
}

pub async fn set_close_to_tray(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_CLOSE_TO_TRAY, value).await
}

pub async fn set_notifications_enabled(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_NOTIFICATIONS_ENABLED, value).await
}

pub async fn set_notification_sound(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_NOTIFICATION_SOUND, value).await
}

pub async fn set_background_update_notifications(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_BACKGROUND_UPDATE_NOTIFICATIONS, value).await
}

pub async fn set_auto_open_folder_on_completion(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_AUTO_OPEN_FOLDER_ON_COMPLETION, value).await
}

pub async fn set_history_retention_days(pool: &SqlitePool, value: Option<u32>) -> Result<()> {
    set_optional_u32(pool, KEY_HISTORY_RETENTION_DAYS, value).await
}

pub async fn set_history_max_rows(pool: &SqlitePool, value: Option<u32>) -> Result<()> {
    set_optional_u32(pool, KEY_HISTORY_MAX_ROWS, value).await
}

pub async fn set_first_run_completed(pool: &SqlitePool, value: bool) -> Result<()> {
    set_bool_setting(pool, KEY_FIRST_RUN_COMPLETED, value).await
}

pub async fn set_theme(pool: &SqlitePool, theme: &str) -> Result<()> {
    set_setting(pool, KEY_THEME, theme).await
}

pub async fn get_download_validators(
    pool: &SqlitePool,
    id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let row = sqlx::query("SELECT etag, last_modified FROM downloads WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(match row {
        Some(r) => (
            r.try_get("etag").ok().flatten(),
            r.try_get("last_modified").ok().flatten(),
        ),
        None => (None, None),
    })
}

pub async fn clear_validators(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE downloads SET etag = NULL, last_modified = NULL, updated_at = ? WHERE id = ?",
    )
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn store_validators(
    pool: &SqlitePool,
    id: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE downloads SET etag = COALESCE(?, etag), last_modified = COALESCE(?, last_modified), updated_at = ? WHERE id = ?",
    )
    .bind(etag)
    .bind(last_modified)
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_download_filename(pool: &SqlitePool, id: &str, file_name: &str) -> Result<()> {
    sqlx::query("UPDATE downloads SET file_name = ?, updated_at = ? WHERE id = ?")
        .bind(file_name)
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_download_destination(
    pool: &SqlitePool,
    id: &str,
    destination: &str,
    file_name: &str,
) -> Result<()> {
    sqlx::query("UPDATE downloads SET destination = ?, file_name = ?, updated_at = ? WHERE id = ?")
        .bind(destination)
        .bind(file_name)
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_next_retry_at(pool: &SqlitePool, id: &str, retry_at: &str) -> Result<()> {
    sqlx::query("UPDATE downloads SET next_retry_at = ?, updated_at = ? WHERE id = ?")
        .bind(retry_at)
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_download_checksum(
    pool: &SqlitePool,
    id: &str,
    checksum_sha256: Option<&str>,
) -> Result<Option<Download>> {
    sqlx::query("UPDATE downloads SET checksum_sha256 = ?, updated_at = ? WHERE id = ?")
        .bind(checksum_sha256)
        .bind(now())
        .bind(id)
        .execute(pool)
        .await?;

    get_download(pool, id).await
}

pub async fn cleanup_history(
    pool: &SqlitePool,
    retention_days: Option<u32>,
    max_rows: Option<u32>,
) -> Result<Vec<String>> {
    let mut removed = Vec::new();

    if let Some(days) = retention_days.filter(|value| *value > 0) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
        let cutoff = cutoff.to_rfc3339();
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM downloads WHERE status IN ('completed', 'failed', 'cancelled') AND updated_at < ?",
        )
        .bind(&cutoff)
        .fetch_all(pool)
        .await?;

        if !ids.is_empty() {
            sqlx::query(
                "DELETE FROM downloads WHERE status IN ('completed', 'failed', 'cancelled') AND updated_at < ?",
            )
            .bind(cutoff)
            .execute(pool)
            .await?;
            removed.extend(ids);
        }
    }

    if let Some(limit) = max_rows.filter(|value| *value > 0) {
        let ids: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM downloads
            WHERE status IN ('completed', 'failed', 'cancelled')
            ORDER BY updated_at DESC
            LIMIT -1 OFFSET ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await?;

        if !ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("DELETE FROM downloads WHERE id IN ({placeholders})");
            let mut query = sqlx::query(&sql);
            for id in &ids {
                query = query.bind(id);
            }
            query.execute(pool).await?;
            removed.extend(ids);
        }
    }

    removed.sort();
    removed.dedup();
    Ok(removed)
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

#[cfg(test)]
async fn get_bool_setting(pool: &SqlitePool, key: &str, fallback: bool) -> Result<bool> {
    Ok(get_setting(pool, key)
        .await?
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(fallback))
}

#[cfg(test)]
async fn get_u32_setting(pool: &SqlitePool, key: &str) -> Result<Option<u32>> {
    Ok(get_setting(pool, key)
        .await?
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0))
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

async fn set_bool_setting(pool: &SqlitePool, key: &str, value: bool) -> Result<()> {
    set_setting(pool, key, if value { "true" } else { "false" }).await
}

async fn set_optional_u64(pool: &SqlitePool, key: &str, value: Option<u64>) -> Result<()> {
    match value.filter(|value| *value > 0) {
        Some(value) => set_setting(pool, key, &value.to_string()).await,
        None => delete_setting(pool, key).await,
    }
}

async fn set_optional_u32(pool: &SqlitePool, key: &str, value: Option<u32>) -> Result<()> {
    match value.filter(|value| *value > 0) {
        Some(value) => set_setting(pool, key, &value.to_string()).await,
        None => delete_setting(pool, key).await,
    }
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
        retry_count: row.try_get("retry_count")?,
        max_retries: row.try_get("max_retries")?,
        checksum_sha256: row.try_get("checksum_sha256")?,
    }))
}

pub async fn get_media_format(pool: &SqlitePool, id: &str) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT media_format FROM downloads WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(|opt: Option<Option<String>>| opt.flatten())
        .map_err(Into::into)
}

pub async fn insert_video_download(
    pool: &SqlitePool,
    download: &Download,
    media_format: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO downloads (
            id, url, file_name, destination, temp_path, total_bytes,
            downloaded_bytes, status, speed_bps, error, created_at, updated_at,
            speed_limit_bps, retry_count, max_retries,
            queued_at, checksum_sha256, media_format
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(download.retry_count as i64)
    .bind(download.max_retries as i64)
    .bind(now())
    .bind(download.checksum_sha256.as_deref())
    .bind(media_format)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn in_memory_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite");
        migrate(&pool).await.expect("migration");
        pool
    }

    #[tokio::test]
    async fn migration_is_idempotent() {
        let pool = in_memory_pool().await;
        // Running migration twice must not error
        migrate(&pool).await.expect("second migration run");
    }

    #[tokio::test]
    async fn insert_and_retrieve_download() {
        let pool = in_memory_pool().await;

        let dl = crate::models::Download::new(
            "test-id-1".to_string(),
            "https://example.com/file.zip".to_string(),
            "file.zip".to_string(),
            "/tmp/file.zip".to_string(),
            "/tmp/file.zip.part".to_string(),
            None,
        );

        insert_download(&pool, &dl).await.expect("insert");
        let result = get_download(&pool, "test-id-1").await.expect("get");
        assert!(result.is_some());
        let fetched = result.unwrap();
        assert_eq!(fetched.id, "test-id-1");
        assert_eq!(fetched.status, crate::models::DownloadStatus::Queued);
    }

    #[tokio::test]
    async fn set_status_changes_download_status() {
        let pool = in_memory_pool().await;

        let dl = crate::models::Download::new(
            "test-id-2".to_string(),
            "https://example.com/file.zip".to_string(),
            "file.zip".to_string(),
            "/tmp/file.zip".to_string(),
            "/tmp/file.zip.part".to_string(),
            None,
        );
        insert_download(&pool, &dl).await.expect("insert");

        let updated = set_status(
            &pool,
            "test-id-2",
            crate::models::DownloadStatus::Downloading,
            None,
        )
        .await
        .expect("set_status");
        assert_eq!(
            updated.unwrap().status,
            crate::models::DownloadStatus::Downloading
        );
    }

    #[tokio::test]
    async fn get_next_queued_respects_fifo_order() {
        let pool = in_memory_pool().await;

        for (index, id) in ["first", "second", "third"].into_iter().enumerate() {
            let mut dl = crate::models::Download::new(
                id.to_string(),
                format!("https://example.com/{id}.zip"),
                format!("{id}.zip"),
                format!("/tmp/{id}.zip"),
                format!("/tmp/{id}.zip.part"),
                None,
            );
            dl.created_at = format!("2025-01-01T00:00:0{index}Z");
            dl.updated_at = dl.created_at.clone();
            insert_download(&pool, &dl).await.expect("insert");
        }

        let queued = get_next_queued(&pool, 10).await.expect("get_next_queued");
        assert_eq!(queued.len(), 3);
        assert_eq!(queued[0].id, "first");
        assert_eq!(queued[2].id, "third");
    }

    #[tokio::test]
    async fn bulk_pause_returns_affected_ids() {
        let pool = in_memory_pool().await;

        for id in ["q1", "q2", "d1"] {
            let status = if id.starts_with('q') {
                "queued"
            } else {
                "downloading"
            };
            let dl = crate::models::Download::new(
                id.to_string(),
                format!("https://example.com/{id}.zip"),
                format!("{id}.zip"),
                format!("/tmp/{id}.zip"),
                format!("/tmp/{id}.zip.part"),
                None,
            );
            insert_download(&pool, &dl).await.expect("insert");
            if status != "queued" {
                set_status(&pool, id, crate::models::DownloadStatus::Downloading, None)
                    .await
                    .expect("set status");
            }
        }

        let paused = set_all_downloading_and_queued_to_paused(&pool)
            .await
            .expect("pause all");
        assert_eq!(paused.len(), 3);
    }

    #[tokio::test]
    async fn settings_round_trip() {
        let pool = in_memory_pool().await;

        set_max_concurrent_downloads(&pool, 5).await.expect("set");
        let val = get_max_concurrent_downloads(&pool).await.expect("get");
        assert_eq!(val, 5);
    }

    #[tokio::test]
    async fn extended_settings_parse_round_trip() {
        let pool = in_memory_pool().await;

        set_global_speed_limit(&pool, Some(2_000_000))
            .await
            .expect("global limit");
        set_notifications_enabled(&pool, false)
            .await
            .expect("notifications");
        set_history_retention_days(&pool, Some(14))
            .await
            .expect("retention");
        set_history_max_rows(&pool, Some(500)).await.expect("rows");

        assert_eq!(
            get_global_speed_limit(&pool).await.expect("get global"),
            Some(2_000_000),
        );
        assert_eq!(
            get_u32_setting(&pool, KEY_HISTORY_RETENTION_DAYS)
                .await
                .expect("get retention"),
            Some(14),
        );
        assert_eq!(
            get_u32_setting(&pool, KEY_HISTORY_MAX_ROWS)
                .await
                .expect("get rows"),
            Some(500),
        );
        assert!(!get_bool_setting(&pool, KEY_NOTIFICATIONS_ENABLED, true)
            .await
            .expect("get notifications"));
    }
}
