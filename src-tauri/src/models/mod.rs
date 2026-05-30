use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DownloadStatus {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "downloading" => Ok(Self::Downloading),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDownloadRequest {
    pub url: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
    #[serde(default)]
    pub speed_limit_bps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_download_directory: String,
    pub default_speed_limit_bps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    #[serde(default)]
    pub default_download_directory: Option<String>,
    #[serde(default)]
    pub default_speed_limit_bps: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub destination: String,
    pub temp_path: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub progress: f64,
    pub speed_bps: f64,
    pub eta_seconds: Option<u64>,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub speed_limit_bps: Option<u64>,
}

impl Download {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        url: String,
        file_name: String,
        destination: String,
        temp_path: String,
        speed_limit_bps: Option<u64>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            id,
            url,
            file_name,
            destination,
            temp_path,
            total_bytes: None,
            downloaded_bytes: 0,
            progress: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            status: DownloadStatus::Queued,
            error: None,
            created_at: now.clone(),
            updated_at: now,
            speed_limit_bps,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub destination: String,
    pub temp_path: String,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub status: String,
    pub speed_bps: f64,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub speed_limit_bps: Option<i64>,
}

impl From<DownloadRow> for Download {
    fn from(row: DownloadRow) -> Self {
        let total_bytes = row.total_bytes.and_then(non_negative_u64);
        let downloaded_bytes = non_negative_u64(row.downloaded_bytes).unwrap_or(0);
        let progress = match total_bytes {
            Some(total) if total > 0 => {
                ((downloaded_bytes as f64 / total as f64) * 100.0).min(100.0)
            }
            Some(_) => 100.0,
            None => 0.0,
        };

        Self {
            id: row.id,
            url: row.url,
            file_name: row.file_name,
            destination: row.destination,
            temp_path: row.temp_path,
            total_bytes,
            downloaded_bytes,
            progress,
            speed_bps: row.speed_bps.max(0.0),
            eta_seconds: None,
            status: DownloadStatus::from_str(&row.status).unwrap_or(DownloadStatus::Failed),
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            speed_limit_bps: row.speed_limit_bps.and_then(non_negative_u64),
        }
    }
}

fn non_negative_u64(value: i64) -> Option<u64> {
    if value < 0 {
        None
    } else {
        Some(value as u64)
    }
}
