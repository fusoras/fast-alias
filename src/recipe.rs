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

/// Builds the guided TOML scaffold written by `fa recipe new <name>`:
/// a valid recipe entry plus commented examples for aliases and steps.
pub fn scaffold_toml(name: &str) -> String {
    format!(
        r##"# fa recipe: {name}
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
        name = name
    )
}

/// Comment-only scratch scaffold for `fa recipe new` (no name). fa never
/// renames this file — the user promotes it manually to `<recipe-name>.toml`.
pub fn scratch_toml() -> String {
    r##"# fa scratch recipe (untitled)
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
# hello = { command = "echo hi", description = "Example alias" }
#
# Optional step:
# [[recipes."<recipe-name>".steps]]
# command = "cargo --version"
# description = "Check toolchain"
"##
    .to_string()
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
pub fn recipe_new(config_dir: &Path, name: Option<&str>) -> anyhow::Result<PathBuf> {
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
            fs::write(&file, scaffold_toml(name))
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
    let mut found: Option<PathBuf> = None;
    for path in config_files(config_dir) {
        if let Ok(cfg) = parse_config_file(&path) && cfg.recipes.contains_key(name) {
            found = Some(path);
        }
    }
    found.ok_or_else(|| anyhow::anyhow!("recipe '{name}' no existe"))
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

/// Per-file TOML parse errors plus duplicate recipe keys across ALL config
/// files (the no-name `fa recipe validate` path).
fn all_recipe_issues(config_dir: &Path) -> Vec<String> {
    let mut issues = Vec::new();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    for path in config_files(config_dir) {
        match parse_config_file(&path) {
            Ok(cfg) => {
                for key in cfg.recipes.keys() {
                    if let Some(prev) = seen.get(key) {
                        if prev != &path {
                            issues.push(format!(
                                "duplicate recipe '{key}' defined in {} and {}",
                                prev.display(),
                                path.display()
                            ));
                        }
                    } else {
                        seen.insert(key.clone(), path.clone());
                    }
                }
            }
            Err(e) => issues.push(e.to_string()),
        }
    }
    issues
}

/// Validation issues for `fa recipe validate` (empty = ok): per-file TOML
/// parse errors plus duplicate recipe keys across files. Schema checks
/// (steps/templates/variables) are intentionally out of scope.
///
/// When `name` is `Some`, only the recipe with that key is validated: the
/// file that contains it (resolved like `recipe_edit_path`) is parsed and
/// duplicate definitions of that same key are reported; a missing name
/// yields `recipe '<name>' no existe`.
pub fn recipe_issues(config_dir: &Path, name: Option<&str>) -> anyhow::Result<Vec<String>> {
    let Some(name) = name else {
        return Ok(all_recipe_issues(config_dir));
    };

    let path = recipe_edit_path(config_dir, name)?;
    let mut issues = Vec::new();
    match parse_config_file(&path) {
        Ok(_) => {
            let mut defined_in: Vec<PathBuf> = Vec::new();
            for p in config_files(config_dir) {
                if let Ok(cfg) = parse_config_file(&p) && cfg.recipes.contains_key(name) {
                    defined_in.push(p);
                }
            }
            if let Some(first) = defined_in.first() {
                for other in &defined_in[1..] {
                    issues.push(format!(
                        "duplicate recipe '{name}' defined in {} and {}",
                        first.display(),
                        other.display()
                    ));
                }
            }
        }
        Err(e) => issues.push(e.to_string()),
    }
    Ok(issues)
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
        let err = recipe_new(&dir, Some("my-stack"))
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
        let err = recipe_new(&dir, Some("other-stack"))
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

        let path = recipe_new(&dir, Some("rust-cli")).expect("new recipe file must be created");
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
            content.contains("[aliases.\"rust-cli\"]"),
            "scaffold must include the [aliases.\"<name>\"] example comment, got:\n{content}"
        );

        let scratch = recipe_new(&dir, None).expect("scratch file must be created");
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
            let content = scaffold_toml(name);
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
}
