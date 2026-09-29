use crate::config::{Config, CreateStep, Recipe};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::colors::*;
use crate::platform::Platform;
use crate::templating::{find_unknown_placeholders, substitute, substitute_shell};

/// Resolves the list of component names to install.
/// Priority: explicit component > explicit pack > recipe default.
/// Returns (list_of_components, description).
pub(crate) fn resolve_components(
    recipe: &Recipe,
    opts_component: &Option<String>,
    opts_pack: &Option<String>,
) -> anyhow::Result<(Vec<String>, String)> {
    let packs_dir = recipe.packs_dir.as_deref().unwrap_or("packs");
    if let Some(pack_name) = opts_pack {
        let pack = Config::find_pack(Some(recipe), packs_dir, pack_name)?;
        return Ok((pack.components, format!("pack '{pack_name}'")));
    }
    if let Some(comp) = opts_component {
        return Ok((vec![comp.clone()], format!("component '{comp}'")));
    }
    if let Some(default_pack) = &recipe.default_pack {
        let pack = Config::find_pack(Some(recipe), packs_dir, default_pack)?;
        return Ok((pack.components, format!("default pack '{default_pack}'")));
    }
    Ok((vec![], "no component or pack specified".to_string()))
}

/// Formats the available packs and components for a recipe as a human-readable list.
pub fn format_recipe_packs_and_components(recipe: &Recipe, recipe_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}[Recipe] {}{} · {}{}\n\n", BOLD_CYAN, recipe_name, RESET, DIM, recipe.description));

    let packs_dir = recipe.packs_dir.as_deref().unwrap_or("packs");
    let packs = Config::list_recipe_packs(recipe);
    out.push_str(&format!("{}Available packs for '{}' ({}):{}\n", BOLD_GREEN, recipe_name, packs_dir, RESET));
    if packs.is_empty() {
        out.push_str(&format!("  {}(no packs found in {}){}\n", DIM, packs_dir, RESET));
    } else {
        for pack in &packs {
            let comps = pack.components.join(", ");
            out.push_str(&format!("  • {}{}{} {}(components: {}){}\n", BOLD_CYAN, pack.name, RESET, DIM, comps, RESET));
        }
    }
    out.push('\n');

    let templates_dir = Config::resolve_templates_dir(recipe);
    let components = Config::list_components(recipe);
    out.push_str(&format!("{}Available components ({}):{}\n", BOLD_GREEN, templates_dir, RESET));
    if components.is_empty() {
        out.push_str(&format!("  {}(no components found in {}){}\n", DIM, templates_dir, RESET));
    } else {
        for comp in &components {
            out.push_str(&format!("  • {}{}{}\n", WHITE, comp, RESET));
        }
    }
    out.push('\n');

    if let Some(ref dp) = recipe.default_pack {
        out.push_str(&format!("{}Default pack:{} {}\n\n", DIM, RESET, dp));
    }

    out.push_str(&format!("Run {}fa new {} <pack>{} or {}fa new {} <component>{} to install.\n", BOLD_CYAN, recipe_name, RESET, BOLD_CYAN, recipe_name, RESET));
    out
}

/// Displays the available packs and components for a recipe.
pub fn display_recipe_packs_and_components(recipe: &Recipe, recipe_name: &str) {
    print!("{}", format_recipe_packs_and_components(recipe, recipe_name));
}

/// Copies all files from a resolved `CreateStep` source directory to
/// the destination. Applies `{{variable}}` substitution to paths.
pub(crate) fn execute_create_step(
    create: &CreateStep,
    vars: &HashMap<String, String>,
    templates_dir: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    let from_resolved = substitute(&create.from, vars);
    let to_resolved = substitute(&create.to, vars);

    if from_resolved.starts_with('/') || templates_dir.starts_with('/') {
        anyhow::bail!("Root paths starting with '/' are not allowed; use '~/' or paths relative to ~/.config/fa");
    }

    if to_resolved.starts_with('/') {
        anyhow::bail!("create step 'to' path '{to_resolved}' must be relative to project and cannot start with '/'");
    }

    if from_resolved.split('/').any(|c| c == "..") {
        anyhow::bail!("create step 'from' path '{from_resolved}' must not contain '..'");
    }

    if to_resolved.split('/').any(|c| c == "..") {
        anyhow::bail!("create step 'to' path '{to_resolved}' must not contain '..'");
    }

    let from_full = if let Some(rest) = from_resolved.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        PathBuf::from(home).join(rest)
    } else if let Some(rest) = templates_dir.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        let base = PathBuf::from(home).join(rest);
        base.join(&from_resolved)
    } else {
        let user_config_dir = Config::get_user_config_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        let clean_templates_dir = templates_dir.strip_prefix("templates/").unwrap_or(templates_dir);
        if from_resolved.starts_with("templates/") || from_resolved.starts_with(templates_dir) {
            user_config_dir.join(&from_resolved)
        } else if !clean_templates_dir.is_empty() && from_resolved.starts_with(clean_templates_dir) {
            user_config_dir.join("templates").join(&from_resolved)
        } else {
            user_config_dir.join(templates_dir).join(&from_resolved)
        }
    };

    let dest_full = PathBuf::from(&to_resolved);

    if !from_full.exists() {
        anyhow::bail!(
            "create step source not found: {} (resolved from '{}')",
            from_full.display(),
            create.from
        );
    }

    if dry_run {
        println!(
            "  {DIM}[Dry-Run]{RESET} Would copy {} → {}",
            from_full.display(),
            dest_full.display()
        );
        return Ok(());
    }

    fs::create_dir_all(&dest_full)
        .map_err(|e| anyhow::anyhow!("Failed to create dest dir {}: {e}", dest_full.display()))?;

    let mut copied = 0usize;
    for entry in fs::read_dir(&from_full)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", from_full.display()))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("Dir entry error: {e}"))?;
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dest_full.join(&file_name);

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path)
                .map_err(|e| anyhow::anyhow!("Failed to copy {}: {e}", entry_path.display()))?;
            copied += 1;
        }
    }

    println!("  {BOLD_GREEN}✓{RESET} Copied {} files ({} → {})", copied, from_resolved, to_resolved);
    Ok(())
}

/// Recursively copies a directory.
pub(crate) fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dest.join(&file_name);
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dest_path)?;
        } else {
            fs::copy(&entry_path, &dest_path)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewOptions {
    pub recipe_key: String,
    pub project_name: String,
    pub variant: Option<String>,
    pub dry_run: bool,
    pub no_install: bool,
    pub pin_versions: bool,
    /// Component name to install (overrides default pack).
    pub component: Option<String>,
    /// Pack name to install (contains multiple components).
    pub pack: Option<String>,
}

/// Result of a `fa new` run: whether the project should be registered in
/// state.toml (and with which `installed` flag) plus the actual outcome.
#[derive(Debug, Clone)]
pub struct NewOutcome {
    pub project_dir: String,
    pub installed: bool,
    /// Whether the project directory survived and should be tracked in state.
    pub register: bool,
}

/// Full result of `run_new`: the outcome (for state tracking) and the
/// success/error, so a kept-but-broken project still exits non-zero (like Astro).
#[derive(Debug)]
pub struct NewResult {
    pub outcome: NewOutcome,
    pub result: anyhow::Result<()>,
}

/// Expands a tilde-prefixed path (~/...) to an absolute user path.
pub fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest).to_string_lossy().to_string();
        }
    path.to_string()
}

// Reads a template file from a templates directory (`<dir>/<rel>`).
// Templates are user-provided files on disk, so adding a template never
// requires editing Rust code.
fn read_user_template_with_dir(templates_dir: &Path, rel: &str) -> Option<String> {
    let path = templates_dir.join(rel);
    fs::read_to_string(&path).ok()
}

/// Shell builtins and keywords that never require an external binary, so they
/// must not be reported as missing applications during preflight.
const SHELL_BUILTINS: &[&str] = &[
    ".", ":", "[", "[[", "alias", "bg", "break", "case", "cd", "command",
    "continue", "coproc", "do", "done", "echo", "elif", "else", "esac",
    "eval", "exec", "exit", "export", "false", "fi", "for", "function",
    "hash", "if", "in", "jobs", "local", "pwd", "read", "readonly",
    "return", "select", "set", "shift", "source", "then", "time", "times",
    "trap", "true", "type", "typeset", "ulimit", "umask", "unalias",
    "unset", "until", "wait", "while",
];

/// Extracts the standalone application names a shell command invokes: the
/// first token of each segment separated by shell operators (`&&`, `||`, `;`,
/// `|`, newline), de-duplicated in order of appearance. Shell builtins and
/// leading environment assignments (e.g. `FOO=bar`) are skipped.
pub fn extract_commands(command: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for segment in command.split(['&', '|', ';', '\n']) {
        let Some(first) = segment.split_whitespace().next() else {
            continue;
        };
        let token = first.trim_start_matches(['\'', '"', '(', '{']).to_string();
        if token.is_empty()
            || token.contains('=')
            || SHELL_BUILTINS.contains(&token.as_str())
        {
            continue;
        }
        if seen.insert(token.clone()) {
            out.push(token);
        }
    }
    out
}

/// Returns the subset of `commands` that are not present on the system PATH.
pub fn missing_commands(commands: &[String]) -> Vec<String> {
    commands
        .iter()
        .filter(|cmd| !crate::platform::command_exists(cmd))
        .cloned()
        .collect()
}

/// Suggested install command hint for the current platform.
fn install_hint(cmd: &str) -> String {
    match crate::platform::Platform::detect() {
        crate::platform::Platform::Debian => format!("sudo apt install {cmd}"),
        crate::platform::Platform::Termux => format!("pkg install {cmd}"),
        crate::platform::Platform::Unsupported(_) => "your system package manager".to_string(),
    }
}

/// Ahead-of-execution check: verifies every application a shell command
/// references exists on PATH. When something is missing, prints a friendly
/// diagnostic and returns an error so the command never runs and no ugly
/// "command not found" wall of text reaches the user.
pub fn preflight(command: &str) -> anyhow::Result<()> {
    let missing = missing_commands(&extract_commands(command));
    if missing.is_empty() {
        return Ok(());
    }
    for cmd in &missing {
        println!("{BOLD_RED}✗ Missing application:{RESET} {BOLD_YELLOW}{cmd}{RESET} is not installed on this system.");
        println!("  {DIM}Install it with your package manager — {}.{RESET}", install_hint(cmd));
    }
    anyhow::bail!("Missing system application(s): {}", missing.join(", "))
}

/// Runs the full `fa new` flow: create → files → steps.
pub fn run_new(config: &Config, opts: &NewOptions) -> NewResult {
    let recipe = match config.recipes.get(&opts.recipe_key) {
        Some(recipe) => recipe,
        None => {
            return NewResult {
                outcome: NewOutcome {
                    project_dir: opts.project_name.clone(),
                    installed: false,
                    register: false,
                },
                result: Err(anyhow::anyhow!("Recipe '{}' not found", opts.recipe_key)),
            };
        }
    };

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
    vars.insert("templates_dir".to_string(), Config::resolve_templates_dir(recipe));
    if let Some(packs_dir) = &recipe.packs_dir {
        vars.insert("packs_dir".to_string(), packs_dir.clone());
    }
    vars.insert("component".to_string(), opts.component.clone().unwrap_or_default());

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
        inputs.push(step.command.clone().unwrap_or_default());
        if let Some(create) = &step.create {
            inputs.push(create.from.clone());
            inputs.push(create.to.clone());
        }
    }

    // Resolve components from pack/component options or recipe default.
    let (components, component_desc) = match resolve_components(recipe, &opts.component, &opts.pack) {
        Ok(res) => res,
        Err(e) => {
            return NewResult {
                outcome: NewOutcome {
                    project_dir: opts.project_name.clone(),
                    installed: false,
                    register: false,
                },
                result: Err(e),
            };
        }
    };
    if !components.is_empty() {
        println!("  {DIM}Installing: {component_desc}{RESET}");
    }

    // Build defaults from recipe `variables` table (prompt: label, default: value).
    if let Err(e) = collect_validated_vars(
        recipe,
        &inputs,
        &mut vars,
        opts.dry_run,
        std::io::IsTerminal::is_terminal(&std::io::stdin()),
        &mut |label, default| prompt_input(label, default),
    ) {
        return NewResult {
            outcome: NewOutcome {
                project_dir: opts.project_name.clone(),
                installed: false,
                register: false,
            },
            result: Err(e),
        };
    }

    // 1. Create base
    let original_cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    // Whether a project directory already existed before this run. A directory
    // that existed beforehand is NEVER removed, even on internal failures.
    let project_existed = Path::new(&opts.project_name).exists();
    // Whether the base skeleton (create + files) completed. Failures before
    // this point are internal (create/files); failures after are recoverable
    // (steps), so the project is kept (Astro-style).
    let mut scaffold_ok = false;

    let flow_result: anyhow::Result<()> = (|| {
        if let Some(create) = &recipe.create
            && let Some(command) = &create.command {
                let rendered = substitute_shell(command, &vars);
                if opts.dry_run {
                    println!("{DIM}[Dry-Run]{RESET} Would scaffold base via: {rendered}");
                } else {
                    preflight(&rendered)?;
                    run_shell(&rendered)?;
                    // Scaffold CLIs (create-tool, cargo new, ...) generate a
                    // subdirectory named after the project; run the rest of the
                    // flow (files + steps) inside it.
                    std::env::set_current_dir(&opts.project_name).map_err(|e| {
                        anyhow::anyhow!(
                            "Scaffold did not produce directory '{}': {e}",
                            opts.project_name
                        )
                    })?;
                }
            }

        // 2. Write files
        println!("\nWriting configuration files:");
        let mut errors: Vec<String> = Vec::new();
        let mut written = 0usize;
        for (dest, spec) in &recipe.files {
            // When dest is empty string, the file goes to the project root.
            // The filename is inferred from the basename of `from` or `template`.
            let (target, display_dest) = if dest.is_empty() {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let file_name = spec.from.as_ref()
                    .or(spec.template.as_ref())
                    .and_then(|s| Path::new(s).file_name())
                    .and_then(|f| f.to_str())
                    .unwrap_or("");
                let target = PathBuf::from(&cwd).join(file_name);
                (target, file_name)
            } else {
                let dest_path = expand_home(dest);
                (PathBuf::from(&dest_path), dest.as_str())
            };

            if opts.dry_run {
                println!("  {DIM}[Dry-Run]{RESET} Would write file: {display_dest}");
                continue;
            }

            if target.exists() && spec.skip_if_exists == Some(true) {
                println!("  {BOLD_YELLOW}[SKIP]{RESET} {display_dest} (already exists)");
                continue;
            }

            let content = match resolve_file_content(spec, &vars, recipe.template_base.as_deref()) {
                Ok(Some(c)) => c,
                Ok(None) => {
                    errors.push(format!(
                        "No content source for file '{display_dest}' (missing from/inline/template)"
                    ));
                    continue;
                }
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            if let Some(parent) = target.parent()
                && let Err(e) = fs::create_dir_all(parent) {
                    errors.push(format!("Failed to create dir {}: {e}", parent.display()));
                    continue;
                }
            if let Err(e) = fs::write(&target, content) {
                errors.push(format!("Failed to write {display_dest}: {e}"));
                continue;
            }
            written += 1;
        }

        if !errors.is_empty() {
            for err in &errors {
                println!("  {BOLD_RED}✗{RESET} {err}");
            }
            anyhow::bail!(
                "{} of {} configuration files failed to write",
                errors.len(),
                recipe.files.len()
            );
        }

        println!("  {BOLD_GREEN}✓{RESET} {written} configuration files written.");

        // The skeleton (create + files) is complete; from here on, failures are
        // recoverable (e.g. dependency installation) and must keep the project.
        scaffold_ok = true;

        // 2.5 Execute create steps (packs/components file copy)
        if !components.is_empty() {
            println!("\nInstalling components:");
            let templates_dir = Config::resolve_templates_dir(recipe);
            for comp in &components {
                let mut comp_vars = vars.clone();
                comp_vars.insert("component".to_string(), comp.clone());
                for step in &recipe.steps {
                    if let Some(create) = &step.create
                        && let Err(e) = execute_create_step(create, &comp_vars, &templates_dir, opts.dry_run) {
                            anyhow::bail!("create step failed: {e}");
                    }
                }
            }
        }

        // 3. Steps
        if !recipe.steps.is_empty() {
            let last_install_idx = recipe.steps.iter().rposition(|s| s.install);
            println!("\nSteps:");
            for (idx, step) in recipe.steps.iter().enumerate() {
                if step.command.is_none() {
                    continue;
                }
                if let Some(platform) = &step.platform
                    && platform != "all" && !platform_matches(platform)? {
                        continue;
                    }
                if step.install && opts.no_install {
                    println!("  {BOLD_YELLOW}[SKIP]{RESET} {} (--no-install)", step.description.as_deref().unwrap_or(step.command.as_deref().unwrap_or("")));
                    continue;
                }
                let rendered = substitute_shell(step.command.as_deref().unwrap_or(""), &vars);
                if opts.dry_run {
                    println!("  {DIM}[Dry-Run]{RESET} Would run: {rendered}");
                } else {
                    preflight(&rendered)?;
                    let label = step.description.as_deref().unwrap_or(step.command.as_deref().unwrap_or(""));
                    if step.install {
                        run_shell_quiet_with_spinner(&rendered, label)?;
                        println!("  {BOLD_GREEN}✓{RESET} {label}");
                    } else {
                        println!("  {BOLD_GREEN}✓{RESET} {label}");
                        run_shell(&rendered)?;
                    }
                }
                if Some(idx) == last_install_idx && opts.pin_versions {
                    if opts.dry_run {
                        println!("  {DIM}[Dry-Run]{RESET} Would pin versions in package.json");
                    } else {
                        println!("\nPinning exact versions:");
                        let cwd = std::env::current_dir().unwrap_or_default();
                        for r in crate::pinning::pin_project(&cwd) {
                            match &r.status {
                                crate::pinning::PinStatus::Pinned(n) => {
                                    println!("  {BOLD_GREEN}✓{RESET} {} — removed prefix from {n} version(s)", r.manifest);
                                }
                                crate::pinning::PinStatus::Unchanged => {
                                    println!("  {DIM}−{RESET} {} — unchanged", r.manifest);
                                }
                                crate::pinning::PinStatus::NotFound => {
                                    println!("  {DIM}−{RESET} {} — not found (skipped)", r.manifest);
                                }
                                crate::pinning::PinStatus::Error(e) => {
                                    println!("  {BOLD_YELLOW}⚠{RESET} {} — {e}", r.manifest);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    })();

    if let Err(e) = flow_result {
        // Always restore the original working directory.
        let _ = std::env::set_current_dir(&original_cwd);

        let project_path = Path::new(&opts.project_name);

        if !opts.dry_run && !project_existed && !scaffold_ok {
            // Internal failure before the skeleton completed AND the directory
            // did not pre-exist: remove the incomplete project.
            if project_path.exists() {
                match fs::remove_dir_all(project_path) {
                    Ok(()) => println!(
                        "{BOLD_YELLOW}[ROLLBACK]{RESET} Removed incomplete project '{}'",
                        opts.project_name
                    ),
                    Err(rm_err) => println!(
                        "{BOLD_YELLOW}[WARN]{RESET} Could not remove incomplete project '{}': {rm_err}",
                        opts.project_name
                    ),
                }
            }
            return NewResult {
                outcome: NewOutcome {
                    project_dir: opts.project_name.clone(),
                    installed: false,
                    register: false,
                },
                result: Err(e),
            };
        }

        // Recoverable failure (steps) OR a pre-existing directory: keep the
        // project and let the user finish installation manually (Astro-style).
        if project_existed {
            println!(
                "{BOLD_YELLOW}[WARN]{RESET} Pre-existing directory '{}' was left untouched. Run the failed command manually inside it.",
                opts.project_name
            );
        } else if !opts.dry_run {
            println!(
                "{BOLD_YELLOW}[WARN]{RESET} Dependencies could not be installed. Project kept at './{}'.",
                opts.project_name
            );
            println!(
                "Run `{DIM}cd {}{RESET} && {DIM}pnpm install{RESET}` to finish manually.",
                opts.project_name
            );
        }

        return NewResult {
            outcome: NewOutcome {
                project_dir: opts.project_name.clone(),
                installed: false,
                register: !opts.dry_run && !project_existed,
            },
            result: Err(e),
        };
    }

    if recipe.is_pack_recipe() && (opts.project_name == "." || opts.project_name.is_empty()) {
        if let Some(comp) = &opts.component {
            println!("\n{BOLD_GREEN}Component '{comp}' installed successfully.{RESET}");
        } else if let Some(pack) = &opts.pack {
            println!("\n{BOLD_GREEN}Pack '{pack}' installed successfully.{RESET}");
        } else {
            println!("\n{BOLD_GREEN}Components installed successfully.{RESET}");
        }
        if recipe.final_message.is_some() {
            let final_msg = resolve_final_message(recipe, &opts.project_name, &vars);
            println!("{final_msg}");
        }
    } else {
        println!("\n{BOLD_GREEN}Project '{}' created successfully.{RESET}", opts.project_name);
        let final_msg = resolve_final_message(recipe, &opts.project_name, &vars);
        println!("{final_msg}");
    }
    NewResult {
        outcome: NewOutcome {
            project_dir: opts.project_name.clone(),
            installed: !opts.no_install,
            register: !opts.dry_run && opts.project_name != "." && !opts.project_name.is_empty(),
        },
        result: Ok(()),
    }
}

/// Formats the final post-scaffold message shown to the user upon success.
/// If the recipe specifies a `final_message`, it is rendered with variables (`{{name}}`, `{name}`, etc.) substituted.
/// Otherwise, returns the default neutral navigation hint: "Run `cd <name>` to go to project."
pub fn resolve_final_message(recipe: &Recipe, project_name: &str, vars: &HashMap<String, String>) -> String {
    if let Some(msg) = &recipe.final_message {
        let mut rendered = substitute(msg, vars);
        rendered = rendered.replace("{name}", project_name);
        rendered = rendered.replace("{project_name}", project_name);
        rendered
    } else {
        format!("Run `{DIM}cd {project_name}{RESET}` to go to project.")
    }
}

/// Resolves the content of a file spec (from / inline / template) honoring an
/// optional recipe-level `template_base` (`None` = legacy byte-identical).
/// Returns `Ok(None)` only when the spec declares no content source at all;
/// missing template files and out-of-range paths are `Err` with a clear
/// message (never a silent "No content source").
fn resolve_file_content(
    spec: &crate::config::FileSpec,
    vars: &HashMap<String, String>,
    base: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(templates_dir) = Config::get_user_templates_dir() else {
        // Without HOME there is no templates dir: inline still works, file
        // refs report a clear missing-file error instead of silent None.
        return resolve_file_content_with_dir(spec, vars, Path::new("/nonexistent"), base);
    };
    resolve_file_content_with_dir(spec, vars, &templates_dir, base)
}

/// Dir-injected resolver honoring an optional `template_base`. Exists so
/// tests can exercise resolution against an isolated temp templates dir
/// without mutating HOME (see `resolve_eq` precedent).
fn resolve_file_content_with_dir(
    spec: &crate::config::FileSpec,
    vars: &HashMap<String, String>,
    templates_dir: &Path,
    base: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(src) = &spec.from {
        let rel = template_rel_with_base(base, src)?;
        return match read_user_template_with_dir(templates_dir, &rel) {
            Some(c) => Ok(Some(c)),
            None => Err(format!(
                "template file '{src}' not found in ~/.config/fa/templates/{rel}"
            )),
        };
    }
    if let Some(inline) = &spec.inline {
        return Ok(Some(inline.clone()));
    }
    if let Some(tpl) = &spec.template {
        let rel = template_rel_with_base(base, tpl)?;
        return match read_user_template_with_dir(templates_dir, &rel) {
            Some(content) => Ok(Some(substitute(&content, vars))),
            None => Err(format!(
                "template file '{tpl}' not found in ~/.config/fa/templates/{rel}"
            )),
        };
    }
    Ok(None)
}

/// Normalizes a `from`/`template` spec value to a path relative to the user
/// templates directory: a single leading `templates/` prefix is stripped when
/// present, otherwise the value is used as-is (e.g. `templates/my-recipe/f`
/// and `my-recipe/f` both → `my-recipe/f`). Absolute paths and `..` escapes
/// are rejected with a clear error (see [`Config::normalize_template_rel`]).
/// Normalizes a `from`/`template` spec value against an optional
/// `template_base` (see [`Config::resolve_rel`]): `None` is legacy
/// byte-identical; a `templates/`-prefixed spec always ignores the base.
fn template_rel_with_base(base: Option<&str>, spec: &str) -> Result<String, String> {
    Config::resolve_rel(base, spec).map_err(|e| e.to_string())
}

/// Controls how a spawned command's output is presented to the user.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    /// Stream stdout/stderr straight to the terminal.
    Inherit,
    /// Capture output; on failure only a short excerpt is shown.
    Captured,
}

/// Executes a shell command, forwarding stdout/stderr.
///
/// When the command runs without an interactive terminal (stdin is not a TTY),
/// `fa` auto-feeds `y\n` to stdin so package managers that prompt for approval
/// (e.g. pnpm's `minimumReleaseAge` continue prompt) do not hang or abort the
/// install. In an interactive terminal the user answers prompts normally.
pub fn run_shell(command: &str) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    run_shell_inner(command, !std::io::stdin().is_terminal(), OutputMode::Inherit, None)
}

/// Executes a shell command while showing an animated spinner with `label` on
/// stderr, so long-running installs don't look frozen. The spinner stops (and
/// its line is erased) before any error excerpt is printed.
pub fn run_shell_quiet_with_spinner(command: &str, label: &str) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    run_shell_inner(command, !std::io::stdin().is_terminal(), OutputMode::Captured, Some(label))
}

/// Number of captured output lines shown when a quiet command fails.
const ERROR_EXCERPT_LINES: usize = 8;

/// Prints the first lines of a failed quiet command's output (errors usually
/// land first on stderr), collapsing long dependency logs to a short excerpt.
fn print_error_excerpt(stdout: &[u8], stderr: &[u8]) {
    let stderr_text = String::from_utf8_lossy(stderr);
    let stdout_text = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = stderr_text
        .lines()
        .chain(stdout_text.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        println!("  {DIM}(no error output){RESET}");
        return;
    }
    println!(
        "  {BOLD_YELLOW}Failed command output (first {} lines):{RESET}",
        ERROR_EXCERPT_LINES.min(lines.len())
    );
    for line in lines.iter().take(ERROR_EXCERPT_LINES) {
        println!("  {DIM}{line}{RESET}");
    }
    let hidden = lines.len().saturating_sub(ERROR_EXCERPT_LINES);
    if hidden > 0 {
        println!("  {DIM}… {hidden} more lines hidden. Run the command manually for full output.{RESET}");
    }
}

fn run_shell_inner(
    command: &str,
    auto_answer: bool,
    mode: OutputMode,
    label: Option<&str>,
) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::Stdio;

    let captured = mode == OutputMode::Captured;
    let spinner = if captured { crate::spinner::Spinner::start(label) } else { None };

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(if auto_answer { Stdio::piped() } else { Stdio::inherit() })
        .stdout(if captured { Stdio::piped() } else { Stdio::inherit() })
        .stderr(if captured { Stdio::piped() } else { Stdio::inherit() })
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to execute '{command}': {e}"))?;

    if auto_answer
        && let Some(mut stdin) = child.stdin.take() {
            // Feed `y` answers continuously until the child closes the pipe
            // (i.e. it stops asking). Mirrors `yes | <command>`.
            std::thread::spawn(move || loop {
                if stdin.write_all(b"y\n").is_err() {
                    break;
                }
                let _ = stdin.flush();
                std::thread::sleep(std::time::Duration::from_millis(50));
            });
        }

    if captured {
        let output = child
            .wait_with_output()
            .map_err(|e| anyhow::anyhow!("Failed to execute '{command}': {e}"))?;
        if let Some(spinner) = spinner {
            spinner.stop();
        }
        if !output.status.success() {
            print_error_excerpt(&output.stdout, &output.stderr);
            anyhow::bail!("Command failed with exit code {}: {command}", output.status.code().unwrap_or(-1));
        }
        return Ok(());
    }

    let status = child
        .wait()
        .map_err(|e| anyhow::anyhow!("Failed to execute '{command}': {e}"))?;
    if !status.success() {
        anyhow::bail!("Command failed with exit code {}: {command}", status.code().unwrap_or(-1));
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
/// Name (bold) followed by a dimmed description to keep the line readable.
pub fn format_list_line(recipe_key: &str, recipe: &Recipe) -> String {
    let mut name = recipe_key.to_string();
    if !recipe.aliases.is_empty() {
        name.push_str(", ");
        name.push_str(&recipe.aliases.join(", "));
    }
    format!("{BOLD_BLUE}{name}{RESET} · {DIM_GRAY}{}{RESET}", recipe.description)
}

/// Formats a single-line list entry for an executable command.
/// Canonical name with its aliases (comma-separated), then the description.
pub fn format_command_line(command_key: &str, command: &crate::config::Command) -> String {
    let mut name = command_key.to_string();
    if !command.aliases.is_empty() {
        name.push_str(", ");
        name.push_str(&command.aliases.join(", "));
    }
    let description = command.description.as_deref().unwrap_or("");
    format!("{BOLD_BLUE}{name}{RESET} · {DIM_GRAY}{description}{RESET}")
}

/// Formats an alias section header for `fa list` (`<section>:`).
/// The name comes from the TOML `[aliases.<section>]` section at runtime;
/// an empty section name falls back to [`Config::FALLBACK_ALIAS_SECTION`].
pub fn format_section_header(section: &str) -> String {
    format!("  {}:", Config::display_section(section))
}

/// Returns `fa list` alias lines grouped by section: one `<section>:` header
/// per non-empty `[aliases.<section>]` section, followed by its single-line
/// command entries, with a blank line between groups for readability.
/// Iteration follows `BTreeMap` order, so output is deterministic. Recipes are
/// intentionally untouched.
pub fn format_alias_groups(config: &Config) -> Vec<String> {
    let mut lines = Vec::new();
    for (section, commands) in &config.aliases {
        if commands.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format_section_header(section));
        for (command_key, command) in commands {
            lines.push(format!("    {}", format_command_line(command_key, command)));
        }
    }
    lines
}

/// Returns a human-readable "supported" marker for a recipe on the current platform.
pub fn is_supported(_recipe: &Recipe) -> bool {
    true
}

/// Max interactive re-prompts after a validation failure before giving up.
pub const MAX_PROMPT_RETRIES: u32 = 3;

/// Collects template variables with optional typed validation.
///
/// - `dry_run`: fill defaults without prompting (never calls `prompt`);
///   invalid defaults still fail fast.
/// - `interactive` (TTY): prompt via `prompt`; a failed validation re-prompts
///   (bounded by [`MAX_PROMPT_RETRIES`]) instead of hanging forever.
/// - non-interactive: `FA_VAR_<KEY>` env override or default, never prompts,
///   invalid values fail fast with the variable name in the error.
///
/// Untyped variables behave exactly as before.
///
/// Validated values flow into `{{var}}` templating unchanged.
pub fn collect_validated_vars(
    recipe: &Recipe,
    inputs: &[String],
    vars: &mut HashMap<String, String>,
    dry_run: bool,
    interactive: bool,
    prompt: &mut dyn FnMut(&str, &str) -> String,
) -> anyhow::Result<()> {
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

    // Validates one collected value against its declared rules (if any).
    let check = |key: &str, value: &str| -> anyhow::Result<()> {
        if let Some(spec) = recipe.variables.get(key)
            && spec.is_typed() {
                spec.validate_value(key, value).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        Ok(())
    };

    if dry_run {
        for (k, d) in &defaults {
            check(k, d)?;
            vars.entry(k.clone()).or_insert_with(|| d.clone());
        }
        return Ok(());
    }

    for input in inputs {
        let unknowns = find_unknown_placeholders(input, vars);
        for key in unknowns {
            let label = prompts.get(&key).cloned().unwrap_or_else(|| format!("Value for {key}"));
            let default = defaults.get(&key).cloned().unwrap_or_default();
            if interactive {
                let mut attempts = 0;
                loop {
                    let answer = prompt(&label, &default);
                    let value = if answer.trim().is_empty() { default.clone() } else { answer };
                    match check(&key, &value) {
                        Ok(()) => {
                            vars.insert(key.clone(), value);
                            break;
                        }
                        Err(e) => {
                            attempts += 1;
                            if attempts > MAX_PROMPT_RETRIES {
                                anyhow::bail!("{e} (gave up after {MAX_PROMPT_RETRIES} retries)");
                            }
                            println!("{BOLD_YELLOW}Invalid value:{RESET} {e} — try again.");
                        }
                    }
                }
            } else {
                let env_key = format!("FA_VAR_{}", key.to_ascii_uppercase());
                let value = std::env::var(&env_key).unwrap_or(default);
                check(&key, &value)?;
                vars.insert(key, value);
            }
        }
    }
    Ok(())
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
    use crate::config::{CreateStep, Step};
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};

    /// Serializes cwd-mutating tests: `run_new` changes the process cwd, so the
    /// rollback tests must never run concurrently with each other.
    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Builds a throwaway config with a single inline-files recipe.
    fn test_config(files: &[(&str, &str)]) -> Config {
        let mut recipe_files = BTreeMap::new();
        for (dest, content) in files {
            recipe_files.insert(
                dest.to_string(),
                crate::config::FileSpec {
                    from: None,
                    inline: Some(content.to_string()),
                    template: None,
                    skip_if_exists: None,
                },
            );
        }
        let recipe = Recipe {
            name: "Test".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: recipe_files,
            variables: Default::default(),
            steps: vec![],
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let mut config = Config::default();
        config.recipes.insert("test".to_string(), recipe);
        config
    }

    fn test_options(project: &str) -> NewOptions {
        NewOptions {
            recipe_key: "test".to_string(),
            project_name: project.to_string(),
            variant: None,
            dry_run: false,
            no_install: true,
            pin_versions: false,
            component: None,
            pack: None,
        }
    }

    fn test_options_with_install(project: &str) -> NewOptions {
        NewOptions {
            recipe_key: "test".to_string(),
            project_name: project.to_string(),
            variant: None,
            dry_run: false,
            no_install: false,
            pin_versions: false,
            component: None,
            pack: None,
        }
    }

    /// Runs a closure inside a fresh unique temp directory, then cleans it up.
    /// Serialized against other cwd-mutating tests via `cwd_lock`.
    fn with_temp_dir(label: &str, f: impl FnOnce(&Path)) {
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "fa-test-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        f(&dir);
        std::env::set_current_dir(std::env::temp_dir()).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_should_remove_project_on_internal_failure() {
        println!("\n🔍 [TEST] Rollback — internal failure removes incomplete project");
        // A create command that builds the project dir, then a file spec with no
        // content source triggers an internal error before steps.
        let mut files = BTreeMap::new();
        files.insert(
            ".broken".to_string(),
            crate::config::FileSpec {
                from: None,
                inline: None,
                template: None,
                skip_if_exists: None,
            },
        );
        files.insert(
            ".keep".to_string(),
            crate::config::FileSpec {
                from: None,
                inline: Some("x".to_string()),
                template: None,
                skip_if_exists: None,
            },
        );
        let recipe = Recipe {
            name: "Test".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
            variants: vec![],
            create: Some(crate::config::Create {
                command: Some("mkdir {{name}}".to_string()),
                template_dir: None,
            }),
            pm: None,
            tooling: None,
            files,
            variables: Default::default(),
            steps: vec![],
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let mut config = Config::default();
        config.recipes.insert("test".to_string(), recipe);

        with_temp_dir("internal", |_dir| {
            let result = run_new(&config, &test_options("myapp"));
            assert!(result.result.is_err(), "Internal failure must error");
            assert!(
                !result.outcome.register,
                "Rolled-back project must not be registered"
            );
            assert!(
                !Path::new("myapp").exists(),
                "Incomplete project dir must be removed"
            );
            println!("   ✓ Incomplete project removed and not registered.\n");
        });
    }

    #[test]
    fn rollback_should_keep_project_when_step_fails() {
        println!("\n🔍 [TEST] Rollback — step failure keeps the project (Astro-style)");
        let mut config = test_config(&[(".editorconfig", "root = true")]);
        config.recipes.get_mut("test").unwrap().steps.push(Step {
            command: Some("false".to_string()),
            create: None,
            description: Some("Failing step".to_string()),
            platform: None,
            install: false,
        });

        with_temp_dir("step", |dir| {
            let result = run_new(&config, &test_options("myapp"));
            assert!(result.result.is_err(), "Step failure must error");
            assert!(
                result.outcome.register,
                "Kept project must be registered (installed: false)"
            );
            assert!(!result.outcome.installed, "installed must be false");
            // Without a create command, files are written to the cwd.
            assert!(
                dir.join(".editorconfig").exists(),
                "Project files must survive a failed step"
            );
            println!("   ✓ Project kept after step failure, registered as not installed.\n");
        });
    }

    #[test]
    fn rollback_should_never_remove_preexisting_dir() {
        println!("\n🔍 [TEST] Rollback — pre-existing directory is never removed");
        let mut config = test_config(&[(".editorconfig", "root = true")]);
        config.recipes.get_mut("test").unwrap().steps.push(Step {
            command: Some("false".to_string()),
            create: None,
            description: Some("Failing step".to_string()),
            platform: None,
            install: false,
        });

        with_temp_dir("preexisting", |dir| {
            fs::create_dir_all(dir.join("myapp")).unwrap();
            fs::write(dir.join("myapp/keep.txt"), "precious").unwrap();

            let result = run_new(&config, &test_options("myapp"));
            assert!(result.result.is_err());
            assert!(
                !result.outcome.register,
                "Pre-existing dir must not be claimed as a fa project"
            );
            assert!(
                dir.join("myapp/keep.txt").exists(),
                "Pre-existing content must survive"
            );
            println!("   ✓ Pre-existing directory and its content preserved.\n");
        });
    }

    #[test]
    fn success_should_register_project_as_installed() {
        println!("\n🔍 [TEST] Success — project registered as installed");
        let config = test_config(&[(".editorconfig", "root = true")]);
        with_temp_dir("success", |dir| {
            let result = run_new(&config, &test_options_with_install("myapp"));
            assert!(result.result.is_ok());
            assert!(result.outcome.register);
            assert!(result.outcome.installed);
            assert!(dir.join(".editorconfig").exists());
            println!("   ✓ Successful run registered and installed.\n");
        });
    }

    #[test]
    fn run_shell_should_auto_answer_confirmation_prompt() {
        println!("\n🔍 [TEST] Shell — auto-answers interactive confirmation prompts");
        // A command that reads a yes/no line and only succeeds on `y`, the way
        // pnpm's minimumReleaseAge prompt behaves. Without auto-answering it
        // would hang; with it, the piped `y\n` satisfies the read.
        let result = run_shell_inner(
            "read -r ans && [ \"$ans\" = y ]",
            true,
            OutputMode::Inherit,
            None,
        );
        assert!(result.is_ok(), "Auto-answered prompt should succeed: {result:?}");
        println!("   ✓ Confirmation prompt auto-answered with `y`.\n");
    }

    #[test]
    fn run_shell_should_report_failing_command() {
        println!("\n🔍 [TEST] Shell — failing command surfaces the exit status");
        let result = run_shell_inner("exit 3", true, OutputMode::Inherit, None);
        assert!(result.is_err(), "Non-zero exit must surface as an error");
        let msg = format!("{result:?}");
        assert!(msg.contains("3"), "Error should mention the exit status: {msg}");
        println!("   ✓ Failing command reported with exit status.\n");
    }

    #[test]
    fn run_shell_quiet_should_capture_output_and_still_report_failure() {
        println!("\n🔍 [TEST] Shell — quiet mode captures output and still reports failures");
        // Quiet mode must succeed on success and surface a failure the same way.
        let ok = run_shell_inner("echo hidden", true, OutputMode::Captured, None);
        assert!(ok.is_ok(), "Quiet success must not fail: {ok:?}");
        let err = run_shell_inner("exit 4", true, OutputMode::Captured, None);
        assert!(err.is_err(), "Quiet failure must surface as an error");
        let msg = format!("{err:?}");
        assert!(msg.contains("4"), "Quiet error should mention the exit status: {msg}");
        println!("   ✓ Quiet mode captured output and surfaced the failure.\n");
    }

    #[test]
    fn run_shell_quiet_with_spinner_should_succeed_and_surface_failures() {
        println!("\n🔍 [TEST] Shell — spinner variant succeeds on success and reports failures");
        let ok = run_shell_quiet_with_spinner("echo hidden", "Installing");
        assert!(ok.is_ok(), "Spinner success must not fail: {ok:?}");
        let err = run_shell_quiet_with_spinner("exit 5", "Installing");
        assert!(err.is_err(), "Spinner failure must surface as an error");
        let msg = format!("{err:?}");
        assert!(msg.contains("5"), "Error should mention the exit status: {msg}");
        println!("   ✓ Spinner variant succeeded and surfaced the failure.\n");
    }

    #[test]
    fn extract_commands_should_pull_first_token_of_each_segment() {
        let cmds = extract_commands("node --run build && pnpx wrangler pages deploy dist");
        assert_eq!(cmds, vec!["node".to_string(), "pnpx".to_string()]);
        println!("   ✓ Multi-command first tokens extracted in order.");

        let git = extract_commands("git init --quiet && git add -A && (git commit -q || git checkout -b main)");
        assert_eq!(git, vec!["git".to_string()]);
        println!("   ✓ Parenthesized/duplicated commands de-duplicated to one app.\n");
    }

    #[test]
    fn extract_commands_should_skip_shell_builtins_and_assignments() {
        let cmds = extract_commands("FOO=bar echo 'hi' ; cd src && git status");
        assert_eq!(cmds, vec!["git".to_string()]);
        println!("   ✓ Builtins (echo, cd) and env assignments excluded.\n");
    }

    #[test]
    fn missing_commands_should_report_only_absent_tools() {
        let missing = missing_commands(&[
            "git".to_string(),
            "fa_non_existent_tool_xyz".to_string(),
        ]);
        assert_eq!(missing, vec!["fa_non_existent_tool_xyz".to_string()]);
        println!("   ✓ Only the absent application is reported.\n");
    }

    #[test]
    fn preflight_should_pass_when_all_apps_exist() {
        let result = preflight("node --run build && pnpx wrangler pages deploy dist");
        assert!(result.is_ok(), "Present apps must pass preflight: {result:?}");
        println!("   ✓ Preflight passes when applications exist.\n");
    }

    #[test]
    fn expand_home_exact_path_and_non_tilde() {
        println!("\n🔍 [TEST] Expand Home Utility — Exact Path & Non-Tilde Preservation");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::var_os("HOME")
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "/tmp".to_string());
        let expected = std::path::PathBuf::from(&home).join("projects/app").to_string_lossy().to_string();

        let expanded = expand_home("~/projects/app");
        assert_eq!(expanded, expected, "Tilde path must expand to exact $HOME/projects/app");

        let absolute_non_tilde = expand_home("/usr/local/bin");
        assert_eq!(absolute_non_tilde, "/usr/local/bin", "Non-tilde paths must remain unchanged");
    }

    #[test]
    fn format_list_line_should_render_name_and_description() {
        let recipe = Recipe {
            name: "Demo".to_string(),
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
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let line = format_list_line("demo", &recipe);
        assert!(line.contains("demo"));
        assert!(line.contains("test"));
        assert!(!line.contains("[apply]"));
        assert!(!line.contains("pnpm / bun"));
        assert!(!line.contains("web · typescript"));
    }

    #[test]
    fn resolve_final_message_should_replace_single_brace_name() {
        let recipe = Recipe {
            name: "Go stack".to_string(),
            description: "Go stack".to_string(),
            aliases: vec![],
            language: None,
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables: Default::default(),
            steps: vec![],
            final_message: Some("cd {name} && go run .".to_string()),
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let vars = HashMap::new();
        let msg = resolve_final_message(&recipe, "my-go-tool", &vars);
        assert_eq!(msg, "cd my-go-tool && go run .");
        println!("   ✓ single-brace name placeholder replaced successfully.\n");
    }

    // ── Packs tests ──────────────────────────────────────────────────

    #[test]
    fn load_pack_should_read_pack_toml() {
        println!("\n🔍 [TEST] load_pack — reads pack definition from TOML");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-pack-test");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".config/fa/packs/wc-lib")).unwrap();

        let pack_path = home.join(".config/fa/packs/wc-lib/wc-ui.toml");
        fs::write(&pack_path, r#"name = "wc-ui"
components = ["toggle-theme", "btn-ally"]
"#).unwrap();

        let original_home = std::env::var_os("HOME");
        let env_home = home.to_string_lossy().to_string();
        unsafe { std::env::set_var("HOME", env_home); }

        // Invalidate cached HOME by re-reading config dir
        let pack = Config::load_pack("packs/wc-lib", "wc-ui");
        assert!(pack.is_ok(), "load_pack should succeed: {pack:?}");
        let pack = pack.unwrap();
        assert_eq!(pack.name, "wc-ui");
        assert_eq!(pack.components, vec!["toggle-theme", "btn-ally"]);

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Pack loaded correctly from TOML.\n");
    }

    #[test]
    fn load_pack_should_fail_for_missing_pack() {
        println!("\n🔍 [TEST] load_pack — fails for missing pack file");
        let result = Config::load_pack("packs/wc-lib", "nonexistent");
        assert!(result.is_err(), "load_pack for missing pack must error");
        println!("   ✓ Missing pack correctly returns error.\n");
    }

    #[test]
    fn load_pack_should_find_pack_by_name_when_filename_differs() {
        println!("\n🔍 [TEST] load_pack — finds pack by internal name when filename differs");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-pack-name-diff");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".config/fa/packs/wc-lib")).unwrap();

        // Note the filename has typo 'wc-toggle-them.toml' but inside name is 'wc-toggle-theme'
        let pack_path = home.join(".config/fa/packs/wc-lib/wc-toggle-them.toml");
        fs::write(
            &pack_path,
            r#"name = "wc-toggle-theme"
components = ["toggle-theme"]
"#,
        )
        .unwrap();

        let original_home = std::env::var_os("HOME");
        let env_home = home.to_string_lossy().to_string();
        unsafe { std::env::set_var("HOME", env_home); }

        let pack = Config::load_pack("packs/wc-lib", "wc-toggle-theme");
        assert!(pack.is_ok(), "load_pack should find pack by internal name even with typo in filename: {pack:?}");
        let pack = pack.unwrap();
        assert_eq!(pack.name, "wc-toggle-theme");
        assert_eq!(pack.components, vec!["toggle-theme"]);

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Pack loaded correctly by internal name match.\n");
    }

    #[test]
    fn resolve_components_should_pick_explicit_component() {
        println!("\n🔍 [TEST] resolve_components — explicit component wins");
        let recipe = Recipe {
            name: "wc-lib".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
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
            default_pack: Some("default".to_string()),
            packs: Default::default(),
        };
        let (components, desc) = resolve_components(&recipe, &Some("toggle-theme".to_string()), &None).unwrap();
        assert_eq!(components, vec!["toggle-theme"]);
        assert!(desc.contains("toggle-theme"));
        println!("   ✓ Explicit component resolved: {desc}\n");
    }

    #[test]
    fn resolve_components_should_error_on_missing_pack() {
        println!("\n🔍 [TEST] resolve_components — errors when explicit pack is not found");
        let recipe = Recipe {
            name: "wc-lib".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
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
            packs_dir: Some("packs/wc-lib".to_string()),
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let res = resolve_components(&recipe, &None, &Some("nonexistent-pack".to_string()));
        assert!(res.is_err(), "Must return error on missing pack");
        println!("   ✓ Missing pack correctly returns error.\n");
    }

    #[test]
    fn resolve_components_should_pick_explicit_pack() {
        println!("\n🔍 [TEST] resolve_components — explicit pack wins over default");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-pack-resolve");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".config/fa/packs/wc-lib")).unwrap();
        fs::write(
            home.join(".config/fa/packs/wc-lib/wc-ui.toml"),
            r#"name = "wc-ui"
components = ["toggle-theme", "btn-ally"]
"#,
        )
        .unwrap();
        let original_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.to_string_lossy().to_string()); }

        let recipe = Recipe {
            name: "wc-lib".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
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
            packs_dir: Some("packs/wc-lib".to_string()),
            templates_dir: None,
            default_pack: Some("default".to_string()),
            packs: Default::default(),
        };
        let (components, desc) =
            resolve_components(&recipe, &None, &Some("wc-ui".to_string())).unwrap();
        assert_eq!(components, vec!["toggle-theme", "btn-ally"]);
        assert!(desc.contains("wc-ui"));

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Explicit pack resolved: {desc}\n");
    }

    #[test]
    fn execute_create_step_should_copy_files() {
        println!("\n🔍 [TEST] execute_create_step — copies template files to destination");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-create-step");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".config/fa/templates/wc-lib/toggle-theme")).unwrap();
        fs::write(
            home.join(".config/fa/templates/wc-lib/toggle-theme/toggle-theme.astro"),
            "<button>Click</button>",
        )
        .unwrap();
        fs::write(
            home.join(".config/fa/templates/wc-lib/toggle-theme/toggle-theme.js"),
            "export default {}",
        )
        .unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&home).unwrap();
        let original_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.to_string_lossy().to_string()); }

        let create = CreateStep {
            from: "templates/wc-lib/toggle-theme".to_string(),
            to: "src/components/toggle-theme".to_string(),
        };
        let vars = HashMap::new();
        let result = execute_create_step(&create, &vars, "templates/wc-lib", false);
        assert!(result.is_ok(), "execute_create_step should succeed: {result:?}");

        let dest = home.join("src/components/toggle-theme");
        assert!(dest.join("toggle-theme.astro").exists(), "astro file must exist");
        assert!(dest.join("toggle-theme.js").exists(), "js file must exist");

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        std::env::set_current_dir(&original_cwd).unwrap();
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Files copied correctly to destination.\n");
    }

    #[test]
    fn execute_create_step_dry_run_should_not_copy() {
        println!("\n🔍 [TEST] execute_create_step — dry_run does not copy files");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-create-dry");
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(".config/fa/templates/wc-lib/toggle-theme")).unwrap();
        fs::write(
            home.join(".config/fa/templates/wc-lib/toggle-theme/toggle-theme.astro"),
            "<button>Click</button>",
        )
        .unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&home).unwrap();
        let original_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.to_string_lossy().to_string()); }

        let create = CreateStep {
            from: "templates/wc-lib/toggle-theme".to_string(),
            to: "src/components/toggle-theme".to_string(),
        };
        let vars = HashMap::new();
        let result = execute_create_step(&create, &vars, "templates/wc-lib", true);
        assert!(result.is_ok(), "dry_run should not error");

        let dest = home.join("src/components/toggle-theme");
        assert!(!dest.exists(), "destination should NOT exist in dry_run");

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        std::env::set_current_dir(&original_cwd).unwrap();
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Dry-run correctly skips file copy.\n");
    }

    #[test]
    fn execute_create_step_should_support_tilde_path_in_templates_dir() {
        println!("\n🔍 [TEST] execute_create_step — supports ~/ path in templates_dir");
        let _guard = cwd_lock().lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join("fa-create-tilde");
        let _ = fs::remove_dir_all(&home);
        // External template path: ~/work/web-component/toggle-theme
        fs::create_dir_all(home.join("work/web-component/toggle-theme")).unwrap();
        fs::write(
            home.join("work/web-component/toggle-theme/toggle-theme.astro"),
            "<button>Tilde Button</button>",
        )
        .unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&home).unwrap();
        let original_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.to_string_lossy().to_string()); }

        let create = CreateStep {
            from: "{{component}}".to_string(),
            to: "src/components/{{component}}".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("component".to_string(), "toggle-theme".to_string());
        let result = execute_create_step(&create, &vars, "~/work/web-component", false);
        assert!(result.is_ok(), "execute_create_step with ~/ in templates_dir should succeed: {result:?}");

        let dest = home.join("src/components/toggle-theme");
        assert!(dest.join("toggle-theme.astro").exists(), "astro file from external ~/ dir must exist");

        if let Some(h) = original_home {
            unsafe { std::env::set_var("HOME", h); }
        } else {
            unsafe { std::env::remove_var("HOME"); }
        }
        std::env::set_current_dir(&original_cwd).unwrap();
        let _ = fs::remove_dir_all(&home);
        println!("   ✓ Tilde path in templates_dir copied files correctly.\n");
    }

    #[test]
    fn execute_create_step_should_reject_root_path_with_slash() {
        println!("\n🔍 [TEST] execute_create_step — rejects root path with slash");
        let create = CreateStep {
            from: "toggle-theme".to_string(),
            to: "src/components".to_string(),
        };
        let vars = HashMap::new();
        let result = execute_create_step(&create, &vars, "/var/templates", false);
        assert!(result.is_err(), "Root path starting with / must be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Root paths starting with '/' are not allowed") || err_msg.contains("not allowed"), "error was: {err_msg}");
        println!("   ✓ Root path / correctly rejected: {err_msg}\n");
    }

    #[test]
    fn format_command_line_should_render_name_aliases_and_description() {
        let cmd = crate::config::Command {
            command: "node --run build".to_string(),
            description: Some("Build the project".to_string()),
            platform: None,
            aliases: vec!["fb".to_string(), "bld".to_string()],
        };

        let line = format_command_line("build", &cmd);
        assert!(line.contains("build"));
        assert!(line.contains("fb, bld"));
        assert!(line.contains("Build the project"));
    }

    #[test]
    fn alias_list_should_group_by_section() {
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

        let lines = format_alias_groups(&config);
        let plain: Vec<String> = lines
            .iter()
            .map(|l| {
                // Strip ANSI color codes for stable assertions.
                let mut s = l.clone();
                for code in [BOLD_BLUE, RESET, DIM_GRAY, BOLD_CYAN] {
                    s = s.replace(code, "");
                }
                s
            })
            .collect();

        assert_eq!(plain.len(), 6, "2 headers + 3 commands + 1 blank separator, got: {plain:?}");
        assert_eq!(plain[0], "  git:");
        assert!(
            plain[1].starts_with("    ") && plain[1].contains("gco"),
            "git group must contain indented gco, got: {:?}",
            plain[1]
        );
        assert!(
            plain[2].starts_with("    ") && plain[2].contains("status"),
            "git group must contain indented status, got: {:?}",
            plain[2]
        );
        assert_eq!(plain[3], "", "groups must be separated by a blank line, got: {plain:?}");
        assert_eq!(plain[4], "  sistema:");
        assert!(
            plain[5].starts_with("    ") && plain[5].contains("free"),
            "sistema group must contain indented free, got: {:?}",
            plain[5]
        );
        // Deterministic order: BTreeMap sorts sections, commands sort by key.
        assert!(plain.iter().position(|l| l == "  git:").unwrap()
            < plain.iter().position(|l| l == "  sistema:").unwrap());
    }

    #[test]
    fn template_rel_should_accept_with_or_without_prefix() {
        println!("\n🔍 [TEST] Engine template_rel — prefixed and prefixless normalize equally");
        let with = template_rel_with_base(None, "templates/my-recipe/file.txt").expect("prefixed must Ok");
        let without = template_rel_with_base(None, "my-recipe/file.txt").expect("prefixless must Ok");
        assert_eq!(with, "my-recipe/file.txt");
        assert_eq!(without, "my-recipe/file.txt");
        println!("   ✓ Both forms resolve to '{with}'.\n");
    }

    #[test]
    fn template_rel_should_reject_escape_with_clear_error() {
        println!("\n🔍 [TEST] Engine template_rel — traversal/absolute rejected clearly");
        for bad in ["../evil.txt", "templates/../evil.txt", "/abs/x.txt"] {
            let err = template_rel_with_base(None, bad).expect_err(&format!("'{bad}' must be rejected"));
            assert!(
                err.contains("must stay inside ~/.config/fa/templates/"),
                "Error must name the templates dir, got: '{err}'"
            );
        }
        println!("   ✓ Out-of-range paths rejected (not silent).\n");
    }

    #[test]
    fn resolve_file_content_should_resolve_with_and_without_prefix_equally() {
        println!("\n🔍 [TEST] Engine resolve — `from` with/without prefix reads the same file");
        // No HOME mutation (unsafe is forbidden): exercise the dir-injected
        // resolver against an isolated temp templates dir.
        let base = std::env::temp_dir().join(format!(
            "fa-test-tpl-{}-{}",
            std::process::id(),
            "resolve-eq"
        ));
        let tpl_dir = base.join("my-recipe");
        fs::create_dir_all(&tpl_dir).unwrap();
        fs::write(tpl_dir.join("file.txt"), "hello").unwrap();
        fs::write(tpl_dir.join("greet.txt"), "hi {{name}}").unwrap();

        let vars: HashMap<String, String> =
            [("name".to_string(), "world".to_string())].into_iter().collect();
        let run = |spec: crate::config::FileSpec| {
            resolve_file_content_with_dir(&spec, &vars, &base, None)
        };

        let prefixed = run(crate::config::FileSpec {
            from: Some("templates/my-recipe/file.txt".to_string()),
            inline: None,
            template: None,
            skip_if_exists: None,
        });
        let bare = run(crate::config::FileSpec {
            from: Some("my-recipe/file.txt".to_string()),
            inline: None,
            template: None,
            skip_if_exists: None,
        });
        assert_eq!(prefixed, Ok(Some("hello".to_string())), "prefixed `from` must read");
        assert_eq!(bare, Ok(Some("hello".to_string())), "prefixless `from` must read equally");

        let tpl_bare = run(crate::config::FileSpec {
            from: None,
            inline: None,
            template: Some("my-recipe/greet.txt".to_string()),
            skip_if_exists: None,
        });
        assert_eq!(tpl_bare, Ok(Some("hi world".to_string())), "prefixless `template` must substitute");

        let _ = fs::remove_dir_all(&base);
        println!("   ✓ Prefixless `from`/`template` resolve exactly like prefixed.\n");
    }

    #[test]
    fn resolve_file_content_should_reject_escape_with_clear_error() {
        println!("\n🔍 [TEST] Engine resolve — escape gives clear error, empty stays silent-source");
        let vars: HashMap<String, String> = HashMap::new();
        let evil = resolve_file_content(
            &crate::config::FileSpec {
                from: Some("../evil.txt".to_string()),
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            None,
        );
        let msg = evil.expect_err("traversal must be Err, not silent None");
        assert!(
            msg.contains("must stay inside ~/.config/fa/templates/"),
            "Escape must name the templates dir, got: '{msg}'"
        );
        let empty = resolve_file_content(
            &crate::config::FileSpec {
                from: None,
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            None,
        );
        assert_eq!(empty, Ok(None), "genuinely sourceless spec stays Ok(None)");
        println!("   ✓ Traversal errors clearly; empty spec keeps legacy Ok(None).\n");
    }

    #[test]
    fn resolve_with_base_should_join_from_under_base() {
        println!("\n🔍 [TEST] Engine resolve — base + from joins under base");
        let base = std::env::temp_dir().join(format!(
            "fa-test-tpl-{}-{}",
            std::process::id(),
            "base-join"
        ));
        let stack_dir = base.join("rust-stack");
        fs::create_dir_all(&stack_dir).unwrap();
        fs::write(stack_dir.join(".gitignore"), "target/").unwrap();

        let vars: HashMap<String, String> = HashMap::new();
        let got = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: Some(".gitignore".to_string()),
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            &base,
            Some("rust-stack"),
        );
        assert_eq!(got, Ok(Some("target/".to_string())), "base + from must join");
        let _ = fs::remove_dir_all(&base);
        println!("   ✓ base + from joined correctly.\n");
    }

    #[test]
    fn resolve_with_base_should_ignore_base_for_prefixed_legacy() {
        println!("\n🔍 [TEST] Engine resolve — `templates/` spec ignores base (legacy)");
        let base = std::env::temp_dir().join(format!(
            "fa-test-tpl-{}-{}",
            std::process::id(),
            "base-legacy"
        ));
        let other_dir = base.join("other");
        fs::create_dir_all(&other_dir).unwrap();
        fs::write(other_dir.join("file.txt"), "legacy").unwrap();

        let vars: HashMap<String, String> = HashMap::new();
        let with_base = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: Some("templates/other/file.txt".to_string()),
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            &base,
            Some("rust-stack"),
        );
        let without_base = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: Some("templates/other/file.txt".to_string()),
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            &base,
            None,
        );
        assert_eq!(with_base, Ok(Some("legacy".to_string())));
        assert_eq!(with_base, without_base, "prefixed spec must ignore base");
        let _ = fs::remove_dir_all(&base);
        println!("   ✓ Prefixed spec identical with/without base.\n");
    }

    #[test]
    fn resolve_with_base_should_ignore_base_for_inline() {
        println!("\n🔍 [TEST] Engine resolve — inline ignores base");
        let vars: HashMap<String, String> = HashMap::new();
        let dir = std::env::temp_dir();
        let got = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: None,
                inline: Some("verbatim".to_string()),
                template: None,
                skip_if_exists: None,
            },
            &vars,
            &dir,
            Some("rust-stack"),
        );
        assert_eq!(got, Ok(Some("verbatim".to_string())), "inline must ignore base");
        println!("   ✓ inline ignores base.\n");
    }

    #[test]
    fn resolve_with_base_should_apply_base_to_template_equally() {
        println!("\n🔍 [TEST] Engine resolve — template applies base + substitution");
        let base = std::env::temp_dir().join(format!(
            "fa-test-tpl-{}-{}",
            std::process::id(),
            "base-tpl"
        ));
        let stack_dir = base.join("rust-stack");
        fs::create_dir_all(&stack_dir).unwrap();
        fs::write(stack_dir.join("greet.txt"), "hi {{name}}").unwrap();

        let vars: HashMap<String, String> =
            [("name".to_string(), "world".to_string())].into_iter().collect();
        let got = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: None,
                inline: None,
                template: Some("greet.txt".to_string()),
                skip_if_exists: None,
            },
            &vars,
            &base,
            Some("rust-stack"),
        );
        assert_eq!(got, Ok(Some("hi world".to_string())), "template + base must substitute");
        let _ = fs::remove_dir_all(&base);
        println!("   ✓ template + base substituted correctly.\n");
    }

    #[test]
    fn resolve_with_base_should_reject_escapes_with_clear_error() {
        println!("\n🔍 [TEST] Engine resolve — base escapes rejected clearly");
        let vars: HashMap<String, String> = HashMap::new();
        let dir = std::env::temp_dir();
        let evil = resolve_file_content_with_dir(
            &crate::config::FileSpec {
                from: Some("../evil.txt".to_string()),
                inline: None,
                template: None,
                skip_if_exists: None,
            },
            &vars,
            &dir,
            Some("rust-stack"),
        );
        let msg = evil.expect_err("traversal with base must be Err");
        assert!(
            msg.contains("must stay inside ~/.config/fa/templates/"),
            "Escape must name the templates dir, got: '{msg}'"
        );
        println!("   ✓ Base escape rejected clearly.\n");
    }

    /// Builds a recipe with a single typed variable for validation tests.
    fn typed_test_recipe(var: crate::config::Variable) -> Recipe {
        let mut variables = BTreeMap::new();
        variables.insert("port".to_string(), var);
        Recipe {
            name: "Test".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables,
            steps: vec![],
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        }
    }

    fn int_port_var(default: &str) -> crate::config::Variable {
        crate::config::Variable {
            prompt: "Port".to_string(),
            default: Some(default.to_string()),
            var_type: Some("integer".to_string()),
            choices: None,
            pattern: None,
            required: None,
        }
    }

    #[test]
    fn non_interactive_should_fail_fast_on_invalid_default_without_prompting() {
        let recipe = typed_test_recipe(int_port_var("abc"));
        let inputs = vec!["listen {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut calls = 0;
        let mut prompt = |_: &str, _: &str| {
            calls += 1;
            "unreachable".to_string()
        };
        let err = collect_validated_vars(&recipe, &inputs, &mut vars, false, false, &mut prompt)
            .expect_err("invalid default must fail fast off-TTY");
        assert_eq!(calls, 0, "non-interactive path must never prompt (no hang)");
        assert!(err.to_string().contains("port"), "error must name the variable, got: '{err}'");
    }

    #[test]
    fn non_interactive_should_accept_valid_default_silently() {
        let recipe = typed_test_recipe(int_port_var("8080"));
        let inputs = vec!["listen {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut prompt = |_: &str, _: &str| panic!("must not prompt off-TTY");
        collect_validated_vars(&recipe, &inputs, &mut vars, false, false, &mut prompt)
            .expect("valid default must pass");
        assert_eq!(vars.get("port"), Some(&"8080".to_string()));
    }

    #[test]
    fn interactive_should_reprompt_on_invalid_then_accept_valid() {
        let recipe = typed_test_recipe(crate::config::Variable {
            prompt: "PM".to_string(),
            default: None,
            var_type: None,
            choices: Some(vec!["pnpm".to_string(), "bun".to_string()]),
            pattern: None,
            required: None,
        });
        let inputs = vec!["install via {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut answers = vec!["npm".to_string(), "pnpm".to_string()].into_iter();
        let mut calls = 0;
        let mut prompt = |_: &str, _: &str| {
            calls += 1;
            answers.next().unwrap()
        };
        collect_validated_vars(&recipe, &inputs, &mut vars, false, true, &mut prompt)
            .expect("second valid answer must be accepted");
        assert_eq!(calls, 2, "one rejection + one acceptance");
        assert_eq!(vars.get("port"), Some(&"pnpm".to_string()));
    }

    #[test]
    fn interactive_should_give_up_after_max_retries() {
        let recipe = typed_test_recipe(int_port_var(""));
        let inputs = vec!["listen {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut calls = 0;
        let mut prompt = |_: &str, _: &str| {
            calls += 1;
            "abc".to_string()
        };
        let err = collect_validated_vars(&recipe, &inputs, &mut vars, false, true, &mut prompt)
            .expect_err("persistent invalid input must error, not hang");
        assert_eq!(calls, MAX_PROMPT_RETRIES as usize + 1, "bounded retries, no infinite hang");
        assert!(err.to_string().contains("port"), "got: '{err}'");
    }

    #[test]
    fn dry_run_should_validate_defaults_without_prompting() {
        let recipe = typed_test_recipe(int_port_var("abc"));
        let inputs = vec!["listen {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut prompt = |_: &str, _: &str| panic!("dry-run must not prompt");
        let err = collect_validated_vars(&recipe, &inputs, &mut vars, true, false, &mut prompt)
            .expect_err("invalid default must fail even in dry-run");
        assert!(err.to_string().contains("port"), "got: '{err}'");
    }

    #[test]
    fn interactive_empty_answer_should_fall_back_to_default() {
        let recipe = typed_test_recipe(int_port_var("3000"));
        let inputs = vec!["listen {{port}}".to_string()];
        let mut vars = HashMap::new();
        let mut prompt = |_: &str, _: &str| String::new();
        collect_validated_vars(&recipe, &inputs, &mut vars, false, true, &mut prompt)
            .expect("empty answer must fall back to default");
        assert_eq!(vars.get("port"), Some(&"3000".to_string()));
    }

    #[test]
    fn untyped_variable_should_behave_exactly_as_before() {
        let mut variables = BTreeMap::new();
        variables.insert(
            "name".to_string(),
            crate::config::Variable {
                prompt: "Project name".to_string(),
                default: Some("app".to_string()),
                var_type: None,
                choices: None,
                pattern: None,
                required: None,
            },
        );
        let recipe = Recipe {
            name: "Test".to_string(),
            description: "test".to_string(),
            language: None,
            aliases: vec![],
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables,
            steps: vec![],
            final_message: None,
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let inputs = vec!["create {{name}}".to_string()];
        let mut vars = HashMap::new();
        let mut prompt = |_: &str, _: &str| panic!("must not prompt off-TTY");
        collect_validated_vars(&recipe, &inputs, &mut vars, false, false, &mut prompt)
            .expect("untyped default must pass untouched");
        assert_eq!(vars.get("name"), Some(&"app".to_string()));
    }

    #[test]
    fn resolve_final_message_should_default_to_cd_project_name() {
        println!("\n🔍 [TEST] Final Message — defaults to cd project-name");
        let recipe = Recipe {
            name: "Rust CLI".to_string(),
            description: "Rust stack".to_string(),
            aliases: vec![],
            language: Some("rust".to_string()),
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
        };
        let vars = HashMap::new();
        let msg = resolve_final_message(&recipe, "my-app", &vars);
        assert_eq!(
            msg,
            format!("Run `{DIM}cd my-app{RESET}` to go to project."),
            "Default message must instruct to cd to project"
        );
        println!("   ✓ Default navigation hint matches 'Run cd <name> to go to project'.\n");
    }

    #[test]
    fn resolve_final_message_should_use_custom_recipe_message() {
        println!("\n🔍 [TEST] Final Message — uses custom message when specified");
        let recipe = Recipe {
            name: "Custom Stack".to_string(),
            description: "Custom stack".to_string(),
            aliases: vec![],
            language: None,
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables: Default::default(),
            steps: vec![],
            final_message: Some("Run `cd {{name}} && cargo run` to start.".to_string()),
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "my-app".to_string());
        let msg = resolve_final_message(&recipe, "my-app", &vars);
        assert_eq!(
            msg,
            "Run `cd my-app && cargo run` to start.",
            "Custom final_message must override default and substitute {{name}}"
        );
        println!("   ✓ Custom message respected and {{name}} substituted.\n");
    }

    #[test]
    fn resolve_final_message_should_support_brace_name_fallbacks() {
        println!("\n🔍 [TEST] Final Message — supports {{name}} and {{project_name}} placeholders");
        let recipe = Recipe {
            name: "Go Stack".to_string(),
            description: "Go stack".to_string(),
            aliases: vec![],
            language: None,
            variants: vec![],
            create: None,
            pm: None,
            tooling: None,
            files: Default::default(),
            variables: Default::default(),
            steps: vec![],
            final_message: Some("cd {name} && go run .".to_string()),
            pin_versions: None,
            template_base: None,
            packs_dir: None,
            templates_dir: None,
            default_pack: None,
            packs: Default::default(),
        };
        let vars = HashMap::new();
        let msg = resolve_final_message(&recipe, "my-go-tool", &vars);
        assert_eq!(msg, "cd my-go-tool && go run .");
        println!("   ✓ single-brace name placeholder replaced successfully.\n");
    }

    #[test]
    fn format_recipe_packs_and_components_should_render_details() {
        let recipe = Recipe {
            name: "wc-lib".to_string(),
            description: "Modular components".to_string(),
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
            packs_dir: Some("packs/wc-lib".to_string()),
            templates_dir: None,
            default_pack: Some("toggle-theme".to_string()),
            packs: Default::default(),
        };
        let out = format_recipe_packs_and_components(&recipe, "wc-lib");
        assert!(out.contains("[Recipe] wc-lib"));
        assert!(out.contains("Modular components"));
        assert!(out.contains("Available packs for 'wc-lib'"));
        assert!(out.contains("Default pack:"));
        assert!(out.contains("toggle-theme"));
        assert!(out.contains("fa new wc-lib <pack>"));
    }
}
