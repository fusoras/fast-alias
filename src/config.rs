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

# General-purpose aliases, grouped by category. Run them with `fa alias <name>`.
[aliases.demo]
hello = { command = "echo 'Hello from fa!'", description = "Example alias" }
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

/// Executable command declared in the alias catalog, invoked via `fa alias <name>`.
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
    pub steps: Vec<Step>,
    /// Optional message printed after a successful `fa new`, reminding the
    /// user of manual follow-ups (e.g. "edit 'src/config.ts'").
    #[serde(default)]
    pub final_message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
    /// General-purpose executable commands, organized by category. Each entry
    /// maps a category name (e.g. `git`, `sistema`) to its commands, so aliases
    /// live independently of scaffolding recipes.
    #[serde(default)]
    pub aliases: BTreeMap<String, BTreeMap<String, Command>>,
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

            for (category, commands) in sub_config.aliases {
                config
                    .aliases
                    .entry(category)
                    .or_default()
                    .extend(commands);
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

    /// Resolves an input query to its full command definition, returning
    /// `(category, command_key, &Command)`.
    pub fn resolve_command(&self, query: &str) -> Option<(String, String, &Command)> {
        for (category, commands) in &self.aliases {
            if let Some((key, command)) = find_command(commands, query) {
                return Some((category.clone(), key.clone(), command));
            }
        }
        None
    }

    /// Returns (category, command_key, &Command) for every command in the
    /// alias catalog.
    pub fn all_commands(&self) -> Vec<(&str, &String, &Command)> {
        let mut out = Vec::new();
        for (category, commands) in &self.aliases {
            for (command_key, command) in commands {
                out.push((category.as_str(), command_key, command));
            }
        }
        out
    }
}

/// Helper to resolve the user's home directory from environment.
pub fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Finds a command by its canonical key or any alias (case-insensitive).
fn find_command<'a>(
    commands: &'a BTreeMap<String, Command>,
    query: &str,
) -> Option<(&'a String, &'a Command)> {
    if let Some(key) = commands.keys().find(|k| k.eq_ignore_ascii_case(query)) {
        return commands.get_key_value(key);
    }
    commands.iter().find_map(|(k, cmd)| {
        cmd.aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(query))
            .then_some((k, cmd))
    })
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

[aliases.deploy]
deploy = { command = "node --run build", description = "Build and deploy", aliases = ["fpages", "cfp"] }
check = { command = "node --run check", description = "Run checks" }
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
            cfg.aliases["demo"].contains_key("hello"),
            "Example config should declare a 'hello' alias"
        );
        println!("   ✓ Alias 'hello' verified in catalog.\n");
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
            config.resolve_command("deploy").map(|(_, key, _)| key),
            Some("deploy".to_string())
        );
        assert_eq!(
            config.resolve_command("fpages").map(|(_, key, _)| key),
            Some("deploy".to_string())
        );
        assert!(config.resolve_command("nonexistent").is_none());
    }

    #[test]
    fn alias_categories_should_resolve_across_sections() {
        let config: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"

[aliases.git]
status = { command = "git status", description = "Repo state" }
gco = { command = "git checkout {{branch}}", description = "Switch branch", aliases = ["co"] }

[aliases.sistema]
free = { command = "free -h", description = "Free memory" }
"#,
        )
        .expect("Should parse config with alias categories");

        assert_eq!(
            config.resolve_command("gco").map(|(cat, key, _)| (cat, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("co").map(|(cat, key, _)| (cat, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("free").map(|(cat, key, _)| (cat, key)),
            Some(("sistema".to_string(), "free".to_string()))
        );
        assert_eq!(
            config.resolve_command("status").map(|(cat, key, _)| (cat, key)),
            Some(("git".to_string(), "status".to_string()))
        );
        assert!(config.resolve_command("ghost").is_none());

        let grouped = config.all_commands();
        assert_eq!(grouped.len(), 3, "Both alias categories are listed");
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

    #[test]
    fn invalid_toml_should_fail_gracefully() {
        let broken_toml = "this is not = [valid toml content {{";
        let parsed: Result<Config, _> = toml::from_str(broken_toml);
        assert!(parsed.is_err(), "Invalid TOML must return deserialization error");
    }
}
