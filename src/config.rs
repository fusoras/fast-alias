use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// First-run example configuration, written to `~/.config/fa/recipes.toml`
/// when the user config directory does not exist. The user can delete it;
/// an empty catalog then shows no recipes and no aliases.
pub const EXAMPLE_CONFIG: &str = r##"# fa example configuration
# Edit this file or add more .toml files under recipes.d/ to define your own
# recipes and aliases. Templates referenced as `from = "templates/<path>"`
# are resolved from ~/.config/fa/templates/<path>.

[recipes.example]
name = "Example"
description = "Example recipe — replace it with your own"
language = "shell"

[recipes.example.commands.example]
command = "echo 'Hello from fa!'"
description = "Example alias"
aliases = []
"##;

/// File generation specification. Each entry is one of:
/// - `{ from = "templates/my-recipe/.prettierrc" }`   copy static file
/// - `{ inline = "..." }`                          write content verbatim
/// - `{ template = "templates/x.tpl" }`            copy file AND substitute {{var}}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileSpec {
    pub from: Option<String>,
    pub inline: Option<String>,
    pub template: Option<String>,
    pub skip_if_exists: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Create {
    pub command: Option<String>,
    pub template_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Pm {
    pub install: Option<BTreeMap<String, String>>,
    pub dev_install: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tool {
    pub tool: String,
    pub script: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tooling {
    pub linter: Option<Tool>,
    pub formatter: Option<Tool>,
    pub check: Option<Tool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Step {
    pub command: String,
    pub description: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub install: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Variable {
    pub prompt: String,
    pub default: Option<String>,
}

/// Executable command exposed by a recipe, invoked via `fa alias <name>`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Command {
    pub command: String,
    pub description: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    pub language: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub variants: Vec<String>,
    pub create: Option<Create>,
    pub pm: Option<Pm>,
    pub tooling: Option<Tooling>,
    #[serde(default)]
    pub files: BTreeMap<String, FileSpec>,
    #[serde(default)]
    pub variables: BTreeMap<String, Variable>,
    #[serde(default)]
    pub commands: BTreeMap<String, Command>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Optional message printed after a successful `fa new`, reminding the
    /// user of manual follow-ups (e.g. "edit src/config.ts").
    #[serde(default)]
    pub final_message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
}

impl Config {
    /// Loads the user configuration from `~/.config/fa/recipes.toml` and
    /// `~/.config/fa/recipes.d/*.toml`. On first run (no config directory yet)
    /// it provisions an example configuration so the user always has a starting
    /// point; deleting it yields an empty catalog (no recipes, no aliases).
    pub fn load() -> anyhow::Result<(Self, String)> {
        let user_dir = Self::get_user_config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;

        if !user_dir.exists() {
            Self::provision_example(&user_dir)?;
        }

        let mut config = Self::default();
        let mut primary_source = String::new();

        let xdg_path = user_dir.join("recipes.toml");
        if xdg_path.exists() {
            let content = fs::read_to_string(&xdg_path)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", xdg_path.display()))?;
            config = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", xdg_path.display()))?;
            primary_source = xdg_path.to_string_lossy().to_string();
        }

        let mut loaded_modular_files = Vec::new();
        let xdg_d = user_dir.join("recipes.d");
        if xdg_d.is_dir() {
            Self::load_directory_into(&mut config, &xdg_d, &mut loaded_modular_files)?;
        }

        let source_summary = if loaded_modular_files.is_empty() {
            primary_source
        } else {
            format!(
                "{} (+ {} modular file(s) in recipes.d/)",
                primary_source,
                loaded_modular_files.len()
            )
        };

        Ok((config, source_summary))
    }

    /// Creates `~/.config/fa/` and writes the example configuration into it.
    /// The example is generated by `fa` itself (harmless `echo` alias), so it is
    /// auto-trusted: the user is never asked to confirm their own example file.
    fn provision_example(user_dir: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(user_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create {}: {e}", user_dir.display()))?;
        let example_path = user_dir.join("recipes.toml");
        fs::write(&example_path, EXAMPLE_CONFIG)
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", example_path.display()))?;
        let mut state = crate::state::State::load();
        state.trust(&example_path.to_string_lossy());
        state.save()?;
        println!(
            "{}Initialized example config at {}{}",
            crate::colors::BOLD_GREEN,
            example_path.display(),
            crate::colors::RESET
        );
        Ok(())
    }

    fn load_directory_into(
        config: &mut Self,
        dir: &Path,
        loaded_files: &mut Vec<PathBuf>,
    ) -> anyhow::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(path);
            }
        }

        paths.sort();

        for path in paths {
            let content = fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read modular config {}: {e}", path.display()))?;
            let sub_config: Self = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse modular config {}: {e}", path.display()))?;

            for (recipe_name, recipe) in sub_config.recipes {
                config.recipes.insert(recipe_name, recipe);
            }

            loaded_files.push(path);
        }

        Ok(())
    }

    /// Returns the standard user configuration directory (~/.config/fa).
    pub fn get_user_config_dir() -> Option<PathBuf> {
        dirs_home_dir().map(|home| home.join(".config/fa"))
    }

    /// Returns the user templates directory (~/.config/fa/templates).
    pub fn get_user_templates_dir() -> Option<PathBuf> {
        Self::get_user_config_dir().map(|dir| dir.join("templates"))
    }

    /// Returns the single path whose trust grants access to all loaded config
    /// files ("one covers all"): the primary `recipes.toml` when present, or
    /// the `recipes.d/` directory when only modular files exist. Returns `None`
    /// when there is nothing to trust (empty catalog).
    pub fn trust_anchor() -> Option<PathBuf> {
        let user_dir = Self::get_user_config_dir()?;
        let primary = user_dir.join("recipes.toml");
        if primary.is_file() {
            return Some(primary);
        }
        let modular = user_dir.join("recipes.d");
        if modular.is_dir() {
            return Some(modular);
        }
        None
    }

    /// Resolves an input query (canonical recipe ID or any defined alias) to the canonical recipe key.
    pub fn resolve_recipe_key<'a>(&'a self, query: &'a str) -> Option<&'a String> {
        if self.recipes.contains_key(query) {
            return self.recipes.get_key_value(query).map(|(k, _)| k);
        }

        for (key, recipe) in &self.recipes {
            if recipe
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(query))
            {
                return Some(key);
            }
        }

        None
    }

    /// Returns the effective default variant for a recipe (first declared, or empty).
    pub fn default_variant(recipe: &Recipe) -> String {
        recipe.variants.first().cloned().unwrap_or_default()
    }

    /// Resolves an input query (canonical command ID or any defined alias) to the
    /// canonical command key. Searches across all recipes' command tables.
    pub fn resolve_command_key(&self, query: &str) -> Option<String> {
        for recipe in self.recipes.values() {
            if let Some(key) = recipe.commands.keys().find(|k| k.eq_ignore_ascii_case(query)) {
                return Some(key.clone());
            }
            if let Some(key) = recipe.commands.iter().find_map(|(k, cmd)| {
                cmd.aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(query))
                    .then_some(k)
            }) {
                return Some(key.clone());
            }
        }
        None
    }

    /// Returns (recipe_key, command_key, &Command) for every command in the catalog.
    pub fn all_commands(&self) -> Vec<(&str, &String, &Command)> {
        let mut out = Vec::new();
        for (recipe_key, recipe) in &self.recipes {
            for (command_key, command) in &recipe.commands {
                out.push((recipe_key.as_str(), command_key, command));
            }
        }
        out
    }
}

/// Helper to resolve the user's home directory from environment.
pub fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal sample catalog used to exercise resolution logic without
    /// depending on any user-provided or example configuration.
    const SAMPLE_CONFIG: &str = r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"
language = "web · typescript"
variants = ["pnpm", "bun"]

[recipes.demo.commands.deploy]
command = "node --run build"
description = "Build and deploy"
aliases = ["fpages", "cfp"]

[recipes.demo.commands.check]
command = "node --run check"
description = "Run checks"
aliases = []
"#;

    #[test]
    fn example_config_should_parse_and_contain_example_alias() {
        println!("\n🔍 [TEST] Example Config TOML Parsing");
        println!("   Explanation: Verifies the first-run example catalog parses successfully and contains the 'example' alias.");

        let config: Result<Config, _> = toml::from_str(EXAMPLE_CONFIG);
        assert!(config.is_ok(), "Example TOML should parse without errors");

        let cfg = config.unwrap();
        println!("   ✓ Valid TOML structure. Total recipes loaded: {}", cfg.recipes.len());

        assert!(cfg.recipes.contains_key("example"), "Must include 'example' recipe");
        println!("   ✓ Recipe 'example' verified in catalog.\n");

        assert!(
            cfg.recipes["example"].commands.contains_key("example"),
            "Example recipe should declare an 'example' command"
        );
        println!("   ✓ Command 'example' verified in catalog.\n");
    }

    #[test]
    fn recipe_alias_resolution_should_match_canonical_and_alias_keys() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");

        assert_eq!(config.resolve_recipe_key("demo"), Some(&"demo".to_string()));
        assert_eq!(config.resolve_recipe_key("nonexistent"), None);
    }

    #[test]
    fn command_resolution_should_match_canonical_and_alias_keys() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");

        assert_eq!(
            config.resolve_command_key("deploy"),
            Some("deploy".to_string())
        );
        assert_eq!(
            config.resolve_command_key("fpages"),
            Some("deploy".to_string())
        );
        assert_eq!(config.resolve_command_key("nonexistent"), None);
    }

    #[test]
    fn recipe_names_and_aliases_should_not_have_exact_duplicates() {
        use std::collections::HashSet;

        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");
        let mut seen_keys = HashSet::new();

        for (key, recipe) in &config.recipes {
            assert!(
                seen_keys.insert(key.to_lowercase()),
                "Duplicate recipe name found: '{key}'"
            );
            for alias in &recipe.aliases {
                assert!(
                    seen_keys.insert(alias.to_lowercase()),
                    "Duplicate recipe alias or collision with recipe name found: '{alias}'"
                );
            }
        }
    }

    #[test]
    fn variant_resolution_should_fall_back_to_first_declared() {
        let config: Config = toml::from_str(SAMPLE_CONFIG).expect("Should parse sample config");
        let demo = config.recipes.get("demo").expect("demo recipe should exist");
        assert_eq!(Config::default_variant(demo), "pnpm");
    }
}
