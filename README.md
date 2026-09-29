# fast-alias (`fa`)

> Recipe-based project scaffolder that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on **Debian** and **Termux**.

`fa` is a small CLI tool written in Rust that scaffolds a project with a single short command, replicating your own stack workflows across all your devices.

---

# Install

> [!WARNING] Development version
> This installs the **development build** from the `develop` branch (pre-release, not a stable release).

To install `fa` directly on your machine (Debian or Termux) without requiring Rust or Cargo:

```bash
curl -fsSL https://raw.githubusercontent.com/fusoras/fast-alias/develop/install.sh | sh
```

> [!NOTE]
> This command automatically detects your platform (Debian or Termux) and architecture (`x86_64` or `aarch64`), downloads the pre-compiled binary asset, and installs it to `~/.local/bin/fa` (or `$PREFIX/bin` on Termux).

---

### Quick CLI Overview

> [!TIP]
> **Getting Help for Any Command**:
> You can inspect usage and flags for any command or subcommand at any time by appending `--help` or `-h` (or using `fa help <command>`):
> ```bash
> fa --list --help          # or: fa -l -h
> fa --new --help           # or: fa -n -h
> fa --recipe --help        # or: fa -r -h
> fa --template add --help  # or: fa -t add -h
> ```

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

> [!NOTE]
> **Command Style**: In documentation and examples, `fa` uses the explicit long form (`fa --new`, `fa --list`, etc.) for teaching clarity. In your daily shell workflow, use the ultra-fast short flags (`fa -n`, `fa -l`, `fa -se`, `fa -sh`, `fa -r`, `fa -t`, `fa -a`). Native commands always use `--` or `-`, leaving all un-prefixed words exclusively available for your own custom aliases without naming collisions.

---

# Configuring Your Own Aliases & Recipes

`fa` is a **recipe player**: everything lives in declarative TOML files in your personal config directory, so adding a recipe or command alias is config only.

## Where the configuration lives

`fa` reads your catalog exclusively from `~/.config/fa/`:

1. `~/.config/fa/recipes.toml` — your primary catalog (works from anywhere)
2. Modular files: `~/.config/fa/recipes.d/*.toml` (merged alphabetically)

Template files referenced as `{ from = "templates/<path>" }` are resolved from `~/.config/fa/templates/<path>` at runtime.

On first run (no `~/.config/fa/` yet) `fa` creates it with a small example configuration (an `example` recipe and a `hello` alias). Delete it and, with no other entries, the lists appear empty.

> [!WARNING]
> Commands in recipes execute arbitrary shell on your machine. `fa` asks for a one-time `[TRUST]` confirmation before running recipe steps or command aliases, and remembers it per config file. Only define commands you trust.

## Declaring a command alias

Beyond scaffolding, `fa` is a section-grouped alternative to Bash aliases. General-purpose commands live in **top-level `[aliases]` sections — one per section** (e.g. `git`, `system`, `deploy`), completely independent from scaffold recipes. Run them from anywhere with `fa alias <alias>`.

Each alias has:

- **`command`** (required): the shell command to run.
- **`description`** (optional): shown in `fa list`.
- **`aliases`** (optional): short names to invoke it with.
- **`platform`** (optional): restrict to `debian` or `termux`.

### Example

Aliases are grouped by section and can live together in `~/.config/fa/recipes.toml` or split across `recipes.d/*.toml`:

```toml
[aliases.deploy]
deploy = { command = "node --run build && rsync -av dist/ server:/srv/www", description = "Build and deploy", aliases = ["dep"] }

[aliases.git]
status = { command = "git status", description = "Show repo status" }

[aliases.system]
free = { command = "free -h", description = "Show available memory" }
```

```bash
fa --alias deploy   # run by canonical name (recommended)
fa -a dep           # run by alias shorthand (recommended)
fa deploy           # direct shorthand invocation
```

> [!TIP]
> **Usage recommendation:** prefer `fa --alias <name>` or `fa -a <name>`. Direct `fa <name>` is quick but a future native command with same name would take precedence.

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
- [Aliases](docs/commands.md) - Command aliases and argument passthrough (see also [Recipes](docs/recipes.md)).
- [Recipes](docs/recipes.md) - Recipe schema and template conventions.
- [Commands](docs/commands.md) - CLI reference and subcommands.
