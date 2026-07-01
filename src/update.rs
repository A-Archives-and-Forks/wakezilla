use anyhow::{anyhow, bail, Context, Result};
use flate2::read::GzDecoder;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tar::Archive;

pub const REPO_OWNER: &str = "guibeira";
pub const REPO_NAME: &str = "wakezilla";
pub const BIN_NAME: &str = "wakezilla";
pub const WINDOWS_BIN_NAME: &str = "wakezilla.exe";

#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    Current { current: String },
    Available { current: String, latest: String },
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub fn normalize_tag(tag: &str) -> &str {
    tag.strip_prefix("wakezilla/v")
        .or_else(|| tag.strip_prefix('v'))
        .unwrap_or(tag)
}

pub fn release_api_url(version: Option<&str>) -> String {
    match version {
        Some(version) => format!(
            "https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/tags/v{}",
            normalize_tag(version)
        ),
        None => format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"),
    }
}

pub fn checksum_url(version: &str) -> String {
    format!(
        "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{}/SHA256SUMS",
        normalize_tag(version)
    )
}

fn asset_name(version: &str, target: &str) -> String {
    format!("{BIN_NAME}-{}-{target}.tar.gz", normalize_tag(version))
}

fn binary_name_for_target(target: &str) -> &'static str {
    if target.contains("windows") {
        WINDOWS_BIN_NAME
    } else {
        BIN_NAME
    }
}

fn target_from_parts(os: &str, arch: &str, libc: Option<&str>) -> Result<String> {
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        other => bail!("unsupported architecture for Wakezilla releases: {other}"),
    };

    match os {
        "linux" => Ok(format!(
            "{arch}-unknown-linux-{}",
            libc.filter(|value| !value.is_empty()).unwrap_or("gnu")
        )),
        "macos" | "darwin" => Ok(format!("{arch}-apple-darwin")),
        "windows" => match arch {
            "x86_64" => Ok("x86_64-pc-windows-msvc".to_string()),
            other => bail!("unsupported architecture for Windows Wakezilla releases: {other}"),
        },
        other => bail!("unsupported OS for Wakezilla releases: {other}"),
    }
}

pub fn detect_target() -> Result<String> {
    let libc = if cfg!(target_os = "linux") {
        Some(if cfg!(target_env = "musl") {
            "musl"
        } else {
            "gnu"
        })
    } else {
        None
    };

    target_from_parts(std::env::consts::OS, std::env::consts::ARCH, libc)
}

fn available_targets(release: &GitHubRelease) -> Vec<String> {
    let version = normalize_tag(&release.tag_name);
    let prefix = format!("{BIN_NAME}-{version}-");

    let mut targets = release
        .assets
        .iter()
        .filter_map(|asset| {
            asset
                .name
                .strip_prefix(&prefix)
                .and_then(|name| name.strip_suffix(".tar.gz"))
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn select_asset<'a>(release: &'a GitHubRelease, target: &str) -> Result<&'a GitHubAsset> {
    let version = normalize_tag(&release.tag_name);
    let expected = asset_name(version, target);

    release
        .assets
        .iter()
        .find(|asset| asset.name == expected)
        .ok_or_else(|| {
            let available = available_targets(release);
            anyhow!(
                "no release asset found for target {target}; expected {expected}; available targets: {}",
                if available.is_empty() {
                    "none".to_string()
                } else {
                    available.join(", ")
                }
            )
        })
}

fn status_from_release(current: &str, release: &GitHubRelease) -> Result<UpdateStatus> {
    let latest = normalize_tag(&release.tag_name);
    if release.prerelease {
        return Ok(UpdateStatus::Current {
            current: current.to_string(),
        });
    }

    let current_version = Version::parse(current)
        .with_context(|| format!("failed to parse current version '{current}'"))?;
    let latest_version = Version::parse(latest)
        .with_context(|| format!("failed to parse latest version '{latest}'"))?;

    if !latest_version.pre.is_empty() {
        return Ok(UpdateStatus::Current {
            current: current.to_string(),
        });
    }

    if current_version.cmp_precedence(&latest_version).is_lt() {
        Ok(UpdateStatus::Available {
            current: current.to_string(),
            latest: latest.to_string(),
        })
    } else {
        Ok(UpdateStatus::Current {
            current: current.to_string(),
        })
    }
}

fn github_request(client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
    let request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(
            reqwest::header::USER_AGENT,
            format!("{BIN_NAME}/{}", env!("CARGO_PKG_VERSION")),
        );

    match std::env::var("GITHUB_TOKEN") {
        Ok(token) if !token.trim().is_empty() => request.bearer_auth(token),
        _ => request,
    }
}

async fn fetch_release(client: &reqwest::Client, version: Option<&str>) -> Result<GitHubRelease> {
    let url = release_api_url(version);
    let response = github_request(client, &url)
        .send()
        .await
        .with_context(|| format!("failed to fetch release metadata from {url}"))?;

    let status = response.status();
    if !status.is_success() {
        bail!("GitHub release request failed with {status} for {url}");
    }

    response
        .json::<GitHubRelease>()
        .await
        .context("failed to parse GitHub release metadata")
}

async fn download_bytes(client: &reqwest::Client, url: &str, label: &str) -> Result<Vec<u8>> {
    let response = github_request(client, url)
        .send()
        .await
        .with_context(|| format!("failed to download {label}"))?;

    let status = response.status();
    if !status.is_success() {
        bail!("download failed with {status} for {label}");
    }

    Ok(response
        .bytes()
        .await
        .with_context(|| format!("failed to read downloaded {label}"))?
        .to_vec())
}

pub async fn check_latest(client: &reqwest::Client, current: &str) -> Result<UpdateStatus> {
    let release = fetch_release(client, None).await?;
    status_from_release(current, &release)
}

pub async fn warn_if_update_available(current: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .context("failed to create update check HTTP client")?;

    if let UpdateStatus::Available { current, latest } = check_latest(&client, current).await? {
        tracing::warn!(
            "Wakezilla {latest} is available (current {current}). Run `wakezilla update` to update."
        );
    }

    Ok(())
}

fn checksum_for_asset(checksums: &str, asset_name: &str) -> Result<String> {
    checksums
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?.trim_start_matches('*');
            (name == asset_name).then(|| hash.to_string())
        })
        .next()
        .ok_or_else(|| anyhow!("checksum not found for {asset_name}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to string should not fail");
    }
    output
}

fn verify_checksum_bytes(bytes: &[u8], checksums: &str, asset_name: &str) -> Result<()> {
    let expected = checksum_for_asset(checksums, asset_name)?;
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        bail!("checksum verification failed for {asset_name}")
    }
}

fn extract_binary(archive_path: &Path, out_dir: &Path, bin_name: &str) -> Result<PathBuf> {
    fs::create_dir_all(out_dir).with_context(|| {
        format!(
            "failed to create extraction directory {}",
            out_dir.display()
        )
    })?;

    let file = fs::File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(out_dir)
        .with_context(|| format!("failed to extract {}", archive_path.display()))?;

    let direct = out_dir.join(bin_name);
    if direct.is_file() {
        return Ok(direct);
    }

    let mut pending = vec![out_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in
            fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
        {
            let entry = entry.context("failed to read extracted archive entry")?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("failed to read file type for {}", path.display()))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && entry.file_name() == bin_name {
                return Ok(path);
            }
        }
    }

    bail!("binary {bin_name} not found in downloaded asset")
}

fn install_binary(src: &Path, dst: &Path) -> Result<()> {
    let parent = dst
        .parent()
        .ok_or_else(|| anyhow!("destination has no parent directory: {}", dst.display()))?;
    if !parent.is_dir() {
        bail!("destination directory does not exist: {}", parent.display());
    }

    let tmp = parent.join(format!(".{BIN_NAME}.update.{}.tmp", std::process::id()));
    if tmp.exists() {
        fs::remove_file(&tmp).with_context(|| format!("failed to remove {}", tmp.display()))?;
    }

    fs::copy(src, &tmp).with_context(|| {
        format!(
            "failed to stage updated binary at {}. Try rerunning with appropriate privileges.",
            tmp.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to mark {} executable", tmp.display()))?;
    }

    if let Err(err) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(err).with_context(|| {
            format!(
                "failed to replace {}. Try rerunning with appropriate privileges.",
                dst.display()
            )
        });
    }

    Ok(())
}

pub async fn run_update(request: UpdateRequest) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("failed to create update HTTP client")?;

    let release = fetch_release(&client, request.version.as_deref()).await?;
    let release_version = normalize_tag(&release.tag_name).to_string();

    if request.version.is_none() {
        match status_from_release(env!("CARGO_PKG_VERSION"), &release)? {
            UpdateStatus::Current { current } => {
                println!("wakezilla is already up to date ({current}).");
                return Ok(());
            }
            UpdateStatus::Available { .. } => {}
        }
    }

    let target = detect_target()?;
    let asset = select_asset(&release, &target)?.clone();
    let checksums_url = checksum_url(&release_version);

    println!("Installing wakezilla v{release_version} for {target}...");

    let archive_bytes = download_bytes(&client, &asset.browser_download_url, &asset.name).await?;
    let checksums_bytes = download_bytes(&client, &checksums_url, "SHA256SUMS").await?;
    let checksums = String::from_utf8(checksums_bytes).context("SHA256SUMS was not valid UTF-8")?;
    verify_checksum_bytes(&archive_bytes, &checksums, &asset.name)?;

    let tmpdir = tempfile::tempdir().context("failed to create temporary update directory")?;
    let archive_path = tmpdir.path().join(&asset.name);
    fs::write(&archive_path, &archive_bytes)
        .with_context(|| format!("failed to write {}", archive_path.display()))?;
    let extracted = extract_binary(
        &archive_path,
        &tmpdir.path().join("extract"),
        binary_name_for_target(&target),
    )?;
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    install_binary(&extracted, &current_exe)?;

    println!(
        "Updated wakezilla to v{release_version} at {}.",
        current_exe.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(tag: &str, target: &str, prerelease: bool) -> GitHubRelease {
        let version = normalize_tag(tag);
        GitHubRelease {
            tag_name: tag.to_string(),
            prerelease,
            assets: vec![GitHubAsset {
                name: asset_name(version, target),
                browser_download_url: format!(
                    "https://example.test/{}",
                    asset_name(version, target)
                ),
            }],
        }
    }

    #[test]
    fn update_normalizes_tag_names() {
        assert_eq!(normalize_tag("v0.2.3"), "0.2.3");
        assert_eq!(normalize_tag("wakezilla/v0.2.3"), "0.2.3");
        assert_eq!(normalize_tag("0.2.3"), "0.2.3");
    }

    #[test]
    fn update_builds_release_urls() {
        assert_eq!(
            release_api_url(None),
            "https://api.github.com/repos/guibeira/wakezilla/releases/latest"
        );
        assert_eq!(
            release_api_url(Some("0.2.3")),
            "https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.2.3"
        );
        assert_eq!(
            release_api_url(Some("v0.2.3")),
            "https://api.github.com/repos/guibeira/wakezilla/releases/tags/v0.2.3"
        );
        assert_eq!(
            checksum_url("0.2.3"),
            "https://github.com/guibeira/wakezilla/releases/download/v0.2.3/SHA256SUMS"
        );
    }

    #[test]
    fn update_selects_release_asset_for_target() {
        let release = release_json("v0.2.3", "x86_64-unknown-linux-gnu", false);
        let asset = select_asset(&release, "x86_64-unknown-linux-gnu").expect("asset exists");

        assert_eq!(
            asset.name,
            "wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            asset.browser_download_url,
            "https://example.test/wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn update_missing_asset_reports_available_targets() {
        let release = release_json("v0.2.3", "x86_64-apple-darwin", false);
        let error = select_asset(&release, "x86_64-unknown-linux-gnu")
            .expect_err("target should be missing")
            .to_string();

        assert!(error.contains("x86_64-unknown-linux-gnu"));
        assert!(error.contains("x86_64-apple-darwin"));
    }

    #[test]
    fn update_detects_targets_from_parts() {
        assert_eq!(
            target_from_parts("linux", "x86_64", Some("gnu")).unwrap(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target_from_parts("linux", "arm64", Some("musl")).unwrap(),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            target_from_parts("darwin", "arm64", None).unwrap(),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            target_from_parts("windows", "x86_64", None).unwrap(),
            "x86_64-pc-windows-msvc"
        );
    }

    #[test]
    fn update_uses_exe_binary_name_for_windows_targets() {
        assert_eq!(
            asset_name("0.2.3", "x86_64-pc-windows-msvc"),
            "wakezilla-0.2.3-x86_64-pc-windows-msvc.tar.gz"
        );
        assert_eq!(
            binary_name_for_target("x86_64-pc-windows-msvc"),
            WINDOWS_BIN_NAME
        );
        assert_eq!(binary_name_for_target("x86_64-unknown-linux-gnu"), BIN_NAME);
    }

    #[test]
    fn update_status_reports_available_release() {
        let release = release_json("v0.2.3", "x86_64-unknown-linux-gnu", false);
        assert_eq!(
            status_from_release("0.2.2", &release).unwrap(),
            UpdateStatus::Available {
                current: "0.2.2".to_string(),
                latest: "0.2.3".to_string(),
            }
        );
    }

    #[test]
    fn update_status_reports_current_release() {
        let release = release_json("v0.2.2", "x86_64-unknown-linux-gnu", false);
        assert_eq!(
            status_from_release("0.2.2", &release).unwrap(),
            UpdateStatus::Current {
                current: "0.2.2".to_string(),
            }
        );
    }

    #[test]
    fn update_status_ignores_prerelease_for_startup_check() {
        let release = release_json("v0.2.3-rc1", "x86_64-unknown-linux-gnu", true);
        assert_eq!(
            status_from_release("0.2.2", &release).unwrap(),
            UpdateStatus::Current {
                current: "0.2.2".to_string(),
            }
        );
    }

    #[test]
    fn update_checksum_passes_for_matching_hash() {
        let bytes = b"wakezilla";
        let hash = sha256_hex(bytes);
        let checksums = format!("{hash}  wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz\n");

        verify_checksum_bytes(
            bytes,
            &checksums,
            "wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz",
        )
        .expect("checksum should match");
    }

    #[test]
    fn update_checksum_fails_for_mismatched_hash() {
        let error = verify_checksum_bytes(
            b"wakezilla",
            "000000  wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz\n",
            "wakezilla-0.2.3-x86_64-unknown-linux-gnu.tar.gz",
        )
        .expect_err("checksum should fail")
        .to_string();

        assert!(error.contains("checksum verification failed"));
    }

    #[test]
    fn update_extracts_binary_from_archive_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let archive_path = dir.path().join("wakezilla.tar.gz");
        let archive_file = fs::File::create(&archive_path).expect("archive file");
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        let body = b"#!/bin/sh\n";
        header.set_path(BIN_NAME).expect("header path");
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append(&header, &body[..])
            .expect("append binary to tar");
        let encoder = builder.into_inner().expect("finish tar");
        encoder.finish().expect("finish gzip");

        let extracted =
            extract_binary(&archive_path, &dir.path().join("extract"), BIN_NAME).expect("extract");
        assert_eq!(
            fs::read_to_string(extracted).expect("read binary"),
            "#!/bin/sh\n"
        );
    }

    #[test]
    fn update_installs_binary_to_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("wakezilla.new");
        let dst = dir.path().join("wakezilla");
        fs::write(&src, b"new").expect("write src");
        fs::write(&dst, b"old").expect("write dst");

        install_binary(&src, &dst).expect("install");

        assert_eq!(fs::read(&dst).expect("read dst"), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dst).expect("metadata").permissions().mode() & 0o777,
                0o755
            );
        }
    }
}
