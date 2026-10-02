//! Template management: copy files and folders into recipe templates.

use std::fs;
use std::path::{Path, PathBuf};

use crate::colors::*;
use crate::config::Config;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct AddSummary {
    pub added: usize,
    pub overwritten: usize,
    pub skipped: usize,
}

/// Resolves the destination directory under `~/.config/fa/templates/` for a recipe.
pub fn resolve_template_dir(
    config: &Config,
    recipe_name: &str,
    templates_base: &Path,
) -> anyhow::Result<PathBuf> {
    let canonical_key = config
        .resolve_recipe_key(recipe_name)
        .ok_or_else(|| anyhow::anyhow!("Recipe '{recipe_name}' not found in configuration"))?;

    let recipe = &config.recipes[canonical_key];

    if let Some(base) = &recipe.template_base {
        let clean = base.strip_prefix("templates/").unwrap_or(base);
        return Ok(templates_base.join(clean));
    }

    if let Some(td) = &recipe.templates_dir {
        if let Some(rest) = td.strip_prefix("~/") {
            let home = crate::config::dirs_home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;
            return Ok(home.join(rest));
        }
        let clean = td.strip_prefix("templates/").unwrap_or(td);
        return Ok(templates_base.join(clean));
    }

    Ok(templates_base.join(canonical_key))
}

fn copy_single_file(
    src: &Path,
    dest: &Path,
    force: bool,
    is_interactive: bool,
    confirm: &mut dyn FnMut(&Path) -> bool,
    summary: &mut AddSummary,
) -> anyhow::Result<()> {
    if dest.exists() {
        if force {
            fs::copy(src, dest)?;
            summary.overwritten += 1;
            println!("  {BOLD_GREEN}✓{RESET} Overwrote: {}", dest.display());
        } else if is_interactive {
            if confirm(dest) {
                fs::copy(src, dest)?;
                summary.overwritten += 1;
                println!("  {BOLD_GREEN}✓{RESET} Overwrote: {}", dest.display());
            } else {
                summary.skipped += 1;
                println!("  {DIM}−{RESET} Skipped: {}", dest.display());
            }
        } else {
            summary.skipped += 1;
            println!("  {DIM}−{RESET} Skipped (already exists): {}", dest.display());
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
        summary.added += 1;
        println!("  {BOLD_GREEN}✓{RESET} Added: {}", dest.display());
    }
    Ok(())
}

fn copy_directory_recursive_with_confirm(
    src_dir: &Path,
    dest_dir: &Path,
    force: bool,
    is_interactive: bool,
    confirm: &mut dyn FnMut(&Path) -> bool,
    summary: &mut AddSummary,
) -> anyhow::Result<()> {
    fs::create_dir_all(dest_dir)?;

    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        let name = entry.file_name();
        let target_path = dest_dir.join(&name);

        if entry_path.is_dir() {
            copy_directory_recursive_with_confirm(
                &entry_path,
                &target_path,
                force,
                is_interactive,
                confirm,
                summary,
            )?;
        } else {
            copy_single_file(
                &entry_path,
                &target_path,
                force,
                is_interactive,
                confirm,
                summary,
            )?;
        }
    }

    Ok(())
}

/// Copies files and folders recursively into the recipe's template directory.
pub fn add_to_template(
    dest_dir: &Path,
    paths: &[PathBuf],
    force: bool,
    is_interactive: bool,
    confirm: &mut dyn FnMut(&Path) -> bool,
) -> anyhow::Result<AddSummary> {
    if paths.is_empty() {
        anyhow::bail!("No source paths provided");
    }

    // Verify all source paths exist first before modifying anything
    for p in paths {
        if !p.exists() {
            anyhow::bail!("Source path '{}' does not exist", p.display());
        }
    }

    fs::create_dir_all(dest_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create template directory {}: {e}", dest_dir.display()))?;

    let mut summary = AddSummary::default();

    for p in paths {
        let file_name = p
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid path '{}'", p.display()))?;

        if p.is_file() {
            let target_file = dest_dir.join(file_name);
            copy_single_file(p, &target_file, force, is_interactive, confirm, &mut summary)?;
        } else if p.is_dir() {
            let target_sub = dest_dir.join(file_name);
            copy_directory_recursive_with_confirm(
                p,
                &target_sub,
                force,
                is_interactive,
                confirm,
                &mut summary,
            )?;
        }
    }

    Ok(summary)
}

/// CLI entrypoint for `fa template add <recipe> <paths...> [-f|--force]`.
pub fn run_template_add(
    recipe_name: &str,
    paths: &[PathBuf],
    force: bool,
) -> anyhow::Result<()> {
    let (config, _) = Config::load()?;
    let templates_base = Config::get_user_templates_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine user templates directory"))?;

    let dest_dir = resolve_template_dir(&config, recipe_name, &templates_base)?;
    let is_interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());

    println!("{BOLD_CYAN}[Template]{RESET} Adding to recipe '{recipe_name}' ({})", dest_dir.display());

    let mut confirm_fn = |target: &Path| -> bool {
        crate::prompt_yes_no(
            &format!("File '{}' already exists. Overwrite?", target.display()),
            false,
        )
    };

    let summary = add_to_template(
        &dest_dir,
        paths,
        force,
        is_interactive,
        &mut confirm_fn,
    )?;

    println!(
        "\n{BOLD_GREEN}✓{RESET} Finished adding to template '{}' ({} added, {} overwritten, {} skipped).",
        recipe_name, summary.added, summary.overwritten, summary.skipped
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Recipe;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(1);

    fn temp_dir(label: &str) -> PathBuf {
        let count = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("fa-tpl-{label}-{}-{}", std::process::id(), count));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn dummy_config(recipes: Vec<(&str, Recipe)>) -> Config {
        let mut map = BTreeMap::new();
        for (name, r) in recipes {
            map.insert(name.to_string(), r);
        }
        Config {
            recipes: map,
            ..Default::default()
        }
    }

    fn dummy_recipe(template_base: Option<&str>, templates_dir: Option<&str>) -> Recipe {
        Recipe {
            name: "Test".to_string(),
            description: "desc".to_string(),
            language: None,
            aliases: vec![],
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            template_base: template_base.map(|s| s.to_string()),
            pin_versions: None,
            files: BTreeMap::new(),
            variables: BTreeMap::new(),
            steps: vec![],
            final_message: None,
            packs_dir: None,
            templates_dir: templates_dir.map(|s| s.to_string()),
            default_pack: None,
            packs: BTreeMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_template_dir_defaults_to_recipe_name() {
        let base = temp_dir("res-def");
        let cfg = dummy_config(vec![("my-stack", dummy_recipe(None, None))]);

        let dest = resolve_template_dir(&cfg, "my-stack", &base).unwrap();
        assert_eq!(dest, base.join("my-stack"));
    }

    #[test]
    fn test_resolve_template_dir_honors_template_base() {
        let base = temp_dir("res-base");
        let cfg = dummy_config(vec![("rust-cli", dummy_recipe(Some("rust-stack"), None))]);

        let dest = resolve_template_dir(&cfg, "rust-cli", &base).unwrap();
        assert_eq!(dest, base.join("rust-stack"));
    }

    #[test]
    fn test_resolve_template_dir_honors_templates_dir() {
        let base = temp_dir("res-td");
        let cfg = dummy_config(vec![("wc-lib", dummy_recipe(None, Some("templates/wc-lib")))]);

        let dest = resolve_template_dir(&cfg, "wc-lib", &base).unwrap();
        assert_eq!(dest, base.join("wc-lib"));
    }

    #[test]
    fn test_resolve_template_dir_errors_on_unknown_recipe() {
        let base = temp_dir("res-err");
        let cfg = dummy_config(vec![]);

        let err = resolve_template_dir(&cfg, "nonexistent", &base);
        assert!(err.is_err(), "Must error on unknown recipe");
    }

    #[test]
    fn test_template_add_copies_single_file() {
        let temp = temp_dir("add-file");
        let src_file = temp.join("hello.txt");
        fs::write(&src_file, "hello content").unwrap();

        let dest_dir = temp.join("template_dest");
        let summary = add_to_template(
            &dest_dir,
            &[src_file],
            false,
            true,
            &mut |_| panic!("Should not prompt for new file"),
        )
        .unwrap();

        assert_eq!(summary.added, 1);
        assert_eq!(summary.overwritten, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(
            fs::read_to_string(dest_dir.join("hello.txt")).unwrap(),
            "hello content"
        );
    }

    #[test]
    fn test_template_add_copies_directory_recursively() {
        let temp = temp_dir("add-dir");
        let src_dir = temp.join("my-comp");
        fs::create_dir_all(src_dir.join("sub")).unwrap();
        fs::write(src_dir.join("comp.astro"), "astro content").unwrap();
        fs::write(src_dir.join("sub/style.css"), "css content").unwrap();

        let dest_dir = temp.join("dest");
        let summary = add_to_template(
            &dest_dir,
            &[src_dir],
            false,
            true,
            &mut |_| panic!("Should not prompt for new directory"),
        )
        .unwrap();

        assert_eq!(summary.added, 2);
        assert_eq!(
            fs::read_to_string(dest_dir.join("my-comp/comp.astro")).unwrap(),
            "astro content"
        );
        assert_eq!(
            fs::read_to_string(dest_dir.join("my-comp/sub/style.css")).unwrap(),
            "css content"
        );
    }

    #[test]
    fn test_template_add_prompts_file_by_file_on_overwrite() {
        let temp = temp_dir("add-prompt");
        let dest_dir = temp.join("dest");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("f1.txt"), "old 1").unwrap();
        fs::write(dest_dir.join("f2.txt"), "old 2").unwrap();

        let s1 = temp.join("f1.txt");
        let s2 = temp.join("f2.txt");
        fs::write(&s1, "new 1").unwrap();
        fs::write(&s2, "new 2").unwrap();

        let mut prompted = Vec::new();
        let summary = add_to_template(
            &dest_dir,
            &[s1, s2],
            false,
            true,
            &mut |path| {
                prompted.push(path.file_name().unwrap().to_string_lossy().to_string());
                // Confirm f1, reject f2
                path.file_name().unwrap() == "f1.txt"
            },
        )
        .unwrap();

        assert_eq!(prompted, vec!["f1.txt", "f2.txt"]);
        assert_eq!(summary.added, 0);
        assert_eq!(summary.overwritten, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(fs::read_to_string(dest_dir.join("f1.txt")).unwrap(), "new 1");
        assert_eq!(fs::read_to_string(dest_dir.join("f2.txt")).unwrap(), "old 2");
    }

    #[test]
    fn test_template_add_force_overwrites_without_prompting() {
        let temp = temp_dir("add-force");
        let dest_dir = temp.join("dest");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("file.txt"), "old content").unwrap();

        let s = temp.join("file.txt");
        fs::write(&s, "new content").unwrap();

        let summary = add_to_template(
            &dest_dir,
            &[s],
            true, // force = true
            true,
            &mut |_| panic!("Must not prompt when force is true"),
        )
        .unwrap();

        assert_eq!(summary.overwritten, 1);
        assert_eq!(
            fs::read_to_string(dest_dir.join("file.txt")).unwrap(),
            "new content"
        );
    }

    #[test]
    fn test_template_add_non_interactive_skips_without_hanging() {
        let temp = temp_dir("add-non-int");
        let dest_dir = temp.join("dest");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("f.txt"), "original").unwrap();

        let s = temp.join("f.txt");
        fs::write(&s, "incoming").unwrap();

        let summary = add_to_template(
            &dest_dir,
            &[s],
            false,
            false, // non-interactive
            &mut |_| panic!("Must never prompt in non-interactive mode"),
        )
        .unwrap();

        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.overwritten, 0);
        assert_eq!(fs::read_to_string(dest_dir.join("f.txt")).unwrap(), "original");
    }

    #[test]
    fn test_template_add_errors_on_nonexistent_source() {
        let temp = temp_dir("add-err");
        let dest_dir = temp.join("dest");
        let non_existent = temp.join("ghost.txt");

        let res = add_to_template(&dest_dir, &[non_existent], false, false, &mut |_| true);
        assert!(res.is_err(), "Must return error for non-existent source file");
    }
}
