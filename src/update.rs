use crate::platform::command_exists;
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
}