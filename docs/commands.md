# CLI Reference — fa

`fa` provides a simple, safety-first command-line interface to scaffold projects, apply tooling, and manage recipe catalogs across **Debian** and **Termux**.

## Overview of Commands

| Command (Long) | Shorthand | Purpose | Key Flags |
| -------------- | --------- | ------- | --------- |
| `fa --new <recipe> <name>` | `fa -n` | Scaffold a project into `<name>` | `-v, --variant <name>`, `-d, --dry-run`, `--no-install` |
| `fa --list [category]` | `fa -l` | List available recipes, aliases, or packs | `-r, --recipes`, `-a, --aliases`, `-p, --packs`, `-s, --show-hidden` |
| `fa --search <query>` | `fa -se` | Search recipes and aliases by keyword | (case-insensitive) |
| `fa --show <target>` | `fa -sh` | Show full details of a recipe or alias | Accepts recipe or alias name |
| `fa --alias <name> [args...]` | `fa -a`, `fa <name>` | Run an alias defined in `[aliases]` | Passthrough or positional arguments |
| `fa --recipe <action>` | `fa -r` | Manage recipe files (`new`, `edit`, `validate`) | Subcommands: `new`, `edit`, `validate` |
| `fa --template add <recipe> <paths...>` | `fa -t` | Add files or folders into a recipe's template directory | `-f, --force` |
| `fa --self-update` | — | Update `fa` binary to the latest GitHub release | `-d, --dry-run` |
| `fa --self-uninstall` | — | Remove `fa` binary, state, and config | `-y, --yes`, `-n, --no`, `-d, --dry-run` |
| `fa --version` | `fa -v` | Display version and check for updates | — |
| `fa --help` | `fa -h` | Display CLI help | — |

> **Style note:** Examples below use the long form (`fa --new`, `fa --list`) for educational clarity. For faster terminal typing, use short flags (`fa -n`, `fa -l`, `fa -se`, `fa -sh`, `fa -r`, `fa -t`, `fa -a`). Native commands always start with `--` or `-`, reserving all un-prefixed words exclusively for user aliases to eliminate name collisions.

---

## Commands

### 1. `fa --new <recipe> <name>`
Scaffolds a new project directory using the specified recipe.

```bash
fa --new next-ts myapp               # Default variant
fa --new next-ts myapp -v bun        # Specific variant
fa --new next-ts myapp --dry-run     # Preview actions without creating files
fa --new next-ts myapp --no-install  # Write files but skip package installation
fa -n next-ts myapp                  # Ultra-compact shorthand
```

### 2. `fa --list [category]` & `fa --search <query>`
Inspect available recipes, command aliases, and component packs.

```bash
# Default: lists recipes and command aliases
fa --list                            # or: fa -l

# Category filtering via flags:
fa --list -r                         # or: fa -l -r (recipes only)
fa --list -a                         # or: fa -l -a (aliases only)
fa --list -p                         # or: fa -l -p (packs only)
fa --list -r -p                      # combine flags (recipes and packs)

# Category filtering via positional arguments:
fa --list recipes                    # or: fa -l recipes
fa --list aliases                    # or: fa -l aliases
fa --list packs                      # or: fa -l packs

# Include unsupported recipes on current platform:
fa --list -s                         # or: fa -l -s

# Search recipes and aliases by keyword:
fa --search typescript               # or: fa -se typescript
```

### 3. `fa --show <target>`
Displays detailed recipe configuration (variants, tooling, scaffolded files, and execution steps) or command alias definitions.

```bash
fa --show next-ts                    # or: fa -sh next-ts
fa --show gco                        # or: fa -sh gco
```

### 4. `fa --alias <name> [args...]`
Executes a command alias configured under `[aliases]` in `recipes.toml`. Aliases can also be invoked directly (`fa <name>`).

```bash
fa --alias gco main                  # Explicit canonical command
fa -a free                           # Short flag
fa gco main                          # Direct invocation shorthand
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

#### Namespaced Command Groups (`[aliases.":<namespace>"]`)
Sections starting with `:` act as isolated command namespaces. Their subcommands are called with multi-word syntax (`fa <namespace> <command>`) and do not pollute the global root namespace:

```toml
# Declared in recipes.toml or recipes.d/skills.toml:
[aliases.":skills"]
skills = { command = "tabernaculo status", description = "Overview status" }
ls     = { command = "bunx tabernaculo list", description = "List installed skills" }
add    = { command = "bunx tabernaculo add {{1}}", description = "Install a skill" }

[aliases.":docker"]
up   = { command = "docker compose up -d", description = "Start containers" }
down = { command = "docker compose down", description = "Stop containers" }
```

```bash
# Invoking namespaced subcommands:
fa skills ls                          # Runs: bunx tabernaculo list (or: fa :skills ls)
fa skills add github                  # Runs: bunx tabernaculo add 'github'
fa docker up                          # Runs: docker compose up -d

# Root command execution vs Help listing:
fa skills                             # Runs 'tabernaculo status' because root 'skills' command exists
fa docker                             # Displays subcommands (up, down) because no root command exists
fa docker help                        # Displays subcommands for the namespace (or: fa docker --help)

# Strict isolation:
fa ls                                 # Does NOT execute 'skills ls'; stays isolated within its namespace
```

### 5. `fa --template add <recipe> <path>...`
Copies files or directories recursively into the template directory of a recipe (`~/.config/fa/templates/<recipe>/`, honoring `template_base` or `templates_dir` if defined).

```bash
fa --template add wc-lib button.astro                 # Copy single file (or: fa -t add ...)
fa --template add wc-lib src/components/toggle-theme/ # Copy folder recursively
fa --template add wc-lib file1.ts folder2/ -f         # Force overwrite without prompting
```

### 6. `fa --recipe <action>`
Manages recipe configuration files under `~/.config/fa/recipes.d/`.

```bash
fa --recipe new my-app                 # Scaffold a clean minimal recipe (or: fa -r new ...)
fa --recipe new my-app standard        # Scaffold a guided standard recipe with examples
fa --recipe new my-lib pack            # Scaffold a component/pack library recipe
fa --recipe new my-shortcuts alias     # Scaffold a clean command alias catalog recipe
fa --recipe edit my-app                # Open an existing recipe in $EDITOR
fa --recipe edit                       # List available recipes and their source file paths
fa --recipe validate                   # Lint and validate all recipes with diagnostics & suggestions
fa --recipe validate my-app            # Validate a specific recipe
fa --recipe rm my-app                  # Remove recipe TOML file (preserves templates and packs; or: fa -r rm ...)
fa --recipe rm my-app -y               # Remove recipe TOML file without interactive confirmation
```

---

## Features & Utilities

### Dependency Preflight
Before running any recipe step or alias command, `fa` inspects the shell command and verifies all required binaries exist on `PATH`. If a tool is missing, execution halts immediately with a clear installation hint.

### Git-Style Aliases (`~/.config/fa/config.toml`)
Configure custom command aliases directly under `[alias]` in `~/.config/fa/config.toml`. Supports native command shortcuts as well as external shell commands prefixed with `!`:

```toml
[alias]
# Native command shortcuts:
n  = "--new"
l  = "--list"
r  = "--recipe"
rn = "--recipe new"
rv = "--recipe validate"
re = "--recipe edit"
rm = "--recipe rm"

# External shell commands (prefixed with '!'):
b  = "!git branch"
s  = "!git switch"
st = "!git status"
ac = "!git add -A && git commit -m"
```

Invoking `fa rn my-app` expands to `fa --recipe new my-app`. Invoking `fa ac "feat: init"` executes the shell command with appended arguments.

### Version & Background Update Check
`fa --version` prints the active version immediately. It reads cached release data from `~/.local/state/fa/state.toml` and displays an update hint if a newer release exists. It spawns a non-blocking background check so the CLI never hangs on network requests.

### `fa --self-update`
Downloads and replaces the current binary with the latest release from GitHub Releases.

```bash
fa --self-update          # Update in-place
fa --self-update --dry-run # Check latest release asset without modifying binary
```

### `fa --self-uninstall`
Uninstalls the `fa` binary and prompts to clean up configuration (`~/.config/fa`) and state (`~/.local/state/fa`) directories.

```bash
fa --self-uninstall            # Interactive prompt for config/state
fa --self-uninstall --yes      # Non-interactive, removes binary and config/state
fa --self-uninstall --no       # Removes binary only, preserves config/state
fa --self-uninstall --dry-run  # Preview paths targeted for deletion
```

### Bootstrap Installation Script
Installs `fa` on a fresh machine (Debian or Termux) without requiring Rust or Cargo:

```bash
curl -sSL https://raw.githubusercontent.com/<user>/fast-alias/develop/install.sh | sh
```

### Environment Variables
- `FAST_ALIAS_REPO`: Override target GitHub repository (`owner/repo`) for updates.
- `GITHUB_TOKEN`: GitHub personal access token used to avoid API rate limits when checking releases.
