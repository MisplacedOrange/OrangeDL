use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{Duration, Instant};

const DEFAULT_REPO: &str = "MisplacedOrange/OrangeDL";
const USER_AGENT: &str = concat!("orangedl-bootstrap/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 10;
const MAX_DOWNLOAD_RETRIES: usize = 3;

#[derive(Debug)]
struct Options {
    repo: String,
    tag: String,
    channel: Channel,
    asset_url: Option<String>,
    output: Option<PathBuf>,
    install_dir: Option<String>,
    download_only: bool,
    silent: bool,
    verify_checksum: bool,
    show_help: bool,
    show_version: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Stable,
    Beta,
    Nightly,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            repo: DEFAULT_REPO.to_string(),
            tag: String::new(),
            channel: Channel::Stable,
            asset_url: None,
            output: None,
            install_dir: None,
            download_only: false,
            silent: false,
            verify_checksum: true,
            show_help: false,
            show_version: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("\nError: {error:#}");
        eprintln!("\nRun with --help for usage information.");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let options = parse_args()?;

    if options.show_help {
        print_help();
        return Ok(());
    }

    if options.show_version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    validate_repo(&options.repo)?;
    if !options.tag.is_empty() {
        validate_tag(&options.tag)?;
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .redirect(download_redirect_policy())
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60 * 30))
        .build()
        .context("failed to create HTTP client")?;

    let asset = match options.asset_url.as_ref() {
        Some(url) => {
            validate_download_url(url)?;
            ReleaseAsset {
                name: safe_file_name(
                    &file_name_from_url(url).unwrap_or_else(|| "OrangeDL-installer".to_string()),
                ),
                browser_download_url: url.clone(),
                size: 0,
            }
        }
        None => {
            let tag = if options.tag.is_empty() {
                resolve_latest_tag(&client, &options.repo, options.channel)?
            } else {
                options.tag.clone()
            };
            resolve_release_asset(&client, &options.repo, &tag)?
        }
    };

    let destination = options
        .output
        .clone()
        .unwrap_or_else(|| env::temp_dir().join(safe_file_name(&asset.name)));

    println!("OrangeDL Bootstrapper v{}", env!("CARGO_PKG_VERSION"));
    println!(
        "Repository  : {}/{}",
        options.repo,
        asset_tag_display(&asset)
    );
    println!("Asset       : {}", asset.name);
    println!("Destination : {}", destination.display());
    if options.silent {
        println!("Mode        : silent install");
    }
    println!();

    // Clean up any stale partial file from a previous run
    let partial = partial_path_for(&destination);
    if partial.exists() {
        println!("Cleaning up stale partial file: {}", partial.display());
        let _ = fs::remove_file(&partial);
    }

    download_with_retries(&client, &asset, &destination, MAX_DOWNLOAD_RETRIES)?;

    if options.verify_checksum {
        if let Some(checksum) = fetch_checksum_for(&client, &asset)? {
            println!("Verifying checksum...");
            verify_sha256(&destination, &checksum)?;
            println!("Checksum verified.");
        } else {
            println!("No checksum sidecar found — skipping verification.");
        }
    }

    println!("\nInstaller downloaded to: {}", destination.display());

    if options.download_only {
        println!("--download-only specified. Exiting without launching installer.");
        return Ok(());
    }

    launch_installer(&destination, options.silent, options.install_dir.as_deref())?;
    Ok(())
}

fn asset_tag_display(asset: &ReleaseAsset) -> String {
    let url = asset.browser_download_url.as_str();
    // Try to extract tag from GitHub releases URL pattern
    // https://github.com/owner/repo/releases/download/v1.0.0/asset.exe
    url.split('/').nth(7).unwrap_or("?").to_string()
}

fn resolve_latest_tag(client: &Client, repo: &str, channel: Channel) -> Result<String> {
    if channel == Channel::Stable {
        println!("Fetching latest stable release...");
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        let release = client
            .get(&url)
            .send()
            .with_context(|| format!("failed to request latest release from {url}"))?
            .error_for_status()
            .context("GitHub latest release request failed")?
            .json::<Release>()
            .context("failed to parse latest release metadata")?;

        println!("Latest release: {}", release.tag_name);
        return Ok(release.tag_name);
    }

    // For beta/nightly: list all releases and find first matching channel
    println!("Fetching {} releases...", channel.as_str());
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=10");
    let releases = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to list releases from {url}"))?
        .error_for_status()
        .context("GitHub releases list request failed")?
        .json::<Vec<Release>>()
        .context("failed to parse releases list")?;

    let channel_str = channel.as_str();
    let matching = releases
        .into_iter()
        .find(|r| r.tag_name.contains(channel_str))
        .with_context(|| format!("no {channel_str} release found for {repo}"))?;

    println!("Found {} release: {}", channel_str, matching.tag_name);
    Ok(matching.tag_name)
}

fn resolve_release_asset(client: &Client, repo: &str, tag: &str) -> Result<ReleaseAsset> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let release = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to request release metadata from {url}"))?
        .error_for_status()
        .with_context(|| format!("release metadata request failed for {repo}@{tag}"))?
        .json::<Release>()
        .context("failed to parse release metadata")?;

    release
        .assets
        .into_iter()
        .filter_map(|asset| asset_score(&asset.name).map(|score| (score, asset)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, asset)| asset)
        .with_context(|| format!("no compatible OrangeDL installer found in release {repo}@{tag}"))
}

fn fetch_checksum_for(client: &Client, asset: &ReleaseAsset) -> Result<Option<String>> {
    // Look for a .sha256 sidecar next to the asset
    let checksum_url = format!("{}.sha256", asset.browser_download_url);

    match client.get(&checksum_url).send() {
        Ok(response) if response.status().is_success() => {
            let text = response.text().context("failed to read checksum file")?;
            Ok(parse_checksum_hash(&text))
        }
        _ => Ok(None),
    }
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<()> {
    let mut file = File::open(path).with_context(|| {
        format!(
            "failed to open {} for checksum verification",
            path.display()
        )
    })?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .context("failed to read file for checksum")?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let actual_hex = hex::encode(hasher.finalize());

    if actual_hex != expected_hex.to_lowercase() {
        bail!(
            "checksum mismatch for {}:\n  expected: {}\n  actual:   {}\n\nThe downloaded file may be corrupt or tampered with. It has been removed.",
            path.display(),
            expected_hex,
            actual_hex
        );
    }

    Ok(())
}

fn download_with_retries(
    client: &Client,
    asset: &ReleaseAsset,
    destination: &Path,
    max_retries: usize,
) -> Result<()> {
    let mut last_error = None;

    for attempt in 1..=(max_retries + 1) {
        if attempt > 1 {
            let delay = Duration::from_secs((attempt as u64 - 1) * 2);
            println!(
                "Retry {}/{} in {}s...",
                attempt - 1,
                max_retries,
                delay.as_secs()
            );
            std::thread::sleep(delay);
        }

        match download_asset(client, asset, destination) {
            Ok(()) => return Ok(()),
            Err(error) => {
                eprintln!("Download attempt {attempt} failed: {error:#}");
                // Clean up partial file before retry
                let partial = partial_path_for(destination);
                let _ = fs::remove_file(&partial);
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap().context(format!(
        "download failed after {} attempt(s)",
        max_retries + 1
    )))
}

fn download_asset(client: &Client, asset: &ReleaseAsset, destination: &Path) -> Result<()> {
    validate_download_url(&asset.browser_download_url)?;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .with_context(|| format!("failed to connect to {}", asset.browser_download_url))?
        .error_for_status()
        .with_context(|| format!("download failed for {}", asset.browser_download_url))?;

    let expected_len = response.content_length().or({
        if asset.size > 0 {
            Some(asset.size)
        } else {
            None
        }
    });

    let partial_destination = partial_path_for(destination);
    let mut file = File::create(&partial_destination)
        .with_context(|| format!("failed to create {}", partial_destination.display()))?;

    let mut buffer = [0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    let mut last_report = Instant::now();

    loop {
        let bytes_read = response
            .read(&mut buffer)
            .context("connection interrupted while reading release asset")?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .context("failed to write to partial file")?;
        downloaded += bytes_read as u64;

        if last_report.elapsed() >= Duration::from_millis(750) {
            print_progress(downloaded, expected_len);
            last_report = Instant::now();
        }
    }

    file.flush()
        .context("failed to flush downloaded installer")?;
    print_progress(downloaded, expected_len);
    println!();
    drop(file);

    if let Some(total) = expected_len {
        if downloaded != total {
            let _ = fs::remove_file(&partial_destination);
            bail!(
                "size mismatch: expected {}, got {} — partial file removed",
                format_bytes(total),
                format_bytes(downloaded)
            );
        }
    }

    if destination.exists() {
        fs::remove_file(destination)
            .with_context(|| format!("failed to replace existing {}", destination.display()))?;
    }
    fs::rename(&partial_destination, destination).with_context(|| {
        format!(
            "failed to move {} to {}",
            partial_destination.display(),
            destination.display()
        )
    })?;

    Ok(())
}

fn parse_args() -> Result<Options> {
    let mut options = Options::default();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => options.show_help = true,
            "-V" | "--version" => options.show_version = true,
            "--download-only" => options.download_only = true,
            "--silent" => options.silent = true,
            "--no-verify" => options.verify_checksum = false,
            "--repo" => {
                options.repo = args
                    .next()
                    .context("--repo requires a value like owner/repo")?;
            }
            "--tag" => {
                options.tag = args.next().context("--tag requires a value like v0.1.0")?;
            }
            "--channel" => {
                let ch = args
                    .next()
                    .context("--channel requires stable, beta, or nightly")?;
                options.channel = match ch.as_str() {
                    "stable" => Channel::Stable,
                    "beta" => Channel::Beta,
                    "nightly" => Channel::Nightly,
                    other => {
                        bail!("unknown channel `{other}`; valid values are: stable, beta, nightly")
                    }
                };
            }
            "--asset-url" => {
                options.asset_url = Some(
                    args.next()
                        .context("--asset-url requires a direct release asset URL")?,
                );
            }
            "--output" => {
                options.output = Some(PathBuf::from(
                    args.next().context("--output requires a file path")?,
                ));
            }
            "--install-dir" => {
                options.install_dir = Some(
                    args.next()
                        .context("--install-dir requires a directory path")?,
                );
            }
            value => bail!("unknown argument `{value}`; run with --help for usage"),
        }
    }

    Ok(options)
}

fn print_help() {
    println!(
        "\
OrangeDL release bootstrapper

Downloads and launches the correct OrangeDL installer for this platform.
When no --tag is specified, the latest stable release is used.

Usage:
  orangedl-bootstrap [options]

Options:
  --tag <tag>           GitHub release tag to install (default: latest stable)
  --channel <channel>   Release channel: stable (default), beta, nightly
  --repo <owner/repo>   GitHub repository [default: MisplacedOrange/OrangeDL]
  --asset-url <url>     Download a specific asset URL instead of auto-detecting
  --output <path>       Save the installer to this path instead of the temp directory
  --install-dir <path>  Pass a custom install directory to the installer (NSIS /D flag)
  --download-only       Download the installer without launching it
  --silent              Launch the installer in silent (unattended) mode
  --no-verify           Skip SHA256 checksum verification
  -V, --version         Print bootstrapper version
  -h, --help            Print help
"
    );
}

fn launch_installer(path: &Path, silent: bool, install_dir: Option<&str>) -> Result<()> {
    println!("Launching installer: {}", path.display());

    let status = match env::consts::OS {
        "windows" => launch_windows(path, silent, install_dir)?,
        "macos" => Command::new("open")
            .arg(path)
            .status()
            .context("failed to launch macOS installer with open")?,
        "linux" => launch_linux(path)?,
        os => bail!(
            "automatic launching is not supported on {os}; installer is at {}",
            path.display()
        ),
    };

    if !status.success() {
        bail!("installer exited with status {status}");
    }

    println!("\nInstallation complete.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_windows(
    path: &Path,
    silent: bool,
    install_dir: Option<&str>,
) -> Result<std::process::ExitStatus> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "msi" {
        let mut cmd = Command::new("msiexec");
        cmd.arg("/i").arg(path);
        if silent {
            cmd.arg("/quiet").arg("/norestart");
        }
        if let Some(dir) = install_dir {
            cmd.arg(format!("INSTALLDIR={dir}"));
        }
        return cmd
            .status()
            .context("failed to launch MSI installer with msiexec");
    }

    // NSIS installer
    let mut cmd = Command::new(path);
    if silent {
        cmd.arg("/S");
    }
    if let Some(dir) = install_dir {
        cmd.arg(format!("/D={dir}"));
    }
    cmd.status().context("failed to launch NSIS installer")
}

#[cfg(not(target_os = "windows"))]
fn launch_windows(
    _path: &Path,
    _silent: bool,
    _install_dir: Option<&str>,
) -> Result<std::process::ExitStatus> {
    bail!("Windows installer launching is unavailable on this platform")
}

#[cfg(target_os = "linux")]
fn launch_linux(path: &Path) -> Result<std::process::ExitStatus> {
    use std::os::unix::fs::PermissionsExt;

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.ends_with(".appimage") {
        let mut permissions = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to make {} executable", path.display()))?;

        return Command::new(path)
            .status()
            .context("failed to launch AppImage installer");
    }

    Command::new("xdg-open")
        .arg(path)
        .status()
        .context("failed to open Linux installer with xdg-open")
}

#[cfg(not(target_os = "linux"))]
fn launch_linux(_path: &Path) -> Result<std::process::ExitStatus> {
    bail!("Linux installer launching is unavailable on this platform")
}

fn print_progress(downloaded: u64, expected_len: Option<u64>) {
    match expected_len {
        Some(total) if total > 0 => {
            let percent = (downloaded as f64 / total as f64) * 100.0;
            let bar_width = 30_usize;
            let filled = ((percent / 100.0) * bar_width as f64).round() as usize;
            let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(bar_width - filled));
            print!(
                "\r{bar} {} / {} ({percent:.1}%)",
                format_bytes(downloaded),
                format_bytes(total)
            );
        }
        _ => print!("\rDownloaded {}", format_bytes(downloaded)),
    }

    let _ = std::io::stdout().flush();
}

fn asset_score(name: &str) -> Option<i32> {
    let lower = name.to_ascii_lowercase();

    // Exclude checksums and the bootstrapper itself
    if lower.ends_with(".sha256")
        || lower.contains("bootstrap")
        || lower.contains("release-downloader")
    {
        return None;
    }

    let mut score = match env::consts::OS {
        "windows" if lower.ends_with(".exe") => 320,
        "windows" if lower.ends_with(".msi") => 300,
        "macos" if lower.ends_with(".dmg") => 300,
        "macos" if lower.ends_with(".app.tar.gz") || lower.ends_with(".app.tgz") => 240,
        "linux" if lower.ends_with(".appimage") => 300,
        "linux" if lower.ends_with(".deb") => 260,
        "linux" if lower.ends_with(".rpm") => 240,
        "linux" if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") => 200,
        _ => return None,
    };

    let (matching_arch_terms, wrong_arch_terms): (&[&str], &[&str]) = match env::consts::ARCH {
        "x86_64" => (&["x64", "x86_64", "amd64"], &["aarch64", "arm64"]),
        "aarch64" => (&["aarch64", "arm64"], &["x64", "x86_64", "amd64"]),
        _ => (&[] as &[&str], &[] as &[&str]),
    };

    if wrong_arch_terms.iter().any(|term| lower.contains(term)) {
        return None;
    }

    if matching_arch_terms.iter().any(|term| lower.contains(term)) {
        score += 50;
    }

    Some(score)
}

fn file_name_from_url(url: &str) -> Option<String> {
    url.split('?')
        .next()
        .and_then(|value| value.rsplit('/').next())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn partial_path_for(destination: &Path) -> PathBuf {
    let mut path = destination.as_os_str().to_os_string();
    path.push(".download");
    PathBuf::from(path)
}

fn validate_repo(repo: &str) -> Result<()> {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();

    if parts.next().is_some() || !is_safe_identifier(owner) || !is_safe_identifier(name) {
        bail!(
            "--repo must be in owner/repo form with only letters, numbers, dots, underscores, and hyphens"
        );
    }

    Ok(())
}

fn validate_tag(tag: &str) -> Result<()> {
    if !is_safe_identifier(tag) {
        bail!("--tag may only contain letters, numbers, dots, underscores, and hyphens");
    }

    Ok(())
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .any(|character| character.is_ascii_alphanumeric())
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

fn validate_download_url(url: &str) -> Result<()> {
    let parsed = Url::parse(url).with_context(|| format!("invalid download URL: {url}"))?;

    if parsed.scheme() != "https" {
        bail!("release asset URLs must use HTTPS");
    }

    if parsed.host_str().is_none() {
        bail!("release asset URL must include a host");
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        bail!("release asset URLs must not include embedded credentials");
    }

    Ok(())
}

fn parse_checksum_hash(text: &str) -> Option<String> {
    text.split_whitespace()
        .next()
        .filter(|hash| hash.len() == 64)
        .map(str::to_lowercase)
}

fn download_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if let Some(error) = redirect_policy_error(attempt.url(), attempt.previous().len()) {
            return attempt.error(error);
        }

        attempt.follow()
    })
}

fn redirect_policy_error(url: &Url, previous_redirects: usize) -> Option<&'static str> {
    if previous_redirects > MAX_REDIRECTS {
        return Some("too many redirects");
    }

    if url.scheme() != "https" {
        return Some("refusing to follow a non-HTTPS release asset redirect");
    }

    None
}

fn safe_file_name(input: &str) -> String {
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

    let name = cleaned
        .trim_matches(|character| character == '.' || character == ' ')
        .chars()
        .take(180)
        .collect::<String>();

    if name.is_empty() {
        "OrangeDL-installer".to_string()
    } else {
        name
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current_platform_asset_name(arch: Option<&str>) -> Option<String> {
        let arch = arch.map(|value| format!("-{value}")).unwrap_or_default();

        match env::consts::OS {
            "windows" => Some(format!("OrangeDL{arch}-setup.exe")),
            "macos" => Some(format!("OrangeDL{arch}.dmg")),
            "linux" => Some(format!("OrangeDL{arch}.AppImage")),
            _ => None,
        }
    }

    #[test]
    fn validate_repo_accepts_owner_repo_identifiers() {
        assert!(validate_repo("MisplacedOrange/OrangeDL").is_ok());
        assert!(validate_repo("owner.name/repo_name-1").is_ok());
    }

    #[test]
    fn validate_repo_rejects_malformed_or_unsafe_values() {
        for repo in [
            "",
            "owner",
            "owner/",
            "/repo",
            "owner/repo/extra",
            "owner/repo name",
            "owner/repo@main",
            "../repo",
        ] {
            assert!(validate_repo(repo).is_err(), "{repo} should be rejected");
        }
    }

    #[test]
    fn validate_tag_accepts_safe_release_tags() {
        for tag in ["v0.2.0", "release_2026-06-06", "beta.1"] {
            assert!(validate_tag(tag).is_ok(), "{tag} should be accepted");
        }
    }

    #[test]
    fn validate_tag_rejects_empty_or_unsafe_values() {
        for tag in ["", "v0.2.0/extra", "v0.2.0 beta", "v0.2.0+build"] {
            assert!(validate_tag(tag).is_err(), "{tag} should be rejected");
        }
    }

    #[test]
    fn safe_file_name_sanitizes_and_trims_names() {
        assert_eq!(safe_file_name(" ..Orange:DL?.exe "), "Orange_DL_.exe");
        assert_eq!(safe_file_name("...   "), "OrangeDL-installer");
    }

    #[test]
    fn safe_file_name_caps_output_length() {
        let input = format!("{}-setup.exe", "a".repeat(220));
        let output = safe_file_name(&input);

        assert_eq!(output.len(), 180);
        assert!(output.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ' ')
        }));
    }

    #[test]
    fn asset_score_accepts_current_platform_installer() {
        let Some(asset_name) = current_platform_asset_name(None) else {
            return;
        };

        assert!(asset_score(&asset_name).is_some());
    }

    #[test]
    fn asset_score_excludes_checksums_and_bootstrappers() {
        assert_eq!(asset_score("OrangeDL-setup.exe.sha256"), None);
        assert_eq!(asset_score("orangedl-bootstrap-windows-x64.exe"), None);
        assert_eq!(asset_score("orangedl-release-downloader.exe"), None);
    }

    #[test]
    fn asset_score_prefers_current_architecture() {
        let matching_arch = match env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            _ => return,
        };

        let Some(generic_name) = current_platform_asset_name(None) else {
            return;
        };
        let Some(arch_name) = current_platform_asset_name(Some(matching_arch)) else {
            return;
        };

        let generic_score = asset_score(&generic_name).expect("generic asset should match");
        let arch_score = asset_score(&arch_name).expect("arch asset should match");

        assert!(arch_score > generic_score);
    }

    #[test]
    fn asset_score_rejects_wrong_architecture() {
        let wrong_arch = match env::consts::ARCH {
            "x86_64" => "arm64",
            "aarch64" => "x64",
            _ => return,
        };

        let Some(asset_name) = current_platform_asset_name(Some(wrong_arch)) else {
            return;
        };

        assert_eq!(asset_score(&asset_name), None);
    }

    #[test]
    fn parse_checksum_hash_reads_first_sha256_token() {
        let checksum = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert_eq!(
            parse_checksum_hash(&format!("{checksum}  OrangeDL.exe\n")),
            Some(checksum.to_ascii_lowercase())
        );
    }

    #[test]
    fn parse_checksum_hash_ignores_missing_or_short_hashes() {
        assert_eq!(parse_checksum_hash(""), None);
        assert_eq!(parse_checksum_hash("abc123  OrangeDL.exe"), None);
    }

    #[test]
    fn validate_download_url_accepts_https_without_credentials() {
        assert!(validate_download_url(
            "https://github.com/MisplacedOrange/OrangeDL/releases/download/v0.2.0/OrangeDL.exe"
        )
        .is_ok());
    }

    #[test]
    fn validate_download_url_rejects_non_https_or_credentials() {
        for url in [
            "http://github.com/MisplacedOrange/OrangeDL/releases/download/v0.2.0/OrangeDL.exe",
            "https://user:secret@github.com/MisplacedOrange/OrangeDL/releases/download/v0.2.0/OrangeDL.exe",
            "not a url",
        ] {
            assert!(validate_download_url(url).is_err(), "{url} should be rejected");
        }
    }

    #[test]
    fn redirect_policy_boundaries_require_https_and_limit_redirects() {
        let https_url = Url::parse("https://github.com/MisplacedOrange/OrangeDL").unwrap();
        let http_url = Url::parse("http://github.com/MisplacedOrange/OrangeDL").unwrap();

        assert_eq!(redirect_policy_error(&https_url, MAX_REDIRECTS), None);
        assert_eq!(
            redirect_policy_error(&https_url, MAX_REDIRECTS + 1),
            Some("too many redirects")
        );
        assert_eq!(
            redirect_policy_error(&http_url, 0),
            Some("refusing to follow a non-HTTPS release asset redirect")
        );
    }
}
