use crate::config::{Config, Recipe};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::colors::*;
use crate::platform::Platform;
use crate::templating::{resolve_all, substitute};

#[derive(Debug, Clone)]
pub struct NewOptions {
    pub recipe_key: String,
    pub project_name: String,
    pub variant: Option<String>,
    pub dry_run: bool,
    pub no_install: bool,
}

/// Expands a tilde-prefixed path (~/...) to an absolute user path.
pub fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().to_string();
        }
    path.to_string()
}

/// Resolves embedded template content for a recipe file. Returns None if the
/// template does not exist in the embedded catalog.
pub fn embedded_template(recipe: &str, file: &str) -> Option<&'static str> {
    let key = format!("{recipe}/{file}");
    match key.as_str() {
        "astro/.prettierrc" => Some(include_str!("../templates/astro/.prettierrc")),
        "astro/.prettierignore" => Some(include_str!("../templates/astro/.prettierignore")),
        "astro/.stylelintrc.json" => Some(include_str!("../templates/astro/.stylelintrc.json")),
        "astro/.gitignore" => Some(include_str!("../templates/astro/.gitignore")),
        "astro/tsconfig.json" => Some(include_str!("../templates/astro/tsconfig.json")),
        "astro/pnpm-workspace.yaml" => Some(include_str!("../templates/astro/pnpm-workspace.yaml")),
        "astro/astro.config.mjs" => Some(include_str!("../templates/astro/astro.config.mjs")),
        "astro/.oxlintrc.json" => Some(include_str!("../templates/astro/.oxlintrc.json")),
        _ => None,
    }
}

/// Runs the full `fa new` flow: create → files → steps.
pub fn run_new(config: &Config, opts: &NewOptions) -> anyhow::Result<()> {
    let recipe = config
        .recipes
        .get(&opts.recipe_key)
        .ok_or_else(|| anyhow::anyhow!("Recipe '{}' not found", opts.recipe_key))?;

    let variant = opts
        .variant
        .clone()
        .unwrap_or_else(|| Config::default_variant(recipe));

    println!("{BOLD_CYAN}[Recipe]{RESET} {} · {}", recipe.name, recipe.description);
    println!("{DIM}[Variant]{RESET} {variant}\n");

    if opts.dry_run {
        println!("{BOLD_YELLOW}=== DRY-RUN MODE ACTIVE: No changes will be made ==={RESET}");
    }

    // Build variable map: {{name}} and {{variant}} are always available.
    let mut vars: HashMap<String, String> = HashMap::new();
    vars.insert("name".to_string(), opts.project_name.clone());
    vars.insert("variant".to_string(), variant.clone());

    // Collect all template inputs across create command, file paths/content and steps.
    let mut inputs: Vec<String> = Vec::new();
    if let Some(create) = &recipe.create
        && let Some(cmd) = &create.command {
            inputs.push(cmd.clone());
        }
    for (dest, spec) in &recipe.files {
        inputs.push(dest.clone());
        if let Some(tpl) = &spec.template {
            inputs.push(tpl.clone());
        }
    }
    for step in &recipe.steps {
        inputs.push(step.command.clone());
    }

    // Build defaults from recipe `variables` table (prompt: label, default: value).
    let defaults: HashMap<String, String> = recipe
        .variables
        .iter()
        .filter_map(|(k, v)| v.default.clone().map(|d| (k.clone(), d)))
        .collect();
    let prompts: HashMap<String, String> = recipe
        .variables
        .iter()
        .map(|(k, v)| (k.clone(), v.prompt.clone()))
        .collect();

    if !opts.dry_run {
        resolve_all(&inputs, &mut vars, &defaults, |key| {
            let label = prompts.get(key).cloned().unwrap_or_else(|| format!("Value for {key}"));
            let default = defaults.get(key).cloned().unwrap_or_default();
            prompt_input(&label, &default)
        });
    } else {
        // In dry-run, use defaults without prompting.
        for (k, d) in &defaults {
            vars.entry(k.clone()).or_insert_with(|| d.clone());
        }
    }

    // 1. Create base
    if let Some(create) = &recipe.create
        && let Some(command) = &create.command {
            let rendered = substitute(command, &vars);
            if opts.dry_run {
                println!("{DIM}[Dry-Run]{RESET} Would scaffold base via: {rendered}");
            } else {
                println!("Scaffolding base via: {rendered}");
                run_shell(&rendered)?;
            }
        }

    // 2. Write files
    println!("\nWriting configuration files:");
    for (dest, spec) in &recipe.files {
        let dest_path = expand_home(dest);
        let target = Path::new(&dest_path);

        if opts.dry_run {
            println!("  {DIM}[Dry-Run]{RESET} Would write file: {dest}");
            continue;
        }

        if target.exists() && spec.skip_if_exists == Some(true) {
            println!("  {BOLD_YELLOW}[SKIP]{RESET} {dest} (already exists)");
            continue;
        }

        let content = resolve_file_content(recipe, spec, &vars);
        let content = match content {
            Some(c) => c,
            None => {
                anyhow::bail!("No content source for file '{dest}' (missing from/inline/template)")
            }
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create dir {}: {e}", parent.display()))?;
        }
        fs::write(target, content)
            .map_err(|e| anyhow::anyhow!("Failed to write {dest}: {e}"))?;
        println!("  {BOLD_GREEN}✓{RESET} {dest}");
    }

    // 3. Install dependencies (skip if --no-install or no pm install dev_install)
    if !opts.no_install && !opts.dry_run
        && let Some(pm) = &recipe.pm
            && let Some(dev_install) = &pm.dev_install
                && let Some(cmd_template) = dev_install.get(&variant) {
                    println!("\nInstalling dependencies...");
                    run_shell(cmd_template)?;
                }

    // 4. Steps
    if !recipe.steps.is_empty() {
        println!("\nSteps:");
        for step in &recipe.steps {
            if let Some(platform) = &step.platform
                && platform != "all" && !platform_matches(platform)? {
                    continue;
                }
            let rendered = substitute(&step.command, &vars);
            if opts.dry_run {
                println!("  {DIM}[Dry-Run]{RESET} Would run: {rendered}");
            } else {
                println!("  {BOLD_GREEN}✓{RESET} {}", step.description.as_deref().unwrap_or(&step.command));
                run_shell(&rendered)?;
            }
        }
    }

    println!("\n{BOLD_GREEN}Project '{}' created successfully.{RESET}", opts.project_name);
    if !opts.no_install {
        println!("Run `{DIM}cd {}{RESET} && {DIM}{} dev{RESET}` to start developing.", opts.project_name, variant);
    }
    Ok(())
}

/// Resolves the content of a file spec (from / inline / template).
fn resolve_file_content(
    recipe: &Recipe,
    spec: &crate::config::FileSpec,
    vars: &HashMap<String, String>,
) -> Option<String> {
    if let Some(src) = &spec.from {
        let file_name = src.rsplit('/').next()?;
        return embedded_template(&recipe.name.to_lowercase(), file_name).map(|c| c.to_string());
    }
    if let Some(inline) = &spec.inline {
        return Some(inline.clone());
    }
    if let Some(tpl) = &spec.template {
        let file_name = tpl.rsplit('/').next()?;
        if let Some(content) = embedded_template(&recipe.name.to_lowercase(), file_name) {
            return Some(substitute(content, vars));
        }
    }
    None
}

/// Executes a shell command, forwarding stdout/stderr.
fn run_shell(command: &str) -> anyhow::Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute '{command}': {e}"))?;
    if !status.success() {
        anyhow::bail!("Command failed with exit status {status}: {command}");
    }
    Ok(())
}

/// Checks whether the given platform label matches the current platform.
fn platform_matches(label: &str) -> anyhow::Result<bool> {
    let current = Platform::detect();
    Ok(match label {
        "debian" => current == Platform::Debian,
        "termux" => current == Platform::Termux,
        _ => true,
    })
}

/// Formats a single-line list entry for `fa list` / `fa search`.
pub fn format_list_line(recipe_key: &str, recipe: &Recipe) -> String {
    let mut parts = Vec::new();
    parts.push(recipe_key.to_string());
    if !recipe.variants.is_empty() {
        parts.push(format!("({})", recipe.variants.join(" / ")));
    }
    if let Some(lang) = &recipe.language {
        parts.push(lang.clone());
    }
    parts.push("[apply]".to_string());
    parts.join(" · ")
}

/// Returns a human-readable "supported" marker for a recipe on the current platform.
pub fn is_supported(_recipe: &Recipe) -> bool {
    true
}

/// Prompts the user for input via stdin with a label and optional default.
/// Empty input falls back to the default value.
pub fn prompt_input(label: &str, default: &str) -> String {
    use std::io::Write;

    if default.is_empty() {
        print!("{BOLD_CYAN}?{RESET} {label}: ");
    } else {
        print!("{BOLD_CYAN}?{RESET} {label} [{default}]: ");
    }
    let _ = std::io::stdout().flush();

    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    let answer = answer.trim().to_string();

    if answer.is_empty() {
        default.to_string()
    } else {
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_utility_should_expand_tilde_paths() {
        println!("\n🔍 [TEST] Expand Home Utility");
        println!("   Explanation: Verifies path expansion from tilde paths to absolute user paths.");

        let home = std::env::var_os("HOME").map(|h| h.to_string_lossy().to_string()).unwrap();
        let expanded = expand_home("~/projects/app");
        assert!(expanded.starts_with(&home), "Should expand to user home, got: {expanded}");
        println!("   ✓ Tilde expanded to: {expanded}\n");
    }

    #[test]
    fn format_list_line_should_render_concise_single_line() {
        let recipe = Recipe {
            name: "Astro".to_string(),
            description: "test".to_string(),
            language: Some("web · typescript".to_string()),
            aliases: vec![],
            variants: vec!["pnpm".to_string(), "bun".to_string()],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables: Default::default(),
            steps: vec![],
        };
        let line = format_list_line("astro", &recipe);
        assert!(line.contains("astro"));
        assert!(line.contains("pnpm / bun"));
        assert!(line.contains("web · typescript"));
        assert!(line.contains("[apply]"));
    }
}
