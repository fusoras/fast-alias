//! User recipe config files: create, edit, and validate under `~/.config/fa`.
//!
//! All functions take an injected `config_dir` (normally `~/.config/fa`) so
//! tests run against temp dirs without touching the real user config.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// Rejects empty names and path-hostile characters (`/`, `\`, `"`).
pub fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("recipe name must not be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains('"') {
        anyhow::bail!("invalid recipe name '{name}': must not contain '/', '\\' or '\"'");
    }
    Ok(())
}

/// Builds the TOML scaffold written by `fa recipe new <name> [type]`:
/// - `None` (or "minimal" / "clean"): Clean, minimal scaffold with only essential fields.
/// - `Some("standard" | "full")`: Guided scaffold with commented-out examples and options.
/// - `Some("pack" | "packs")`: Component/pack library preset scaffold.
/// - `Some("alias" | "aliases")`: Clean, basic alias catalog scaffold.
pub fn scaffold_toml(name: &str, recipe_type: Option<&str>) -> String {
    let schema_url = crate::schema::SCHEMA_URL;

    match recipe_type {
        Some("alias" | "aliases") => format!(
            r##"#:schema {schema_url}

[aliases."{name}"]
example = {{ command = "echo 'Hello from {name}'", description = "Example alias" }}
"##,
            schema_url = schema_url,
            name = name
        ),
        Some("pack" | "packs") => format!(
            r##"#:schema {schema_url}
# fa pack library recipe: {name}
# Created by `fa recipe new {name} pack`.
# Run: fa new {name} [component|pack]

[recipes."{name}"]
name = "{name}"
description = "TODO: describe the {name} component library"
templates_dir = "templates/{name}"
packs_dir = "packs/{name}"
# default_pack = "default"

# Optional inline pack definition (or place small .toml files under packs/{name}/):
# [recipes."{name}".packs.default]
# description = "Default component bundle"
# components = ["item-1"]

# Step to copy component files into your project destination:
[[recipes."{name}".steps]]
create = {{ from = "{{{{templates_dir}}}}/{{{{component}}}}", to = "src/components/{{{{component}}}}" }}
description = "Install component files into src/components/"
"##,
            schema_url = schema_url,
            name = name
        ),
        Some("standard" | "full") => format!(
            r##"#:schema {schema_url}
# fa recipe: {name}
# Created by `fa recipe new`. Edit it, then run: fa new {name} <project-dir>

[recipes."{name}"]
name = "{name}"
description = "TODO: describe the {name} recipe"
# language = "rust"
# aliases = []
# variants = []
# final_message = "Run 'cd <project-name>' to get started"
# pin_versions = false

# Example command alias (run with: fa alias hello):
# [aliases."{name}"]
# hello = {{ command = "echo hi", description = "Example alias" }}

# Example step:
# [[recipes."{name}".steps]]
# command = "cargo --version"
# description = "Check toolchain"
"##,
            schema_url = schema_url,
            name = name
        ),
        _ => format!(
            r##"#:schema {schema_url}

[recipes."{name}"]
name = "{name}"
description = "TODO: describe the {name} recipe"

[[recipes."{name}".steps]]
command = "echo 'Initializing {name}'"
description = "Setup step"
"##,
            schema_url = schema_url,
            name = name
        ),
    }
}

/// Comment-only scratch scaffold for `fa recipe new` (no name). fa never
/// renames this file — the user promotes it manually to `<recipe-name>.toml`.
pub fn scratch_toml() -> String {
    format!(
        r##"#:schema {schema_url}
# fa scratch recipe (untitled)
# fa does NOT rename this file — promote it manually:
#   mv <this-file> ~/.config/fa/recipes.d/<recipe-name>.toml
#
# Guide — minimal recipe schema:
# [recipes."<recipe-name>"]
# name = "<recipe-name>"
# description = "What this recipe scaffolds"
# language = "rust"
# variants = []
#
# Optional command alias:
# [aliases."<recipe-name>"]
# hello = {{ command = "echo hi", description = "Example alias" }}
#
# Optional step:
# [[recipes."<recipe-name>".steps]]
# command = "cargo --version"
# description = "Check toolchain"
"##,
        schema_url = crate::schema::SCHEMA_URL
    )
}

/// Lists every config file in load order: primary `recipes.toml`, then
/// `recipes.d/*.toml` sorted by name.
pub fn config_files(config_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let primary = config_dir.join("recipes.toml");
    if primary.is_file() {
        files.push(primary);
    }
    let modular_dir = config_dir.join("recipes.d");
    if let Ok(entries) = fs::read_dir(&modular_dir) {
        let mut tomls: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml")
            })
            .collect();
        tomls.sort();
        files.extend(tomls);
    }
    files
}

/// Parses a single config file into `Config`, prefixing errors with the path.
fn parse_config_file(path: &Path) -> anyhow::Result<Config> {
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
    toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))
}

/// Creates `recipes.d/<name>.toml` (or an untitled scratch file when `name`
/// is `None`) with scaffold content. Never overwrites an existing file and
/// rejects names already defined as a recipe key in any config file.
pub fn recipe_new(
    config_dir: &Path,
    name: Option<&str>,
    recipe_type: Option<&str>,
) -> anyhow::Result<PathBuf> {
    if let Some(t) = recipe_type
        && !matches!(t, "standard" | "app" | "pack" | "packs" | "alias" | "aliases" | "minimal" | "clean")
    {
        anyhow::bail!("invalid recipe type '{t}': expected 'standard', 'pack', or 'alias'");
    }
    let recipes_d = config_dir.join("recipes.d");

    let target = match name {
        Some(name) => {
            validate_name(name)?;
            for path in config_files(config_dir) {
                if let Ok(cfg) = parse_config_file(&path) && cfg.recipes.contains_key(name) {
                    anyhow::bail!("recipe '{name}' ya existe (definida en {})", path.display());
                }
            }
            let file = recipes_d.join(format!("{name}.toml"));
            if file.exists() {
                anyhow::bail!("recipe '{name}' ya existe");
            }
            fs::create_dir_all(&recipes_d)
                .map_err(|e| anyhow::anyhow!("Failed to create {}: {e}", recipes_d.display()))?;
            fs::write(&file, scaffold_toml(name, recipe_type))
                .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", file.display()))?;
            file
        }
        None => {
            fs::create_dir_all(&recipes_d)
                .map_err(|e| anyhow::anyhow!("Failed to create {}: {e}", recipes_d.display()))?;
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let file = recipes_d.join(format!("untitled-{secs}-{}.toml", std::process::id()));
            if file.exists() {
                anyhow::bail!("recipe '{}' ya existe", file.display());
            }
            fs::write(&file, scratch_toml())
                .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", file.display()))?;
            file
        }
    };

    Ok(target)
}

/// Resolves `fa recipe edit <name>` to an existing config file path:
/// 1. `recipes.d/<name>.toml` when that file exists;
/// 2. otherwise a recipe-key lookup across all config files (last match
///    wins, mirroring `Config::load` merge order);
/// 3. otherwise `recipe '<name>' does not exist` — never creates files.
pub fn recipe_edit_path(config_dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
    validate_name(name)?;
    let direct = config_dir.join("recipes.d").join(format!("{name}.toml"));
    if direct.is_file() {
        return Ok(direct);
    }
    if name == "recipes" || name == "recipes.toml" {
        let primary = config_dir.join("recipes.toml");
        if primary.is_file() {
            return Ok(primary);
        }
    }
    let mut found: Option<PathBuf> = None;
    for path in config_files(config_dir) {
        if let Ok(cfg) = parse_config_file(&path) {
            if cfg.recipes.contains_key(name) || cfg.aliases.contains_key(name) {
                found = Some(path);
            }
        } else if let Ok(raw) = fs::read_to_string(&path) {
            let target_recipe = format!("[recipes.{name}]");
            let target_alias = format!("[aliases.{name}]");
            let target_ns = format!("[aliases.\":{name}\"]");
            if raw.contains(&target_recipe) || raw.contains(&target_alias) || raw.contains(&target_ns) {
                found = Some(path);
            }
        }
    }
    found.ok_or_else(|| anyhow::anyhow!("recipe '{name}' no existe"))
}

/// Deletes a recipe's dedicated TOML file under `recipes.d/<name>.toml`.
/// Leaves templates and packs untouched.
/// Rejects deleting primary `recipes.toml` or multi-recipe files to prevent
/// destroying other recipes/aliases.
pub fn recipe_rm(config_dir: &Path, name: &str) -> anyhow::Result<PathBuf> {
    validate_name(name)?;
    let direct = config_dir.join("recipes.d").join(format!("{name}.toml"));
    if direct.is_file() {
        fs::remove_file(&direct)
            .map_err(|e| anyhow::anyhow!("Failed to remove {}: {e}", direct.display()))?;
        return Ok(direct);
    }

    let mut found: Option<PathBuf> = None;
    for path in config_files(config_dir) {
        if let Ok(cfg) = parse_config_file(&path)
            && cfg.recipes.contains_key(name)
        {
            found = Some(path);
        }
    }

    match found {
        Some(path) => {
            if path == config_dir.join("recipes.toml") {
                anyhow::bail!(
                    "recipe '{name}' is defined in primary recipes.toml; delete the [recipes.\"{name}\"] section manually"
                );
            }
            if let Ok(cfg) = parse_config_file(&path)
                && cfg.recipes.len() > 1
            {
                anyhow::bail!(
                    "file '{}' defines multiple recipes; remove [recipes.\"{name}\"] manually",
                    path.display()
                );
            }
            fs::remove_file(&path)
                .map_err(|e| anyhow::anyhow!("Failed to remove {}: {e}", path.display()))?;
            Ok(path)
        }
        None => anyhow::bail!("recipe '{name}' no existe"),
    }
}

/// Recipe key + defining file pairs for `fa recipe edit` without a name.
/// Sorted by key; later files override earlier ones (mirrors `Config::load`).
/// Unparseable files are skipped so listing always succeeds.
pub fn recipe_list(config_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut map: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in config_files(config_dir) {
        if let Ok(cfg) = parse_config_file(&path) {
            for key in cfg.recipes.keys() {
                map.insert(key.clone(), path.clone());
            }
        }
    }
    map.into_iter().collect()
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut d = vec![vec![0; n + 1]; m + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }
    d[m][n]
}

pub fn suggest_best_match<'a>(unknown: &str, candidates: &'a [&'a str]) -> Option<&'a str> {
    for &cand in candidates {
        if cand.starts_with(unknown) || cand.contains(unknown) {
            return Some(cand);
        }
    }

    let mut best: Option<(&'a str, usize)> = None;
    for &cand in candidates {
        let dist = levenshtein(unknown, cand);
        let threshold = (unknown.len() / 2).clamp(2, 4);
        if dist <= threshold {
            match best {
                None => best = Some((cand, dist)),
                Some((_, prev_dist)) if dist < prev_dist => best = Some((cand, dist)),
                _ => {}
            }
        }
    }
    best.map(|(cand, _)| cand)
}

/// Suggests the closest match from a list of candidates for a typo.
pub fn suggest_closest(unknown: &str, candidates: &[&str]) -> Option<String> {
    if unknown.is_empty() || candidates.is_empty() {
        return None;
    }
    let lower_unknown = unknown.to_lowercase();
    let mut best: Option<(&str, usize)> = None;

    for &cand in candidates {
        let lower_cand = cand.to_lowercase();
        if lower_cand == lower_unknown {
            return Some(cand.to_string());
        }

        let dist = levenshtein(&lower_unknown, &lower_cand);
        // Threshold: for short words (len <= 4), distance <= 2; for longer, distance <= max(3, len / 2)
        let threshold = if lower_unknown.len() <= 4 {
            2
        } else {
            (lower_unknown.len() / 2).clamp(2, 4)
        };

        let effective_dist = if lower_cand.starts_with(&lower_unknown) || lower_unknown.starts_with(&lower_cand) {
            dist.saturating_sub(1)
        } else {
            dist
        };

        if dist <= threshold || lower_cand.starts_with(&lower_unknown) {
            match best {
                None => best = Some((cand, effective_dist)),
                Some((_, prev_dist)) if effective_dist < prev_dist => best = Some((cand, effective_dist)),
                _ => {}
            }
        }
    }

    best.map(|(cand, _)| cand.to_string())
}

const KNOWN_RECIPE_FIELDS: &[&str] = &[
    "name",
    "description",
    "language",
    "aliases",
    "variants",
    "files",
    "pm",
    "tools",
    "tooling",
    "steps",
    "templates_dir",
    "template_base",
    "packs_dir",
    "default_pack",
    "packs",
    "create",
    "final_message",
    "pin_versions",
    "variables",
];

const KNOWN_CREATE_FIELDS: &[&str] = &["command", "template_dir", "from", "to"];

const KNOWN_VARIABLE_FIELDS: &[&str] = &[
    "prompt",
    "description",
    "default",
    "type",
    "choices",
    "pattern",
    "required",
];

const KNOWN_STEP_FIELDS: &[&str] = &[
    "command",
    "description",
    "dir",
    "create",
    "platform",
    "script",
    "aliases",
    "install",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueSeverity {
    Error,
    Warning,
    Notice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: IssueSeverity,
    pub message: String,
    pub suggestion: Option<String>,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.severity {
            IssueSeverity::Error => write!(f, "error: {}", self.message)?,
            IssueSeverity::Warning => write!(f, "warning: {}", self.message)?,
            IssueSeverity::Notice => write!(f, "notice: {}", self.message)?,
        }
        if let Some(sugg) = &self.suggestion {
            write!(f, "\n  --> Did you mean '{sugg}'?")?;
        }
        Ok(())
    }
}

fn validate_toml_file(config_dir: &Path, file_path: &Path, issues: &mut Vec<ValidationIssue>) {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Error,
                message: format!("Failed to read {}: {e}", file_path.display()),
                suggestion: None,
            });
            return;
        }
    };

    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Error,
                message: format!("Syntax error in {}: {e}", file_path.display()),
                suggestion: None,
            });
            return;
        }
    };

    if let Some(recipes) = value.get("recipes").and_then(|r| r.as_table()) {
        for (recipe_name, recipe_val) in recipes {
            if let Some(recipe_table) = recipe_val.as_table() {
                for key in recipe_table.keys() {
                    if !KNOWN_RECIPE_FIELDS.contains(&key.as_str()) {
                        let sugg = suggest_best_match(key, KNOWN_RECIPE_FIELDS);
                        issues.push(ValidationIssue {
                            severity: IssueSeverity::Error,
                            message: format!(
                                "Unknown key '{key}' in [recipes.\"{recipe_name}\"] ({})",
                                file_path.display()
                            ),
                            suggestion: sugg.map(|s| s.to_string()),
                        });
                    }
                }

                if let Some(create_table) = recipe_table.get("create").and_then(|c| c.as_table()) {
                    for key in create_table.keys() {
                        if !KNOWN_CREATE_FIELDS.contains(&key.as_str()) {
                            let sugg = suggest_best_match(key, KNOWN_CREATE_FIELDS);
                            issues.push(ValidationIssue {
                                severity: IssueSeverity::Error,
                                message: format!(
                                    "Unknown key '{key}' in [recipes.\"{recipe_name}\".create] ({})",
                                    file_path.display()
                                ),
                                suggestion: sugg.map(|s| s.to_string()),
                            });
                        }
                    }
                }

                if let Some(vars_table) = recipe_table.get("variables").and_then(|v| v.as_table()) {
                    for (var_name, var_val) in vars_table {
                        if let Some(var_table) = var_val.as_table() {
                            for key in var_table.keys() {
                                if !KNOWN_VARIABLE_FIELDS.contains(&key.as_str()) {
                                    let sugg = suggest_best_match(key, KNOWN_VARIABLE_FIELDS);
                                    issues.push(ValidationIssue {
                                        severity: IssueSeverity::Error,
                                        message: format!(
                                            "Unknown key '{key}' in [recipes.\"{recipe_name}\".variables.\"{var_name}\"] ({})",
                                            file_path.display()
                                        ),
                                        suggestion: sugg.map(|s| s.to_string()),
                                    });
                                }
                            }
                        }
                    }
                }

                if let Some(steps_array) = recipe_table.get("steps").and_then(|s| s.as_array()) {
                    for (idx, step_val) in steps_array.iter().enumerate() {
                        if let Some(step_table) = step_val.as_table() {
                            for key in step_table.keys() {
                                if !KNOWN_STEP_FIELDS.contains(&key.as_str()) {
                                    let sugg = suggest_best_match(key, KNOWN_STEP_FIELDS);
                                    issues.push(ValidationIssue {
                                        severity: IssueSeverity::Error,
                                        message: format!(
                                            "Unknown key '{key}' in [[recipes.\"{recipe_name}\".steps]] item {idx} ({})",
                                            file_path.display()
                                        ),
                                        suggestion: sugg.map(|s| s.to_string()),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Ok(cfg) = toml::from_str::<Config>(&content) {
        for (recipe_name, recipe) in &cfg.recipes {
            let base_dir = config_dir.join("templates");
            let recipe_templates_dir: Option<PathBuf> = recipe.templates_dir.as_deref().map(|td| {
                if td.starts_with("~/") {
                    PathBuf::from(crate::engine::expand_home(td))
                } else if td.starts_with("templates/") {
                    config_dir.join(td)
                } else {
                    base_dir.join(td)
                }
            });

            if let Some(td) = &recipe_templates_dir
                && !td.exists()
            {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Warning,
                    message: format!(
                        "templates_dir '{}' referenced in recipe '{recipe_name}' does not exist on disk",
                        td.display()
                    ),
                    suggestion: None,
                });
            }

            let declared_vars: std::collections::HashSet<String> = recipe
                .variables
                .keys()
                .cloned()
                .collect();

            let builtins = ["name", "variant", "component", "templates_dir", "packs_dir"];

            let mut check_placeholder = |s: &str| {
                let mut start = 0;
                while let Some(open) = s[start..].find("{{") {
                    let idx = start + open + 2;
                    if let Some(close) = s[idx..].find("}}") {
                        let var = s[idx..idx + close].trim();
                        if !var.is_empty()
                            && !builtins.contains(&var)
                            && !declared_vars.contains(var)
                        {
                            issues.push(ValidationIssue {
                                severity: IssueSeverity::Warning,
                                message: format!(
                                    "placeholder '{{{{{var}}}}}' is used in steps/create of recipe '{recipe_name}', but not declared under [variables]"
                                ),
                                suggestion: None,
                            });
                        }
                        start = idx + close + 2;
                    } else {
                        break;
                    }
                }
            };

            for step in &recipe.steps {
                if let Some(cmd) = &step.command {
                    check_placeholder(cmd);
                }
                if let Some(create) = &step.create {
                    check_placeholder(&create.from);
                    check_placeholder(&create.to);
                }
            }
            if let Some(create) = &recipe.create
                && let Some(cmd) = &create.command
            {
                check_placeholder(cmd);
            }

            let global_config_path = config_dir.join("config.toml");
            if let Some(dp) = &recipe.default_pack
                && let Ok(content) = fs::read_to_string(&global_config_path)
                && let Ok(global) = toml::from_str::<crate::config::GlobalConfig>(&content)
                && global.packs.default_behavior != "default"
            {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Notice,
                    message: format!(
                        "recipe '{recipe_name}' defines default_pack = '{dp}', but global default_behavior is '{}' in config.toml",
                        global.packs.default_behavior
                    ),
                    suggestion: Some("default_behavior = \"default\"".to_string()),
                });
            }
        }
    }
}

pub fn recipe_diagnostics(
    config_dir: &Path,
    name: Option<&str>,
) -> anyhow::Result<(Vec<ValidationIssue>, bool)> {
    let mut issues = Vec::new();

    let files_to_check = match name {
        Some(recipe_name) => {
            let path = recipe_edit_path(config_dir, recipe_name)?;
            vec![path]
        }
        None => config_files(config_dir),
    };

    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in config_files(config_dir) {
        if let Ok(cfg) = parse_config_file(&path) {
            for key in cfg.recipes.keys() {
                if let Some(recipe_name) = name
                    && key != recipe_name
                {
                    continue;
                }
                if let Some(prev) = seen.get(key) {
                    if prev != &path {
                        issues.push(ValidationIssue {
                            severity: IssueSeverity::Error,
                            message: format!(
                                "duplicate recipe '{key}' defined in {} and {}",
                                prev.display(),
                                path.display()
                            ),
                            suggestion: None,
                        });
                    }
                } else {
                    seen.insert(key.clone(), path.clone());
                }
            }
        }
    }

    for file_path in files_to_check {
        validate_toml_file(config_dir, &file_path, &mut issues);
    }

    let has_errors = issues.iter().any(|i| i.severity == IssueSeverity::Error);
    Ok((issues, has_errors))
}

#[allow(dead_code)]
pub fn recipe_issues(config_dir: &Path, name: Option<&str>) -> anyhow::Result<Vec<String>> {
    let (issues, _) = recipe_diagnostics(config_dir, name)?;
    Ok(issues.into_iter().map(|i| i.to_string()).collect())
}

/// Opens `path` with `$VISUAL` → `$EDITOR` → `nano`. Without a TTY on stdin
/// it prints a warning and returns `Ok` so non-interactive runs never hang.
pub fn open_editor(path: &Path) -> anyhow::Result<()> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        eprintln!(
            "warning: no interactive terminal; skipped opening editor for {}",
            path.display()
        );
        return Ok(());
    }

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "nano".to_string());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("fa-editor")
        .arg(path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to launch editor '{editor}': {e}"))?;
    if !status.success() {
        anyhow::bail!("editor '{editor}' exited with {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fa-recipe-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("recipes.d")).unwrap();
        dir
    }

    #[test]
    fn recipe_new_should_error_when_name_already_exists_in_any_config_file() {
        println!("\n🔍 [TEST] recipe new — duplicate recipe name rejected across config files");
        let dir = temp_dir("dup");

        fs::write(
            dir.join("recipes.toml"),
            "[recipes.my-stack]\nname = \"My Stack\"\ndescription = \"primary dup\"\n",
        )
        .unwrap();
        let err = recipe_new(&dir, Some("my-stack"), None)
            .expect_err("recipe name present in recipes.toml must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("recipe 'my-stack' ya existe"),
            "error must say recipe already exists, got: '{msg}'"
        );
        assert!(
            !dir.join("recipes.d/my-stack.toml").exists(),
            "must not create the file when the name is taken"
        );

        fs::remove_file(dir.join("recipes.toml")).unwrap();
        fs::write(
            dir.join("recipes.d/base.toml"),
            "[recipes.other-stack]\nname = \"Other\"\ndescription = \"modular dup\"\n",
        )
        .unwrap();
        let err = recipe_new(&dir, Some("other-stack"), None)
            .expect_err("recipe name present in recipes.d/* must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("recipe 'other-stack' ya existe"),
            "error must say recipe already exists, got: '{msg}'"
        );
        assert!(
            !dir.join("recipes.d/other-stack.toml").exists(),
            "must not create the modular file when the name is taken"
        );
        println!("   ✓ Duplicate names rejected in primary and modular files.\n");
    }

    #[test]
    fn recipe_new_should_create_file_with_valid_toml_scaffold() {
        println!("\n🔍 [TEST] recipe new — creates recipes.d/<name>.toml with valid scaffold");
        let dir = temp_dir("create");

        let path = recipe_new(&dir, Some("rust-cli"), None).expect("new recipe file must be created");
        assert_eq!(
            path,
            dir.join("recipes.d/rust-cli.toml"),
            "file must land in recipes.d/<name>.toml"
        );
        let content = fs::read_to_string(&path).expect("scaffold must be readable");
        let cfg: Config = toml::from_str(&content).expect("scaffold must be valid TOML for Config");
        assert!(
            cfg.recipes.contains_key("rust-cli"),
            "scaffold must define [recipes.\"rust-cli\"], keys: {:?}",
            cfg.recipes.keys().collect::<Vec<_>>()
        );
        assert!(
            !content.contains("[aliases."),
            "clean default scaffold must not include alias comments, got:\n{content}"
        );

        let guided_path = recipe_new(&dir, Some("guided"), Some("standard")).expect("guided recipe must be created");
        let guided_content = fs::read_to_string(&guided_path).expect("guided scaffold must be readable");
        assert!(
            guided_content.contains("[aliases.\"guided\"]"),
            "standard scaffold must include the [aliases.\"<name>\"] example comment, got:\n{guided_content}"
        );

        let alias_path = recipe_new(&dir, Some("git-shortcuts"), Some("alias")).expect("alias recipe must be created");
        let alias_content = fs::read_to_string(&alias_path).expect("alias scaffold must be readable");
        let alias_cfg: Config = toml::from_str(&alias_content).expect("alias scaffold must be valid TOML");
        assert!(alias_cfg.aliases.contains_key("git-shortcuts"), "alias config must contain section");
        assert!(alias_cfg.recipes.is_empty(), "alias config must not have recipes");

        let scratch = recipe_new(&dir, None, None).expect("scratch file must be created");
        let fname = scratch.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            fname.starts_with("untitled-") && fname.ends_with(".toml"),
            "scratch must be untitled-<ts>-<pid>.toml, got: '{fname}'"
        );
        let scratch_content = fs::read_to_string(&scratch).expect("scratch must be readable");
        let scratch_cfg: Config =
            toml::from_str(&scratch_content).expect("scratch must be valid TOML");
        assert!(
            scratch_cfg.recipes.is_empty(),
            "scratch must contain only guide comments, keys: {:?}",
            scratch_cfg.recipes.keys().collect::<Vec<_>>()
        );
        println!("   ✓ Named scaffold and scratch untitled file both valid.\n");
    }

    #[test]
    fn recipe_edit_should_error_when_recipe_missing() {
        println!("\n🔍 [TEST] recipe edit — missing recipe errors, never creates");
        let dir = temp_dir("edit-missing");
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.known]\nname = \"Known\"\ndescription = \"exists in primary file\"\n",
        )
        .unwrap();

        let err = recipe_edit_path(&dir, "ghost")
            .expect_err("missing recipe must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("recipe 'ghost' no existe"),
            "error must say recipe does not exist, got: '{msg}'"
        );
        assert!(
            !dir.join("recipes.d/ghost.toml").exists(),
            "edit must never create files"
        );

        let known_path = recipe_edit_path(&dir, "known")
            .expect("recipe defined only in recipes.toml must resolve by key lookup");
        assert_eq!(known_path, dir.join("recipes.toml"));

        let modular = temp_dir("edit-modular");
        fs::write(
            modular.join("recipes.d/custom.toml"),
            "[recipes.custom]\nname = \"Custom\"\ndescription = \"modular\"\n",
        )
        .unwrap();
        let by_key = recipe_edit_path(&modular, "custom")
            .expect("recipe must resolve by key when file matches");
        assert_eq!(by_key, modular.join("recipes.d/custom.toml"));
        let direct = recipe_edit_path(&modular, "custom.toml").ok();
        println!("   ✓ Missing recipe errors; existing recipes resolve by file and key.\n{direct:?}\n");
    }

    #[test]
    fn test_recipe_edit_path_with_syntax_errors_and_aliases() {
        let dir = temp_dir("edit-syntax-err");
        fs::write(
            dir.join("recipes.d/broken.toml"),
            "[[invalid toml syntax {{{",
        )
        .unwrap();

        // 1. Direct modular file name match works even if the TOML is completely malformed
        let broken_path = recipe_edit_path(&dir, "broken").expect("should resolve broken file by name");
        assert_eq!(broken_path, dir.join("recipes.d/broken.toml"));

        // 2. Primary recipes.toml resolves when named "recipes"
        fs::write(dir.join("recipes.toml"), "# primary").unwrap();
        let primary_path = recipe_edit_path(&dir, "recipes").expect("should resolve recipes.toml");
        assert_eq!(primary_path, dir.join("recipes.toml"));

        // 3. Substring match finds file with syntax error that defines target recipe
        let dir2 = temp_dir("edit-broken-content");
        fs::write(
            dir2.join("recipes.d/malformed.toml"),
            "[recipes.my-target]\nbroken_syntax = = =\n",
        )
        .unwrap();
        let target_path = recipe_edit_path(&dir2, "my-target").expect("should find file containing target section even with syntax error");
        assert_eq!(target_path, dir2.join("recipes.d/malformed.toml"));
    }

    #[test]
    fn recipe_edit_without_name_should_list_available_recipes() {
        println!("\n🔍 [TEST] recipe edit (no name) — lists recipe name + file");
        let dir = temp_dir("list");
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.alpha]\nname = \"Alpha\"\ndescription = \"primary\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("recipes.d/beta.toml"),
            "[recipes.beta]\nname = \"Beta\"\ndescription = \"modular\"\n",
        )
        .unwrap();

        let list = recipe_list(&dir);
        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.contains(&"alpha") && names.contains(&"beta"),
            "list must contain both recipes, got: {names:?}"
        );
        for (key, path) in &list {
            assert!(path.is_file(), "path for '{key}' must exist: {}", path.display());
        }
        let alpha = list.iter().find(|(n, _)| n == "alpha").map(|(_, p)| p.clone());
        assert_eq!(alpha, Some(dir.join("recipes.toml")));
        let beta = list.iter().find(|(n, _)| n == "beta").map(|(_, p)| p.clone());
        assert_eq!(beta, Some(dir.join("recipes.d/beta.toml")));
        println!("   ✓ Listed name+file pairs for primary and modular recipes.\n");
    }

    #[test]
    fn recipe_validate_should_detect_duplicate_recipe_names_across_files() {
        println!("\n🔍 [TEST] recipe validate — duplicate keys across files + parse errors");
        let dir = temp_dir("dup-keys");
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.demo]\nname = \"A\"\ndescription = \"primary\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("recipes.d/other.toml"),
            "[recipes.demo]\nname = \"B\"\ndescription = \"modular\"\n",
        )
        .unwrap();

        let issues = recipe_issues(&dir, None).unwrap();
        assert!(
            !issues.is_empty(),
            "duplicate recipe keys must be reported as issues"
        );
        let joined = issues.join("\n");
        assert!(
            joined.contains("demo"),
            "issue must name the duplicated recipe 'demo', got: '{joined}'"
        );

        fs::write(dir.join("recipes.d/broken.toml"), "not valid = [ toml {{").unwrap();
        let issues = recipe_issues(&dir, None).unwrap();
        let joined = issues.join("\n");
        assert!(
            joined.contains("broken.toml") || joined.to_lowercase().contains("parse"),
            "parse error of broken.toml must be reported, got: '{joined}'"
        );
        println!("   ✓ Duplicates and broken TOML surfaced as issues.\n");
    }

    #[test]
    fn recipe_validate_should_report_ok_when_parseable_and_unique() {
        println!("\n🔍 [TEST] recipe validate — clean config yields no issues");
        let dir = temp_dir("ok");
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.alpha]\nname = \"Alpha\"\ndescription = \"a\"\n\n[aliases.demo]\nhi = { command = \"echo hi\" }\n",
        )
        .unwrap();
        fs::write(
            dir.join("recipes.d/beta.toml"),
            "[recipes.beta]\nname = \"Beta\"\ndescription = \"b\"\n",
        )
        .unwrap();

        let issues = recipe_issues(&dir, None).unwrap();
        assert!(
            issues.is_empty(),
            "parseable + unique config must produce no issues, got: {issues:?}"
        );

        // Schema-level problems (variables/steps/templates) are intentionally
        // out of scope for `recipe validate` — they must not surface here.
        fs::write(
            dir.join("recipes.d/schema.toml"),
            "[recipes.typed]\nname = \"Typed\"\ndescription = \"t\"\n\n[recipes.typed.variables]\nport = { prompt = \"Port\", type = \"date\" }\n",
        )
        .unwrap();
        let issues = recipe_issues(&dir, None).unwrap();
        assert!(
            issues.is_empty(),
            "validate must not run schema checks, got: {issues:?}"
        );
        println!("   ✓ Clean config reports OK; schema checks stay out of scope.\n");
    }

    #[test]
    fn recipe_validate_with_name_should_check_only_that_recipe() {
        println!("\n🔍 [TEST] recipe validate <name> — scoped to that recipe only");
        let dir = temp_dir("validate-named");
        fs::write(
            dir.join("recipes.d/astro.toml"),
            "[recipes.astro]\nname = \"Astro\"\ndescription = \"ok\"\n",
        )
        .unwrap();
        // Another file is broken: its parse error must NOT surface when
        // validating only the healthy recipe `astro`.
        fs::write(dir.join("recipes.d/broken.toml"), "not valid = [ toml {{").unwrap();

        let issues = recipe_issues(&dir, Some("astro"))
            .expect("existing recipe must validate without erroring");
        assert!(
            issues.is_empty(),
            "issues of OTHER files must not appear when 'astro' is ok, got: {issues:?}"
        );

        fs::remove_file(dir.join("recipes.d/broken.toml")).unwrap();
        let issues = recipe_issues(&dir, Some("astro"))
            .expect("healthy recipe in a clean config must validate");
        assert!(
            issues.is_empty(),
            "healthy recipe must yield no issues, got: {issues:?}"
        );

        // Duplicate definitions of THAT SAME name must still be reported.
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.astro]\nname = \"Astro\"\ndescription = \"dup\"\n",
        )
        .unwrap();
        let issues = recipe_issues(&dir, Some("astro"))
            .expect("recipe exists, so validation must run (issues reported separately)");
        let joined = issues.join("\n");
        assert!(
            joined.contains("duplicate") && joined.contains("astro"),
            "duplicate of the validated name 'astro' must be an issue, got: '{joined}'"
        );
        println!("   ✓ Scoped validation ignores other files but reports its own dups.\n");
    }

    #[test]
    fn recipe_validate_with_missing_name_should_error() {
        println!("\n🔍 [TEST] recipe validate <name> — missing recipe errors (exit != 0)");
        let dir = temp_dir("validate-missing");
        fs::write(
            dir.join("recipes.toml"),
            "[recipes.known]\nname = \"Known\"\ndescription = \"exists in primary file\"\n",
        )
        .unwrap();

        let err = recipe_issues(&dir, Some("ghost"))
            .expect_err("missing recipe must make validation fail (CLI exit != 0)");
        let msg = err.to_string();
        assert!(
            msg.contains("recipe 'ghost' no existe"),
            "error must contain \"recipe 'ghost' no existe\" (non-zero exit via Err), got: '{msg}'"
        );
        assert!(
            msg.contains("no existe"),
            "error message must contain 'no existe', got: '{msg}'"
        );
        println!("   ✓ Missing name errors with 'no existe' so the CLI exits non-zero.\n");
    }

    #[test]
    fn scaffold_parses_with_config_schema() {
        println!("\n🔍 [TEST] scaffold — parses against the current Config/Recipe schema");
        for name in ["rust-cli", "web-next-ts", "my.app_2"] {
            let content = scaffold_toml(name, None);
            let cfg: Config = toml::from_str(&content)
                .unwrap_or_else(|e| panic!("scaffold for '{name}' must parse as Config: {e}"));
            assert!(
                cfg.recipes.contains_key(name),
                "scaffold must define recipe key '{name}', keys: {:?}",
                cfg.recipes.keys().collect::<Vec<_>>()
            );
        }

        let scratch = scratch_toml();
        let cfg: Config = toml::from_str(&scratch)
            .unwrap_or_else(|e| panic!("scratch scaffold must parse as Config: {e}"));
        assert!(
            cfg.recipes.is_empty(),
            "scratch must be comments-only, keys: {:?}",
            cfg.recipes.keys().collect::<Vec<_>>()
        );
        assert!(
            scratch.contains("recipes"),
            "scratch must contain recipe guide comments, got:\n{scratch}"
        );
        println!("   ✓ Named and scratch scaffolds both parse with the current schema.\n");
    }

    #[test]
    fn test_ensure_schema_file_creates_valid_json_schema() {
        let dir = temp_dir("schema-test");
        let path = crate::schema::ensure_schema_file(&dir).unwrap();
        assert_eq!(path, dir.join("recipe.schema.json"), "Schema file must be stored as recipe.schema.json in state dir");
        assert!(path.is_file(), "Schema file must be created on disk");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"definitions\""), "Schema must contain definitions");
        assert!(content.contains("\"Recipe\""), "Schema must define Recipe");
    }

    #[test]
    fn test_scaffold_toml_includes_schema_header_and_supports_types() {
        let expected_header = "#:schema https://raw.githubusercontent.com/fusoras/fast-alias/main/schema/recipe.schema.json\n";
        let default_clean = scaffold_toml("my-app", None);
        assert!(default_clean.starts_with(expected_header), "Scaffold must start with schema URL header");
        assert!(default_clean.contains("[recipes.\"my-app\"]"));
        assert!(!default_clean.contains("# language ="), "Default scaffold must be clean without commented lines");
        assert!(!default_clean.contains("[aliases."), "Default scaffold must not contain alias comments");

        let standard = scaffold_toml("my-app", Some("standard"));
        assert!(standard.starts_with(expected_header));
        assert!(standard.contains("# language = \"rust\""));
        assert!(standard.contains("[aliases.\"my-app\"]"));

        let pack = scaffold_toml("wc-lib", Some("pack"));
        assert!(pack.starts_with(expected_header), "Pack scaffold must start with schema URL header");
        assert!(pack.contains("templates_dir = \"templates/wc-lib\""));
        assert!(pack.contains("packs_dir = \"packs/wc-lib\""));
        assert!(pack.contains("{{templates_dir}}/{{component}}"));

        let alias = scaffold_toml("git-tools", Some("alias"));
        assert!(alias.starts_with(expected_header), "Alias scaffold must start with schema URL header");
        assert!(alias.contains("[aliases.\"git-tools\"]"), "Alias scaffold must define alias section");
        assert!(alias.contains("example = { command ="), "Alias scaffold must contain basic example alias");
        assert!(!alias.contains("[recipes."), "Alias scaffold must not contain recipe section");

        let scratch = scratch_toml();
        assert!(scratch.starts_with(expected_header), "Scratch scaffold must start with schema URL header");
    }

    #[test]
    fn test_recipe_validate_detects_unknown_keys_with_suggestions() {
        let dir = temp_dir("val-typos");
        fs::write(
            dir.join("recipes.toml"),
            r#"[recipes.my-rec]
name = "My Rec"
decription = "Typo in description"
default = "wc-ui"
"#,
        ).unwrap();
        let (issues, has_errors) = recipe_diagnostics(&dir, None).unwrap();
        assert!(has_errors, "Typos in keys must count as errors");
        let msgs = issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        assert!(msgs.contains("Did you mean 'description'"), "Must suggest 'description' for 'decription'");
        assert!(msgs.contains("Did you mean 'default_pack'"), "Must suggest 'default_pack' for 'default'");
    }

    #[test]
    fn test_recipe_validate_accepts_install_flag_in_steps() {
        let dir = temp_dir("val-install");
        fs::write(
            dir.join("recipes.toml"),
            r#"[recipes.my-rec]
name = "My Rec"
description = "Valid"

[[recipes.my-rec.steps]]
command = "pnpm install"
description = "Install dependencies"
install = true
"#,
        ).unwrap();
        let (issues, has_errors) = recipe_diagnostics(&dir, None).unwrap();
        let msgs = issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        assert!(!has_errors, "install = true in step must be valid, but got errors:\n{msgs}");
        assert!(issues.is_empty(), "Should have no validation issues, but got:\n{msgs}");
    }

    #[test]
    fn test_recipe_validate_warns_on_missing_template_and_orphan_var() {
        let dir = temp_dir("val-warnings");
        fs::write(
            dir.join("recipes.toml"),
            r#"[recipes.my-rec]
name = "My Rec"
description = "Valid"
templates_dir = "templates/nonexistent"

[[recipes.my-rec.steps]]
command = "echo {{orphan_var}}"
description = "Echo"
"#,
        ).unwrap();
        let (issues, has_errors) = recipe_diagnostics(&dir, None).unwrap();
        assert!(!has_errors, "Warnings must not count as errors (has_errors = false)");
        let msgs = issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        assert!(msgs.contains("orphan_var"), "Must warn about orphan variable");
        assert!(msgs.contains("warning:"), "Must be categorized as warning");
    }

    #[test]
    fn test_recipe_rm_deletes_toml_file_but_preserves_templates_and_packs() {
        let dir = temp_dir("rm-success");
        let toml_file = dir.join("recipes.d/my-tool.toml");
        fs::write(&toml_file, "[recipes.my-tool]\nname = \"My Tool\"\ndescription = \"Desc\"\n").unwrap();

        let tpl_file = dir.join("templates/my-tool/template.txt");
        fs::create_dir_all(tpl_file.parent().unwrap()).unwrap();
        fs::write(&tpl_file, "template content").unwrap();

        let pack_file = dir.join("packs/my-tool/default.toml");
        fs::create_dir_all(pack_file.parent().unwrap()).unwrap();
        fs::write(&pack_file, "components = []").unwrap();

        let removed = recipe_rm(&dir, "my-tool").unwrap();
        assert_eq!(removed, toml_file);
        assert!(!toml_file.exists(), "TOML file must be deleted");
        assert!(tpl_file.exists(), "Template files must be preserved");
        assert!(pack_file.exists(), "Pack files must be preserved");
    }

    #[test]
    fn test_recipe_rm_errors_on_nonexistent_recipe() {
        let dir = temp_dir("rm-missing");
        let err = recipe_rm(&dir, "ghost").unwrap_err();
        assert!(err.to_string().contains("no existe"), "Must error with 'no existe'");
    }

    #[test]
    fn test_recipe_rm_refuses_to_delete_primary_recipes_toml() {
        let dir = temp_dir("rm-primary");
        fs::write(dir.join("recipes.toml"), "[recipes.core]\nname = \"Core\"\ndescription = \"Desc\"\n").unwrap();
        let err = recipe_rm(&dir, "core").unwrap_err();
        assert!(err.to_string().contains("primary recipes.toml"), "Must refuse to delete primary recipes.toml");
    }

    #[test]
    fn test_suggest_closest_finds_typos() {
        assert_eq!(
            suggest_closest("stts", &["status", "free", "gco"]),
            Some("status".to_string()),
            "stts must suggest status"
        );
        assert_eq!(
            suggest_closest("skill", &["skills", "docker"]),
            Some("skills".to_string()),
            "skill must suggest skills"
        );
        assert_eq!(
            suggest_closest("nxt-ts", &["next-ts", "rust-cli"]),
            Some("next-ts".to_string()),
            "nxt-ts must suggest next-ts"
        );
        assert_eq!(
            suggest_closest("lss", &["ls", "add"]),
            Some("ls".to_string()),
            "lss must suggest ls"
        );
        assert_eq!(
            suggest_closest("completelydifferent", &["status", "ls"]),
            None,
            "No suggestion for completely unrelated string"
        );
    }
}
