use std::env;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    Debian,
    Termux,
    Unsupported(String),
}

impl Platform {
    /// Detects the running platform based on environment variables and filesystem indicators.
    pub fn detect() -> Self {
        if env::var("TERMUX_VERSION").is_ok() || Path::new("/data/data/com.termux").exists() {
            return Platform::Termux;
        }

        if Path::new("/etc/debian_version").exists() {
            return Platform::Debian;
        }

        if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
            let lower = os_release.to_lowercase();
            if lower.contains("id=debian")
                || lower.contains("id=ubuntu")
                || lower.contains("id_like=debian")
            {
                return Platform::Debian;
            }
        }

        Platform::Unsupported("Unknown Linux/POSIX system".to_string())
    }

    /// Returns the human-readable platform label for diagnostics.
    pub fn as_label(&self) -> String {
        match self {
            Platform::Debian => "Debian".to_string(),
            Platform::Termux => "Termux".to_string(),
            Platform::Unsupported(os) => format!("Unsupported ({os})"),
        }
    }
}

/// Detects the system architecture (e.g. x86_64, aarch64).
pub fn detect_arch() -> String {
    env::consts::ARCH.to_string()
}

/// Utility function to check if a binary exists in the system PATH.
pub fn command_exists(cmd: &str) -> bool {
    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            let bin = dir.join(cmd);
            if bin.is_file() {
                return true;
            }
        }
    }

    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_exists_should_find_cargo_in_path() {
        println!("\n🔍 [TEST] System Command Presence Check (PATH Inspection)");
        println!("   Explanation: Verifies that command_exists checks PATH directories directly without relying on external 'which'.");

        assert!(command_exists("cargo"), "cargo command should exist in test environment");
        println!("   ✓ Command 'cargo' found in system PATH.");

        assert!(!command_exists("non_existent_binary_xyz_123"), "Non-existent command should return false");
        println!("   ✓ Non-existent command correctly identified as missing.\n");
    }

    #[test]
    fn platform_detection_and_labels() {
        println!("\n🔍 [TEST] Platform Detection & Labeling");
        let detected = Platform::detect();
        let label = detected.as_label();
        assert!(!label.is_empty(), "Platform label must not be empty");

        assert_eq!(Platform::Debian.as_label(), "Debian");
        assert_eq!(Platform::Termux.as_label(), "Termux");
        assert_eq!(
            Platform::Unsupported("CustomOS".to_string()).as_label(),
            "Unsupported (CustomOS)"
        );
    }

    #[test]
    fn detect_arch_should_return_non_empty() {
        let arch = detect_arch();
        assert!(!arch.is_empty(), "Architecture detection must return a non-empty string");
    }
}
