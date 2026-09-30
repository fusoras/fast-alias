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

# General-purpose aliases, grouped by section. Run them with `fa alias <name>`.
[aliases.demo]
hello = { command = "echo 'Hello from fa!'", description = "Example alias" }
"##;

/// Global configuration template, written to `~/.config/fa/config.toml`
/// when initializing the example configuration directory.
pub const EXAMPLE_GLOBAL_CONFIG: &str = r##"[packs]
# Behavior when running `fa --new <recipe>` without specifying a pack or component:
# "list" (default) -> displays all available packs and components
# "default" -> automatically installs the pack specified in `default_pack` in the recipe
# "error" -> raises an error requiring an explicit pack or component
default_behavior = "list"
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

/// A file-copy step: copies a template directory to a destination.
/// Used by the packs system to deploy web components.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateStep {
    pub from: String,
    pub to: String,
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
    #[serde(default)]
    pub command: Option<String>,
    pub create: Option<CreateStep>,
    pub description: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub install: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub prompt: String,
    pub default: Option<String>,
    /// Optional type check: "string" | "integer" | "float" | "boolean".
    /// Absent = free string, no type check (legacy behavior).
    #[serde(rename = "type", default)]
    pub var_type: Option<String>,
    /// Optional allow-list: input must equal one of these strings.
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    /// Optional regex the string form must match (type must be string).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Optional: when true, empty input is rejected.
    #[serde(default)]
    pub required: Option<bool>,
}

impl Variable {
    /// True when the variable declares any validation rule.
    pub fn is_typed(&self) -> bool {
        self.var_type.is_some()
            || self.choices.is_some()
            || self.pattern.is_some()
            || self.required == Some(true)
    }

    /// Schema check at config-load time: unknown types, empty choices,
    /// `pattern` on non-string types and invalid regexes fail with the
    /// recipe + variable name in the message.
    pub fn validate_schema(&self, recipe: &str, name: &str) -> anyhow::Result<()> {
        if let Some(t) = &self.var_type
            && !matches!(t.as_str(), "string" | "integer" | "float" | "boolean") {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': unknown type '{t}' (expected one of: string, integer, float, boolean)"
                );
            }
        if let Some(choices) = &self.choices
            && choices.is_empty() {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': 'choices' must not be empty"
                );
            }
        if let Some(p) = &self.pattern {
            let t = self.var_type.as_deref().unwrap_or("string");
            if t != "string" {
                anyhow::bail!(
                    "recipe '{recipe}' variable '{name}': 'pattern' requires type = \"string\" (got '{t}')"
                );
            }
            regex::Regex::new(p).map_err(|e| {
                anyhow::anyhow!(
                    "recipe '{recipe}' variable '{name}': invalid pattern regex '{p}': {e}"
                )
            })?;
        }
        Ok(())
    }

    /// Runtime value check: required-emptiness, type, choices and pattern.
    /// Empty (or blank when required) values skip type/choices/pattern so an
    /// optional variable left blank stays allowed.
    pub fn validate_value(&self, name: &str, value: &str) -> Result<(), String> {
        if self.required == Some(true) && value.trim().is_empty() {
            return Err(format!("'{name}' is required and must not be empty"));
        }
        if value.is_empty() {
            return Ok(());
        }
        if let Some(t) = self.var_type.as_deref() {
            let ok = match t {
                "string" => true,
                "integer" => value.trim().parse::<i64>().is_ok(),
                "float" => value.trim().parse::<f64>().is_ok(),
                "boolean" => matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "true" | "false" | "1" | "0" | "yes" | "no" | "y" | "n"
                ),
                unknown => {
                    return Err(format!(
                        "'{name}': unknown type '{unknown}' (expected one of: string, integer, float, boolean)"
                    ));
                }
            };
            if !ok {
                return Err(format!("'{name}': '{value}' is not a valid {t}"));
            }
        }
        if let Some(choices) = &self.choices
            && !choices.iter().any(|c| c == value) {
                return Err(format!("'{name}': '{value}' is not one of: {}", choices.join(", ")));
            }
        if let Some(p) = &self.pattern {
            let re = regex::Regex::new(p)
                .map_err(|e| format!("'{name}': invalid pattern regex '{p}': {e}"))?;
            if !re.is_match(value) {
                return Err(format!("'{name}': '{value}' does not match pattern '{p}'"));
            }
        }
        Ok(())
    }
}

/// Pack definition: a named group of components.
/// Can live in `~/.config/fa/<packs_dir>/<name>.toml`, multi-pack TOML files,
/// or directly inline in `[recipes.<name>.packs.<pack_name>]`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Pack {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub components: Vec<String>,
}

/// Inline pack definition inside a recipe.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PackDefinition {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub components: Vec<String>,
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
    /// Optional base directory under `~/.config/fa/templates/` prepended to
    /// every prefixless `from`/`template` spec (e.g. `template_base =
    /// "rust-stack"` + `from = ".gitignore"` → `rust-stack/.gitignore`).
    /// `None` (default) keeps the legacy behavior byte-identical: specs
    /// resolve exactly as `normalize_template_rel` does today.
    #[serde(default)]
    pub template_base: Option<String>,
    /// When true, the engine strips floating range prefixes (`^`/`~`) from
    /// `package.json` dependency versions after all installs complete.
    #[serde(default)]
    pub pin_versions: Option<bool>,
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
    /// Directory (relative to ~/.config/fa/) where pack definitions live.
    /// Packs are .toml files listing components to install together.
    #[serde(default)]
    pub packs_dir: Option<String>,
    /// Directory (relative to ~/.config/fa/) where component templates live.
    /// Overrides the default templates/ directory for this recipe's create steps.
    #[serde(default)]
    pub templates_dir: Option<String>,
    /// Default pack name used when `fa new <recipe>` is called without
    /// specifying a component or pack.
    #[serde(default, alias = "default")]
    pub default_pack: Option<String>,
    /// Optional inline packs defined directly inside the recipe.
    #[serde(default)]
    pub packs: BTreeMap<String, PackDefinition>,
}

fn default_packs_behavior() -> String {
    "list".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PacksSettings {
    /// Behavior when running `fa new <recipe>` without specifying pack or component.
    /// Values: "list" (default) | "default" (installs recipe default_pack)
    #[serde(default = "default_packs_behavior")]
    pub default_behavior: String,
}

impl Default for PacksSettings {
    fn default() -> Self {
        Self {
            default_behavior: default_packs_behavior(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub packs: PacksSettings,
}

/// Parses pack definitions from a TOML string. Supports:
/// 1. `[packs.<name>]` tables
/// 2. `[[packs]]` array of tables
/// 3. Top-level tables where each table contains `components = [...]`
/// 4. Root table containing `components = [...]` (single pack file)
pub fn parse_packs_from_toml(content: &str, file_stem: &str) -> Vec<Pack> {
    let Ok(val) = content.parse::<toml::Value>() else {
        return Vec::new();
    };

    let Some(table) = val.as_table() else {
        return Vec::new();
    };

    // Case 1: `packs` key exists
    if let Some(packs_val) = table.get("packs") {
        // 1a: Table of packs: [packs.<name>]
        if let Some(packs_table) = packs_val.as_table() {
            let mut packs = Vec::new();
            for (key, sub_val) in packs_table {
                if let Some(sub_table) = sub_val.as_table() {
                    let name = sub_table
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(key)
                        .to_string();
                    let desc = sub_table
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let components = extract_components(sub_table);
                    packs.push(Pack {
                        name,
                        description: desc,
                        components,
                    });
                }
            }
            return packs;
        }

        // 1b: Array of packs: [[packs]]
        if let Some(packs_array) = packs_val.as_array() {
            let mut packs = Vec::new();
            for item in packs_array {
                if let Some(sub_table) = item.as_table() {
                    let name = sub_table
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(file_stem)
                        .to_string();
                    let desc = sub_table
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let components = extract_components(sub_table);
                    packs.push(Pack {
                        name,
                        description: desc,
                        components,
                    });
                }
            }
            return packs;
        }
    }

    // Case 2: Root level has `components` (single pack file)
    if table.contains_key("components") {
        let name = table
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(file_stem)
            .to_string();
        let desc = table
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let components = extract_components(table);
        return vec![Pack {
            name,
            description: desc,
            components,
        }];
    }

    // Case 3: Top-level tables where each table has `components`
    let mut top_packs = Vec::new();
    for (key, sub_val) in table {
        if let Some(sub_table) = sub_val.as_table()
            && sub_table.contains_key("components")
        {
            let name = sub_table
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(key)
                .to_string();
            let desc = sub_table
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let components = extract_components(sub_table);
            top_packs.push(Pack {
                name,
                description: desc,
                components,
            });
        }
    }
    if !top_packs.is_empty() {
        return top_packs;
    }

    Vec::new()
}

fn extract_components(table: &toml::map::Map<String, toml::Value>) -> Vec<String> {
    table
        .get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|val| val.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub settings: GlobalConfig,
    #[serde(default)]
    pub recipes: BTreeMap<String, Recipe>,
    /// General-purpose executable commands, organized by section. Each entry
    /// maps a section name (e.g. `git`, `sistema`) to its commands, so aliases
    /// live independently of scaffolding recipes.
    #[serde(default)]
    pub aliases: BTreeMap<String, BTreeMap<String, Command>>,
}

impl Config {
    /// Fallback section label used only when a TOML section name arrives
    /// empty/whitespace. Real sections always come from `[aliases.<name>]`
    /// sections at runtime (Recipe Player Principle); nothing is hardcoded.
    pub const FALLBACK_ALIAS_SECTION: &'static str = "general";

    /// Returns the display name for an alias section, falling back to
    /// [`Self::FALLBACK_ALIAS_SECTION`] for empty/whitespace names.
    pub fn display_section(section: &str) -> &str {
        if section.trim().is_empty() {
            Self::FALLBACK_ALIAS_SECTION
        } else {
            section
        }
    }

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

        let global_config_path = user_dir.join("config.toml");
        if global_config_path.exists() {
            let content = fs::read_to_string(&global_config_path)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", global_config_path.display()))?;
            let global: GlobalConfig = toml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", global_config_path.display()))?;
            config.settings = global;
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

        config.validate_template_paths()?;
        config.validate_variables()?;
        config.validate_steps()?;
        config.validate_packs()?;

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
        let global_config_path = user_dir.join("config.toml");
        if !global_config_path.exists() {
            fs::write(&global_config_path, EXAMPLE_GLOBAL_CONFIG)
                .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", global_config_path.display()))?;
        }
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

            for (section, commands) in sub_config.aliases {
                config
                    .aliases
                    .entry(section)
                    .or_default()
                    .extend(commands);
            }

            loaded_files.push(path);
        }

        Ok(())
    }

    /// Normalizes a `from`/`template` spec to a path relative to
    /// `~/.config/fa/templates/`. Accepts the value with or without a single
    /// leading `templates/` prefix (it is stripped once when present, used
    /// as-is otherwise). Rejects absolute paths and `..` escapes with a clear
    /// error so misconfigurations never fail silently downstream.
    pub fn normalize_template_rel(spec: &str) -> anyhow::Result<String> {
        let rel = spec.strip_prefix("templates/").unwrap_or(spec);
        if rel.is_empty() {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        if Path::new(spec).is_absolute() || Path::new(rel).is_absolute() {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        if rel.split('/').any(|c| c == "..") {
            anyhow::bail!(
                "template path '{spec}' must stay inside ~/.config/fa/templates/"
            );
        }
        Ok(rel.to_string())
    }

    /// Validates every `from`/`template` reference in all recipes, failing fast
    /// with a clear error when a path would escape `~/.config/fa/templates/`.
    /// Resolution honors each recipe's `template_base` (see
    /// [`Self::resolve_rel`]): a `None` base resolves exactly like
    /// [`Self::normalize_template_rel`] (legacy byte-identical).
    fn validate_template_paths(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            if let Some(base) = &recipe.template_base {
                Self::normalize_template_rel(base).map_err(|e| {
                    anyhow::anyhow!(
                        "recipe '{recipe_key}': invalid template_base '{base}': {e}"
                    )
                })?;
            }
            if let Some(td) = &recipe.templates_dir
                && td.starts_with('/') {
                    anyhow::bail!(
                        "recipe '{recipe_key}': root paths starting with '/' are not allowed in templates_dir ('{td}'); use '~/' or paths relative to ~/.config/fa"
                    );
                }
            if let Some(pd) = &recipe.packs_dir
                && pd.starts_with('/') {
                    anyhow::bail!(
                        "recipe '{recipe_key}': root paths starting with '/' are not allowed in packs_dir ('{pd}'); use '~/' or paths relative to ~/.config/fa"
                    );
                }
            for (dest, spec) in &recipe.files {
                for value in [&spec.from, &spec.template].into_iter().flatten() {
                    Self::resolve_rel(recipe.template_base.as_deref(), value).map_err(|e| {
                        anyhow::anyhow!(
                            "recipe '{recipe_key}' file '{dest}': {e}"
                        )
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Resolves a `from`/`template` spec against an optional recipe-level
    /// `template_base`, returning a path relative to
    /// `~/.config/fa/templates/`.
    ///
    /// - `None` base → identical to [`Self::normalize_template_rel`] (legacy).
    /// - `Some(base)` + prefixless spec → `base/spec` (both sides normalized).
    /// - `Some(base)` + spec starting with `templates/` → base is ignored
    ///   (legacy byte-identical escape hatch).
    /// - Absolute paths, `..` escapes and empty paths are rejected with a
    ///   `must stay inside ~/.config/fa/templates/` error.
    pub fn resolve_rel(base: Option<&str>, spec: &str) -> anyhow::Result<String> {
        // Legacy escape hatch: an explicit `templates/` prefix always ignores
        // the base so old configs keep resolving byte-identically.
        if spec.starts_with("templates/") {
            return Self::normalize_template_rel(spec);
        }
        let Some(base) = base else {
            return Self::normalize_template_rel(spec);
        };
        let base_rel = Self::normalize_template_rel(base)?;
        let spec_rel = Self::normalize_template_rel(spec)?;
        let joined = format!("{base_rel}/{spec_rel}");
        // Both halves are individually clean, so the join cannot escape; run
        // it through the normalizer once more for a single clear error path.
        Self::normalize_template_rel(&joined)
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

    /// Schema check for every `[variables]` entry in every recipe.
    pub fn validate_variables(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            for (var_name, var) in &recipe.variables {
                var.validate_schema(recipe_key, var_name)?;
            }
        }
        Ok(())
    }

    /// Validates that every `[[steps]]` has either `command` or `create`
    /// (not both, not neither).
    pub fn validate_steps(&self) -> anyhow::Result<()> {
        for (recipe_key, recipe) in &self.recipes {
            for (idx, step) in recipe.steps.iter().enumerate() {
                let has_command = step.command.is_some();
                let has_create = step.create.is_some();
                if !has_command && !has_create {
                    anyhow::bail!(
                        "recipe '{recipe_key}' step {idx}: must have either 'command' or 'create'"
                    );
                }
                if has_command && has_create {
                    anyhow::bail!(
                        "recipe '{recipe_key}' step {idx}: cannot have both 'command' and 'create'"
                    );
                }
                if let Some(create) = &step.create {
                    if create.from.starts_with('/') {
                        anyhow::bail!(
                            "recipe '{recipe_key}' step {idx}: root paths starting with '/' are not allowed in create.from ('{}'); use '~/' or paths relative to ~/.config/fa",
                            create.from
                        );
                    }
                    if create.to.starts_with('/') {
                        anyhow::bail!(
                            "recipe '{recipe_key}' step {idx}: create.to path ('{}') must be relative to project and cannot start with '/'",
                            create.to
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Validates packs configuration:
    /// 1. `settings.packs.default_behavior` must be one of: 'list', 'default', 'error'.
    /// 2. If any recipe defines `default_pack`, `default_behavior` must be set to 'default'.
    ///    Otherwise, it errors instructing the user to change `default_behavior = "default"` in `~/.config/fa/config.toml`.
    pub fn validate_packs(&self) -> anyhow::Result<()> {
        let behavior = self.settings.packs.default_behavior.as_str();
        if !matches!(behavior, "list" | "default" | "error") {
            anyhow::bail!(
                "invalid packs.default_behavior '{behavior}' in config.toml; expected one of: 'list', 'default', 'error'"
            );
        }

        for (recipe_key, recipe) in &self.recipes {
            if let Some(ref dp) = recipe.default_pack
                && behavior != "default" {
                    anyhow::bail!(
                        "recipe '{recipe_key}': 'default_pack' is set to '{dp}', but global packs behavior is '{behavior}'. Change default_behavior = \"default\" in ~/.config/fa/config.toml to enable default packs in recipes"
                    );
                }
        }
        Ok(())
    }

    /// Finds a pack definition from either:
    /// 1. The recipe's inline `[recipes.<name>.packs.<pack_name>]`
    /// 2. A `<pack_name>.toml` file in `packs_dir`
    /// 3. A multi-pack TOML file in `packs_dir` (e.g. `packs.toml` or any `.toml` containing `[packs.<name>]` or `[<name>]`)
    pub fn find_pack(recipe: Option<&Recipe>, packs_dir: &str, pack_name: &str) -> anyhow::Result<Pack> {
        // 1. Check inline recipe packs
        if let Some(r) = recipe {
            if let Some(def) = r.packs.get(pack_name) {
                return Ok(Pack {
                    name: def.name.clone().unwrap_or_else(|| pack_name.to_string()),
                    description: def.description.clone(),
                    components: def.components.clone(),
                });
            }
            for (key, def) in &r.packs {
                if def.name.as_deref() == Some(pack_name) {
                    return Ok(Pack {
                        name: key.clone(),
                        description: def.description.clone(),
                        components: def.components.clone(),
                    });
                }
            }
        }

        // 2. Check packs_dir
        if packs_dir.starts_with('/') {
            anyhow::bail!("Root paths starting with '/' are not allowed; use '~/' or paths relative to ~/.config/fa");
        }
        let pack_dir = if let Some(rest) = packs_dir.strip_prefix("~/") {
            let home = dirs_home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;
            home.join(rest)
        } else {
            let Some(user_dir) = Self::get_user_config_dir() else {
                anyhow::bail!("Cannot determine home directory (HOME not set)");
            };
            user_dir.join(packs_dir)
        };

        // Try exact filename first
        let pack_path = pack_dir.join(format!("{pack_name}.toml"));
        if pack_path.is_file()
            && let Ok(content) = fs::read_to_string(&pack_path)
        {
            let stem = pack_path.file_stem().and_then(|s| s.to_str()).unwrap_or(pack_name);
            let parsed = parse_packs_from_toml(&content, stem);
            if let Some(found) = parsed.into_iter().find(|p| p.name == pack_name) {
                return Ok(found);
            }
        }

        // Scan all .toml files in pack_dir (including multi-pack files)
        if let Ok(entries) = fs::read_dir(&pack_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|s| s.to_str()) == Some("toml")
                    && let Ok(content) = fs::read_to_string(&path)
                {
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let parsed = parse_packs_from_toml(&content, stem);
                    if let Some(found) = parsed.into_iter().find(|p| p.name == pack_name) {
                        return Ok(found);
                    }
                }
            }
        }

        anyhow::bail!("Pack '{pack_name}' not found at {}", pack_path.display());
    }

    /// Loads a pack definition from `~/.config/fa/<packs_dir>/<name>.toml`
    /// or from a custom user directory starting with `~/`.
    #[allow(dead_code)]
    pub fn load_pack(packs_dir: &str, pack_name: &str) -> anyhow::Result<Pack> {
        Self::find_pack(None, packs_dir, pack_name)
    }

    /// Lists all packs for a recipe, combining inline packs with packs found in `packs_dir`.
    pub fn list_recipe_packs(recipe: &Recipe) -> Vec<Pack> {
        let mut map: BTreeMap<String, Pack> = BTreeMap::new();

        // 1. Inline packs
        for (key, def) in &recipe.packs {
            let name = def.name.clone().unwrap_or_else(|| key.clone());
            map.insert(name.clone(), Pack {
                name,
                description: def.description.clone(),
                components: def.components.clone(),
            });
        }

        // 2. Packs from packs_dir
        let packs_dir = recipe.packs_dir.as_deref().unwrap_or("packs");
        for pack in Self::list_packs(packs_dir) {
            map.entry(pack.name.clone()).or_insert(pack);
        }

        map.into_values().collect()
    }

    /// Returns the effective templates directory for a recipe.
    /// Falls back to the default `~/.config/fa/templates/`.
    pub fn resolve_templates_dir(recipe: &Recipe) -> String {
        recipe.templates_dir.clone().unwrap_or_else(|| "templates".to_string())
    }

    /// Resolves an input query to its full command definition, returning
    /// `(section, command_key, &Command)`.
    pub fn resolve_command(&self, query: &str) -> Option<(String, String, &Command)> {
        for (section, commands) in &self.aliases {
            if let Some((key, command)) = find_command(commands, query) {
                return Some((section.clone(), key.clone(), command));
            }
        }
        None
    }

    /// Lists all packs found in `packs_dir` (resolving `~/` or `~/.config/fa/<packs_dir>`).
    pub fn list_packs(packs_dir: &str) -> Vec<Pack> {
        let pack_dir = if let Some(rest) = packs_dir.strip_prefix("~/") {
            dirs_home_dir().map(|h| h.join(rest))
        } else {
            Self::get_user_config_dir().map(|u| u.join(packs_dir))
        };
        let Some(dir) = pack_dir else {
            return Vec::new();
        };
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut map: BTreeMap<String, Pack> = BTreeMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|s| s.to_str()) == Some("toml")
                && let Ok(content) = fs::read_to_string(&path)
            {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                for pack in parse_packs_from_toml(&content, stem) {
                    map.entry(pack.name.clone()).or_insert(pack);
                }
            }
        }
        map.into_values().collect()
    }

    /// Lists all components found in the recipe's `templates_dir`.
    pub fn list_components(recipe: &Recipe) -> Vec<String> {
        let templates_dir = Self::resolve_templates_dir(recipe);
        let comp_dir = if let Some(rest) = templates_dir.strip_prefix("~/") {
            dirs_home_dir().map(|h| h.join(rest))
        } else {
            Self::get_user_config_dir().map(|u| u.join(&templates_dir))
        };
        let Some(dir) = comp_dir else {
            return Vec::new();
        };
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        let mut components = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                components.push(name);
            }
        }
        components.sort();
        components
    }

    /// Returns (section, command_key, &Command) for every command in the
    /// alias catalog.
    pub fn all_commands(&self) -> Vec<(&str, &String, &Command)> {
        let mut out = Vec::new();
        for (section, commands) in &self.aliases {
            for (command_key, command) in commands {
                out.push((section.as_str(), command_key, command));
            }
        }
        out
    }
}

impl Recipe {
    /// True if the recipe defines pack/component workflows (has `packs_dir`, `templates_dir`,
    /// `default_pack`, inline `packs`, or steps with `create`).
    pub fn is_pack_recipe(&self) -> bool {
        self.packs_dir.is_some()
            || self.templates_dir.is_some()
            || self.default_pack.is_some()
            || !self.packs.is_empty()
            || self.steps.iter().any(|s| s.create.is_some())
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
    fn alias_sections_should_resolve_across_sections() {
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
        .expect("Should parse config with alias sections");

        assert_eq!(
            config.resolve_command("gco").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("co").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "gco".to_string()))
        );
        assert_eq!(
            config.resolve_command("free").map(|(section, key, _)| (section, key)),
            Some(("sistema".to_string(), "free".to_string()))
        );
        assert_eq!(
            config.resolve_command("status").map(|(section, key, _)| (section, key)),
            Some(("git".to_string(), "status".to_string()))
        );
        assert!(config.resolve_command("ghost").is_none());

        let grouped = config.all_commands();
        assert_eq!(grouped.len(), 3, "Both alias sections are listed");
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

    #[test]
    fn template_path_should_accept_with_or_without_prefix() {
        println!("\n🔍 [TEST] Template path — with/without `templates/` prefix resolves equally");
        let with = Config::normalize_template_rel("templates/my-recipe/file.txt")
            .expect("prefixed path must normalize");
        let without = Config::normalize_template_rel("my-recipe/file.txt")
            .expect("prefixless path must normalize");
        assert_eq!(with, "my-recipe/file.txt");
        assert_eq!(without, "my-recipe/file.txt");
        // Only one leading prefix is stripped: a nested `templates/` dir stays.
        let nested = Config::normalize_template_rel("templates/templates/x")
            .expect("nested templates dir must normalize");
        assert_eq!(nested, "templates/x");
        println!("   ✓ Prefixed and prefixless paths normalize to the same rel.\n");
    }

    #[test]
    fn template_path_should_reject_escape_with_clear_error() {
        println!("\n🔍 [TEST] Template path — traversal/absolute rejected with clear error");
        for bad in ["../evil.txt", "templates/../evil.txt", "a/../../b", "/abs/path.txt", "templates/"] {
            let err = Config::normalize_template_rel(bad)
                .err()
                .unwrap_or_else(|| panic!("'{bad}' must be rejected"));
            let msg = err.to_string();
            assert!(
                msg.contains("must stay inside ~/.config/fa/templates/"),
                "Error must name the templates dir, got: '{msg}'"
            );
        }
        println!("   ✓ Out-of-range paths rejected with clear error (not silent).\n");
    }

    #[test]
    fn template_base_should_default_to_none_for_legacy_compat() {
        println!("\n🔍 [TEST] template_base — defaults to None (legacy byte-identical)");
        let config: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "A demo recipe"
"#,
        )
        .expect("Should parse recipe without template_base");
        let demo = config.recipes.get("demo").expect("demo must exist");
        assert!(demo.template_base.is_none(), "legacy recipes default to None");
        println!("   ✓ Missing template_base defaults to None.\n");
    }

    #[test]
    fn resolve_rel_should_keep_legacy_identical_when_base_is_none() {
        println!("\n🔍 [TEST] resolve_rel — base None keeps legacy identical");
        let with = Config::resolve_rel(None, "templates/my-recipe/file.txt")
            .expect("prefixed must resolve");
        let bare = Config::resolve_rel(None, "my-recipe/file.txt")
            .expect("prefixless must resolve");
        assert_eq!(with, "my-recipe/file.txt");
        assert_eq!(bare, "my-recipe/file.txt");
        assert_eq!(with, bare, "both forms must resolve equally with None base");
        println!("   ✓ Base None resolves exactly like legacy normalize.\n");
    }

    #[test]
    fn resolve_rel_should_join_base_and_spec() {
        println!("\n🔍 [TEST] resolve_rel — base + from join");
        let rel = Config::resolve_rel(Some("rust-stack"), ".gitignore")
            .expect("base + spec must join");
        assert_eq!(rel, "rust-stack/.gitignore");
        let nested = Config::resolve_rel(Some("rust-stack"), "config/.editorconfig")
            .expect("nested spec must join");
        assert_eq!(nested, "rust-stack/config/.editorconfig");
        println!("   ✓ Joined rel verified.\n");
    }

    #[test]
    fn resolve_rel_should_ignore_base_when_spec_has_templates_prefix() {
        println!("\n🔍 [TEST] resolve_rel — `templates/` spec ignores base (legacy byte-identical)");
        let with_base =
            Config::resolve_rel(Some("rust-stack"), "templates/other/file.txt")
                .expect("prefixed spec must resolve");
        let without_base = Config::resolve_rel(None, "templates/other/file.txt")
            .expect("legacy must resolve");
        assert_eq!(with_base, "other/file.txt");
        assert_eq!(with_base, without_base, "prefixed spec must ignore base entirely");
        println!("   ✓ Prefixed spec ignores base.\n");
    }

    #[test]
    fn resolve_rel_should_reject_escapes_with_clear_error() {
        println!("\n🔍 [TEST] resolve_rel — escapes/absolutes rejected with clear error");
        for (base, spec) in [
            (Some("rust-stack"), "../evil.txt"),
            (Some("../evil"), "file.txt"),
            (Some("rust-stack"), "/abs/x.txt"),
            (Some("rust-stack"), ""),
            (Some("rust-stack"), "templates/../evil.txt"),
            (None, "../evil.txt"),
        ] {
            let err = Config::resolve_rel(base, spec)
                .err()
                .unwrap_or_else(|| panic!("base={base:?} spec='{spec}' must be rejected"));
            let msg = err.to_string();
            assert!(
                msg.contains("must stay inside ~/.config/fa/templates/"),
                "Error must name the templates dir, got: '{msg}'"
            );
        }
        println!("   ✓ Escapes rejected clearly.\n");
    }

    #[test]
    fn var_schema_should_reject_unknown_type() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", type = "date" }
"#,
        )
        .expect("shape must parse; type checked at validation");
        let err = cfg.validate_variables().expect_err("unknown type must fail");
        let msg = err.to_string();
        assert!(msg.contains("demo") && msg.contains("port") && msg.contains("date"),
            "error must name recipe + variable + bad type, got: '{msg}'");
    }

    #[test]
    fn var_schema_should_reject_unknown_keys() {
        let parsed: Result<Config, _> = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", bogus_key = "x" }
"#,
        );
        let err = parsed.expect_err("unknown variable key must fail at load");
        assert!(err.to_string().contains("bogus_key"),
            "error must name the unknown key, got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_empty_choices() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
pm = { prompt = "PM", choices = [] }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("empty choices must fail");
        assert!(err.to_string().contains("must not be empty"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_pattern_with_non_string_type() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", type = "integer", pattern = "^[0-9]+$" }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("pattern + integer must fail");
        assert!(err.to_string().contains("requires type"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_reject_invalid_regex() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
slug = { prompt = "Slug", pattern = "([a-" }
"#,
        )
        .expect("shape must parse");
        let err = cfg.validate_variables().expect_err("invalid regex must fail");
        assert!(err.to_string().contains("invalid pattern"), "got: '{err}'");
    }

    #[test]
    fn var_schema_should_accept_valid_optional_keys() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", type = "integer", choices = ["3000", "8080"] }
slug = { prompt = "Slug", type = "string", pattern = "^[a-z0-9-]+$", required = true }
"#,
        )
        .expect("valid typed variables must parse");
        cfg.validate_variables().expect("valid typed variables must validate");
        assert!(cfg.recipes["demo"].variables["port"].is_typed());
    }

    #[test]
    fn var_plain_prompt_without_typing_should_keep_working() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.demo]
name = "Demo"
description = "d"
[recipes.demo.variables]
name = { prompt = "Project name", default = "app" }
"#,
        )
        .expect("plain variable must parse");
        cfg.validate_variables().expect("plain variable must validate");
        let v = &cfg.recipes["demo"].variables["name"];
        assert!(!v.is_typed(), "untyped variable must report is_typed() == false");
        assert!(v.validate_value("name", "anything at all!@#").is_ok());
        assert!(v.validate_value("name", "").is_ok(), "empty stays allowed when not required");
    }

    #[test]
    fn var_value_should_reject_bad_type_choice_pattern_and_required_empty() {
        let int_var = Variable {
            prompt: "Port".into(), default: Some("3000".into()),
            var_type: Some("integer".into()), choices: None, pattern: None, required: None,
        };
        assert!(int_var.validate_value("port", "abc").is_err(), "non-integer must fail");
        assert!(int_var.validate_value("port", "8080").is_ok());

        let choice_var = Variable {
            prompt: "PM".into(), default: None,
            var_type: None, choices: Some(vec!["pnpm".into(), "bun".into()]),
            pattern: None, required: None,
        };
        assert!(choice_var.validate_value("pm", "npm").is_err(), "off-list choice must fail");
        assert!(choice_var.validate_value("pm", "pnpm").is_ok());

        let pattern_var = Variable {
            prompt: "Slug".into(), default: None,
            var_type: Some("string".into()), choices: None,
            pattern: Some("^[a-z0-9-]+$".into()), required: None,
        };
        assert!(pattern_var.validate_value("slug", "Bad Name!").is_err(), "pattern mismatch must fail");
        assert!(pattern_var.validate_value("slug", "my-app-1").is_ok());

        let req_var = Variable {
            prompt: "Token".into(), default: None,
            var_type: None, choices: None, pattern: None, required: Some(true),
        };
        assert!(req_var.validate_value("token", "").is_err(), "empty required must fail");
        assert!(req_var.validate_value("token", "  ").is_err(), "blank required must fail");
        assert!(req_var.validate_value("token", "x").is_ok());
    }

    #[test]
    fn validate_packs_should_reject_default_pack_when_behavior_is_not_default() {
        let toml_content = r#"
[recipes.wc-lib]
name = "wc-lib"
description = "Web Components"
default_pack = "toggle-theme"
"#;
        let cfg: Config = toml::from_str(toml_content).unwrap();
        // default_behavior is "list" by default
        assert_eq!(cfg.settings.packs.default_behavior, "list");
        let res = cfg.validate_packs();
        assert!(res.is_err(), "Must reject default_pack when default_behavior is 'list'");
        let err = res.unwrap_err().to_string();
        assert!(
            err.contains("Change default_behavior = \"default\" in ~/.config/fa/config.toml"),
            "Error must instruct to change config.toml, got: {err}"
        );
    }

    #[test]
    fn validate_packs_should_allow_default_pack_when_behavior_is_default() {
        let toml_content = r#"
[recipes.wc-lib]
name = "wc-lib"
description = "Web Components"
default_pack = "toggle-theme"
"#;
        let mut cfg: Config = toml::from_str(toml_content).unwrap();
        cfg.settings.packs.default_behavior = "default".to_string();
        assert!(cfg.validate_packs().is_ok(), "Must allow default_pack when default_behavior is 'default'");
    }

    #[test]
    fn validate_packs_should_reject_invalid_behavior() {
        let mut cfg = Config::default();
        cfg.settings.packs.default_behavior = "unknown_mode".to_string();
        let res = cfg.validate_packs();
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("expected one of: 'list', 'default', 'error'"));
    }

    #[test]
    fn test_recipe_default_pack_rename_and_alias() {
        let toml_new = r#"
[recipes.test1]
name = "Test 1"
description = "d"
default_pack = "my-pack"
"#;
        let cfg: Config = toml::from_str(toml_new).unwrap();
        assert_eq!(cfg.recipes["test1"].default_pack.as_deref(), Some("my-pack"));

        let toml_legacy = r#"
[recipes.test2]
name = "Test 2"
description = "d"
default = "my-pack"
"#;
        let cfg: Config = toml::from_str(toml_legacy).unwrap();
        assert_eq!(cfg.recipes["test2"].default_pack.as_deref(), Some("my-pack"));
    }

    #[test]
    fn test_global_config_loads_all_three_modes() {
        let cfg_default = GlobalConfig::default();
        assert_eq!(cfg_default.packs.default_behavior, "list");

        let toml_list = "[packs]\ndefault_behavior = \"list\"";
        let cfg_list: GlobalConfig = toml::from_str(toml_list).unwrap();
        assert_eq!(cfg_list.packs.default_behavior, "list");

        let toml_def = "[packs]\ndefault_behavior = \"default\"";
        let cfg_def: GlobalConfig = toml::from_str(toml_def).unwrap();
        assert_eq!(cfg_def.packs.default_behavior, "default");

        let toml_err = "[packs]\ndefault_behavior = \"error\"";
        let cfg_err: GlobalConfig = toml::from_str(toml_err).unwrap();
        assert_eq!(cfg_err.packs.default_behavior, "error");
    }

    #[test]
    fn test_parse_packs_multi_pack_table() {
        let toml_str = r#"
[packs.default]
description = "Default UI bundle"
components = ["toggle-theme", "btn-ally"]

[packs.wc-ui]
description = "Full UI bundle"
components = ["toggle-theme", "btn-ally", "wc-modal"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from [packs.<name>] table");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[0].components, vec!["toggle-theme", "btn-ally"]);
        assert_eq!(packs[1].name, "wc-ui");
        assert_eq!(packs[1].components, vec!["toggle-theme", "btn-ally", "wc-modal"]);
    }

    #[test]
    fn test_parse_packs_array_of_tables() {
        let toml_str = r#"
[[packs]]
name = "default"
components = ["toggle-theme"]

[[packs]]
name = "full"
components = ["toggle-theme", "wc-modal"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from [[packs]] array");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[1].name, "full");
    }

    #[test]
    fn test_parse_packs_top_level_tables() {
        let toml_str = r#"
[default]
components = ["toggle-theme"]

[wc-ui]
components = ["toggle-theme", "btn-ally"]
"#;
        let packs = parse_packs_from_toml(toml_str, "packs");
        assert_eq!(packs.len(), 2, "Must parse 2 packs from top-level tables");
        assert_eq!(packs[0].name, "default");
        assert_eq!(packs[1].name, "wc-ui");
    }

    #[test]
    fn test_recipe_inline_packs() {
        let toml_str = r#"
[recipes.wc-lib]
name = "Web Components"
description = "Modular components"

[recipes.wc-lib.packs.default]
components = ["toggle-theme"]

[recipes.wc-lib.packs.wc-ui]
components = ["toggle-theme", "btn-ally"]
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        let recipe = &cfg.recipes["wc-lib"];
        assert_eq!(recipe.packs.len(), 2);
        assert!(recipe.is_pack_recipe(), "Recipe with inline packs must be recognized as pack recipe");

        let pack = Config::find_pack(Some(recipe), "nonexistent_dir", "wc-ui").unwrap();
        assert_eq!(pack.name, "wc-ui");
        assert_eq!(pack.components, vec!["toggle-theme", "btn-ally"]);
    }

    #[test]
    fn test_provision_example_creates_config_toml_with_english_comments() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let user_dir = std::env::temp_dir().join(format!("fa-test-prov-{}-{}", std::process::id(), count));
        let _ = fs::remove_dir_all(&user_dir);

        Config::provision_example(&user_dir).unwrap();

        let config_toml_path = user_dir.join("config.toml");
        assert!(config_toml_path.exists(), "provision_example must create config.toml");

        let content = fs::read_to_string(&config_toml_path).unwrap();
        assert!(content.contains("[packs]"));
        assert!(content.contains("default_behavior = \"list\""));
        assert!(content.contains("# Behavior when running"));
        assert!(!content.contains("Comportamiento"));
    }
}
