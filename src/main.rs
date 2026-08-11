mod colors;
mod config;
mod engine;
mod platform;
mod spinner;
mod state;
mod templating;

use clap::{CommandFactory, Parser, Subcommand};

use crate::colors::*;
use crate::config::Config;
use crate::engine::{
    format_command_line, format_list_line, is_supported, run_new, run_shell, NewOptions,
};
use crate::platform::Platform;
use crate::state::State;/// Recipe-based project scaffolder CLI for Debian and Termux.
#[derive(Parser)]
#[command(name = "fa", about, long_about = None, disable_version_flag = true)]
struct Cli {
    /// Print version
    #[arg(short = 'v', long)]
    version: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new project from a recipe into a directory.
    #[command(visible_aliases = ["-n"])]
    New {
        /// Recipe name or alias (e.g. my-recipe)
        recipe: String,
        /// Project directory name
        name: String,
        /// Toolchain variant (e.g. pnpm, bun, npm)
        #[arg(short, long)]
        variant: Option<String>,
        /// Preview actions without making changes
        #[arg(short, long)]
        dry_run: bool,
        /// Skip dependency installation
        #[arg(long)]
        no_install: bool,
    },
    /// List available recipes.
    #[command(visible_aliases = ["-l"])]
    List {
        /// Display unsupported recipes too
        #[arg(short = 's', long)]
        show_hidden: bool,
    },
    /// Search recipes by name, alias, language, or variant.
    Search {
        query: String,
    },
    /// Show full details of a recipe.
    Show {
        recipe: String,
    },
    /// Run an executable command declared in the recipe catalog.
    #[command(visible_aliases = ["run", "-a"])]
    Alias {
        /// Command name or alias (e.g. cloudflare-pages, fpages)
        name: String,
    },
    /// Detect the device environment (platform, arch, package managers).
    #[command(visible_aliases = ["-d"])]
    Doctor,
}

/// Rewrites short-flag aliases into their subcommand form so `fa -n <recipe> <name>`
/// maps to `fa new ...` and `fa -a <command>` maps to `fa alias <command>`.
fn rewrite_short_flags(mut args: Vec<String>) -> Vec<String> {
    if args.len() >= 2 {
        match args[1].as_str() {
            "-n" => args[1] = "new".to_string(),
            "-a" => args[1] = "alias".to_string(),
            _ => {}
        }
    }
    args
}

/// True if a recipe performs scaffolding (create/files/steps), as opposed to
/// only exposing executable commands.
fn is_scaffold_recipe(recipe: &crate::config::Recipe) -> bool {
    recipe.create.is_some() || !recipe.files.is_empty() || !recipe.steps.is_empty()
}

/// Asks the user once (per config path) whether they trust the shell commands
/// defined in their personal config. The answer is persisted in state.toml, so
/// subsequent runs never prompt again for the same path ("one covers all": the
/// primary recipes.toml trust covers every recipes.d/*.toml modular file).
/// Aborts with an error when the user declines.
fn ensure_trusted() -> anyhow::Result<()> {
    let Some(anchor) = Config::trust_anchor() else {
        return Ok(());
    };
    let anchor = anchor.to_string_lossy().to_string();

    let mut state = State::load();
    if state.is_trusted(&anchor) {
        return Ok(());
    }

    println!(
        "{BOLD_YELLOW}[TRUST]{RESET} '{}' defines shell commands (recipes, steps, aliases) that will be executed by `fa`.",
        anchor
    );
    println!("Review the file before trusting it.");
    let answer = prompt_yes_no("Do you trust this configuration file?", false);

    if !answer {
        anyhow::bail!(
            "Aborted: configuration file '{}' was not trusted. Edit it or run the command again after review.",
            anchor
        );
    }

    state.trust(&anchor);
    state.save()?;
    Ok(())
}

/// Prompts a yes/no question via stdin; `default` is used on empty input.
fn prompt_yes_no(question: &str, default: bool) -> bool {
    use std::io::Write;

    let hint = if default { "Y/n" } else { "y/N" };
    print!("{BOLD_CYAN}?{RESET} {question} [{hint}]: ");
    let _ = std::io::stdout().flush();

    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default,
        _ => default,
    }
}

fn main() -> anyhow::Result<()> {
    let args = rewrite_short_flags(std::env::args().collect());
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            if err.kind() == clap::error::ErrorKind::DisplayHelp {
                let help_str = Cli::command().render_help().to_string();
                println!("{}", format_help_with_inline_aliases(&help_str));
                return Ok(());
            }
            if err.kind() == clap::error::ErrorKind::DisplayVersion {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            err.exit();
        }
    };

    if cli.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        let help_str = Cli::command().render_help().to_string();
        println!("{}", format_help_with_inline_aliases(&help_str));
        return Ok(());
    };

    let (config, _source) = Config::load()?;

    match command {
        Commands::New {
            recipe,
            name,
            variant,
            dry_run,
            no_install,
        } => {
            let key = config
                .resolve_recipe_key(&recipe)
                .ok_or_else(|| anyhow::anyhow!("Unknown recipe '{recipe}'. Run `fa list` to see available recipes."))?;
            if !dry_run {
                ensure_trusted()?;
            }
            let opts = NewOptions {
                recipe_key: key.clone(),
                project_name: name,
                variant,
                dry_run,
                no_install,
            };
            let new_result = run_new(&config, &opts);

            if !dry_run && new_result.outcome.register {
                let default_variant = config
                    .recipes
                    .get(&opts.recipe_key)
                    .map(Config::default_variant)
                    .unwrap_or_default();
                let effective_variant = opts.variant.clone().unwrap_or(default_variant);
                let mut state = State::load();
                state.projects.insert(
                    new_result.outcome.project_dir.clone(),
                    state::ProjectState {
                        recipe: opts.recipe_key.clone(),
                        variant: effective_variant,
                        created_at: timestamp(),
                        path: new_result.outcome.project_dir,
                        installed: new_result.outcome.installed,
                    },
                );
                state.save()?;
            }

            new_result.result?;
        }
        Commands::List { show_hidden } => {
            let mut recipes = Vec::new();
            for (key, recipe) in &config.recipes {
                if !show_hidden && !is_supported(recipe) {
                    continue;
                }
                if !is_scaffold_recipe(recipe) {
                    continue;
                }
                recipes.push(format_list_line(key, recipe));
            }
            if !recipes.is_empty() {
                println!("{BOLD_CYAN}Recipes:{RESET}");
                println!("  {DIM}Usage: fa new <recipe> <name>{RESET}");
                println!();
                for line in &recipes {
                    println!("  {line}");
                }
            }
            let commands = config
                .all_commands()
                .into_iter()
                .map(|(_, key, cmd)| format_command_line(key, cmd))
                .collect::<Vec<_>>();
            if !commands.is_empty() {
                println!("\n{BOLD_CYAN}Aliases:{RESET}");
                println!("  {DIM}Usage: fa alias <name>{RESET}");
                println!();
                for line in commands {
                    println!("  {line}");
                }
            }
        }
        Commands::Search { query } => {
            let q = query.to_lowercase();
            let mut recipes = Vec::new();
            for (key, recipe) in &config.recipes {
                if !is_scaffold_recipe(recipe) {
                    continue;
                }
                let mut haystack = format!("{key} {}", recipe.name.to_lowercase());
                haystack.push_str(&recipe.aliases.join(" "));
                if let Some(lang) = &recipe.language {
                    haystack.push(' ');
                    haystack.push_str(&lang.to_lowercase());
                }
                haystack.push(' ');
                haystack.push_str(&recipe.variants.join(" "));
                if haystack.contains(&q) {
                    recipes.push(format_list_line(key, recipe));
                }
            }
            let mut commands = Vec::new();
            for (_, key, cmd) in config.all_commands() {
                let mut haystack = key.to_lowercase();
                haystack.push(' ');
                haystack.push_str(&cmd.aliases.join(" "));
                if let Some(desc) = &cmd.description {
                    haystack.push(' ');
                    haystack.push_str(&desc.to_lowercase());
                }
                if haystack.contains(&q) {
                    commands.push(format_command_line(key, cmd));
                }
            }
            if !recipes.is_empty() {
                println!("{BOLD_CYAN}Recipes:{RESET}");
                println!("  {DIM}Usage: fa new <recipe> <name>{RESET}");
                println!();
                for line in &recipes {
                    println!("  {line}");
                }
            }
            if !commands.is_empty() {
                if !recipes.is_empty() {
                    println!();
                }
                println!("{BOLD_CYAN}Aliases:{RESET}");
                println!("  {DIM}Usage: fa alias <name>{RESET}");
                println!();
                for line in &commands {
                    println!("  {line}");
                }
            }
        }
        Commands::Show { recipe } => {
            if let Some(key) = config.resolve_recipe_key(&recipe) {
                show_recipe(&config, key);
            } else if let Some((category, key, _)) = config.resolve_command(&recipe) {
                show_command(&config, &category, &key);
            } else {
                anyhow::bail!("Unknown recipe or command '{recipe}'. Run `fa list` to see available options.");
            }
        }
        Commands::Alias { name } => {
            let (_category, _key, cmd) = config.resolve_command(&name).ok_or_else(|| {
                anyhow::anyhow!("Unknown command '{name}'. Run `fa list` to see available aliases.")
            })?;
            ensure_trusted()?;
            run_shell(&cmd.command)?;
        }
        Commands::Doctor => {
            doctor();
        }
    }

    Ok(())
}

fn show_command(config: &Config, category: &str, key: &str) {
    let (_, _, cmd) = config
        .all_commands()
        .into_iter()
        .find(|(cat, ck, _)| *cat == category && ck.as_str() == key)
        .expect("resolved command should exist");
    println!("{BOLD_CYAN}Command:{RESET} {key}");
    println!("Category: {category}");
    if let Some(desc) = &cmd.description {
        println!("Description: {desc}");
    }
    if !cmd.aliases.is_empty() {
        println!("Aliases: {}", cmd.aliases.join(", "));
    }
    println!("\nCommand: {}", cmd.command);
}

fn show_recipe(config: &Config, key: &str) {
    let recipe = &config.recipes[key];
    println!("Recipe: {key}");
    println!("Description: {}", recipe.description);
    if let Some(lang) = &recipe.language {
        println!("Language: {lang}");
    }
    if !recipe.aliases.is_empty() {
        println!("Aliases: {}", recipe.aliases.join(", "));
    }
    if !recipe.variants.is_empty() {
        println!("Variants: {}", recipe.variants.join(", "));
    }
    if let Some(create) = &recipe.create
        && let Some(cmd) = &create.command {
            println!("\nCreate: {cmd}");
        }
    if let Some(tooling) = &recipe.tooling {
        println!("\nTooling:");
        if let Some(l) = &tooling.linter {
            println!("  - linter: {} (script: {})", l.tool, l.script.as_deref().unwrap_or("-"));
        }
        if let Some(f) = &tooling.formatter {
            println!("  - formatter: {} (script: {})", f.tool, f.script.as_deref().unwrap_or("-"));
        }
        if let Some(c) = &tooling.check {
            println!("  - check: {} (script: {})", c.tool, c.script.as_deref().unwrap_or("-"));
        }
    }
    if !recipe.files.is_empty() {
        println!("\nFiles:");
        for (dest, spec) in &recipe.files {
            let mode = if spec.from.is_some() {
                "from"
            } else if spec.template.is_some() {
                "template"
            } else {
                "inline"
            };
            println!("  - {dest} ({mode})");
        }
    }
    if !recipe.steps.is_empty() {
        println!("\nSteps:");
        for step in &recipe.steps {
            println!(
                "  - {} ({})",
                step.command,
                step.description.as_deref().unwrap_or("no description")
            );
        }
    }
}

fn doctor() {
    println!("{BOLD_CYAN}=== fa Environment Diagnosis ==={RESET}");
    let platform = Platform::detect();
    println!("Platform: {}", platform.as_label());
    println!("Architecture: {}", platform::detect_arch());
    println!("\nPackage Managers:");
    for pm in ["pnpm", "bun", "npm"] {
        let marker = if platform::command_exists(pm) { "✓" } else { "✗" };
        let color = if platform::command_exists(pm) { BOLD_GREEN } else { BOLD_RED };
        println!("  {color}{marker}{RESET} {pm}");
    }
    println!("\nPrerequisites:");
    for cmd in ["git", "curl", "tar"] {
        let marker = if platform::command_exists(cmd) { "✓" } else { "✗" };
        let color = if platform::command_exists(cmd) { BOLD_GREEN } else { BOLD_RED };
        println!("  {color}{marker}{RESET} {cmd}");
    }
}

/// Returns the current UTC timestamp in ISO 8601 format (second precision).
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let rem = secs % 86400;
    let h = rem / 3600;
    let min = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
}

/// Converts days since 1970-01-01 to a (year, month, day) civil date.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Formats subcommand aliases inline in help text (e.g. `new, -n`, `alias, run, -a`).
fn format_help_with_inline_aliases(input: &str) -> String {
    let mut out = Vec::new();
    let mut in_commands = false;

    for line in input.lines() {
        if line.trim() == "Commands:" {
            in_commands = true;
            out.push(line.to_string());
            continue;
        }
        if in_commands && (line.trim() == "Options:" || line.trim().is_empty()) {
            in_commands = false;
        }

        if in_commands && line.starts_with("  ") {
            let trimmed = line.trim_start();
            let mut parts = trimmed.split_whitespace();
            if let Some(cmd_name) = parts.next() {
                let rest = trimmed[cmd_name.len()..].trim_start();
                let mut aliases = Vec::new();
                let mut clean_rest = rest.to_string();

                if let Some(alias_start) = rest.rfind(" [alias: ") {
                    let alias_str = &rest[alias_start + 9..rest.len() - 1];
                    aliases.push(alias_str.to_string());
                    clean_rest = rest[..alias_start].trim_end().to_string();
                } else if let Some(alias_start) = rest.rfind(" [aliases: ") {
                    let alias_str = &rest[alias_start + 11..rest.len() - 1];
                    aliases.extend(alias_str.split(", ").map(|s| s.to_string()));
                    clean_rest = rest[..alias_start].trim_end().to_string();
                }

                let mut full_name = cmd_name.to_string();
                if !aliases.is_empty() {
                    full_name.push_str(", ");
                    full_name.push_str(&aliases.join(", "));
                }

                out.push(format!("  {full_name:<16}{clean_rest}"));
                continue;
            }
        }

        out.push(line.to_string());
    }

    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_should_be_iso8601_format() {
        let ts = timestamp();
        assert_eq!(ts.len(), 20, "ISO8601 second precision has 20 chars: {ts}");
        assert!(ts.ends_with('Z'), "Timestamp should end with Z: {ts}");
    }

    #[test]
    fn civil_from_days_epoch_should_be_1970_01_01() {
        let (y, m, d) = civil_from_days(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn cli_help_should_include_visible_aliases_for_short_flags() {
        let mut cmd = Cli::command();
        let raw_help = cmd.render_help().to_string();
        let formatted = format_help_with_inline_aliases(&raw_help);
        assert!(
            formatted.contains("new, -n"),
            "Help output must display inline subcommand short aliases (new, -n): got:\n{formatted}"
        );
        assert!(
            formatted.contains("alias, run, -a"),
            "Help output must display inline subcommand aliases (alias, run, -a): got:\n{formatted}"
        );
    }
}
