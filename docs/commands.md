# CLI Reference — fa

`fa` provides a simple, safety-first command-line interface to scaffold projects, apply tooling, and manage recipe catalogs across **Debian** and **Termux**.

## Overview of Commands

| Command | Shorthand | Purpose | Key Flags |
| ------- | --------- | ------- | --------- |
| `fa new <recipe> <name>` | `fa -n` | Scaffold a project into `<name>` | `-v, --variant <name>`, `-d, --dry-run`, `--no-install` |
| `fa list` | `fa -l` | List available recipes and grouped aliases | `-s, --show-hidden` |
| `fa search <query>` | — | Search recipes and aliases by keyword | (case-insensitive) |
| `fa show <target>` | — | Show full details of a recipe or alias | Accepts recipe or alias name |
| `fa alias <name> [args...]` | `fa -a`, `fa <name>` | Run an alias defined in `[aliases]` | Passthrough or positional arguments |
| `fa self-update` | — | Update `fa` binary to the latest GitHub release | `-d, --dry-run` |
| `fa self-uninstall` | — | Remove `fa` binary, state, and config | `-y, --yes`, `-n, --no`, `-d, --dry-run` |
| `fa completions <shell>` | — | Print shell completion script to stdout | `bash`, `zsh`, `fish`, `powershell`, `elvish` |
| `fa --version` | `fa -v` | Display version and check for updates | — |
| `fa --help` | `fa -h` | Display CLI help | — |

---

## Commands

### 1. `fa new <recipe> <name>`
Scaffolds a new project directory using the specified recipe.

```bash
fa new next-ts myapp               # Default variant
fa new next-ts myapp -v bun        # Specific variant
fa new next-ts myapp --dry-run     # Preview actions without creating files
fa new next-ts myapp --no-install  # Write files but skip package installation
fa -n next-ts myapp                # Short-flag shorthand
```

### 2. `fa list` & `fa search <query>`
Inspect available recipes and command aliases.

```bash
fa list               # List supported recipes and command aliases
fa list -s            # Include unsupported recipes on current platform
fa search typescript  # Search by name, language, tag, or description
```

### 3. `fa show <target>`
Displays detailed recipe configuration (variants, tooling, scaffolded files, and execution steps) or command alias definitions.

```bash
fa show next-ts
fa show gco
```

### 4. `fa alias <name> [args...]`
Executes a command alias configured under `[aliases]` in `recipes.toml`. Aliases can also be invoked directly (`fa <name>`).

```bash
fa alias gco main       # Explicit subcommand
fa gco main             # Direct invocation shorthand
fa -a free              # Short flag
```

#### Argument Substitution & Passthrough
Command aliases support dynamic arguments defined in `recipes.toml`:

- **Positional variables:** `$1`, `$2` or `{{1}}`, `{{2}}` are replaced by arguments and shell-quoted.
- **Default fallbacks:** `${1:-fallback}` or `{{1:-fallback}}` apply when the argument is omitted.
- **Passthrough:** If no positional placeholders are defined, trailing arguments are automatically shell-quoted and appended.

```toml
# Example definitions in recipes.toml
[aliases.images]
avif = { command = "avifenc -s 0 -q ${2:-50} $1 -o ${1%.*}.avif", description = "Convert image" }

[aliases.git]
gco = { command = "git checkout", description = "Switch branch" } # Passthrough: fa gco main -> git checkout 'main'
```

---

## Features & Utilities

### Dependency Preflight
Before running any recipe step or alias command, `fa` inspects the shell command and verifies all required binaries exist on `PATH`. If a tool is missing, execution halts immediately with a clear installation hint.

### Version & Background Update Check
`fa --version` prints the active version immediately. It reads cached release data from `~/.local/state/fa/state.toml` and displays an update hint if a newer release exists. It spawns a non-blocking background check so the CLI never hangs on network requests.

### `fa self-update`
Downloads and replaces the current binary with the latest release from GitHub Releases.

```bash
fa self-update          # Update in-place
fa self-update --dry-run # Check latest release asset without modifying binary
```

### `fa self-uninstall`
Uninstalls the `fa` binary and prompts to clean up configuration (`~/.config/fa`) and state (`~/.local/state/fa`) directories.

```bash
fa self-uninstall            # Interactive prompt for config/state
fa self-uninstall --yes      # Non-interactive, removes binary and config/state
fa self-uninstall --no       # Removes binary only, preserves config/state
fa self-uninstall --dry-run  # Preview paths targeted for deletion
```

### Shell Completions
Prints the completion script for the given shell to stdout (covers all subcommands and flags):

```bash
fa completions bash > ~/.local/share/bash-completion/completions/fa
eval "$(fa completions zsh)"            # or save to ${fpath} for lazy loading
fa completions fish > ~/.config/fish/completions/fa.fish
```

### Bootstrap Installation Script
Installs `fa` on a fresh machine (Debian or Termux) without requiring Rust or Cargo:

```bash
curl -sSL https://raw.githubusercontent.com/<user>/fast-alias/develop/install.sh | sh
```

### Environment Variables
- `FAST_ALIAS_REPO`: Override target GitHub repository (`owner/repo`) for updates.
- `GITHUB_TOKEN`: GitHub personal access token used to avoid API rate limits when checking releases.
