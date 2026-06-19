// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncBufReadExt;
use tokio_util::sync::CancellationToken;

pub type Result<T> = std::result::Result<T, MediaError>;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("yt-dlp failed: {0}")]
    Process(String),
    #[error("could not parse video info: {0}")]
    Parse(String),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    App(String),
    #[error("cancelled")]
    Cancelled,
}

impl serde::Serialize for MediaError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDlpStatus {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub duration_secs: Option<u64>,
    pub uploader: Option<String>,
    pub webpage_url: String,
    pub extractor: String,
}

pub struct YtDlpProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: f64,
}

fn ytdlp_exe() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

pub fn find_ytdlp(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(data_dir) = app.path().app_data_dir() {
        let candidate = data_dir.join("ytdlp").join(ytdlp_exe());
        if candidate.exists() {
            return Some(candidate);
        }
    }
    find_in_path(ytdlp_exe())
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.to_string_lossy().split(sep) {
        let candidate = PathBuf::from(dir).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub async fn check_ytdlp(app: &AppHandle) -> YtDlpStatus {
    let Some(exe) = find_ytdlp(app) else {
        return YtDlpStatus {
            found: false,
            path: None,
            version: None,
        };
    };

    let version = tokio::process::Command::new(&exe)
        .arg("--version")
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|v| v.trim().to_owned());

    YtDlpStatus {
        found: true,
        path: Some(exe.to_string_lossy().to_string()),
        version,
    }
}

pub async fn fetch_video_info(url: &str, ytdlp_path: &Path) -> Result<VideoInfo> {
    let output = tokio::process::Command::new(ytdlp_path)
        .args([
            "--dump-json",
            "--no-playlist",
            "--no-warnings",
            "--quiet",
            "--",
            url,
        ])
        .output()
        .await
        .map_err(|e| MediaError::Process(e.to_string()))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let msg = err
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("yt-dlp failed")
            .to_owned();
        return Err(MediaError::Process(msg));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .next()
        .ok_or_else(|| MediaError::Parse("empty output".into()))?;

    let json: serde_json::Value =
        serde_json::from_str(line).map_err(|e| MediaError::Parse(e.to_string()))?;

    Ok(VideoInfo {
        title: json["title"].as_str().unwrap_or("Video").to_owned(),
        thumbnail_url: json["thumbnail"].as_str().map(ToOwned::to_owned),
        duration_secs: json["duration"].as_f64().map(|d| d as u64),
        uploader: json["uploader"]
            .as_str()
            .or_else(|| json["channel"].as_str())
            .or_else(|| json["creator"].as_str())
            .map(ToOwned::to_owned),
        webpage_url: json["webpage_url"].as_str().unwrap_or(url).to_owned(),
        extractor: json["extractor_key"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned(),
    })
}

pub async fn download_ytdlp_binary(app: &AppHandle, client: &Client) -> Result<PathBuf> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| MediaError::App(e.to_string()))?;
    let ytdlp_dir = data_dir.join("ytdlp");
    tokio::fs::create_dir_all(&ytdlp_dir).await?;

    let exe_path = ytdlp_dir.join(ytdlp_exe());

    let asset_name = if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"
    };
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset_name}");

    let bytes = client
        .get(&url)
        .header("User-Agent", "OrangeDL yt-dlp-installer")
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let expected = fetch_expected_sha256(client, asset_name).await.ok_or_else(|| {
        MediaError::App(
            "could not verify yt-dlp checksum — SHA2-256SUMS file unavailable".to_owned(),
        )
    })?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return Err(MediaError::App(format!(
            "yt-dlp download failed checksum verification (expected {expected}, got {actual})"
        )));
    }

    // Write to a temp path and rename so an interrupted download can never
    // leave a truncated binary at the final location.
    let temp_path = ytdlp_dir.join(format!("{asset_name}.download"));
    tokio::fs::write(&temp_path, &bytes).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = tokio::fs::metadata(&temp_path).await?;
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&temp_path, perms).await?;
    }

    tokio::fs::rename(&temp_path, &exe_path).await?;

    Ok(exe_path)
}

/// Look up the asset's hash in the SHA2-256SUMS file yt-dlp publishes with
/// each release. `None` when the sums file can't be fetched or parsed; a
/// fetched file that lacks the asset still yields `None`.
async fn fetch_expected_sha256(client: &Client, asset_name: &str) -> Option<String> {
    let sums = client
        .get("https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS")
        .header("User-Agent", "OrangeDL yt-dlp-installer")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;

    parse_sha256_sums(&sums, asset_name)
}

fn parse_sha256_sums(sums: &str, asset_name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        (name == asset_name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

/// Map a quality label to a yt-dlp format selector string.
pub fn format_selector(quality: &str) -> &'static str {
    match quality {
        "4k" => "bestvideo[height<=2160]+bestaudio/best[height<=2160]/best",
        "1080p" => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
        "720p" => "bestvideo[height<=720]+bestaudio/best[height<=720]/best",
        "480p" => "bestvideo[height<=480]+bestaudio/best[height<=480]/best",
        "360p" => "bestvideo[height<=360]+bestaudio/best[height<=360]/best",
        "audio" => "bestaudio[ext=m4a]/bestaudio[ext=opus]/bestaudio",
        _ => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best",
    }
}

fn parse_progress(line: &str) -> Option<YtDlpProgress> {
    // Lines from our custom progress template:
    // "ODLPROG:<downloaded>:<total>:<speed>"
    let rest = line.strip_prefix("ODLPROG:")?;
    let mut parts = rest.splitn(4, ':');
    let dl_str = parts.next()?;
    let tot_str = parts.next()?;
    let spd_str = parts.next().unwrap_or("0");

    let downloaded_bytes = dl_str.parse::<u64>().ok()?;
    let total_bytes = tot_str.parse::<u64>().ok().filter(|&v| v > 0);
    let speed_bps = spd_str.parse::<f64>().unwrap_or(0.0).max(0.0);

    Some(YtDlpProgress {
        downloaded_bytes,
        total_bytes,
        speed_bps,
    })
}

/// Run yt-dlp for the given URL and quality, writing output into `temp_dir`.
/// Returns the path of the completed output file, or an error string.
pub async fn run_ytdlp(
    url: &str,
    quality: &str,
    temp_dir: &Path,
    ytdlp_path: &Path,
    token: &CancellationToken,
    mut on_progress: impl FnMut(YtDlpProgress) + Send,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(temp_dir).await?;

    let selector = format_selector(quality);
    let output_template = temp_dir
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .to_string();
    let progress_template =
        "ODLPROG:%(progress.downloaded_bytes)s:%(progress.total_bytes)s:%(progress.speed)s";

    let mut child = tokio::process::Command::new(ytdlp_path)
        .args([
            "-f",
            selector,
            "-o",
            &output_template,
            "--no-playlist",
            "--no-warnings",
            "--newline",
            "--progress-template",
            progress_template,
            "--continue",
            "--",
            url,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| MediaError::Process(e.to_string()))?;

    let stderr = child.stderr.take().unwrap();
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let mut last_error = String::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => {
                let _ = child.kill().await;
                return Err(MediaError::Cancelled);
            }
            line = lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(prog) = parse_progress(&l) {
                            on_progress(prog);
                        } else if !l.trim().is_empty() && !l.starts_with("[download]") {
                            last_error = l;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    let exit = child
        .wait()
        .await
        .map_err(|e| MediaError::Process(e.to_string()))?;

    if !exit.success() {
        let msg = if last_error.is_empty() {
            "yt-dlp exited with an error".to_owned()
        } else {
            last_error
        };
        return Err(MediaError::Process(msg));
    }

    // Find the output file in temp_dir
    let mut read_dir = tokio::fs::read_dir(temp_dir).await?;
    let mut output_file: Option<PathBuf> = None;
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.is_file() {
            output_file = Some(path);
            break;
        }
    }

    output_file.ok_or_else(|| MediaError::Process("yt-dlp produced no output file".into()))
}

/// Find a unique path in `dir` for a file named `file_name`, appending " (N)" as needed.
pub async fn unique_in_dir(dir: &Path, file_name: &str) -> std::io::Result<PathBuf> {
    let first = dir.join(file_name);
    if !tokio::fs::try_exists(&first).await? {
        return Ok(first);
    }

    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    };

    for n in 1..10_000 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !tokio::fs::try_exists(&candidate).await? {
            return Ok(candidate);
        }
    }

    Ok(dir.join(file_name)) // fallback: overwrite
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "9f2b7c1d8a3e5f60912b7c1d8a3e5f60912b7c1d8a3e5f60912b7c1d8a3e5f60";

    #[test]
    fn sha256_sums_finds_matching_asset() {
        let sums = format!("{}  yt-dlp\n{HASH}  yt-dlp.exe\n", "0".repeat(64));
        assert_eq!(
            parse_sha256_sums(&sums, "yt-dlp.exe"),
            Some(HASH.to_owned())
        );
    }

    #[test]
    fn sha256_sums_lowercases_hash() {
        let sums = format!("{}  yt-dlp.exe", HASH.to_ascii_uppercase());
        assert_eq!(
            parse_sha256_sums(&sums, "yt-dlp.exe"),
            Some(HASH.to_owned())
        );
    }

    #[test]
    fn sha256_sums_rejects_missing_or_malformed_entries() {
        assert_eq!(parse_sha256_sums("", "yt-dlp.exe"), None);
        let sums = format!("{HASH}  yt-dlp_macos");
        assert_eq!(parse_sha256_sums(&sums, "yt-dlp.exe"), None);
        assert_eq!(parse_sha256_sums("abc123  yt-dlp.exe", "yt-dlp.exe"), None);
        let not_hex = format!("{}  yt-dlp.exe", "z".repeat(64));
        assert_eq!(parse_sha256_sums(&not_hex, "yt-dlp.exe"), None);
    }

    #[test]
    fn progress_line_parses_template_output() {
        let prog = parse_progress("ODLPROG:1024:4096:512.5").expect("should parse");
        assert_eq!(prog.downloaded_bytes, 1024);
        assert_eq!(prog.total_bytes, Some(4096));
        assert_eq!(prog.speed_bps, 512.5);
    }

    #[test]
    fn progress_line_ignores_other_output() {
        assert!(parse_progress("[download] 42% of ~10MB").is_none());
    }
}
