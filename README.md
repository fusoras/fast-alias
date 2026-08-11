# fast-alias (`fa`)

> Recipe-based project scaffolder that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on **Debian** and **Termux**.

`fa` is a small CLI tool written in Rust that scaffolds a project with a single short command, replicating your own stack workflows across all your devices. It follows the `project-dots` release pattern (`install.sh` + GitHub Releases + cross-compiled musl).

---

# Install

> [!WARNING] Development version
> This installs the **development build** from the `develop` branch (pre-release, not a stable release).

To install the latest development build directly on your machine (Debian or Termux) without requiring Rust or Cargo:

```bash
curl -sSL https://raw.githubusercontent.com/<user>/fast-alias/develop/install.sh | sh
```

> [!NOTE]
> This command downloads the compiled pre-release binary from the active **`develop`** branch build assets and installs it to `~/.local/bin/fa` (or `$PREFIX/bin` on Termux).

---

### Quick CLI Overview

| Command | Description | Notes |
|---|---|---|
| `fa new <recipe> <name>` | Scaffolds a new project from the recipe into directory `<name>` | `-v` / `--variant` to pick a toolchain variant, `-d` / `--dry-run` to preview, `--no-install` to skip dependencies |
| `fa list` | Displays available recipes for current platform | `-sh` / `--show-hidden` to display unsupported recipes |
| `fa search <query>` | Searches recipes by name, alias, language, or variant | Same single-line format as `list` |
| `fa show <recipe>` | Displays full recipe description, tooling, files, and steps | Accepts recipe name or alias |
| `fa doctor` | Detects platform, architecture, and installed package managers | — |
| `fa sync` | Fetches the latest recipe catalog from the repository | `-d` / `--dry-run` to preview |
| `fa self-update` | Checks GitHub Releases and updates `fa` binary in-place | `-d` / `--dry-run` to preview update check |
| `fa self-uninstall` | Safely removes `fa` binary executable, state, and config directories | `--yes` / `-y` to confirm deletion, `--no` / `-n` to keep state/config |

> [!NOTE]
> **Short flags:** `fa -n <recipe> <name>` is shorthand for `fa new <recipe> <name>`, and `fa -a <command>` for `fa alias <command>`. Inside the `new` subcommand, dry-run is `-d` / `--dry-run` (not `-n`).

---

# Configuring Your Own Recipes & Aliases

`fa` is a **recipe player**: everything lives in declarative TOML files in your personal config directory, so adding a recipe or command alias is config only — never touching the binary.

## Where the configuration lives

`fa` reads your catalog exclusively from `~/.config/fa/`:

1. `~/.config/fa/recipes.toml` — your primary catalog (works from anywhere)
2. Modular files: `~/.config/fa/recipes.d/*.toml` (merged alphabetically)

Template files referenced as `{ from = "templates/<path>" }` are resolved from `~/.config/fa/templates/<path>` at runtime.

On first run (no `~/.config/fa/` yet) `fa` creates it with a small example configuration (an `example` alias). Delete it and, with no other aliases, the lists appear empty.

> [!WARNING]
> Commands in recipes execute arbitrary shell on your machine. `fa` asks for a one-time `[TRUST]` confirmation before running recipe steps or command aliases, and remembers it per config file. Only define commands you trust.

## Declaring a command alias

Commands live inside a recipe under the `commands` table. Each command has:

- **`command`** (required): the shell command to run.
- **`description`** (optional): shown in `fa list`.
- **`aliases`** (optional): short names to invoke it with (`fa -a <alias>`).
- **`platform`** (optional): restrict to `debian` or `termux`.

### Example — add a deploy alias

Put this in `~/.config/fa/recipes.toml`:

```toml
[recipes.my-site]
name = "My Site"
description = "Deploy a static site"
language = "web · deployment"

[recipes.my-site.commands.deploy]
command = "node --run build && rsync -av dist/ server:/srv/www"
description = "Build and deploy"
aliases = ["dep"]
```

Now, from inside any project:

```bash
fa alias deploy   # run by canonical name
fa -a dep         # run by alias (shorthand)
```

### Example — your own custom command

```toml
[recipes.my-stack]
name = "My Stack"
description = "Custom commands for my daily workflow"

[recipes.my-stack.commands.clean]
command = "rm -rf dist build node_modules/.cache"
description = "Clean build artifacts"
aliases = ["wipe"]
```

```bash
fa -a wipe                  # runs: rm -rf dist build node_modules/.cache
```

### Rules to remember

- Aliases must be **explicitly defined** — `fa` never invents or infers them.
- Command names and aliases are matched case-sensitively.
- `fa list` groups scaffold recipes under **Recipes** and pure-command recipes under **Aliases**.
