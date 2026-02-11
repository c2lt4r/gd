use clap::Args;
use flate2::read::GzDecoder;
use miette::{miette, Result};
use owo_colors::OwoColorize;
use std::env;
use std::fs;
use std::io;
use std::path::Path;

const GITHUB_REPO: &str = "gdtoolchain/gd";

#[derive(Args)]
pub struct UpgradeArgs {
    /// Just check if a newer version is available, don't install
    #[arg(long)]
    pub check: bool,

    /// Install even if already on the latest version
    #[arg(long)]
    pub force: bool,

    /// Install a specific version instead of latest (e.g., "0.2.0" or "v0.2.0")
    #[arg(long)]
    pub version: Option<String>,
}

pub fn exec(args: UpgradeArgs) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    println!("Checking for updates...");

    let (latest_version, assets) = if let Some(ref ver) = args.version {
        let tag = if ver.starts_with('v') {
            ver.clone()
        } else {
            format!("v{ver}")
        };
        fetch_release_by_tag(&tag)?
    } else {
        fetch_latest_release()?
    };

    let latest_clean = latest_version.strip_prefix('v').unwrap_or(&latest_version);

    if args.check {
        if is_newer(latest_clean, current_version) {
            println!(
                "Current: v{}, Latest: {} {}",
                current_version,
                latest_version.cyan(),
                "(update available)".green()
            );
        } else {
            println!("Already up to date (v{})", current_version);
        }
        return Ok(());
    }

    if !args.force && !is_newer(latest_clean, current_version) {
        println!("Already up to date (v{})", current_version);
        return Ok(());
    }

    let target = current_target();
    let archive_ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let asset_name = format!("gd-{latest_version}-{target}.{archive_ext}");

    let download_url = assets
        .iter()
        .find(|a| a.name == asset_name)
        .map(|a| a.url.clone())
        .ok_or_else(|| miette!("No release found for your platform ({target})"))?;

    println!("Downloading {}...", latest_version.cyan());

    let archive_data = ureq::get(&download_url)
        .call()
        .map_err(|e| miette!("Failed to download release: {e}"))?
        .into_body()
        .read_to_vec()
        .map_err(|e| miette!("Failed to read download: {e}"))?;

    let binary_data = if cfg!(windows) {
        extract_from_zip(&archive_data)?
    } else {
        extract_from_tar_gz(&archive_data)?
    };

    replace_current_exe(&binary_data)?;

    println!(
        "{} v{} -> {}",
        "Updated gd".green().bold(),
        current_version,
        latest_version.cyan()
    );

    Ok(())
}

struct ReleaseAsset {
    name: String,
    url: String,
}

fn fetch_latest_release() -> Result<(String, Vec<ReleaseAsset>)> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    parse_release_response(&url)
}

fn fetch_release_by_tag(tag: &str) -> Result<(String, Vec<ReleaseAsset>)> {
    let url = format!(
        "https://api.github.com/repos/{GITHUB_REPO}/releases/tags/{tag}"
    );
    parse_release_response(&url)
}

fn parse_release_response(url: &str) -> Result<(String, Vec<ReleaseAsset>)> {
    let mut response = ureq::get(url)
        .header("User-Agent", concat!("gd/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| miette!("Failed to check for updates: {e}"))?;

    let json: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|e| miette!("Failed to parse release info: {e}"))?;

    let tag_name = json["tag_name"]
        .as_str()
        .ok_or_else(|| miette!("Invalid release response: missing tag_name"))?
        .to_string();

    let assets = json["assets"]
        .as_array()
        .ok_or_else(|| miette!("Invalid release response: missing assets"))?
        .iter()
        .filter_map(|a| {
            let name = a["name"].as_str()?.to_string();
            let url = a["browser_download_url"].as_str()?.to_string();
            Some(ReleaseAsset { name, url })
        })
        .collect();

    Ok((tag_name, assets))
}

fn current_target() -> &'static str {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        (os, arch) => {
            // This will be a compile-time known string for known platforms,
            // but we handle unknown gracefully at runtime
            panic!("Unsupported platform: {os}-{arch}")
        }
    }
}

/// Compare two semver strings, returning true if `latest` is newer than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<u64> = s.split('.').filter_map(|p| p.parse().ok()).collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(latest) > parse(current)
}

fn extract_from_tar_gz(data: &[u8]) -> Result<Vec<u8>> {
    let decoder = GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    let binary_name = if cfg!(windows) { "gd.exe" } else { "gd" };

    for entry in archive
        .entries()
        .map_err(|e| miette!("Failed to read archive: {e}"))?
    {
        let mut entry = entry.map_err(|e| miette!("Failed to read archive entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| miette!("Failed to read entry path: {e}"))?;

        if path.file_name().and_then(|n| n.to_str()) == Some(binary_name) {
            let mut buf = Vec::new();
            io::Read::read_to_end(&mut entry, &mut buf)
                .map_err(|e| miette!("Failed to extract binary: {e}"))?;
            return Ok(buf);
        }
    }

    Err(miette!(
        "Binary '{binary_name}' not found in the downloaded archive"
    ))
}

fn extract_from_zip(data: &[u8]) -> Result<Vec<u8>> {
    let cursor = io::Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| miette!("Failed to open zip archive: {e}"))?;

    let binary_name = if cfg!(windows) { "gd.exe" } else { "gd" };

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| miette!("Failed to read zip entry: {e}"))?;

        if Path::new(file.name()).file_name().and_then(|n| n.to_str()) == Some(binary_name) {
            let mut buf = Vec::new();
            io::Read::read_to_end(&mut file, &mut buf)
                .map_err(|e| miette!("Failed to extract binary: {e}"))?;
            return Ok(buf);
        }
    }

    Err(miette!(
        "Binary '{binary_name}' not found in the downloaded archive"
    ))
}

fn replace_current_exe(new_binary: &[u8]) -> Result<()> {
    let current_exe =
        env::current_exe().map_err(|e| miette!("Failed to determine current executable: {e}"))?;

    if cfg!(windows) {
        // Windows: can't overwrite a running exe, rename current then place new
        let old_exe = current_exe.with_extension("old");
        fs::rename(&current_exe, &old_exe).map_err(|e| {
            miette!("Failed to replace binary: {e}. Try running with elevated permissions.")
        })?;
        if let Err(e) = fs::write(&current_exe, new_binary) {
            // Restore the old binary if we fail to write the new one
            let _ = fs::rename(&old_exe, &current_exe);
            return Err(miette!(
                "Failed to replace binary: {e}. Try running with elevated permissions."
            ));
        }
        // Clean up old exe (best-effort)
        let _ = fs::remove_file(&old_exe);
    } else {
        // Unix: write to temp file next to current exe, then rename (atomic)
        let temp_exe = current_exe.with_extension("tmp");
        fs::write(&temp_exe, new_binary).map_err(|e| {
            miette!("Failed to replace binary: {e}. Try running with elevated permissions.")
        })?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_exe, fs::Permissions::from_mode(0o755)).map_err(|e| {
                let _ = fs::remove_file(&temp_exe);
                miette!("Failed to set permissions: {e}")
            })?;
        }

        fs::rename(&temp_exe, &current_exe).map_err(|e| {
            let _ = fs::remove_file(&temp_exe);
            miette!("Failed to replace binary: {e}. Try running with elevated permissions.")
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn test_current_target() {
        let target = current_target();
        assert!(!target.is_empty());
        // Should match one of the known targets
        let known = [
            "x86_64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
        ];
        assert!(
            known.contains(&target),
            "Unknown target: {target}"
        );
    }
}
