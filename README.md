# fast-alias (`fa`)

> Recipe-based project scaffolder that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on **Debian** and **Termux**.

`fa` is a small CLI tool written in Rust that scaffolds a project with a single short command, replicating your own stack workflows across all your devices.

---

# Install

> [!WARNING] Development version
> This installs the **development build** from the `develop` branch (pre-release, not a stable release).

### Quick Install (Pre-compiled Binary)

To install `fa` directly on your machine (Debian or Termux) without requiring Rust or Cargo:

```bash
curl -fsSL https://raw.githubusercontent.com/fusoras/fast-alias/develop/install.sh | sh
```

> [!NOTE]
> Automatically downloads the matching pre-compiled binary and installs it to `~/.local/bin/fa` (or `$PREFIX/bin` on Termux).

### Build from Source (Rust & Cargo)

If you have a Rust toolchain installed (Rust 2024 edition), you can compile and install `fa` directly using Cargo:

#### Option A: Install via Cargo

Clone the repository and install the binary to `~/.cargo/bin`:

```bash
git clone https://github.com/fusoras/fast-alias.git
cd fast-alias
cargo install --path .
```

Or install directly from GitHub:

```bash
cargo install --git https://github.com/fusoras/fast-alias.git --branch develop
```

#### Option B: Manual Release Compilation

Compile an optimized release binary:

```bash
cargo build --release
```

The compiled executable is generated at `./target/release/fa`. Move it to any folder in your `$PATH`:

```bash
install -Dm755 target/release/fa ~/.local/bin/fa
```

---

### Quick CLI Overview

> [!TIP]
> Append `--help` or `-h` to any command (e.g. `fa --new -h`) to inspect its usage and flags.

| Command (Long) | Short | Description | Notes & Flags |
|---|---|---|---|
| `fa --new <recipe> <name>` | `fa -n` | Scaffolds a new project from recipe into directory `<name>` | `-v` / `--variant`, `-d` / `--dry-run`, `--no-install` |
| `fa --list [category]` | `fa -l` | Displays available recipes, aliases, or packs | Filter: `recipes` (`-r`), `aliases` (`-a`), `packs` (`-p`); `-s` / `--show-hidden` |
| `fa --search <query>` | `fa -se` | Searches recipes by name, alias, language, or variant | Single-line match overview |
| `fa --show <target>` | `fa -sh` | Displays full recipe description, tooling, files, and steps | Accepts recipe name or alias |
| `fa --alias <name> [args...]` | `fa -a` | Runs a general-purpose command alias from `[aliases]` | Direct `fa <name>` also supported |
| `fa --recipe <action>` | `fa -r` | Manages recipe configuration files (`new`, `edit`, `validate`) | Subcommands: `new`, `edit`, `validate` |
| `fa --template add <recipe> <path>...` | `fa -t` | Copies files or folders into a recipe's template directory | `-f` / `--force` to overwrite without confirmation |
| `fa --self-update` | — | Checks GitHub Releases and updates `fa` binary in-place | `-d` / `--dry-run` to preview update check |
| `fa --self-uninstall` | — | Safely removes `fa` binary executable, state, and config directories | `-y` / `--yes`, `-n` / `--no`, `-d` / `--dry-run` |
| `fa --version` | `fa -v` | Displays version and checks for updates in background | Cached update notifications |
| `fa --help` | `fa -h` | Displays CLI help | — |

---

# Configuring Your Own Aliases & Recipes

`fa` is a **recipe player**: everything lives in declarative TOML files in your personal config directory, so adding a recipe or command alias is config only.

Create a new config file directly from the CLI:

```bash
fa --recipe new example alias   # or: fa -r new example alias
```

Example alias declaration:

```toml
[aliases."example"]
hello = { command = "echo 'Hello from fast-alias!'", description = "Say hello" }
```

```bash
fa -a hello                   # runs: echo 'Hello from fast-alias!'
```

> [!NOTE] Where configuration files live
> - **Modular files**: `fa --recipe new <name>` creates `~/.config/fa/recipes.d/<name>.toml` and opens it in your `$EDITOR`.
> - **Primary catalog**: `~/.config/fa/recipes.toml` (auto-provisioned on first run with sample recipes and aliases).
> - **Templates directory**: Template files referenced as `{ from = "templates/<path>" }` resolve from `~/.config/fa/templates/<path>`.
> - **Automatic merge**: All `.toml` files under `~/.config/fa/recipes.d/` merge alphabetically with `recipes.toml`.

> [!WARNING]
> Commands in recipes execute arbitrary shell on your machine. `fa` asks for a one-time `[TRUST]` confirmation before running recipe steps or command aliases, and remembers it per config file. Only define commands you trust.

## Declaring a command alias

Beyond scaffolding, `fa` is a section-grouped alternative to Bash aliases. Commands live in **`[aliases]` sections** (e.g. `git`, `system`, or namespaced `[aliases.":skills"]`), completely independent from scaffold recipes. Run them from anywhere with `fa --alias <alias>` (or `fa -a <alias>`) or direct syntax `fa <namespace> <command>`.

Each alias supports:

- **`command`** (required): shell command to run.
- **`description`** (optional): shown in `fa --list`.
- **`aliases`** (optional): short names to invoke it with.
- **`args`** (optional): explicit argument signature (e.g. `args = ["<input>", "[output]"]`) or detailed argument objects with descriptions.
- **`env` / `env_force`** (optional): fallback or forced environment variables.
- **`platform`** (optional): restrict to `debian` or `termux`.

### Example

Aliases can live in `~/.config/fa/recipes.toml` or split across `recipes.d/*.toml` (both singular `[alias]` and plural `[aliases]` are supported):

```toml
# Standalone alias with custom argument signature
[alias.wrapper.upscayl]
command = "upscayl -i $1 -o ${2:-./out}"
description = "AI image upscaler"
args = ["<input_image>", "[output_dir]"]

# Grouped alias sections
[aliases.git]
status = { command = "git status", description = "Show repo status" }

[aliases.system]
free = { command = "free -h", description = "Show available memory" }

# Namespaced command group:
[aliases.":skills"]
ls = { command = "bunx tabernaculo list", description = "List skills" }
```

```bash
fa --alias status   # run by canonical name (recommended)
fa skills ls        # run namespaced command directly
fa free             # direct shorthand invocation
```

### Positional Arguments & Passthrough

Aliases accept parameters and arguments dynamically:

1. **Positional Arguments (`$1`, `$2`, etc. or `{{1}}`, `{{2}}`):**
   ```toml
   [aliases.wrapper]
   avif = { command = "avifenc -s 0 -q ${3:-50} $1 -o $2", description = "Convert image to .avif with default quality" }
   ```
   ```bash
   fa avif image.jpg image.avif       # Runs: avifenc -s 0 -q '50' 'image.jpg' -o 'image.avif'
   fa avif image.jpg image.avif 80    # Runs: avifenc -s 0 -q '80' 'image.jpg' -o 'image.avif'
   ```

2. **Default Values (`${N:-value}` or `{{N:-value}}`):**
   Defines default values (numbers or text) used when the user does not provide that specific argument.

3. **Automatic passthrough:** If the command contains no positional variables, arguments are safely appended at the end.

### Rules to remember

- **Case-insensitive matching**: Names and aliases ignore case (e.g. if defined as `dep`, `fa dep`, `fa DEP`, and `fa -a Dep` work identically).
- `fa --list` shows scaffold recipes under **Recipes**, packs under **Packs**, and general-purpose commands under **Aliases** (ordered by section); `fa --show <recipe>` prints full details.
- The same `[TRUST]` confirmation that guards recipe steps also guards command aliases from your config files.

## Declaring a recipe

Scaffolding recipes live under `[recipes.<name>]` in your `~/.config/fa/recipes.toml` (or in modular files under `~/.config/fa/recipes.d/*.toml`). A recipe defines the stack metadata, language/tooling, package manager commands, configuration files, and post-scaffold steps.

### Example — Node / Web Recipe (Vite)

```toml
[recipes.vite-react]
name        = "Vite React TypeScript"
description = "React SPA with Vite, TypeScript and Tailwind"
aliases     = ["vr"]
language    = "web · react · typescript"
variants    = ["pnpm", "npm", "bun"]
pin_versions = true                                  # strip ^/~ from package.json after installs

[recipes.vite-react.create]
command = "pnpm create vite@latest {{name}} --template react-ts"

[recipes.vite-react.pm]
install     = { pnpm = "pnpm add", npm = "npm install", bun = "bun add" }
dev_install = { pnpm = "pnpm add -D", npm = "npm install -D", bun = "bun add -d" }

[recipes.vite-react.tooling]
linter    = { tool = "oxlint", script = "lint" }
formatter = { tool = "prettier", script = "format" }

[recipes.vite-react.files]
"tsconfig.json" = { from = "templates/tsconfig.json" }

[[recipes.vite-react.steps]]
command = "git init"
description = "Initialize git repository"
```

### Version Pinning (`pin_versions`)

When scaffolding modern projects, package managers often install dependencies using floating ranges (`^1.2.3` or `~1.2.3`), which can cause unexpected dependency drift over time.

By adding `pin_versions = true` to your recipe:

- **Exact Version Locking**: `fa` automatically strips floating range prefixes (`^` and `~`), turning them into exact pinned versions (e.g. `"^4.17.21"` becomes `"4.17.21"`).
- **Current Scope**: Currently supported natively for **Node.js (`package.json`)**. It processes `dependencies`, `devDependencies`, and `optionalDependencies` safely in place while preserving exact indentation, key order, and non-floating specs (like `workspace:*`, `catalog:`, git URLs, or file paths).
- **Execution Timing**: Pinning runs automatically after all package installations complete and right before post-install `[[steps]]` (such as `git init`), guaranteeing that your initial git commit records exact, reproducible dependencies.

```toml
[recipes.my-recipe]
pin_versions = true    # Strip ^ and ~ from package.json (default: false)
```

## Docs
- [Recipes](docs/recipes.md) - Recipe schema, package management, and template conventions.
- [Commands](docs/commands.md) - CLI reference and subcommands; see [Configuration](docs/config.md) for git-style aliases.
- [Packs](docs/packs.md) - Modular components and pack bundle installation.
- [Environment Variables](docs/environment.md) - Fallback and forced environment variable injection (`env`, `env_force`).
- [Template Variables](docs/variables.md) - Static template variables (`_vars`, `[vars]`).
