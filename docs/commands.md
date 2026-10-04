# CLI Reference — fa

`fa` provides a simple, safety-first command-line interface to scaffold projects, apply tooling, and manage recipe catalogs.

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

**Argument signatures in listings:**
Commands declaring `args` (or containing positional `$1`, `$2` variables) automatically display their argument signature alongside their name:
```text
resize <input> <dimensions> [output] · Resize image
```

### 3. `fa --show <target>` (or: `fa -sh <target>`)
Displays detailed recipe configuration (variants, tooling, scaffolded files, and execution steps) or command alias definitions, including exact source file and line traceability (`Defined in: <file> (line <n>)`). Supports recipes, flat aliases, and multi-word namespaced commands (`skills ls`).

When inspecting a command alias, `fa --show` displays its execution command, environment variables, exact file location, usage signature, and aligned parameter documentation if `args` are declared:

```text
Command: resize
Section: media
Description: Resize image

Usage: fa resize <input> <dimensions> [output]

Arguments:
  <input>       Source image path
  <dimensions>  Target size (e.g. 800x600)
  [output]      Destination directory

Command: magick $1 -resize $2 $3
```

```bash
fa --show next-ts                    # Inspect recipe details and origin file
fa -sh fpages                        # Inspect alias details and origin file
fa -sh skills ls                     # Inspect namespaced command origin without quotes
```

### 4. `fa --alias <name> [args...]`
Executes a command alias configured under `[alias]` or `[aliases]` in `recipes.toml`. Aliases can also be invoked directly (`fa <name>`).

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

#### Custom Environment Variables (`env`, `env_force`, `_env`, `_env_force`)
Aliases and namespaces support environment variables injected into the execution subshell:
- **`env` / `_env`**: Fallback variables applied only when not present in the system/terminal.
- **`env_force` / `_env_force`**: Forced variables that always override the system environment.
- **Variable expansion**: Values expand `$VAR`, `${VAR}`, `${VAR:-default}`, and `~/`.

```toml
[aliases.":ai"]
_env_force = { LLM_BACKEND = "llama-cpp" }
coder = { command = "llama-cli -m $MODEL_PATH", env_force = {
  MODEL_PATH = "/models/qwen-coder.gguf",
  PATH = "$HOME/.local/ai/bin:$PATH"
} }
```

> [!TIP]
> For full details on environment variable injection and Bash syntax in TOML, see [Syntax Options](environment.md#4-syntax-options).

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

For git-style aliases configured in `~/.config/fa/config.toml`, see [Git-Style Aliases](config.md#git-style-aliases-configfaconfigtoml).

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
