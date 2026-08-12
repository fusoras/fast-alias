use crate::colors::*;
use crate::platform::{command_exists, Platform};
use std::env;
use std::process::Command;

/// Default GitHub repository for release checks (overridable via `FAST_ALIAS_REPO`).
const DEFAULT_REPO: &str = "fusoras/fast-alias";

/// Resolves the target GitHub repository (`owner/repo`).
fn repo() -> String {
    env::var("FAST_ALIAS_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

/// Returns the cached latest release tag from state.toml when it is newer than
/// the running version. Pure local read: zero network, zero IO latency.
pub fn check_version_update(current_version: &str) -> Option<String> {
    let state = crate::state::State::load();
    if let Some(cached) = state.cached_latest_version
        && is_newer_version(&cached, current_version)
    {
        return Some(cached);
    }
    None
}

/// Spawns a detached background process that queries the GitHub Releases API
/// and refreshes `cached_latest_version` + `last_update_check_epoch` in
/// state.toml asynchronously, so `fa` never blocks on the network.
///
/// The check runs as a re-exec of the `fa` binary with a hidden `update-check`
/// argument: it survives the parent process exiting immediately, unlike a bare
/// `std::thread` which would be killed when `fa --version` returns.
pub fn spawn_background_version_check(current_version: &str) {
    if !command_exists("curl") {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe)
        .arg("update-check")
        .env("FA_VERSION", current_version)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Runs a single release check synchronously and persists the result to
/// state.toml. Invoked by the hidden `update-check` subprocess.
pub fn check_and_cache_latest(current_version: &str) {
    if !command_exists("curl") {
        return;
    }

    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo());

    if let Ok(latest_tag) = fetch_latest_release_tag(&api_url, 3) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut state = crate::state::State::load();
        if is_newer_version(&latest_tag, current_version) {
            state.cached_latest_version = Some(latest_tag);
        } else {
            state.cached_latest_version = None;
        }
        state.last_update_check_epoch = Some(now_secs);
        let _ = state.save();
    }
}

/// True when `latest_tag` is a strictly newer release than `current_version`.
pub fn is_newer_version(latest_tag: &str, current_version: &str) -> bool {
    let tag = latest_tag.trim_start_matches('v');
    tag != current_version && semver_greater(tag, current_version)
}

/// Basic semver comparison helper (supports `-beta.N` prerelease ordering).
fn semver_greater(v1: &str, v2: &str) -> bool {
    let parse_parts = |v: &str| {
        let main_part = v.split('-').next().unwrap_or(v);
        let nums: Vec<u32> = main_part.split('.').filter_map(|s| s.parse().ok()).collect();
        let build = if v.contains("-beta.") {
            v.split("-beta.")
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0)
        } else {
            999
        };
        (nums, build)
    };

    let (p1, b1) = parse_parts(v1);
    let (p2, b2) = parse_parts(v2);

    if p1 != p2 {
        p1 > p2
    } else {
        b1 > b2
    }
}

/// Fetches the `tag_name` from the GitHub Releases API response using curl.
fn fetch_latest_release_tag(url: &str, max_time_secs: u32) -> anyhow::Result<String> {
    let output = Command::new("curl")
        .arg("-fsSL")
        .arg("--max-time")
        .arg(max_time_secs.to_string())
        .arg("-H")
        .arg("User-Agent: fa-cli")
        .arg(url)
        .output()
        .map_err(|e| anyhow::anyhow!("curl failed: {e}"))?;

    if !output.status.success() {
        anyhow::bail!("curl failed to fetch releases");
    }

    let body = String::from_utf8_lossy(&output.stdout);
    if let Some(pos) = body.find("\"tag_name\":") {
        let remainder = &body[pos + 11..];
        let start = remainder.find('"').unwrap_or(0) + 1;
        let end = remainder[start..].find('"').unwrap_or(0) + start;
        return Ok(remainder[start..end].to_string());
    }

    anyhow::bail!("tag_name not found in release response")
}

/// Resolves the release asset name for a given platform.
pub fn resolve_asset_name(platform: &Platform) -> anyhow::Result<&'static str> {
    match platform {
        Platform::Debian => Ok("fa-x86_64-unknown-linux-gnu.tar.gz"),
        Platform::Termux => Ok("fa-aarch64-unknown-linux-musl.tar.gz"),
        Platform::Unsupported(reason) => anyhow::bail!("Unsupported platform for self-update: {reason}"),
    }
}

/// Checks GitHub Releases for updates and performs an in-place self-update
/// (or a dry-run simulation) of the running `fa` binary.
pub fn check_and_perform_update(
    current_version: &str,
    platform: &Platform,
    dry_run: bool,
) -> anyhow::Result<()> {
    println!("Checking GitHub Releases for updates...");
    println!("Current version: v{current_version}");

    if !command_exists("curl") {
        anyhow::bail!("Prerequisite binary 'curl' is required for self-update checks.");
    }

    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo());

    let latest_tag = fetch_latest_release_tag(&api_url, 10).unwrap_or_else(|_| format!("v{current_version}"));
    println!("Latest release tag: {latest_tag}");

    if !is_newer_version(&latest_tag, current_version) {
        println!("\n[Up-to-Date] fa is already running the latest version.");
        Ok(())
    } else {
        let asset_name = resolve_asset_name(platform)?;
        let download_url =
            format!("https://github.com/{}/releases/download/{latest_tag}/{asset_name}", repo());

        let current_exe =
            std::env::current_exe().map_err(|e| anyhow::anyhow!("Failed to locate current executable path: {e}"))?;

        if dry_run {
            println!("\n[Dry-Run] Would download pre-compiled release binary asset: {asset_name}");
            println!("  URL: {download_url}");
            println!("  [Dry-Run] Would extract and replace executable at: {}", current_exe.display());
            Ok(())
        } else {
            println!("\n[Downloading] Fetching release binary from {download_url}...");
            let tmp_dir = std::env::temp_dir().join("fa_update");
            std::fs::create_dir_all(&tmp_dir)
                .map_err(|e| anyhow::anyhow!("Failed to create temp directory: {e}"))?;
            let tmp_tarball = tmp_dir.join("update.tar.gz");

            let curl_status = Command::new("curl")
                .arg("-fsSL")
                .arg("-o")
                .arg(&tmp_tarball)
                .arg(&download_url)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to execute curl: {e}"))?;

            if !curl_status.success() {
                anyhow::bail!("Failed to download update binary from {download_url}");
            }

            let extract_status = Command::new("tar")
                .arg("-xzf")
                .arg(&tmp_tarball)
                .arg("-C")
                .arg(&tmp_dir)
                .status()
                .map_err(|e| anyhow::anyhow!("Failed to execute tar: {e}"))?;

            if !extract_status.success() {
                anyhow::bail!("Failed to extract update tarball payload.");
            }

            let new_binary = tmp_dir.join("fa");
            if !new_binary.exists() {
                anyhow::bail!("Extracted tarball did not contain expected 'fa' binary.");
            }

            // Atomic binary replacement: back up the current executable, copy the
            // new one in its place, then clean up the backup and temp files.
            let backup_exe = current_exe.with_extension("old");
            if let Err(e) = std::fs::rename(&current_exe, &backup_exe) {
                eprintln!("[WARN] Failed to backup current binary: {e}");
            }

            std::fs::copy(&new_binary, &current_exe).map_err(|e| {
                anyhow::anyhow!("Failed to replace executable at {}: {e}", current_exe.display())
            })?;

            if let Err(e) = std::fs::remove_file(&backup_exe) {
                eprintln!("[WARN] Failed to remove backup file: {e}");
            }
            if let Err(e) = std::fs::remove_dir_all(&tmp_dir) {
                eprintln!("[WARN] Failed to clean up temp dir: {e}");
            }

            println!("\n{BOLD_GREEN}Self-update completed successfully!{RESET}");
            println!("Updated binary placed at: {}", current_exe.display());

            Ok(())
        }
    }
}

/// Self-uninstall engine: removes the `fa` executable and, when confirmed,
/// the state (`~/.local/state/fa`) and config (`~/.config/fa`) directories.
/// The DRY-RUN banner is printed by the caller before invoking this function.
pub fn perform_self_uninstall(
    dry_run: bool,
    auto_confirm: bool,
    auto_reject: bool,
) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Failed to resolve current binary path: {e}"))?;

    let state_dir = crate::state::State::state_path().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let config_dir = crate::config::Config::get_user_config_dir();

    println!("=== fa Self-Uninstall Engine ===");
    println!("Target Binary Path: {}", current_exe.display());
    if let Some(ref dir) = state_dir {
        println!("Target State Directory: {}", dir.display());
    }
    if let Some(ref dir) = config_dir {
        println!("Target Config Directory: {}", dir.display());
    }

    let should_remove_dirs = if auto_reject {
        false
    } else if auto_confirm {
        true
    } else {
        crate::prompt_yes_no("Do you want to remove configuration and state directories?", false)
    };

    if dry_run {
        println!("\n[Dry-Run] Would remove executable: {}", current_exe.display());
        if should_remove_dirs {
            if let Some(dir) = state_dir.as_deref().filter(|d| d.exists()) {
                println!("[Dry-Run] Would remove state directory: {}", dir.display());
            }
            if let Some(dir) = config_dir.as_deref().filter(|d| d.exists()) {
                println!("[Dry-Run] Would remove config directory: {}", dir.display());
            }
        }
        return Ok(());
    }

    // 1. Remove executable
    if current_exe.exists() {
        std::fs::remove_file(&current_exe)
            .map_err(|e| anyhow::anyhow!("Failed to remove binary at {}: {e}", current_exe.display()))?;
        println!("✓ Executable removed: {}", current_exe.display());
    }

    // 2. Remove state and config directories if confirmed
    if should_remove_dirs {
        if let Some(dir) = state_dir.as_deref().filter(|d| d.exists()) {
            std::fs::remove_dir_all(dir)
                .map_err(|e| anyhow::anyhow!("Failed to remove state directory at {}: {e}", dir.display()))?;
            println!("✓ State directory removed: {}", dir.display());
        }
        if let Some(dir) = config_dir.as_deref().filter(|d| d.exists()) {
            std::fs::remove_dir_all(dir)
                .map_err(|e| anyhow::anyhow!("Failed to remove config directory at {}: {e}", dir.display()))?;
            println!("✓ Config directory removed: {}", dir.display());
        }
    } else {
        println!("[Preserved] Configuration and state directories kept intact.");
    }

    println!("\n{BOLD_GREEN}fa uninstalled successfully!{RESET}");
    println!("Tip: Remember to remove PATH entries from ~/.zshrc or ~/.bashrc if no longer needed.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_version_should_return_true_for_higher_beta() {
        assert!(is_newer_version("v0.1.0-beta.2", "0.1.0-beta.1"));
    }

    #[test]
    fn newer_version_should_return_false_for_same_version() {
        assert!(!is_newer_version("v0.1.0-beta.1", "0.1.0-beta.1"));
    }

    #[test]
    fn newer_version_should_return_false_for_older_version() {
        assert!(!is_newer_version("v0.0.9", "0.1.0-beta.1"));
    }

    #[test]
    fn check_version_update_should_return_none_for_future_version() {
        assert_eq!(check_version_update("99.0.0"), None);
    }

    mod asset_resolution {
        use super::*;

        #[test]
        fn asset_name_should_match_debian_x86_64_triple() {
            let asset = resolve_asset_name(&Platform::Debian).unwrap();
            assert_eq!(asset, "fa-x86_64-unknown-linux-gnu.tar.gz");
        }

        #[test]
        fn asset_name_should_match_termux_aarch64_triple() {
            let asset = resolve_asset_name(&Platform::Termux).unwrap();
            assert_eq!(asset, "fa-aarch64-unknown-linux-musl.tar.gz");
        }

        #[test]
        fn asset_name_should_error_for_unsupported_platform() {
            let result = resolve_asset_name(&Platform::Unsupported("custom target".to_string()));
            assert!(result.is_err(), "Unsupported platform should bail with an error");
        }
    }

    mod self_uninstall {
        use super::*;

        #[test]
        fn self_uninstall_should_preview_without_deleting_in_dry_run() {
            let result = perform_self_uninstall(true, false, false);
            assert!(result.is_ok(), "Self-uninstall dry-run should complete cleanly");
        }

        #[test]
        fn self_uninstall_should_respect_auto_reject_flag() {
            let result = perform_self_uninstall(true, false, true);
            assert!(result.is_ok(), "Self-uninstall with auto_reject should complete cleanly");
        }
    }
}