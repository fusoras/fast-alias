mod colors;
mod config;
mod engine;
mod platform;
mod state;
mod templating;

use clap::{CommandFactory, Parser, Subcommand};

use crate::colors::*;
use crate::config::Config;
use crate::engine::{format_list_line, is_supported, run_new, NewOptions};
use crate::platform::Platform;
use crate::state::State;

/// Recipe-based project scaffolder CLI for Debian and Termux.
#[derive(Parser)]
#[command(name = "fa", about, long_about = None, disable_version_flag = true)]
struct Cli {
    /// Print version
    #[arg(short = 'V', long)]
    version: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new project from a recipe into a directory.
    #[command(alias = "n")]
    New {
        /// Recipe name or alias (e.g. astro)
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
    /// Detect the device environment (platform, arch, package managers).
    Doctor,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

    let (config, source) = Config::load()?;
    let is_external = source != "Embedded default configuration";
    if is_external {
        println!(
            "{BOLD_YELLOW}[WARNING]{RESET} Loading recipes from external source: {source}. Review recipe [[steps]] commands before running them."
        );
    }

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
            let opts = NewOptions {
                recipe_key: key.clone(),
                project_name: name,
                variant,
                dry_run,
                no_install,
            };
            run_new(&config, &opts)?;

            if !dry_run {
                let default_variant = config
                    .recipes
                    .get(&opts.recipe_key)
                    .map(Config::default_variant)
                    .unwrap_or_default();
                let effective_variant = opts.variant.clone().unwrap_or(default_variant);
                let mut state = State::load();
                state.projects.insert(
                    opts.project_name.clone(),
                    state::ProjectState {
                        recipe: opts.recipe_key.clone(),
                        variant: effective_variant,
                        created_at: timestamp(),
                        path: opts.project_name,
                        installed: !no_install,
                    },
                );
                state.save()?;
            }
        }
        Commands::List { show_hidden } => {
            for (key, recipe) in &config.recipes {
                if !show_hidden && !is_supported(recipe) {
                    continue;
                }
                println!("{}", format_list_line(key, recipe));
            }
        }
        Commands::Search { query } => {
            let q = query.to_lowercase();
            for (key, recipe) in &config.recipes {
                let mut haystack = format!("{key} {}", recipe.name.to_lowercase());
                haystack.push_str(&recipe.aliases.join(" "));
                if let Some(lang) = &recipe.language {
                    haystack.push(' ');
                    haystack.push_str(&lang.to_lowercase());
                }
                haystack.push(' ');
                haystack.push_str(&recipe.variants.join(" "));
                if haystack.contains(&q) {
                    println!("{}", format_list_line(key, recipe));
                }
            }
        }
        Commands::Show { recipe } => {
            let key = config
                .resolve_recipe_key(&recipe)
                .ok_or_else(|| anyhow::anyhow!("Unknown recipe '{recipe}'."))?;
            show_recipe(&config, key);
        }
        Commands::Doctor => {
            doctor();
        }
    }

    Ok(())
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
}
