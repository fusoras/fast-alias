mod colors;
mod config;
mod engine;
mod pinning;
mod platform;
mod recipe;
mod spinner;
mod state;
mod templating;
mod update;
mod template;
mod schema;
mod config_editor;

use std::path::Path;

use clap::{CommandFactory, Parser, Subcommand};

use crate::colors::*;
use crate::config::{Command, Config};
use crate::engine::{
    check_circular_recursion, check_self_recursion, format_alias_groups, format_command_line,
    format_list_line, format_section_header, is_supported, preflight, run_new, run_shell_with_env,
    NewOptions, FA_CALL_STACK_ENV,
};
use crate::platform::Platform;
use crate::state::State;

/// Recipe-based project scaffolder CLI for Debian and Termux.
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
    #[command(name = "--new", visible_alias = "-n")]
    New {
        /// Recipe name or alias (e.g. my-recipe)
        recipe: Option<String>,
        /// Project directory name (or pack/component name for pack recipes)
        name: Option<String>,
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
    /// List available recipes, aliases, or packs.
    #[command(name = "--list", visible_alias = "-l")]
    List {
        /// Display unsupported recipes too
        #[arg(short = 's', long)]
        show_hidden: bool,
        /// Filter by category: recipes (r), aliases (a), packs (p)
        #[arg(value_name = "CATEGORY")]
        filter: Option<String>,
        /// Filter: show only recipes
        #[arg(short = 'r', long = "recipes")]
        recipes: bool,
        /// Filter: show only aliases
        #[arg(short = 'a', long = "aliases")]
        aliases: bool,
        /// Filter: show only packs
        #[arg(short = 'p', long = "packs")]
        packs: bool,
    },
    /// Search recipes by name, alias, language, or variant.
    #[command(name = "--search", visible_alias = "-se")]
    Search {
        query: String,
    },
    /// Show full details of a recipe or command, including its source file.
    #[command(name = "--show", visible_alias = "-sh")]
    Show {
        recipe: String,
    },
    /// Run an executable command declared in the recipe catalog.
    #[command(name = "--alias", visible_alias = "-a")]
    Alias {
        /// Command name or alias (e.g. cloudflare-pages, fpages)
        name: String,
        /// Optional arguments passed through to the alias command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage recipe config files (new/edit/validate).
    #[command(name = "--recipe", visible_alias = "-r")]
    Recipe {
        #[command(subcommand)]
        action: RecipeAction,
    },
    /// Manage recipe templates (add files/folders).
    #[command(name = "--template", visible_alias = "-t")]
    Template {
        #[command(subcommand)]
        action: TemplateAction,
    },
    /// Configure fast-alias settings and aliases interactively.
    #[command(name = "--config", visible_alias = "-co")]
    Config,
    /// Checks GitHub Releases and updates the fa binary in-place.
    #[command(name = "--self-update")]
    SelfUpdate {
        /// Preview the update check without replacing the binary
        #[arg(short, long)]
        dry_run: bool,
    },
    /// Uninstalls the fa executable and state/config directories from the system.
    #[command(name = "--self-uninstall")]
    SelfUninstall {
        /// Automatically confirm removal of configuration and state directories
        #[arg(short = 'y', long)]
        yes: bool,
        /// Explicitly reject/skip removal of configuration and state directories
        #[arg(short = 'n', long = "no")]
        no: bool,
        /// Preview uninstallation actions without deleting files
        #[arg(short = 'd', long = "dry-run")]
        dry_run: bool,
    },
    /// Display subcommands for a namespaced alias group.
    #[command(name = "--namespace-help", hide = true)]
    NamespaceHelp {
        namespace: String,
    },
}

/// Actions under `fa --recipe`: create, edit, or validate recipe config files.
#[derive(Subcommand)]
enum RecipeAction {
    /// Create a new recipe file under recipes.d/ and open it in $EDITOR.
    New {
        /// Recipe key/file name (e.g. rust-cli). Omit for a scratch untitled file.
        name: Option<String>,
        /// Recipe scaffold type (e.g. standard, pack, alias). Defaults to clean minimal scaffold.
        #[arg(value_name = "TYPE")]
        recipe_type: Option<String>,
    },
    /// Open an existing recipe file; omit name to list available recipes.
    Edit {
        name: Option<String>,
    },
    /// Parse every config file and report duplicate recipe keys.
    Validate {
        /// Recipe name to validate only that recipe. Omit to validate all files.
        name: Option<String>,
    },
    /// Remove a recipe TOML file under recipes.d/ (leaves templates and packs intact).
    Rm {
        /// Recipe name to remove
        name: String,
        /// Confirm removal without prompting
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

/// Actions under `fa --template`: add files or folders into recipe templates.
#[derive(Subcommand)]
enum TemplateAction {
    /// Add files or folders into a recipe's template directory.
    Add {
        /// Target recipe name
        recipe: String,
        /// Source files or folders to copy into template
        #[arg(required = true)]
        paths: Vec<std::path::PathBuf>,
        /// Overwrite existing files without confirmation
        #[arg(short, long)]
        force: bool,
    },
}

/// Builtin command names and flags that should not be intercepted as direct alias invocations.
const BUILTIN_COMMANDS: &[&str] = &[
    "--new",
    "-n",
    "--list",
    "-l",
    "--search",
    "-se",
    "--show",
    "-sh",
    "--alias",
    "-a",
    "--recipe",
    "-r",
    "--template",
    "-t",
    "--self-update",
    "--self-uninstall",
    "update-check",
    "help",
    "--help",
    "-h",
    "--version",
    "-v",
    "--namespace-help",
    "--config",
    "-co",
];

/// Rewrites CLI arguments:
/// 1. Rewrites multi-word arguments for `fa -sh <target...>` and `fa --show <target...>`.
/// 2. Expands Git-style sub-command aliases and shell commands declared in `config.toml` `[alias]`.
/// 3. Rewrites namespaced command invocations (`fa <ns> <subcmd>` -> `fa --alias "<ns> <subcmd>"`).
/// 4. Intercepts direct flat alias invocations (`fa <alias_name>` -> `fa --alias <alias_name>`).
fn rewrite_args(mut args: Vec<String>, config: Option<&Config>) -> Vec<String> {
    if args.len() > 3 {
        let first = args[1].as_str();
        if first == "--show" || first == "-sh" {
            let target = args[2..].join(" ");
            return vec![args[0].clone(), first.to_string(), target];
        }
    }

    let Some(cfg) = config else {
        return args;
    };

    let mut depth = 0;
    while args.len() >= 2 && depth < 5 {
        let word = args[1].as_str();
        if let Some(target) = cfg.settings.alias.get(word) {
            let target = target.trim();
            if target.starts_with('!') {
                args.insert(1, "--alias".to_string());
                return args;
            } else {
                let tokens: Vec<String> = target.split_whitespace().map(String::from).collect();
                if tokens.is_empty() {
                    break;
                }
                args.remove(1);
                for (i, token) in tokens.into_iter().enumerate() {
                    args.insert(1 + i, token);
                }
                depth += 1;
            }
        } else {
            break;
        }
    }

    if args.len() >= 4 && (args[1] == "--alias" || args[1] == "-a") {
        let ns_part = args[2].as_str();
        if !ns_part.contains(' ') {
            let ns_key = if ns_part.starts_with(':') {
                ns_part.to_string()
            } else {
                format!(":{ns_part}")
            };
            if let Some(commands) = cfg.aliases.get(&ns_key) {
                let raw_ns = ns_key.strip_prefix(':').unwrap_or(&ns_key).to_string();
                let sub_part = args[3].as_str();
                if !sub_part.starts_with('-')
                    && let Some((sub_key, _)) = crate::config::find_command(commands, sub_part)
                {
                    args[2] = format!("{raw_ns} {sub_key}");
                    args.remove(3);
                    return args;
                }
            }
        }
    }

    if args.len() >= 2 {
        let first = args[1].as_str();
        let ns_key = if first.starts_with(':') {
            first.to_string()
        } else {
            format!(":{first}")
        };

        if let Some(commands) = cfg.aliases.get(&ns_key) {
            let raw_ns = ns_key.strip_prefix(':').unwrap_or(&ns_key).to_string();

            if args.len() >= 3 {
                let second = args[2].as_str();
                if second == "help" || second == "--help" || second == "-h" {
                    args.remove(1); // removes <ns>
                    args.remove(1); // removes help argument
                    args.insert(1, "--namespace-help".to_string());
                    args.insert(2, raw_ns);
                    return args;
                } else if !second.starts_with('-') {
                    if let Some((sub_key, _)) = crate::config::find_command(commands, second) {
                        args.remove(1); // removes <ns>
                        args.remove(1); // removes <subcmd>
                        args.insert(1, "--alias".to_string());
                        args.insert(2, format!("{raw_ns} {sub_key}"));
                        return args;
                    } else if crate::config::find_command(commands, &raw_ns).is_some() {
                        // Not a declared subcommand, but a root command exists for this namespace:
                        // Invoke root command and pass all arguments (including `second`) to it.
                        args.remove(1); // removes <ns>
                        args.insert(1, "--alias".to_string());
                        args.insert(2, format!("{raw_ns} {raw_ns}"));
                        return args;
                    } else {
                        // No root command: keep treating as subcommand so error/typo suggestions work
                        args.remove(1); // removes <ns>
                        let sub = args.remove(1); // removes <subcmd>
                        args.insert(1, "--alias".to_string());
                        args.insert(2, format!("{raw_ns} {sub}"));
                        return args;
                    }
                }
            }

            // Either no subcommand provided (`fa <ns>`) or next argument is a flag (`fa <ns> -v`):
            // Check if a root command exists within the namespace matching the namespace name.
            if crate::config::find_command(commands, &raw_ns).is_some() {
                args.remove(1); // removes <ns>
                args.insert(1, "--alias".to_string());
                args.insert(2, format!("{raw_ns} {raw_ns}"));
                return args;
            } else {
                args.remove(1); // removes <ns>
                while args.len() > 1 && (args[1] == "--help" || args[1] == "-h" || args[1] == "help") {
                    args.remove(1);
                }
                args.insert(1, "--namespace-help".to_string());
                args.insert(2, raw_ns);
                return args;
            }
        }
    }

    if args.len() >= 2 {
        let first = args[1].as_str();
        if !BUILTIN_COMMANDS.contains(&first)
            && !first.starts_with('-')
            && cfg.resolve_command(first).is_some()
        {
            args.insert(1, "--alias".to_string());
        }
    }

    args
}

/// True if a recipe performs scaffolding (create/files/steps), as opposed to
/// only exposing executable commands.
fn is_scaffold_recipe(recipe: &crate::config::Recipe) -> bool {
    recipe.create.is_some() || !recipe.files.is_empty() || !recipe.steps.is_empty()
}

pub(crate) fn recipe_matches_search(
    key: &str,
    recipe: &crate::config::Recipe,
    query: &str,
) -> bool {
    let q = query.to_lowercase();
    let mut haystack = format!("{key} {}", recipe.name.to_lowercase());
    haystack.push(' ');
    haystack.push_str(&recipe.aliases.join(" "));
    if let Some(lang) = &recipe.language {
        haystack.push(' ');
        haystack.push_str(&lang.to_lowercase());
    }
    haystack.push(' ');
    haystack.push_str(&recipe.variants.join(" "));
    haystack.push(' ');
    haystack.push_str(&recipe.description.to_lowercase());
    haystack.contains(&q)
}

pub(crate) fn command_matches_search(
    section: &str,
    key: &str,
    cmd: &crate::config::Command,
    query: &str,
) -> bool {
    let q = query.to_lowercase();
    let sec_clean = section.trim_start_matches(':');
    let display_sec = crate::config::Config::display_section(section);
    let mut haystack = format!("{key} {section} {sec_clean} {display_sec}");
    haystack.push(' ');
    haystack.push_str(&cmd.aliases.join(" "));
    if let Some(desc) = &cmd.description {
        haystack.push(' ');
        haystack.push_str(desc);
    }
    for arg in &cmd.args {
        haystack.push(' ');
        haystack.push_str(&arg.name);
        if let Some(desc) = &arg.description {
            haystack.push(' ');
            haystack.push_str(desc);
        }
    }
    haystack.to_lowercase().contains(&q)
}


/// Interactively prompts the user to select a recipe from a numbered list.
pub(crate) fn pick_recipe_from_list<R: std::io::BufRead, W: std::io::Write>(
    recipes: &[(&str, &crate::config::Recipe)],
    input: &mut R,
    output: &mut W,
) -> anyhow::Result<String> {
    if recipes.is_empty() {
        anyhow::bail!("No scaffold recipes available");
    }

    writeln!(output, "{BOLD_CYAN}Available recipes:{RESET}")?;
    for (i, (key, recipe)) in recipes.iter().enumerate() {
        let desc = if recipe.description.is_empty() {
            ""
        } else {
            &recipe.description
        };
        if desc.is_empty() {
            writeln!(output, "  {WHITE}[{}]{RESET} {key}", i + 1)?;
        } else {
            writeln!(
                output,
                "  {WHITE}[{}]{RESET} {key} {DIM}-{RESET} {desc}",
                i + 1
            )?;
        }
    }
    writeln!(output)?;

    loop {
        write!(
            output,
            "{BOLD_CYAN}?{RESET} Select a recipe [1-{}, q to quit]: ",
            recipes.len()
        )?;
        output.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            anyhow::bail!("No recipe selected (end of input)");
        }
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("q") || trimmed.eq_ignore_ascii_case("quit") {
            anyhow::bail!("Operation cancelled by user");
        }
        if let Ok(choice) = trimmed.parse::<usize>()
            && (1..=recipes.len()).contains(&choice)
        {
            return Ok(recipes[choice - 1].0.to_string());
        }
        writeln!(
            output,
            "{BOLD_YELLOW}Invalid selection '{trimmed}'. Please enter a number between 1 and {}.{RESET}",
            recipes.len()
        )?;
    }
}

/// Prompts the user to enter a project directory name.
pub(crate) fn prompt_project_name<R: std::io::BufRead, W: std::io::Write>(
    input: &mut R,
    output: &mut W,
) -> anyhow::Result<String> {
    loop {
        write!(
            output,
            "{BOLD_CYAN}?{RESET} Project directory name (or 'q' to quit): "
        )?;
        output.flush()?;

        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            anyhow::bail!("No project name entered (end of input)");
        }
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("q") || trimmed.eq_ignore_ascii_case("quit") {
            anyhow::bail!("Operation cancelled by user");
        }
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        writeln!(
            output,
            "{BOLD_YELLOW}Project name cannot be empty.{RESET}"
        )?;
    }
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
pub(crate) fn prompt_yes_no(question: &str, default: bool) -> bool {
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
fn is_validation_cmd(args: &[String]) -> bool {
    if args.len() < 2 {
        return false;
    }
    match args[1].as_str() {
        "-rv" | "rv" => true,
        "--recipe" | "-r" | "recipe" => {
            args.len() >= 3 && matches!(args[2].as_str(), "validate" | "-v" | "v")
        }
        _ => false,
    }
}

fn is_help_or_version_cmd(args: &[String]) -> bool {
    if args.len() < 2 {
        return true;
    }
    args.iter().any(|a| matches!(a.as_str(), "--help" | "-h" | "help" | "--version" | "-v"))
}

fn is_recipe_edit_cmd(args: &[String]) -> bool {
    if args.len() < 2 {
        return false;
    }
    match args[1].as_str() {
        "-re" | "re" => true,
        "--recipe" | "-r" | "recipe" => {
            args.len() >= 3 && matches!(args[2].as_str(), "edit" | "-e" | "e")
        }
        _ => false,
    }
}

#[allow(dead_code)]
fn is_recipe_or_help_cmd(args: &[String]) -> bool {
    if args.len() < 2 {
        return false;
    }
    let first = args[1].as_str();
    matches!(
        first,
        "--recipe"
            | "-r"
            | "recipe"
            | "re"
            | "rv"
            | "rn"
            | "rm"
            | "-re"
            | "-rv"
            | "-rn"
            | "-rm"
            | "--help"
            | "-h"
            | "help"
            | "--version"
            | "-v"
    )
}

fn main() {
    if let Err(err) = run_cli() {
        print_fatal_error(&err);
        std::process::exit(1);
    }
}

/// Runs the CLI. Split out of `main` so failures can be rendered with the
/// project's own error style instead of Rust's raw `Error: ...` output.
fn run_cli() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Hidden internal subprocess: refreshes the cached latest release in
    // state.toml synchronously. Detached via `fa --version` so the CLI returns
    // instantly while the check finishes in the background.
    if args.len() >= 2 && args[1] == "update-check" {
        let version = std::env::var("FA_VERSION").unwrap_or_default();
        update::check_and_cache_latest(&version);
        return Ok(());
    }

    let is_validate = is_validation_cmd(&args);
    let is_help_or_version = is_help_or_version_cmd(&args);
    let is_edit = is_recipe_edit_cmd(&args);

    let (config, errors) = match Config::load_lenient() {
        Ok((cfg, _source, errs)) => (cfg, errs),
        Err(err) if is_validate || is_edit || is_help_or_version => {
            (Config::default(), vec![err.to_string()])
        }
        Err(err) => return Err(err),
    };

    if !is_validate && !is_help_or_version && !is_edit && !errors.is_empty() {
        let count = errors.len();
        let word = if count == 1 { "error" } else { "errors" };
        eprintln!(
            "{BOLD_YELLOW}warning:{RESET} {count} config {word} ignored (see {WHITE}'fa -r validate'{RESET})"
        );
    }

    let args = rewrite_args(args, Some(&config));
    let cli = match Cli::try_parse_from(args.clone()) {
        Ok(cli) => cli,
        Err(err) => {
            if err.kind() == clap::error::ErrorKind::DisplayHelp {
                println!("{}", render_cli_help(&err));
                return Ok(());
            }
            if err.kind() == clap::error::ErrorKind::DisplayVersion {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            if err.kind() == clap::error::ErrorKind::InvalidSubcommand && args.len() >= 2 {
                if (args[1] == "--recipe" || args[1] == "-r" || args[1] == "recipe")
                    && args.len() >= 3
                    && let Some(err_msg) = recipe_as_action_error(&args[2], &config)
                {
                    eprintln!("{err_msg}");
                    std::process::exit(2);
                }
                let unknown = &args[1];
                if !BUILTIN_COMMANDS.contains(&unknown.as_str()) {
                    let hint = suggest_unrecognized_subcommand(unknown, &config)
                        .map(|s| format!("\n\n{BOLD_CYAN}Did you mean?{RESET}\n    {WHITE}{s}{RESET}"))
                        .unwrap_or_default();
                    eprintln!("error: unrecognized subcommand '{unknown}'{hint}\n\nUsage: fa [COMMAND]\n\nFor more information, try '--help'.");
                    std::process::exit(2);
                }
            }
            err.exit();
        }
    };

    if cli.version {
        let current_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
        if let Some(latest) = update::check_version_update(env!("CARGO_PKG_VERSION")) {
            let latest_tag = if latest.starts_with('v') {
                latest
            } else {
                format!("v{latest}")
            };
            println!("{current_tag} -> {BOLD_YELLOW}Update: {latest_tag}{RESET}");
            println!("    Run 'fa --self-update' to update.");
        } else {
            println!("{current_tag}");
        }

        // Refresh state asynchronously without blocking the CLI.
        update::spawn_background_version_check(env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        let help_str = Cli::command().render_help().to_string();
        println!("{}", format_help_with_inline_aliases(&help_str));
        return Ok(());
    };

    match command {
        Commands::New {
            recipe,
            name,
            variant,
            dry_run,
            no_install,
        } => {
            let (recipe_str, name_str) = match recipe {
                Some(r) => (r, name),
                None => {
                    use std::io::IsTerminal;
                    if !std::io::stdin().is_terminal() {
                        anyhow::bail!(
                            "Missing required argument <RECIPE>.\nRun `fa -n <recipe> [name]` or run in an interactive terminal to select from a list."
                        );
                    }
                    let mut scaffold_recipes: Vec<(&str, &crate::config::Recipe)> = config
                        .recipes
                        .iter()
                        .filter(|(_, r)| is_scaffold_recipe(r))
                        .map(|(k, r)| (k.as_str(), r))
                        .collect();
                    scaffold_recipes.sort_by_key(|(k, _)| *k);

                    if scaffold_recipes.is_empty() {
                        anyhow::bail!("No scaffold recipes found in configuration.");
                    }

                    let stdin = std::io::stdin();
                    let stdout = std::io::stdout();
                    let mut stdin_lock = stdin.lock();
                    let mut stdout_lock = stdout.lock();

                    let chosen_recipe = pick_recipe_from_list(
                        &scaffold_recipes,
                        &mut stdin_lock,
                        &mut stdout_lock,
                    )?;

                    let recipe_def = config.recipes.get(chosen_recipe.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("Recipe '{chosen_recipe}' not found in configuration")
                    })?;

                    let chosen_name = if !recipe_def.is_pack_recipe() && name.is_none() {
                        let entered = prompt_project_name(&mut stdin_lock, &mut stdout_lock)?;
                        Some(entered)
                    } else {
                        name
                    };

                    (chosen_recipe, chosen_name)
                }
            };

            let key = config
                .resolve_recipe_key(&recipe_str)
                .ok_or_else(|| anyhow::anyhow!("{}", unknown_recipe_error(&recipe_str, &config)))?;
            if !dry_run {
                ensure_trusted()?;
            }
            let recipe_def = config.recipes.get(key.as_str()).ok_or_else(|| {
                anyhow::anyhow!("Recipe '{key}' not found in configuration")
            })?;

            let (project_name, pack_opt, comp_opt) = if recipe_def.is_pack_recipe() {
                match name_str {
                    None => {
                        let pack_env = std::env::var("FA_PACK").ok();
                        let comp_env = std::env::var("FA_COMPONENT").ok();
                        if pack_env.is_some() || comp_env.is_some() {
                            (".".to_string(), pack_env, comp_env)
                        } else {
                            let behavior = config.settings.packs.default_behavior.as_str();
                            match behavior {
                                "default" => {
                                    if let Some(ref dp) = recipe_def.default_pack {
                                        (".".to_string(), Some(dp.clone()), None)
                                    } else {
                                        anyhow::bail!(
                                            "Recipe '{key}' has no default_pack configured.\nRun `fa --new {key} <pack>` or specify default_pack in the recipe."
                                        );
                                    }
                                }
                                "error" => {
                                    anyhow::bail!(
                                        "No pack or component specified for recipe '{key}'.\nRun `fa --new {key} <pack>` or `fa --new {key} <component>`."
                                    );
                                }
                                _ => {
                                    // "list" (default)
                                    crate::engine::display_recipe_packs_and_components(recipe_def, key.as_str());
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Some(target) => {
                        let packs_dir = recipe_def.packs_dir.as_deref().unwrap_or("packs");
                        if Config::find_pack(Some(recipe_def), packs_dir, &target).is_ok() {
                            (".".to_string(), Some(target), None)
                        } else {
                            let templates_dir = Config::resolve_templates_dir(recipe_def);
                            let comp_dir = if let Some(rest) = templates_dir.strip_prefix("~/") {
                                crate::config::dirs_home_dir().map(|h| h.join(rest).join(&target))
                            } else {
                                Config::get_user_config_dir().map(|u| u.join(&templates_dir).join(&target))
                            };
                            if comp_dir.as_ref().map(|d| d.is_dir()).unwrap_or(false) {
                                (".".to_string(), None, Some(target))
                            } else if recipe_def.create.is_some() {
                                (target, std::env::var("FA_PACK").ok().or_else(|| recipe_def.default_pack.clone()), std::env::var("FA_COMPONENT").ok())
                            } else {
                                anyhow::bail!(
                                    "Pack or component '{target}' not found for recipe '{key}' in packs_dir '{packs_dir}' or templates_dir '{templates_dir}'."
                                );
                            }
                        }
                    }
                }
            } else {
                let Some(proj) = name_str else {
                    anyhow::bail!("Missing required argument <NAME>. Run `fa --new {key} <project-name>`");
                };
                (proj, std::env::var("FA_PACK").ok(), std::env::var("FA_COMPONENT").ok())
            };

            let opts = NewOptions {
                recipe_key: key.clone(),
                project_name,
                variant,
                dry_run,
                no_install,
                pin_versions: recipe_def.pin_versions.unwrap_or(false),
                component: comp_opt,
                pack: pack_opt,
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
        Commands::List {
            show_hidden,
            filter,
            recipes,
            aliases,
            packs,
        } => {
            let filter_lower = filter.as_deref().map(|s| s.to_lowercase());
            let mut show_recipes = recipes;
            let mut show_aliases = aliases;
            let mut show_packs = packs;

            if let Some(f) = &filter_lower {
                match f.as_str() {
                    "recipes" | "recipe" | "r" => show_recipes = true,
                    "aliases" | "alias" | "a" => show_aliases = true,
                    "packs" | "pack" | "p" => show_packs = true,
                    other => {
                        anyhow::bail!(
                            "Unknown filter '{other}'. Expected one of: recipes (r), aliases (a), packs (p)"
                        );
                    }
                }
            }

            // If no specific filter or flag is enabled, default to recipes and aliases
            if !show_recipes && !show_aliases && !show_packs {
                show_recipes = true;
                show_aliases = true;
            }

            if show_recipes {
                let mut recipe_lines = Vec::new();
                for (key, recipe) in &config.recipes {
                    if !show_hidden && !is_supported(recipe) {
                        continue;
                    }
                    if !is_scaffold_recipe(recipe) {
                        continue;
                    }
                    recipe_lines.push(format_list_line(key, recipe));
                }
                if !recipe_lines.is_empty() {
                    println!("{BOLD_CYAN}Recipes:{RESET}");
                    println!("  {DIM}Usage: fa --new <recipe> <name>  (or: fa -n){RESET}");
                    println!();
                    for line in &recipe_lines {
                        println!("  {line}");
                    }
                }
            }

            if show_packs {
                let mut found_any = false;
                for (key, recipe) in &config.recipes {
                    let recipe_packs = Config::list_recipe_packs(recipe);
                    if recipe_packs.is_empty() {
                        continue;
                    }
                    if !found_any {
                        if show_recipes {
                            println!();
                        }
                        println!("{BOLD_CYAN}Packs:{RESET}");
                        found_any = true;
                    }
                    let packs_dir = recipe.packs_dir.as_deref().unwrap_or("packs");
                    println!("  {WHITE}{key}{RESET} {DIM}({packs_dir}):{RESET}");
                    for pack in recipe_packs {
                        if let Some(desc) = &pack.description {
                            println!("    • {BOLD_CYAN}{}{RESET} · {desc}", pack.name);
                        } else {
                            println!("    • {BOLD_CYAN}{}{RESET}", pack.name);
                        }
                        let comps = pack.components.join(", ");
                        println!("      {DIM}components: {comps}{RESET}");
                    }
                }
                if !found_any && !show_recipes && !show_aliases {
                    println!("{BOLD_CYAN}Packs:{RESET}");
                    println!("  {DIM}No packs found in configuration.{RESET}");
                }
            }

            if show_aliases {
                let commands = format_alias_groups(&config);
                if !commands.is_empty() {
                    if show_recipes || show_packs {
                        println!();
                    }
                    println!("{BOLD_CYAN}Aliases:{RESET}");
                    println!("  {DIM}Usage: fa <name>  (or: fa -a <name>){RESET}");
                    println!();
                    for line in commands {
                        println!("{line}");
                    }
                }
            }
        }
        Commands::Search { query } => {
            let mut recipes = Vec::new();
            for (key, recipe) in &config.recipes {
                if !is_scaffold_recipe(recipe) {
                    continue;
                }
                if recipe_matches_search(key, recipe, &query) {
                    recipes.push(format_list_line(key, recipe));
                }
            }
            let mut commands = Vec::new();
            for (section, commands_map) in &config.aliases {
                let mut matched_in_section = Vec::new();
                for (command_key, command) in commands_map {
                    if command_matches_search(section, command_key, command, &query) {
                        matched_in_section.push(format!("    {}", format_command_line(command_key, command)));
                    }
                }
                if !matched_in_section.is_empty() {
                    if !commands.is_empty() {
                        commands.push(String::new());
                    }
                    commands.push(format_section_header(section));
                    commands.extend(matched_in_section);
                }
            }
            if !recipes.is_empty() {
                println!("{BOLD_CYAN}Recipes:{RESET}");
                println!("  {DIM}Usage: fa --new <recipe> <name>  (or: fa -n){RESET}");
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
                println!("  {DIM}Usage: fa <name>  (or: fa -a <name>){RESET}");
                println!();
                for line in &commands {
                    println!("{line}");
                }
            }
        }
        Commands::Show { recipe } => {
            if let Some(key) = config.resolve_recipe_key(&recipe) {
                show_recipe(&config, key);
            } else if let Some((section, key, _)) = config.resolve_command(&recipe) {
                show_command(&config, &section, &key);
            } else {
                let mut candidates: Vec<&str> = config.recipes.keys().map(|k| k.as_str()).collect();
                for (_, k, _) in config.all_commands() {
                    candidates.push(k.as_str());
                }
                let hint = crate::recipe::suggest_closest(&recipe, &candidates)
                    .map(|s| format!("\n\nDid you mean?\n    {s}"))
                    .unwrap_or_default();
                anyhow::bail!("Unknown recipe or command '{recipe}'. Run `fa --list` to see available options.{hint}");
            }
        }
        Commands::Alias { mut name, mut args } => {
            if !name.contains(' ') && !args.is_empty() {
                let candidate = format!("{name} {}", args[0]);
                if config.resolve_command(&candidate).is_some() {
                    name = candidate;
                    args.remove(0);
                }
            }
            let (section, key, cmd) = config.resolve_command(&name).ok_or_else(|| {
                anyhow::anyhow!("{}", unknown_alias_error(&name, &config))
            })?;
            let canonical_name = if section.starts_with(':') {
                format!("{} {}", section.trim_start_matches(':'), key)
            } else {
                key.clone()
            };
            check_self_recursion(&canonical_name, &cmd.command)?;
            if name != canonical_name {
                check_self_recursion(&name, &cmd.command)?;
            }
            let new_stack = check_circular_recursion(&canonical_name)?;

            let alias_envs = crate::config::resolve_alias_env(&config, &section, cmd);
            let mut all_envs: Vec<(&str, &str)> = Vec::new();
            all_envs.push((FA_CALL_STACK_ENV, &new_stack));
            for (k, v) in &alias_envs {
                all_envs.push((k.as_str(), v.as_str()));
            }

            ensure_trusted()?;
            let effective_command = crate::templating::substitute_command_args(&cmd.command, &args);
            preflight(&effective_command)?;
            run_shell_with_env(&effective_command, &all_envs)?;
        }
        Commands::Recipe { action } => {
            let user_dir = Config::get_user_config_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory (HOME not set)"))?;
            match action {
                RecipeAction::New { name, recipe_type } => {
                    let path = recipe::recipe_new(&user_dir, name.as_deref(), recipe_type.as_deref())?;
                    recipe::open_editor(&path)?;
                }
                RecipeAction::Edit { name } => match name {
                    Some(name) => {
                        let path = recipe::recipe_edit_path(&user_dir, &name)?;
                        recipe::open_editor(&path)?;
                    }
                    None => {
                        let list = recipe::recipe_list(&user_dir);
                        if list.is_empty() {
                            println!("No recipes found.");
                        } else {
                            println!("{BOLD_CYAN}Recipes:{RESET}");
                            for (key, path) in list {
                                println!("  {key:<24} {}", path.display());
                            }
                        }
                    }
                },
                RecipeAction::Validate { name } => {
                    let (issues, has_errors) = recipe::recipe_diagnostics(&user_dir, name.as_deref())?;
                    if issues.is_empty() {
                        println!("{}ok: recipe config valid{}", BOLD_GREEN, RESET);
                    } else {
                        for issue in &issues {
                            match issue.severity {
                                recipe::IssueSeverity::Error => eprintln!("{}{}{}", BOLD_RED, issue, RESET),
                                recipe::IssueSeverity::Warning => eprintln!("{}{}{}", BOLD_YELLOW, issue, RESET),
                                recipe::IssueSeverity::Notice => eprintln!("{}{}{}", BOLD_CYAN, issue, RESET),
                            }
                        }
                        if has_errors {
                            anyhow::bail!("recipe validation failed");
                        } else {
                            println!("{}ok: recipe config valid (with warnings){}", BOLD_GREEN, RESET);
                        }
                    }
                }
                RecipeAction::Rm { name, yes } => {
                    use std::io::IsTerminal;
                    let should_delete = if yes || !std::io::stdin().is_terminal() {
                        true
                    } else {
                        prompt_yes_no(&format!("Remove recipe '{name}' (TOML file only)?"), false)
                    };

                    if should_delete {
                        let removed = recipe::recipe_rm(&user_dir, &name)?;
                        println!(
                            "{}Removed recipe file {}{} (templates and packs preserved)",
                            BOLD_GREEN,
                            removed.display(),
                            RESET
                        );
                    } else {
                        println!("Aborted.");
                    }
                }
            }
        }
        Commands::Template { action } => match action {
            TemplateAction::Add { recipe, paths, force } => {
                template::run_template_add(&recipe, &paths, force)?;
            }
        },
        Commands::SelfUpdate { dry_run } => {
            if dry_run {
                println!("{BOLD_YELLOW}=== DRY-RUN MODE ACTIVE: No binary changes will be made ==={RESET}");
            }
            let platform = Platform::detect();
            if let Err(e) = update::check_and_perform_update(env!("CARGO_PKG_VERSION"), &platform, dry_run) {
                eprintln!("\n{BOLD_RED}Self-update error:{RESET} {e}");
                std::process::exit(1);
            }
            println!("\n{BOLD_GREEN}Self-update processing completed successfully.{RESET}");
        }
        Commands::SelfUninstall { yes, no, dry_run } => {
            if dry_run {
                println!("{BOLD_YELLOW}=== DRY-RUN MODE ACTIVE: No files will be deleted ==={RESET}");
            }
            if let Err(e) = update::perform_self_uninstall(dry_run, yes, no) {
                eprintln!("\n{BOLD_RED}Self-uninstall error:{RESET} {e}");
                std::process::exit(1);
            }
        }
        Commands::NamespaceHelp { namespace } => {
            display_namespace_help(&config, &namespace);
        }
        Commands::Config => {
            let user_dir = Config::get_user_config_dir().ok_or_else(|| {
                anyhow::anyhow!("Could not determine user configuration directory")
            })?;
            config_editor::run_interactive_config(&user_dir)?;
        }
    }

    Ok(())
}

/// Renders an `anyhow` error (plus a readable cause chain) on stderr using the
/// project's style: a red bold `✗` prefix and up to two `caused by` levels, so
/// long chains stay short. The caller exits with status 1.
fn print_fatal_error(err: &anyhow::Error) {
    let message = err.to_string();
    let mut lines = message.lines();
    if let Some(first) = lines.next() {
        eprintln!("{BOLD_RED}✗{RESET} {first}");
    }
    for line in lines {
        eprintln!("  {line}");
    }

    const MAX_CAUSES: usize = 2;
    let causes: Vec<String> = err.chain().skip(1).map(|cause| cause.to_string()).collect();
    for cause in causes.iter().take(MAX_CAUSES) {
        eprintln!("  {DIM_GRAY}caused by:{RESET} {cause}");
    }
    if causes.len() > MAX_CAUSES {
        eprintln!("  {DIM_GRAY}… and {} more cause(s){RESET}", causes.len() - MAX_CAUSES);
    }
}

fn display_namespace_help(config: &Config, namespace: &str) {
    let ns_key = if namespace.starts_with(':') {
        namespace.to_string()
    } else {
        format!(":{namespace}")
    };
    let raw_ns = ns_key.strip_prefix(':').unwrap_or(&ns_key);

    let Some(commands) = config.aliases.get(&ns_key) else {
        let ns_candidates: Vec<&str> = config
            .aliases
            .keys()
            .filter(|k| k.starts_with(':'))
            .map(|k| k.strip_prefix(':').unwrap_or(k))
            .collect();
        let hint = crate::recipe::suggest_closest(raw_ns, &ns_candidates)
            .map(|s| format!("\n\n{BOLD_CYAN}Did you mean?{RESET}\n    {WHITE}{s}{RESET}"))
            .unwrap_or_default();
        eprintln!("{BOLD_RED}Unknown namespace '{raw_ns}'.{RESET}{hint}");
        return;
    };

    println!("{BOLD_CYAN}Namespace {WHITE}:{raw_ns}{RESET}:");
    println!("  {DIM}Usage: fa {raw_ns} <command> [args...]{RESET}\n");
    println!("{BOLD_CYAN}Commands:{RESET}");
    for line in format_namespace_commands_aligned(commands) {
        println!("{line}");
    }
    if let Some(env_map) = config.alias_env.get(&ns_key)
        && !env_map.is_empty()
    {
        println!("\n{BOLD_CYAN}Namespace Environment:{RESET}");
        for (k, v) in env_map {
            println!("  {DIM}{k}{RESET} = {v}");
        }
    }
    if let Some(env_force_map) = config.alias_env_force.get(&ns_key)
        && !env_force_map.is_empty()
    {
        println!("\n{BOLD_CYAN}Namespace Environment (forced):{RESET}");
        for (k, v) in env_force_map {
            println!("  {DIM}{k}{RESET} = {v}");
        }
    }
}

pub(crate) fn format_namespace_commands_aligned(
    commands: &std::collections::BTreeMap<String, crate::config::Command>,
) -> Vec<String> {
    let mut entries = Vec::new();
    for (command_key, command) in commands {
        let mut name = command_key.to_string();
        if !command.aliases.is_empty() {
            name.push_str(", ");
            name.push_str(&command.aliases.join(", "));
        }
        let sig_raw = if let Some(sig) = command.argument_signature() {
            format!(" {sig}")
        } else if let Some(sig) = crate::engine::extract_argument_signature(&command.command) {
            format!(" {sig}")
        } else {
            String::new()
        };
        let sig_colored = if let Some(sig) = command.argument_signature() {
            format!(" {WHITE}{sig}{RESET}")
        } else if let Some(sig) = crate::engine::extract_argument_signature(&command.command) {
            format!(" {WHITE}{sig}{RESET}")
        } else {
            String::new()
        };
        let raw_len = name.len() + sig_raw.len();
        let colored_cmd = format!("{BOLD_BLUE}{name}{RESET}{sig_colored}");
        let desc = command.description.as_deref().unwrap_or("").to_string();
        entries.push((raw_len, colored_cmd, desc));
    }

    let max_len = entries.iter().map(|(len, _, _)| *len).max().unwrap_or(0);
    let mut out = Vec::new();
    for (raw_len, colored_cmd, desc) in entries {
        if desc.is_empty() {
            out.push(format!("  {colored_cmd}"));
        } else {
            let padding = " ".repeat(max_len.saturating_sub(raw_len));
            out.push(format!("  {colored_cmd}{padding}   - {DIM_GRAY}{desc}{RESET}"));
        }
    }
    out
}

pub(crate) fn unknown_alias_error(name: &str, config: &Config) -> String {
    if let Some((ns, sub)) = name.split_once(' ') {
        let raw_ns = ns.strip_prefix(':').unwrap_or(ns);
        let ns_key = if ns.starts_with(':') {
            ns.to_string()
        } else {
            format!(":{ns}")
        };
        let sub_candidates: Vec<&str> = config
            .aliases
            .get(&ns_key)
            .map(|cmds| cmds.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        let hint = crate::recipe::suggest_closest(sub, &sub_candidates)
            .map(|s| format!("\n\nDid you mean?\n    {s}"))
            .unwrap_or_default();
        format!("Unknown command '{name}'. Run `fa {raw_ns}` to see available subcommands.{hint}")
    } else {
        let all_aliases: Vec<&str> = config
            .all_commands()
            .into_iter()
            .filter(|(sec, _, _)| !sec.starts_with(':'))
            .map(|(_, k, _)| k.as_str())
            .collect();
        let hint = crate::recipe::suggest_closest(name, &all_aliases)
            .map(|s| format!("\n\nDid you mean?\n    {s}"))
            .unwrap_or_default();
        format!("Unknown command '{name}'. Run `fa --list` to see available aliases.{hint}")
    }
}

pub(crate) fn unknown_recipe_error(recipe: &str, config: &Config) -> String {
    let candidates: Vec<&str> = config.recipes.keys().map(|k| k.as_str()).collect();
    let hint = crate::recipe::suggest_closest(recipe, &candidates)
        .map(|s| format!("\n\nDid you mean?\n    {s}"))
        .unwrap_or_default();
    format!("Unknown recipe '{recipe}'. Run `fa --list` to see available recipes.{hint}")
}

pub(crate) fn suggest_unrecognized_subcommand(unknown: &str, config: &Config) -> Option<String> {
    let mut candidates: Vec<&str> = Vec::new();
    for (sec, cmds) in &config.aliases {
        if sec.starts_with(':') {
            candidates.push(sec.strip_prefix(':').unwrap_or(sec));
        } else {
            for k in cmds.keys() {
                candidates.push(k.as_str());
            }
        }
    }
    for k in config.recipes.keys() {
        candidates.push(k.as_str());
    }
    for &b in BUILTIN_COMMANDS {
        if !b.starts_with('-') {
            candidates.push(b);
        }
    }
    crate::recipe::suggest_closest(unknown, &candidates)
}

pub(crate) fn recipe_as_action_error(recipe_arg: &str, config: &Config) -> Option<String> {
    let key = config.resolve_recipe_key(recipe_arg)?;
    Some(format!(
        "error: '{recipe_arg}' is a recipe, not an action for 'fa --recipe'.\n\n{BOLD_CYAN}Did you mean?{RESET}\n    {WHITE}fa -n {key}{RESET}   (scaffold a new project with this recipe)\n    {WHITE}fa -re {key}{RESET}  (edit the recipe file in $EDITOR)\n    {WHITE}fa -sh {key}{RESET}  (show recipe details)\n\nFor recipe actions, run 'fa --recipe --help'."
    ))
}

pub fn format_source_location(file: Option<&Path>, line: Option<usize>) -> Option<String> {
    let path = file?;
    let path_str = path.to_string_lossy();
    let display_path = if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy();
        if let Some(rel) = path_str.strip_prefix(home_str.as_ref()) {
            format!("~{rel}")
        } else {
            path_str.to_string()
        }
    } else {
        path_str.to_string()
    };

    match line {
        Some(l) => Some(format!("{display_path} (line {l})")),
        None => Some(display_path),
    }
}

pub fn format_command_details(cmd: &Command, section: &str, key: &str) -> String {
    let display_name = if let Some(raw_ns) = section.strip_prefix(':') {
        if key == raw_ns {
            raw_ns.to_string()
        } else {
            format!("{raw_ns} {key}")
        }
    } else {
        key.to_string()
    };
    let mut out = String::new();
    out.push_str(&format!("{BOLD_CYAN}Command:{RESET} {display_name}\n"));
    if let Some(loc) = format_source_location(cmd.source_file.as_deref(), cmd.source_line) {
        out.push_str(&format!("Defined in: {loc}\n"));
    }
    out.push_str(&format!("Section: {section}\n"));
    if let Some(desc) = &cmd.description {
        out.push_str(&format!("Description: {desc}\n"));
    }
    if !cmd.aliases.is_empty() {
        out.push_str(&format!("Aliases: {}\n", cmd.aliases.join(", ")));
    }
    if let Some(sig) = cmd.argument_signature() {
        out.push_str(&format!("Usage: fa {display_name} {sig}\n"));
    }
    if !cmd.args.is_empty() && cmd.args.iter().any(|a| a.description.is_some()) {
        out.push_str("Arguments:\n");
        let max_len = cmd
            .args
            .iter()
            .map(|a| a.name.len() + 2)
            .max()
            .unwrap_or(0);
        for a in &cmd.args {
            let label = if a.required {
                format!("<{}>", a.name)
            } else {
                format!("[{}]", a.name)
            };
            if let Some(desc) = &a.description {
                out.push_str(&format!("  {label:<max_len$}  {desc}\n"));
            } else {
                out.push_str(&format!("  {label}\n"));
            }
        }
    }
    if !cmd.env.is_empty() {
        out.push_str("Environment:\n");
        for (k, v) in &cmd.env {
            out.push_str(&format!("  {k} = {v}\n"));
        }
    }
    if !cmd.env_force.is_empty() {
        out.push_str("Environment (forced):\n");
        for (k, v) in &cmd.env_force {
            out.push_str(&format!("  {k} = {v}\n"));
        }
    }
    out.push_str(&format!("\nCommand: {}\n", cmd.command));
    out
}

fn show_command(config: &Config, section: &str, key: &str) {
    let (_, _, cmd) = config
        .all_commands()
        .into_iter()
        .find(|(sec, ck, _)| *sec == section && ck.as_str() == key)
        .expect("resolved command should exist");
    print!("{}", format_command_details(cmd, section, key));
}

fn show_recipe(config: &Config, key: &str) {
    let recipe = &config.recipes[key];
    print!("{}", format_recipe_details(recipe, key));
}

fn format_recipe_details(recipe: &crate::config::Recipe, key: &str) -> String {
    let mut out = String::new();
    // Styled header, mirroring the cyan `Command:` label used by `--show` for
    // commands/aliases.
    out.push_str(&format!("{BOLD_CYAN}Recipe:{RESET} {key}\n"));
    if let Some(loc) = format_source_location(recipe.source_file.as_deref(), recipe.source_line) {
        out.push_str(&format!("Defined in: {loc}\n"));
    }
    out.push_str(&format!("Description: {}\n", recipe.description));
    if let Some(lang) = &recipe.language {
        out.push_str(&format!("Language: {lang}\n"));
    }
    if !recipe.aliases.is_empty() {
        out.push_str(&format!("Aliases: {}\n", recipe.aliases.join(", ")));
    }
    if !recipe.variants.is_empty() {
        out.push_str(&format!("Variants: {}\n", recipe.variants.join(", ")));
    }
    if let Some(create) = &recipe.create
        && let Some(cmd) = &create.command {
            out.push_str(&format!("\nCreate: {cmd}\n"));
        }
    if let Some(tooling) = &recipe.tooling {
        out.push_str("\nTooling:\n");
        // Align the label column the same way `--show` aligns command
        // arguments with `{label:<max_len$}`.
        let mut entries: Vec<(&str, &crate::config::Tool)> = Vec::new();
        if let Some(l) = &tooling.linter {
            entries.push(("linter:", l));
        }
        if let Some(f) = &tooling.formatter {
            entries.push(("formatter:", f));
        }
        if let Some(c) = &tooling.check {
            entries.push(("check:", c));
        }
        let max_len = entries.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
        for (label, tool) in entries {
            out.push_str(&format!(
                "  - {label:<max_len$} {} (script: {})\n",
                tool.tool,
                tool.script.as_deref().unwrap_or("-")
            ));
        }
    }
    if !recipe.files.is_empty() {
        out.push_str("\nFiles:\n");
        for (dest, spec) in &recipe.files {
            let mode = if spec.from.is_some() {
                "from"
            } else if spec.template.is_some() {
                "template"
            } else {
                "inline"
            };
            out.push_str(&format!("  - {dest} ({mode})\n"));
        }
    }
    if !recipe.steps.is_empty() {
        out.push_str("\nSteps:\n");
        for step in &recipe.steps {
            out.push_str(&format!(
                "  - {} ({})\n",
                step.command.as_deref().unwrap_or("no command"),
                step.description.as_deref().unwrap_or("no description")
            ));
        }
    }
    let final_msg = recipe
        .final_message
        .as_deref()
        .unwrap_or("Run 'cd <project-name>' to go to project");
    out.push_str(&format!("\nFinal Message: {final_msg}\n"));
    out
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

/// Formats subcommand aliases inline in help text (e.g. `new, --new, -n`, `alias, --alias, -a`).
fn format_help_with_inline_aliases(input: &str) -> String {
    let mut out = Vec::new();
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        if line.trim() == "Commands:" {
            out.push(line.to_string());
            let mut cmd_entries = Vec::new();
            while let Some(&next_line) = lines.peek() {
                if next_line.trim() == "Options:" || next_line.trim().is_empty() {
                    break;
                }
                let cur_line = lines.next().unwrap();
                if cur_line.starts_with("  ") {
                    let trimmed = cur_line.trim_start();
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

                        let full_name = if !aliases.is_empty() {
                            format!("{}, {cmd_name}", aliases.join(", "))
                        } else {
                            cmd_name.to_string()
                        };

                        cmd_entries.push((full_name, clean_rest));
                        continue;
                    }
                }
                cmd_entries.push((String::new(), cur_line.to_string()));
            }

            let max_name_len = cmd_entries
                .iter()
                .map(|(name, _)| name.len())
                .max()
                .unwrap_or(16);

            for (name, rest) in cmd_entries {
                if name.is_empty() {
                    out.push(rest);
                } else if rest.is_empty() {
                    out.push(format!("  {name}"));
                } else {
                    out.push(format!("  {name:<width$}   - {rest}", width = max_name_len));
                }
            }
            continue;
        }

        out.push(line.to_string());
    }

    out.join("\n")
}

/// Renders help text for a clap `DisplayHelp` error, preserving subcommand context
/// (e.g. `fa --help recipe`, `fa --recipe --help`) and formatting visible aliases inline.
fn render_cli_help(err: &clap::Error) -> String {
    let help_str = err.render().to_string();
    format_help_with_inline_aliases(&help_str)
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_unprefixed_commands_must_not_be_builtins() {
        let unprefixed = [
            vec!["fa", "new", "my-recipe"],
            vec!["fa", "list"],
            vec!["fa", "search", "react"],
            vec!["fa", "show", "react"],
            vec!["fa", "recipe", "new"],
            vec!["fa", "template", "add", "rec", "file.txt"],
            vec!["fa", "alias", "status"],
            vec!["fa", "which", "react"],
        ];

        for args in unprefixed {
            assert!(
                Cli::try_parse_from(&args).is_err(),
                "Unprefixed command '{:?}' must NOT be a native builtin subcommand",
                args
            );
        }
    }

    use super::*;
    use std::path::PathBuf;

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
    fn cli_short_flags_should_parse_to_commands() {
        let cli = Cli::try_parse_from(["fa", "-n", "recipe", "myapp"]).unwrap();
        match cli.command {
            Some(Commands::New { recipe, name, .. }) => {
                assert_eq!(recipe, Some("recipe".to_string()));
                assert_eq!(name, Some("myapp".to_string()));
            }
            _ => panic!("Expected Commands::New"),
        }

        let cli = Cli::try_parse_from(["fa", "-a", "status"]).unwrap();
        match cli.command {
            Some(Commands::Alias { name, .. }) => {
                assert_eq!(name, "status");
            }
            _ => panic!("Expected Commands::Alias"),
        }
    }

    #[test]
    fn rewrite_args_should_rewrite_direct_alias_invocation() {
        let mut config = Config::default();
        let mut git_cmds = std::collections::BTreeMap::new();
        git_cmds.insert(
            "status".to_string(),
            crate::config::Command {
                command: "git status".to_string(),
                description: Some("Repo status".to_string()),
                platform: None,
                aliases: vec!["st".to_string()],
                ..Default::default()
            },
        );
        config.aliases.insert("git".to_string(), git_cmds);

        // Canonical alias name
        let args = vec!["fa".to_string(), "status".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--alias", "status"]);

        // Short alias
        let args = vec!["fa".to_string(), "st".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--alias", "st"]);

        // Builtin commands must NOT be rewritten
        let args = vec!["fa".to_string(), "--new".to_string(), "recipe".to_string(), "app".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--new", "recipe", "app"]);

        // Custom alias using a previously colliding word like "new" IS rewritten to --alias
        let mut custom_cmds = std::collections::BTreeMap::new();
        custom_cmds.insert(
            "new".to_string(),
            crate::config::Command {
                command: "git switch -c".to_string(),
                description: Some("New branch".to_string()),
                platform: None,
                aliases: vec![],
                ..Default::default()
            },
        );
        config.aliases.insert("custom".to_string(), custom_cmds);
        let args = vec!["fa".to_string(), "new".to_string(), "feature".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--alias", "new", "feature"]);
    }

    #[test]
    fn cli_alias_should_accept_passthrough_arguments() {
        let args = vec![
            "fa".to_string(),
            "--alias".to_string(),
            "avif".to_string(),
            "in.jpg".to_string(),
            "out.avif".to_string(),
        ];
        let cli = Cli::try_parse_from(args).expect("fa --alias should accept passthrough arguments");
        match cli.command {
            Some(Commands::Alias { name, args }) => {
                assert_eq!(name, "avif");
                assert_eq!(args, vec!["in.jpg", "out.avif"]);
            }
            _ => panic!("Expected Commands::Alias with args"),
        }
    }

    #[test]
    fn rewrite_args_should_preserve_arguments_for_direct_alias() {
        let mut config = Config::default();
        let mut wrapper = std::collections::BTreeMap::new();
        wrapper.insert(
            "avif".to_string(),
            crate::config::Command {
                command: "avifenc -s 0 -q 50".to_string(),
                description: Some("Convert to avif".to_string()),
                platform: None,
                aliases: vec![],
                ..Default::default()
            },
        );
        config.aliases.insert("wrapper".to_string(), wrapper);

        let args = vec![
            "fa".to_string(),
            "avif".to_string(),
            "ticket.jpeg".to_string(),
            "ticket.avif".to_string(),
        ];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(
            rewritten,
            vec!["fa", "--alias", "avif", "ticket.jpeg", "ticket.avif"]
        );
    }

    #[test]
    fn test_rewrite_args_git_style_aliases_from_config() {
        let mut config = Config::default();
        config.settings.alias.insert("rn".to_string(), "--recipe new".to_string());
        config.settings.alias.insert("n".to_string(), "--new".to_string());
        config.settings.alias.insert("st".to_string(), "--list".to_string());
        config.settings.alias.insert("ac".to_string(), "!git add -A && git commit -m".to_string());

        // 1. Native multi-token alias: `fa rn my-app` -> `fa --recipe new my-app`
        let args = vec!["fa".to_string(), "rn".to_string(), "my-app".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(
            rewritten,
            vec!["fa", "--recipe", "new", "my-app"],
            "rn must expand to --recipe new"
        );

        // 2. Native single-token alias: `fa n my-recipe my-proj` -> `fa --new my-recipe my-proj`
        let args = vec!["fa".to_string(), "n".to_string(), "my-recipe".to_string(), "my-proj".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(
            rewritten,
            vec!["fa", "--new", "my-recipe", "my-proj"],
            "n must expand to --new"
        );

        // 3. Native alias with no extra args: `fa st` -> `fa --list`
        let args = vec!["fa".to_string(), "st".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--list"]);

        // 4. Git-style shell alias with '!': `fa ac "feat: init"` -> `fa --alias ac "feat: init"`
        let args = vec!["fa".to_string(), "ac".to_string(), "feat: init".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(
            rewritten,
            vec!["fa", "--alias", "ac", "feat: init"],
            "Shell alias prefixed with '!' must be rewritten to --alias <name>"
        );
    }

    #[test]
    fn test_rewrite_args_namespaced_aliases() {
        let toml_content = r#"
[aliases.":skills"]
skills = { command = "tabernaculo status", description = "Root skills command" }
ls = { command = "bunx tabernaculo list", description = "List skills" }
add = { command = "bunx tabernaculo add", description = "Add skill", aliases = ["a"] }

[aliases.":docker"]
up = { command = "docker compose up -d" }
down = { command = "docker compose down" }
"#;
        let config: Config = toml::from_str(toml_content).expect("Should parse namespaced aliases");

        // 1. fa skills ls -> fa --alias "skills ls"
        let args = vec!["fa".to_string(), "skills".to_string(), "ls".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills ls"]
        );

        // 2. fa :skills ls -> fa --alias "skills ls"
        let args = vec!["fa".to_string(), ":skills".to_string(), "ls".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills ls"]
        );

        // 3. fa skills add my-skill -> fa --alias "skills add" my-skill
        let args = vec![
            "fa".to_string(),
            "skills".to_string(),
            "add".to_string(),
            "my-skill".to_string(),
        ];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills add", "my-skill"]
        );

        // 4. fa skills -> fa --alias "skills skills" (root command exists)
        let args = vec!["fa".to_string(), "skills".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills skills"]
        );

        // 4b. fa skills -v -> fa --alias "skills skills" -v
        let args = vec!["fa".to_string(), "skills".to_string(), "-v".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills skills", "-v"]
        );

        // 4c. fa skills list -> fa --alias "skills skills" list (non-subcommand argument passed to root command)
        let args = vec!["fa".to_string(), "skills".to_string(), "list".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills skills", "list"],
            "Non-subcommand argument must be passed to root command if root command exists"
        );

        // 5. fa docker -> fa --namespace-help docker (no root command, displays help)
        let args = vec!["fa".to_string(), "docker".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--namespace-help", "docker"]
        );

        // 5b. fa :docker -> fa --namespace-help docker
        let args = vec!["fa".to_string(), ":docker".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--namespace-help", "docker"]
        );

        // 6. fa -a skills add my-skill -> fa -a "skills add" my-skill
        let args = vec![
            "fa".to_string(),
            "-a".to_string(),
            "skills".to_string(),
            "add".to_string(),
            "my-skill".to_string(),
        ];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "-a", "skills add", "my-skill"],
            "fa -a with namespace and subcommand must merge into compound alias"
        );

        // 7. fa --alias skills add my-skill -> fa --alias "skills add" my-skill
        let args = vec![
            "fa".to_string(),
            "--alias".to_string(),
            "skills".to_string(),
            "add".to_string(),
            "my-skill".to_string(),
        ];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--alias", "skills add", "my-skill"],
            "fa --alias with namespace and subcommand must merge into compound alias"
        );

        // 5c. fa docker --help -> fa --namespace-help docker
        let args = vec!["fa".to_string(), "docker".to_string(), "--help".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--namespace-help", "docker"]
        );

        // 5d. fa docker help -> fa --namespace-help docker
        let args = vec!["fa".to_string(), "docker".to_string(), "help".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "--namespace-help", "docker"]
        );

        // 6. Isolation check: fa ls must NOT be rewritten as an alias since ls is namespaced
        let args = vec!["fa".to_string(), "ls".to_string()];
        assert_eq!(
            rewrite_args(args, Some(&config)),
            vec!["fa", "ls"]
        );
    }

    #[test]
    fn test_namespace_help_command_parsing() {
        let cli = Cli::try_parse_from(["fa", "--namespace-help", "docker"]).expect("Should parse --namespace-help");
        match cli.command {
            Some(Commands::NamespaceHelp { namespace }) => assert_eq!(namespace, "docker"),
            _ => panic!("Expected Commands::NamespaceHelp"),
        }
    }


    #[test]
    fn cli_help_should_include_visible_aliases_for_short_flags() {
        let mut cmd = Cli::command();
        let raw_help = cmd.render_help().to_string();
        let formatted = format_help_with_inline_aliases(&raw_help);
        assert!(
            formatted.contains("-n, --new"),
            "Help output must display inline subcommand short aliases (-n, --new): got:\n{formatted}"
        );
        assert!(
            formatted.contains("-a, --alias"),
            "Help output must display inline subcommand aliases (-a, --alias): got:\n{formatted}"
        );
        assert!(
            !formatted.contains("-lList"),
            "Help output must not fuse alias with description (-lList): got:\n{formatted}"
        );
        assert!(
            !formatted.contains("-seSearch"),
            "Help output must not fuse alias with description (-seSearch): got:\n{formatted}"
        );
    }

    #[test]
    fn test_format_help_with_inline_aliases_renders_aligned_hyphens() {
        let mut cmd = Cli::command();
        let raw_help = cmd.render_help().to_string();
        let formatted = format_help_with_inline_aliases(&raw_help);
        assert!(
            formatted.contains("   - "),
            "Help output Commands: section must render aligned hyphen separator '   - ', got:\n{formatted}"
        );
    }

    #[test]
    fn test_cli_help_self_update_and_uninstall_at_end_of_commands() {
        let mut cmd = Cli::command();
        let raw_help = cmd.render_help().to_string();
        let formatted = format_help_with_inline_aliases(&raw_help);
        let config_pos = formatted.find("-co, --config").expect("should contain -co, --config");
        let update_pos = formatted.find("--self-update").expect("should contain --self-update");
        let uninstall_pos = formatted.find("--self-uninstall").expect("should contain --self-uninstall");
        assert!(
            config_pos < update_pos,
            "Expected -co, --config to appear before --self-update, got:\n{formatted}"
        );
        assert!(
            update_pos < uninstall_pos,
            "Expected --self-update to appear before --self-uninstall, got:\n{formatted}"
        );
    }

    #[test]
    fn test_namespace_commands_aligned_format() {
        use std::collections::BTreeMap;
        let mut commands = BTreeMap::new();
        commands.insert(
            "add-repository".to_string(),
            crate::config::Command {
                command: "echo add".to_string(),
                description: Some("Add entries to apt sources.list".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "autoclean".to_string(),
            crate::config::Command {
                command: "echo clean".to_string(),
                description: Some("Erase cache for packages no longer available".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "autopurge".to_string(),
            crate::config::Command {
                command: "echo purge".to_string(),
                description: Some("Erase system-wide config files left by removed packages".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "autoremove".to_string(),
            crate::config::Command {
                command: "echo remove".to_string(),
                description: Some("Remove dependency packages no longer required".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "build".to_string(),
            crate::config::Command {
                command: "echo build".to_string(),
                description: Some("Build binary or source packages from sources".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "build-dep".to_string(),
            crate::config::Command {
                command: "echo build-dep".to_string(),
                description: Some("Configure build-dependencies for source packages".to_string()),
                ..Default::default()
            },
        );
        commands.insert(
            "changelog".to_string(),
            crate::config::Command {
                command: "echo changelog".to_string(),
                description: Some("View a package's changelog".to_string()),
                ..Default::default()
            },
        );

        let lines = format_namespace_commands_aligned(&commands);
        let joined = lines.join("\n");
        assert!(joined.contains(&format!("  {BOLD_BLUE}add-repository{RESET}   - {DIM_GRAY}Add entries to apt sources.list{RESET}")));
        assert!(joined.contains(&format!("  {BOLD_BLUE}autoclean{RESET}        - {DIM_GRAY}Erase cache for packages no longer available{RESET}")));
        assert!(joined.contains(&format!("  {BOLD_BLUE}build{RESET}            - {DIM_GRAY}Build binary or source packages from sources{RESET}")));
    }

    #[test]
    fn show_recipe_should_include_final_message_or_default_hint() {
        println!("\n🔍 [TEST] Show Recipe — displays final_message and fallback hint");
        let mut recipe = crate::config::Recipe {
            name: "Test Stack".to_string(),
            description: "Test description".to_string(),
            aliases: vec![],
            language: None,
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables: Default::default(),
            steps: vec![],
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
            ..Default::default()
        };

        // When final_message is None, defaults to cd project-name hint
        let default_output = format_recipe_details(&recipe, "test-stack");
        assert!(
            default_output.contains("Final Message: Run 'cd <project-name>' to go to project"),
            "Output must contain default navigation hint when unset, got:\n{default_output}"
        );

        // When final_message is Some(...), displays the custom message
        recipe.final_message = Some("Run `cd {{name}} && cargo test` to start".to_string());
        let custom_output = format_recipe_details(&recipe, "test-stack");
        assert!(
            custom_output.contains("Final Message: Run `cd {{name}} && cargo test` to start"),
            "Output must contain the custom final message, got:\n{custom_output}"
        );
        println!("   ✓ Both default and custom final_message displayed in show_recipe.\n");
    }

    #[test]
    fn cli_help_recipe_should_render_subcommand_help() {
        for args in [
            &["fa", "help", "--recipe"][..],
            &["fa", "--recipe", "--help"][..],
            &["fa", "-r", "--help"][..],
        ] {
            let err = match Cli::try_parse_from(args) {
                Err(e) => e,
                Ok(_) => panic!("expected try_parse_from({args:?}) to return DisplayHelp error"),
            };
            assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
            let output = render_cli_help(&err);
            assert!(
                output.contains("Manage recipe config files (new/edit/validate)"),
                "Help output for `--recipe` ({args:?}) must include recipe description, got:\n{output}"
            );
            assert!(
                output.contains("Usage: fa --recipe <COMMAND>"),
                "Help output for `--recipe` ({args:?}) must show usage for recipe, got:\n{output}"
            );
            assert!(
                output.contains("validate"),
                "Help output for `--recipe` ({args:?}) must list `validate` subcommand, got:\n{output}"
            );
            assert!(
                !output.contains("self-uninstall"),
                "Help output for `--recipe` ({args:?}) must not show top-level commands, got:\n{output}"
            );
        }
    }

    #[test]
    fn cli_help_nested_subcommand_should_render_action_help() {
        let err = match Cli::try_parse_from(["fa", "help", "--recipe", "new"]) {
            Err(e) => e,
            Ok(_) => panic!("expected try_parse_from to return DisplayHelp error"),
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        let output = render_cli_help(&err);
        assert!(
            output.contains("Create a new recipe file under recipes.d/ and open it in $EDITOR"),
            "Help output for `--recipe new` must describe action, got:\n{output}"
        );
        assert!(
            output.contains("Usage: fa --recipe new [NAME]"),
            "Help output for `--recipe new` must show usage, got:\n{output}"
        );
    }

    #[test]
    fn cli_help_top_level_should_retain_inline_aliases() {
        for args in [&["fa", "help"][..], &["fa", "--help"][..]] {
            let err = match Cli::try_parse_from(args) {
                Err(e) => e,
                Ok(_) => panic!("expected try_parse_from({args:?}) to return DisplayHelp error"),
            };
            assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
            let output = render_cli_help(&err);
            assert!(
                output.contains("-n, --new"),
                "Top-level help ({args:?}) must contain inline alias for --new: got:\n{output}"
            );
            assert!(
                output.contains("-a, --alias"),
                "Top-level help ({args:?}) must contain inline alias for --alias: got:\n{output}"
            );
            assert!(
                !output.contains("-lList"),
                "Help output must not fuse alias with description (-lList): got:\n{output}"
            );
            assert!(
                !output.contains("-seSearch"),
                "Help output must not fuse alias with description (-seSearch): got:\n{output}"
            );
        }
    }

    #[test]
    fn cli_new_pack_recipe_should_parse_optional_target() {
        let cli = Cli::try_parse_from(["fa", "--new", "wc-lib"]).expect("fa --new wc-lib should parse without name");
        match cli.command {
            Some(Commands::New { recipe, name, .. }) => {
                assert_eq!(recipe, Some("wc-lib".to_string()));
                assert_eq!(name, None);
            }
            _ => panic!("Expected Commands::New"),
        }

        let cli = Cli::try_parse_from(["fa", "-n", "wc-lib", "wc-toggle-theme"]).expect("fa -n wc-lib wc-toggle-theme should parse");
        match cli.command {
            Some(Commands::New { recipe, name, .. }) => {
                assert_eq!(recipe, Some("wc-lib".to_string()));
                assert_eq!(name, Some("wc-toggle-theme".to_string()));
            }
            _ => panic!("Expected Commands::New"),
        }
    }

    #[test]
    fn cli_new_bare_should_parse_optional_recipe() {
        let cli = Cli::try_parse_from(["fa", "--new"]).expect("fa --new should parse bare");
        match cli.command {
            Some(Commands::New { recipe, name, .. }) => {
                assert_eq!(recipe, None);
                assert_eq!(name, None);
            }
            _ => panic!("Expected Commands::New"),
        }

        let cli = Cli::try_parse_from(["fa", "-n"]).expect("fa -n should parse bare");
        match cli.command {
            Some(Commands::New { recipe, name, .. }) => {
                assert_eq!(recipe, None);
                assert_eq!(name, None);
            }
            _ => panic!("Expected Commands::New"),
        }
    }

    #[test]
    fn cli_template_add_should_parse_recipe_and_paths() {
        let cli = Cli::try_parse_from(["fa", "--template", "add", "my-stack", "file.txt", "dir/"])
            .expect("fa --template add should parse");
        match cli.command {
            Some(Commands::Template {
                action: TemplateAction::Add { recipe, paths, force },
            }) => {
                assert_eq!(recipe, "my-stack");
                assert_eq!(paths, vec![std::path::PathBuf::from("file.txt"), std::path::PathBuf::from("dir/")]);
                assert!(!force);
            }
            _ => panic!("Expected Commands::Template"),
        }
    }

    #[test]
    fn cli_template_add_force_flag_should_parse() {
        let cli = Cli::try_parse_from(["fa", "-t", "add", "my-stack", "file.txt", "-f"])
            .expect("fa -t add -f should parse");
        match cli.command {
            Some(Commands::Template {
                action: TemplateAction::Add { recipe, paths, force },
            }) => {
                assert_eq!(recipe, "my-stack");
                assert_eq!(paths, vec![std::path::PathBuf::from("file.txt")]);
                assert!(force);
            }
            _ => panic!("Expected Commands::Template"),
        }
    }

    #[test]
    fn test_builtin_commands_parse_with_prefixed_and_short_flags() {
        assert!(Cli::try_parse_from(["fa", "--new", "my-recipe"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-n", "my-recipe"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--list"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-l"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--search", "test"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-se", "test"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--show", "test"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-sh", "test"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--recipe", "validate"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-r", "validate"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--template", "add", "rec", "file.txt"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-t", "add", "rec", "file.txt"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--alias", "cmd"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-a", "cmd"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--self-update"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--self-uninstall"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "--config"]).is_ok());
        assert!(Cli::try_parse_from(["fa", "-co"]).is_ok());
    }

    #[test]
    fn test_cli_config_flags() {
        let cli = Cli::try_parse_from(["fa", "--config"]).expect("fa --config should parse");
        match cli.command {
            Some(Commands::Config) => {}
            _ => panic!("Expected Commands::Config"),
        }

        let cli = Cli::try_parse_from(["fa", "-co"]).expect("fa -co should parse");
        match cli.command {
            Some(Commands::Config) => {}
            _ => panic!("Expected Commands::Config"),
        }
    }

    #[test]
    fn test_list_filter_cli_parsing() {
        let cli = Cli::try_parse_from(["fa", "--list", "recipes"]).unwrap();
        match cli.command {
            Some(Commands::List { filter, .. }) => {
                assert_eq!(filter, Some("recipes".to_string()));
            }
            _ => panic!("Expected Commands::List"),
        }

        let cli = Cli::try_parse_from(["fa", "-l", "-r"]).unwrap();
        match cli.command {
            Some(Commands::List { recipes, .. }) => {
                assert!(recipes);
            }
            _ => panic!("Expected Commands::List"),
        }

        let cli = Cli::try_parse_from(["fa", "-l", "-p"]).unwrap();
        match cli.command {
            Some(Commands::List { packs, .. }) => {
                assert!(packs);
            }
            _ => panic!("Expected Commands::List"),
        }
    }

    #[test]
    fn test_recipe_rm_cli_parsing() {
        let cli = Cli::try_parse_from(["fa", "--recipe", "rm", "my-stack", "-y"])
            .expect("fa --recipe rm my-stack -y should parse");
        match cli.command {
            Some(Commands::Recipe {
                action: RecipeAction::Rm { name, yes },
            }) => {
                assert_eq!(name, "my-stack");
                assert!(yes);
            }
            _ => panic!("Expected RecipeAction::Rm"),
        }

        let cli = Cli::try_parse_from(["fa", "-r", "rm", "other-stack"])
            .expect("fa -r rm other-stack should parse");
        match cli.command {
            Some(Commands::Recipe {
                action: RecipeAction::Rm { name, yes },
            }) => {
                assert_eq!(name, "other-stack");
                assert!(!yes);
            }
            _ => panic!("Expected RecipeAction::Rm"),
        }
    }

    #[test]
    fn test_unknown_alias_error_includes_did_you_mean() {
        let mut config = Config::default();
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(
            "status".to_string(),
            crate::config::Command {
                command: "git status".to_string(),
                description: Some("Status".to_string()),
                platform: None,
                aliases: vec![],
                ..Default::default()
            },
        );
        config.aliases.insert("git".to_string(), cmds);

        let err = unknown_alias_error("stts", &config);
        assert!(err.contains("Unknown command 'stts'"));
        assert!(err.contains("Did you mean?"), "Error must contain 'Did you mean?': got {err}");
        assert!(err.contains("status"), "Error must suggest 'status': got {err}");
        assert!(err.contains("fa --list"), "Error must recommend 'fa --list': got {err}");
    }

    #[test]
    fn test_unknown_namespaced_subcommand_error_includes_did_you_mean() {
        let mut config = Config::default();
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(
            "ls".to_string(),
            crate::config::Command {
                command: "bunx tabernaculo list".to_string(),
                description: Some("List".to_string()),
                platform: None,
                aliases: vec![],
                ..Default::default()
            },
        );
        config.aliases.insert(":skills".to_string(), cmds);

        let err = unknown_alias_error("skills lss", &config);
        assert!(err.contains("Unknown command 'skills lss'"));
        assert!(err.contains("Did you mean?"), "Error must contain 'Did you mean?': got {err}");
        assert!(err.contains("ls"), "Error must suggest 'ls': got {err}");
    }

    #[test]
    fn test_unknown_recipe_error_includes_did_you_mean() {
        let mut config = Config::default();
        let recipe: crate::config::Recipe = toml::from_str(
            r#"
name = "Next.js"
description = "Next.js TS"
"#,
        ).unwrap();
        config.recipes.insert("next-ts".to_string(), recipe);

        let err = unknown_recipe_error("nxt-ts", &config);
        assert!(err.contains("Unknown recipe 'nxt-ts'"));
        assert!(err.contains("Did you mean?"), "Error must contain 'Did you mean?': got {err}");
        assert!(err.contains("next-ts"), "Error must suggest 'next-ts': got {err}");
        assert!(err.contains("fa --list"), "Error must recommend 'fa --list': got {err}");
    }

    #[test]
    fn test_suggest_unrecognized_subcommand() {
        let mut config = Config::default();
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(
            "ls".to_string(),
            crate::config::Command {
                command: "bunx list".to_string(),
                description: None,
                platform: None,
                aliases: vec![],
                ..Default::default()
            },
        );
        config.aliases.insert(":skills".to_string(), cmds);

        let suggestion = suggest_unrecognized_subcommand("skill", &config);
        assert_eq!(suggestion, Some("skills".to_string()), "Must suggest 'skills' for 'skill'");
    }

    #[test]
    fn test_format_source_location() {
        let path = PathBuf::from("/home/user/.config/fa/recipes.d/skills.toml");
        let formatted = format_source_location(Some(&path), Some(12));
        assert!(formatted.is_some(), "Formatted location must not be None");
        let loc = formatted.unwrap();
        assert!(loc.contains("skills.toml"));
        assert!(loc.contains("(line 12)"));
    }

    #[test]
    fn test_format_command_details_includes_source_location() {
        let mut env_map = std::collections::BTreeMap::new();
        env_map.insert("CONTEXT".to_string(), "2048".to_string());
        let mut force_map = std::collections::BTreeMap::new();
        force_map.insert("MODEL_PATH".to_string(), "/models/coder.gguf".to_string());

        let cmd = crate::config::Command {
            command: "bunx tabernaculo list".to_string(),
            description: Some("List skills".to_string()),
            platform: None,
            aliases: vec![],
            args: vec![],
            env: env_map,
            env_force: force_map,
            source_file: Some(PathBuf::from("/home/user/.config/fa/recipes.d/skills.toml")),
            source_line: Some(12),
        };
        let output = format_command_details(&cmd, ":skills", "ls");
        assert!(output.contains("Command:") && output.contains("skills ls"), "Output must contain full command name");
        assert!(output.contains("Defined in:"), "Output must contain 'Defined in:'");
        assert!(output.contains("skills.toml (line 12)"), "Output must contain file and line");
        assert!(output.contains("Environment:"), "Output must contain Environment section");
        assert!(output.contains("CONTEXT = 2048"), "Output must display env");
        assert!(output.contains("Environment (forced):"), "Output must contain forced section");
        assert!(output.contains("MODEL_PATH = /models/coder.gguf"), "Output must display forced env");
        assert!(output.contains("bunx tabernaculo list"), "Output must contain the command");
    }

    #[test]
    fn test_format_command_details_shows_args_usage_and_documentation() {
        let cmd = crate::config::Command {
            command: "convert $1 $2".to_string(),
            description: Some("Convert images".to_string()),
            platform: None,
            aliases: vec!["cnv".to_string()],
            args: vec![
                crate::config::CommandArg {
                    name: "input".to_string(),
                    description: Some("Source path".to_string()),
                    required: true,
                },
                crate::config::CommandArg {
                    name: "output".to_string(),
                    description: Some("Destination directory".to_string()),
                    required: false,
                },
            ],
            ..Default::default()
        };
        let out = format_command_details(&cmd, "img", "convert");
        assert!(out.contains("Usage: fa convert <input> [output]"), "Must show Usage line: got {out}");
        assert!(out.contains("Arguments:"), "Must show Arguments block: got {out}");
        assert!(out.contains("<input>") && out.contains("Source path"), "Must document input arg: got {out}");
        assert!(out.contains("[output]") && out.contains("Destination directory"), "Must document output arg: got {out}");
    }

    #[test]
    fn test_format_recipe_details_includes_source_location() {
        let recipe = crate::config::Recipe {
            name: "Next.js TS".to_string(),
            description: "Next.js TypeScript stack".to_string(),
            source_file: Some(PathBuf::from("/home/user/.config/fa/recipes.toml")),
            source_line: Some(5),
            ..Default::default()
        };
        let output = format_recipe_details(&recipe, "next-ts");
        assert!(
            output.contains(&format!("{BOLD_CYAN}Recipe:{RESET} next-ts")),
            "Recipe header must use the styled cyan label like `--show` for commands, got:\n{output:?}"
        );
        assert!(output.contains("Defined in:"), "Output must contain 'Defined in:'");
        assert!(output.contains("recipes.toml (line 5)"));
    }

    #[test]
    fn test_rewrite_args_show_multiword() {
        let config = Config::default();

        // 1. `fa -sh skills ls` -> `fa -sh "skills ls"`
        let args = vec!["fa".to_string(), "-sh".to_string(), "skills".to_string(), "ls".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "-sh", "skills ls"]);

        // 2. `fa --show skills ls` -> `fa --show "skills ls"`
        let args = vec!["fa".to_string(), "--show".to_string(), "skills".to_string(), "ls".to_string()];
        let rewritten = rewrite_args(args, Some(&config));
        assert_eq!(rewritten, vec!["fa", "--show", "skills ls"]);
    }

    #[test]
    fn test_is_recipe_cli_command() {
        assert!(is_recipe_or_help_cmd(&["fa".into(), "-r".into(), "edit".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "--recipe".into(), "edit".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "-re".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "-rv".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "recipe".into(), "edit".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "re".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "rv".into(), "ai".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "--help".into()]));
        assert!(is_recipe_or_help_cmd(&["fa".into(), "-h".into()]));
        assert!(!is_recipe_or_help_cmd(&["fa".into(), "ai".into(), "code".into()]));
        assert!(!is_recipe_or_help_cmd(&["fa".into(), "--alias".into(), "code".into()]));
    }

    #[test]
    fn test_is_validation_cmd() {
        assert!(is_validation_cmd(&["fa".into(), "-rv".into()]));
        assert!(is_validation_cmd(&["fa".into(), "rv".into()]));
        assert!(is_validation_cmd(&["fa".into(), "-r".into(), "validate".into()]));
        assert!(is_validation_cmd(&["fa".into(), "--recipe".into(), "validate".into()]));
        assert!(is_validation_cmd(&["fa".into(), "recipe".into(), "validate".into()]));
        assert!(is_validation_cmd(&["fa".into(), "-r".into(), "-v".into()]));
        assert!(is_validation_cmd(&["fa".into(), "-rv".into(), "ai".into()]));
        assert!(!is_validation_cmd(&["fa".into(), "-r".into(), "new".into(), "example".into()]));
        assert!(!is_validation_cmd(&["fa".into(), "-l".into()]));
        assert!(!is_validation_cmd(&["fa".into(), "new".into(), "test".into()]));
    }

    #[test]
    fn test_is_recipe_edit_cmd() {
        assert!(is_recipe_edit_cmd(&["fa".into(), "-re".into()]));
        assert!(is_recipe_edit_cmd(&["fa".into(), "re".into()]));
        assert!(is_recipe_edit_cmd(&["fa".into(), "-r".into(), "edit".into()]));
        assert!(is_recipe_edit_cmd(&["fa".into(), "--recipe".into(), "edit".into()]));
        assert!(is_recipe_edit_cmd(&["fa".into(), "recipe".into(), "edit".into()]));
        assert!(is_recipe_edit_cmd(&["fa".into(), "-re".into(), "ai".into()]));
        assert!(!is_recipe_edit_cmd(&["fa".into(), "-r".into(), "new".into(), "example".into()]));
        assert!(!is_recipe_edit_cmd(&["fa".into(), "-rv".into()]));
    }

    #[test]
    fn test_is_help_or_version_cmd() {
        assert!(is_help_or_version_cmd(&["fa".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "--help".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "-h".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "help".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "--version".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "-v".into()]));
        assert!(is_help_or_version_cmd(&["fa".into(), "-r".into(), "new".into(), "--help".into()]));
        assert!(!is_help_or_version_cmd(&["fa".into(), "-r".into(), "new".into(), "example".into()]));
        assert!(!is_help_or_version_cmd(&["fa".into(), "-l".into()]));
    }

    #[test]
    fn test_search_recipes_matches_description() {
        let recipe = crate::config::Recipe {
            name: "web-app".to_string(),
            description: "Fullstack template with SQLite database".to_string(),
            ..Default::default()
        };
        assert!(
            recipe_matches_search("web", &recipe, "sqlite"),
            "expected recipe search to match keyword in recipe.description"
        );
    }

    #[test]
    fn test_search_aliases_matches_category_section() {
        let cmd = crate::config::Command {
            command: "upscayl -i $1".to_string(),
            description: Some("AI image upscaler".to_string()),
            ..Default::default()
        };
        assert!(
            command_matches_search("wrapper", "upscayl", &cmd, "wrapper"),
            "expected search to match alias category/section"
        );
        assert!(
            command_matches_search(":skills", "ls", &cmd, "skills"),
            "expected search to match namespaced category without colon"
        );
        assert!(
            command_matches_search("media", "resize", &cmd, "resize"),
            "expected search to match command name"
        );
        assert!(
            command_matches_search("media", "resize", &cmd, "upscaler"),
            "expected search to match command description"
        );
        assert!(
            !command_matches_search("media", "resize", &cmd, "nonexistent"),
            "non-matching query should return false"
        );
    }


    #[test]
    fn test_pick_recipe_from_list_selection() {
        let mut r1 = crate::config::Recipe::default();
        r1.description = "First recipe description".to_string();
        let mut r2 = crate::config::Recipe::default();
        r2.description = "Second recipe description".to_string();

        let recipes = vec![("recipe-one", &r1), ("recipe-two", &r2)];

        let mut input = std::io::Cursor::new(b"2\n");
        let mut output = Vec::new();
        let chosen = pick_recipe_from_list(&recipes, &mut input, &mut output).unwrap();
        assert_eq!(chosen, "recipe-two");
    }

    #[test]
    fn test_pick_recipe_from_list_cancel() {
        let r1 = crate::config::Recipe::default();
        let recipes = vec![("recipe-one", &r1)];

        let mut input = std::io::Cursor::new(b"q\n");
        let mut output = Vec::new();
        let res = pick_recipe_from_list(&recipes, &mut input, &mut output);
        assert!(res.is_err());
    }

    #[test]
    fn test_pick_recipe_from_list_empty() {
        let recipes: Vec<(&str, &crate::config::Recipe)> = Vec::new();
        let mut input = std::io::Cursor::new(b"1\n");
        let mut output = Vec::new();
        let res = pick_recipe_from_list(&recipes, &mut input, &mut output);
        assert!(res.is_err());
    }

    #[test]
    fn test_prompt_project_name() {
        let mut input = std::io::Cursor::new(b"my-new-app\n");
        let mut output = Vec::new();
        let name = prompt_project_name(&mut input, &mut output).unwrap();
        assert_eq!(name, "my-new-app");

        let mut input_cancel = std::io::Cursor::new(b"q\n");
        let mut output_cancel = Vec::new();
        assert!(prompt_project_name(&mut input_cancel, &mut output_cancel).is_err());
    }

    #[test]
    fn test_recipe_as_action_error() {
        let mut config = Config::default();
        let mut recipe = crate::config::Recipe::default();
        recipe.name = "Web Components UI".to_string();
        config.recipes.insert("wc-ui".to_string(), recipe);

        let err = recipe_as_action_error("wc-ui", &config);
        assert!(err.is_some(), "expected error for recipe name used as action");
        let msg = err.unwrap();
        assert!(msg.contains("'wc-ui' is a recipe, not an action"));
        assert!(msg.contains("fa -n wc-ui"));
        assert!(msg.contains("fa -re wc-ui"));
        assert!(msg.contains("fa -sh wc-ui"));

        assert_eq!(recipe_as_action_error("nonexistent", &config), None);
    }
}
