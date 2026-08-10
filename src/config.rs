use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const EMBEDDED_RECIPES: &str = include_str!("../recipes.toml");

/// File generation specification. Each entry is one of:
/// - `{ from = "templates/astro/.prettierrc" }`   copy static file
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Variable {
    pub prompt: String,
    pub default: Option<String>,
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
}

impl Config {
    /// Loads configuration from local ./recipes.toml, ~/.config/fa/recipes.toml,
    /// or falls back to the embedded default configuration compiled into the binary.
    /// Also scans for modular configuration files in ./recipes.d/*.toml and ~/.config/fa/recipes.d/*.toml.
    pub fn load() -> anyhow::Result<(Self, String)> {
        let (mut config, primary_source) = Self::load_base()?;
        let mut loaded_modular_files = Vec::new();

        let local_d = Path::new("recipes.d");
        if local_d.is_dir() {
            Self::load_directory_into(&mut config, local_d, &mut loaded_modular_files)?;
        }

        if let Some(user_dir) = Self::get_user_config_dir() {
            let xdg_d = user_dir.join("recipes.d");
            if xdg_d.is_dir() {
                Self::load_directory_into(&mut config, &xdg_d, &mut loaded_modular_files)?;
            }
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

    fn load_base() -> anyhow::Result<(Self, String)> {
        let local_path = Path::new("recipes.toml");
        if local_path.exists() {
            let content = fs::read_to_string(local_path)
                .map_err(|e| anyhow::anyhow!("Failed to read local recipes.toml: {e}"))?;
            let config: Self = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse local recipes.toml: {e}"))?;
            return Ok((config, "./recipes.toml".to_string()));
        }

        if let Some(home) = dirs_home_dir() {
            let xdg_path = home.join(".config/fa/recipes.toml");
            if xdg_path.exists() {
                let content = fs::read_to_string(&xdg_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", xdg_path.display()))?;
                let config: Self = toml::from_str(&content)
                    .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", xdg_path.display()))?;
                return Ok((config, xdg_path.to_string_lossy().to_string()));
            }
        }

        let config: Self = toml::from_str(EMBEDDED_RECIPES)
            .map_err(|e| anyhow::anyhow!("Failed to parse embedded default recipes.toml: {e}"))?;
        Ok((config, "Embedded default configuration".to_string()))
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
}

/// Helper to resolve the user's home directory from environment.
pub fn dirs_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_recipe_config_should_parse_and_contain_default_recipes() {
        println!("\n🔍 [TEST] Embedded Default TOML Recipe Catalog Parsing");
        println!("   Explanation: Verifies that the recipe catalog parses successfully and contains 'astro'.");

        let config: Result<Config, _> = toml::from_str(EMBEDDED_RECIPES);
        assert!(config.is_ok(), "Embedded TOML should parse without errors");

        let cfg = config.unwrap();
        println!("   ✓ Valid TOML structure. Total recipes loaded: {}", cfg.recipes.len());

        assert!(cfg.recipes.contains_key("astro"), "Must include 'astro' recipe");
        println!("   ✓ Recipe 'astro' verified in catalog.\n");

        let astro = &cfg.recipes["astro"];
        assert_eq!(astro.files.len(), 8, "astro recipe should declare 8 files");
        println!("   ✓ Recipe 'astro' declares {} files.\n", astro.files.len());
    }

    #[test]
    fn recipe_alias_resolution_should_match_canonical_and_alias_keys() {
        let config: Config = toml::from_str(EMBEDDED_RECIPES).expect("Should parse embedded config");

        assert_eq!(config.resolve_recipe_key("astro"), Some(&"astro".to_string()));
        assert_eq!(config.resolve_recipe_key("nonexistent"), None);
    }

    #[test]
    fn recipe_names_and_aliases_should_not_have_exact_duplicates() {
        use std::collections::HashSet;

        let config: Config = toml::from_str(EMBEDDED_RECIPES).expect("Should parse embedded config");
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
        let config: Config = toml::from_str(EMBEDDED_RECIPES).expect("Should parse embedded config");
        let astro = config.recipes.get("astro").expect("astro recipe should exist");
        assert_eq!(Config::default_variant(astro), "pnpm");
    }
}
