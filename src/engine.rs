use crate::config::{Config, Recipe};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::colors::*;
use crate::platform::Platform;
use crate::templating::{resolve_all, substitute, substitute_shell};

#[derive(Debug, Clone)]
pub struct NewOptions {
    pub recipe_key: String,
    pub project_name: String,
    pub variant: Option<String>,
    pub dry_run: bool,
    pub no_install: bool,
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

// Reads a template file from the user templates directory
// (`~/.config/fa/templates/<rel>`), keyed by its path relative to `templates/`
// (e.g. `my-recipe/Layout.tsx`). Templates are user-provided files on disk, so
// adding a template never requires editing Rust code.
fn read_user_template(rel: &str) -> Option<String> {
    let templates_dir = Config::get_user_templates_dir()?;
    let path = templates_dir.join(rel);
    fs::read_to_string(&path).ok()
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

            let content = resolve_file_content(spec, &vars);
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
        // The skeleton (create + files) is complete; from here on, failures are
        // recoverable (e.g. dependency installation) and must keep the project.
        scaffold_ok = true;

        // 3. Steps
        if !recipe.steps.is_empty() {
            println!("\nSteps:");
            for step in &recipe.steps {
                if let Some(platform) = &step.platform
                    && platform != "all" && !platform_matches(platform)? {
                        continue;
                    }
                if step.install && opts.no_install {
                    println!("  {BOLD_YELLOW}[SKIP]{RESET} {} (--no-install)", step.description.as_deref().unwrap_or(&step.command));
                    continue;
                }
                let rendered = substitute_shell(&step.command, &vars);
                if opts.dry_run {
                    println!("  {DIM}[Dry-Run]{RESET} Would run: {rendered}");
                } else {
                    let label = step.description.as_deref().unwrap_or(&step.command);
                    if step.install {
                        run_shell_quiet_with_spinner(&rendered, label)?;
                        println!("  {BOLD_GREEN}✓{RESET} {label}");
                    } else {
                        println!("  {BOLD_GREEN}✓{RESET} {label}");
                        run_shell(&rendered)?;
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

    println!("\n{BOLD_GREEN}Project '{}' created successfully.{RESET}", opts.project_name);
    if let Some(msg) = &recipe.final_message {
        println!("{BOLD_CYAN}[Note]{RESET} {msg}");
    }
    if !opts.no_install {
        println!("Run `{DIM}cd {}{RESET} && {DIM}node --run dev{RESET}` to start developing.", opts.project_name);
    }
    NewResult {
        outcome: NewOutcome {
            project_dir: opts.project_name.clone(),
            installed: !opts.no_install,
            register: !opts.dry_run,
        },
        result: Ok(()),
    }
}

/// Resolves the content of a file spec (from / inline / template).
fn resolve_file_content(
    spec: &crate::config::FileSpec,
    vars: &HashMap<String, String>,
) -> Option<String> {
    if let Some(src) = &spec.from {
        let rel = template_rel(src)?;
        return read_user_template(&rel);
    }
    if let Some(inline) = &spec.inline {
        return Some(inline.clone());
    }
    if let Some(tpl) = &spec.template {
        let rel = template_rel(tpl)?;
        if let Some(content) = read_user_template(&rel) {
            return Some(substitute(&content, vars));
        }
    }
    None
}

/// Strips the `templates/` prefix from a `from`/`template` spec value so it can
/// be used as a relative path into the user templates directory (e.g.
/// `templates/my-recipe/Layout.tsx` → `my-recipe/Layout.tsx`).
fn template_rel(spec: &str) -> Option<String> {
    spec.strip_prefix("templates/").map(|s| s.to_string())
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
    format!("{BOLD_BLUE}{recipe_key}{RESET} · {DIM_GRAY}{}{RESET}", recipe.description)
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
    use crate::config::Step;
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
            commands: Default::default(),
            steps: vec![],
            final_message: None,
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
        }
    }

    fn test_options_with_install(project: &str) -> NewOptions {
        NewOptions {
            recipe_key: "test".to_string(),
            project_name: project.to_string(),
            variant: None,
            dry_run: false,
            no_install: false,
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
            commands: Default::default(),
            steps: vec![],
            final_message: None,
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
            command: "false".to_string(),
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
            command: "false".to_string(),
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
    fn expand_home_utility_should_expand_tilde_paths() {
        println!("\n🔍 [TEST] Expand Home Utility");
        println!("   Explanation: Verifies path expansion from tilde paths to absolute user paths.");

        let home = std::env::var_os("HOME").map(|h| h.to_string_lossy().to_string()).unwrap();
        let expanded = expand_home("~/projects/app");
        assert!(expanded.starts_with(&home), "Should expand to user home, got: {expanded}");
        println!("   ✓ Tilde expanded to: {expanded}\n");
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
            commands: Default::default(),
            steps: vec![],
            final_message: None,
        };
        let line = format_list_line("demo", &recipe);
        assert!(line.contains("demo"));
        assert!(line.contains("test"));
        assert!(!line.contains("[apply]"));
        assert!(!line.contains("pnpm / bun"));
        assert!(!line.contains("web · typescript"));
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
}