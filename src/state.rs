use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

pub const STATE_FILE: &str = "state.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub recipe: String,
    pub variant: String,
    pub created_at: String,
    pub path: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectState>,
    /// Config files whose shell commands the user has explicitly trusted.
    /// Persisted by path; once trusted, `fa` never asks again for that path.
    #[serde(default)]
    pub trusted: BTreeSet<String>,
    /// Latest release tag cached by the background update check, when newer
    /// than the running version. Enables zero-latency `fa --version` hints.
    #[serde(default)]
    pub cached_latest_version: Option<String>,
    /// Unix epoch (seconds) of the last background release check.
    #[serde(default)]
    pub last_update_check_epoch: Option<u64>,
}

impl State {
    /// Returns the state file path (~/.local/state/fa/state.toml).
    pub fn state_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let path = PathBuf::from(home).join(".local/state/fa").join(STATE_FILE);
        Some(path)
    }

    /// True if the given config file path has already been trusted by the user.
    pub fn is_trusted(&self, path: &str) -> bool {
        self.trusted.contains(path)
    }

    /// Marks a config file path as trusted.
    pub fn trust(&mut self, path: &str) {
        self.trusted.insert(path.to_string());
    }


    /// Loads state from disk; returns an empty state if absent or unreadable.
    pub fn load() -> Self {
        match Self::state_path() {
            Some(path) if path.exists() => {
                let content = fs::read_to_string(&path).unwrap_or_default();
                toml::from_str(&content).unwrap_or_default()
            }
            _ => State::default(),
        }
    }

    /// Persists the state to disk atomically (temp file + rename).
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = Self::state_path() else {
            return Ok(());
        };
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("Failed to create state dir {}: {e}", parent.display()))?;

        let tmp = parent.join("state.toml.tmp");
        let content = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize state: {e}"))?;
        fs::write(&tmp, content)
            .map_err(|e| anyhow::anyhow!("Failed to write state temp file: {e}"))?;
        fs::rename(&tmp, &path)
            .map_err(|e| anyhow::anyhow!("Failed to atomically save state: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_persistence_save_and_load() {
        println!("\n🔍 [TEST] State Serialization & Deserialization Persistence");
        let mut state = State::default();
        state.projects.insert(
            "myapp".to_string(),
            ProjectState {
                recipe: "demo".to_string(),
                variant: "pnpm".to_string(),
                created_at: "2026-08-10T12:00:00Z".to_string(),
                path: "/home/user/projects/myapp".to_string(),
                installed: true,
            },
        );
        state.trust("/home/user/.config/fa/recipes.toml");

        let serialized = toml::to_string(&state).expect("State should serialize to TOML");
        let restored: State = toml::from_str(&serialized).expect("State should deserialize from TOML");

        assert!(restored.projects.contains_key("myapp"), "Restored state must contain project");
        assert_eq!(restored.projects["myapp"].recipe, "demo");
        assert!(restored.is_trusted("/home/user/.config/fa/recipes.toml"), "Restored state must keep trust");
        assert!(!restored.is_trusted("/untrusted/path"), "Untrusted path must stay untrusted");
    }

    #[test]
    fn trust_should_register_path_and_respect_is_trusted() {
        println!("\n🔍 [TEST] Config Trust Tracking");
        println!("   Explanation: Verifies a config path is trusted after one explicit confirmation.");

        let mut state = State::default();
        let path = "/home/user/.config/fa/recipes.toml";
        assert!(!state.is_trusted(path), "Should start untrusted");

        state.trust(path);
        assert!(state.is_trusted(path), "Path should be trusted after trust()");
        assert!(!state.is_trusted("/other/recipes.toml"), "Other paths must stay untrusted");
        println!("   ✓ Trust persisted for exactly the confirmed path.\n");
    }
}
