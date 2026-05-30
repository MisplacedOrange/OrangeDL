use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{Duration, Instant};

const DEFAULT_REPO: &str = "MisplacedOrange/OrangeDL";
const DEFAULT_TAG: &str = "v0.1.0";
const USER_AGENT: &str = concat!("orangedl-bootstrap/", env!("CARGO_PKG_VERSION"));
const MAX_REDIRECTS: usize = 10;

#[derive(Debug)]
struct Options {
    repo: String,
    tag: String,
    asset_url: Option<String>,
    output: Option<PathBuf>,
    download_only: bool,
    show_help: bool,
    show_version: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            repo: DEFAULT_REPO.to_string(),
            tag: DEFAULT_TAG.to_string(),
            asset_url: None,
            output: None,
            download_only: false,
            show_help: false,
            show_version: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Release {
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
        eprintln!("OrangeDL installer failed: {error:#}");
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
    validate_tag(&options.tag)?;

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
        None => resolve_release_asset(&client, &options.repo, &options.tag)?,
    };

    let destination = options
        .output
        .unwrap_or_else(|| env::temp_dir().join(safe_file_name(&asset.name)));

    println!("OrangeDL release: {}/{}", options.repo, options.tag);
    println!("Selected asset: {}", asset.name);
    println!("Downloading to: {}", destination.display());

    download_asset(&client, &asset, &destination)?;

    if options.download_only {
        println!("Download complete: {}", destination.display());
        return Ok(());
    }

    launch_installer(&destination)?;
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
            "--repo" => {
                options.repo = args
                    .next()
                    .context("--repo requires a value like owner/repo")?;
            }
            "--tag" => {
                options.tag = args.next().context("--tag requires a value like v0.1.0")?;
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
            value => bail!("unknown argument `{value}`; run with --help for usage"),
        }
    }

    Ok(options)
}

fn print_help() {
    println!(
        "\
OrangeDL release bootstrapper

Usage:
  orangedl-bootstrap [options]

Options:
  --tag <tag>          GitHub release tag to install [default: v0.1.0]
  --repo <owner/repo>  GitHub repository [default: MisplacedOrange/OrangeDL]
  --asset-url <url>    Download a specific release asset URL instead of auto-detecting
  --output <path>      Save the installer to this path instead of the temp directory
  --download-only      Download the selected asset without launching it
  -V, --version        Print bootstrapper version
  -h, --help           Print help
"
    );
}

fn resolve_release_asset(client: &Client, repo: &str, tag: &str) -> Result<ReleaseAsset> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let release = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to request GitHub release metadata from {url}"))?
        .error_for_status()
        .with_context(|| format!("GitHub release metadata request failed for {repo}@{tag}"))?
        .json::<Release>()
        .context("failed to parse GitHub release metadata")?;

    release
        .assets
        .into_iter()
        .filter_map(|asset| asset_score(&asset.name).map(|score| (score, asset)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, asset)| asset)
        .with_context(|| {
            format!("no compatible OrangeDL app installer was found on release {repo}@{tag}")
        })
}

fn asset_score(name: &str) -> Option<i32> {
    let lower = name.to_ascii_lowercase();

    if lower.contains("bootstrap") || lower.contains("release-downloader") {
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

fn download_asset(client: &Client, asset: &ReleaseAsset, destination: &Path) -> Result<()> {
    validate_download_url(&asset.browser_download_url)?;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .with_context(|| format!("failed to request {}", asset.browser_download_url))?
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
            .context("failed while reading release asset")?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .context("failed while writing release asset")?;
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
                "downloaded size mismatch: expected {}, got {}",
                format_bytes(total),
                format_bytes(downloaded)
            );
        }
    }

    if destination.exists() {
        fs::remove_file(destination)
            .with_context(|| format!("failed to replace {}", destination.display()))?;
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

fn print_progress(downloaded: u64, expected_len: Option<u64>) {
    match expected_len {
        Some(total) if total > 0 => {
            let percent = (downloaded as f64 / total as f64) * 100.0;
            print!(
                "\rDownloaded {} / {} ({percent:.1}%)",
                format_bytes(downloaded),
                format_bytes(total)
            );
        }
        _ => print!("\rDownloaded {}", format_bytes(downloaded)),
    }

    let _ = std::io::stdout().flush();
}

fn launch_installer(path: &Path) -> Result<()> {
    println!("Launching installer: {}", path.display());

    let status = match env::consts::OS {
        "windows" => launch_windows(path)?,
        "macos" => Command::new("open")
            .arg(path)
            .status()
            .context("failed to launch macOS installer with open")?,
        "linux" => launch_linux(path)?,
        os => bail!(
            "automatic launching is not supported on {os}; downloaded {}",
            path.display()
        ),
    };

    if !status.success() {
        bail!("installer exited with status {status}");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_windows(path: &Path) -> Result<std::process::ExitStatus> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "msi" {
        return Command::new("msiexec")
            .arg("/i")
            .arg(path)
            .status()
            .context("failed to launch MSI installer with msiexec");
    }

    Command::new(path)
        .status()
        .context("failed to launch Windows installer")
}

#[cfg(not(target_os = "windows"))]
fn launch_windows(_path: &Path) -> Result<std::process::ExitStatus> {
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
        bail!("--repo must be in owner/repo form with only letters, numbers, dots, underscores, and hyphens");
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

fn download_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }

        if attempt.url().scheme() != "https" {
            return attempt.error("refusing to follow a non-HTTPS release asset redirect");
        }

        attempt.follow()
    })
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
