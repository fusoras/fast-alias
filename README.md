# fast-alias (`fa`)

> Recipe-based project scaffolder that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on **Debian** and **Termux**.

`fa` is a small CLI tool written in Rust that scaffolds a project with a single short command, replicating your own stack workflows across all your devices. It follows the `project-dots` release pattern (`install.sh` + GitHub Releases + cross-compiled musl).

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

| Command | Description | Notes |
|---|---|---|
| `fa new <recipe> <name>` | Scaffolds a new project from the recipe into directory `<name>` | `-v` / `--variant` to pick a toolchain variant, `-d` / `--dry-run` to preview, `--no-install` to skip dependencies |
| `fa list` | Displays available recipes for current platform | `-sh` / `--show-hidden` to display unsupported recipes |
| `fa search <query>` | Searches recipes by name, alias, language, or variant | Same single-line format as `list` |
| `fa show <recipe>` | Displays full recipe description, tooling, files, and steps | Accepts recipe name or alias |
| `fa alias <name>` | Runs a general-purpose alias from the `[aliases]` catalog | Accepts canonical name or declared alias; `fa -a <name>` is shorthand |
| `fa sync` | Fetches the latest recipe catalog from the repository | `-d` / `--dry-run` to preview |
| `fa self-update` | Checks GitHub Releases and updates `fa` binary in-place | `-d` / `--dry-run` to preview update check |
| `fa self-uninstall` | Safely removes `fa` binary executable, state, and config directories | `--yes` / `-y` to confirm deletion, `--no` / `-n` to keep state/config |

> [!NOTE]
> **Short flags:** `fa -n <recipe> <name>` is shorthand for `fa new <recipe> <name>`, and `fa -a <name>` for `fa alias <name>`. Inside the `new` subcommand, dry-run is `-d` / `--dry-run` (not `-n`).

---

# Configuring Your Own Recipes & Aliases

`fa` is a **recipe player**: everything lives in declarative TOML files in your personal config directory, so adding a recipe or command alias is config only — never touching the binary.

## Where the configuration lives

`fa` reads your catalog exclusively from `~/.config/fa/`:

1. `~/.config/fa/recipes.toml` — your primary catalog (works from anywhere)
2. Modular files: `~/.config/fa/recipes.d/*.toml` (merged alphabetically)

Template files referenced as `{ from = "templates/<path>" }` are resolved from `~/.config/fa/templates/<path>` at runtime.

On first run (no `~/.config/fa/` yet) `fa` creates it with a small example configuration (an `example` recipe and a `hello` alias). Delete it and, with no other entries, the lists appear empty.

> [!WARNING]
> Commands in recipes execute arbitrary shell on your machine. `fa` asks for a one-time `[TRUST]` confirmation before running recipe steps or command aliases, and remembers it per config file. Only define commands you trust.

## Declaring a command alias

Beyond scaffolding, `fa` is a categorized alternative to Bash aliases. General-purpose commands live in **top-level `[aliases]` sections — one per category** (e.g. `git`, `sistema`, `deploy`), completely independent from scaffold recipes. Run them from anywhere with `fa alias <alias>`.

Each alias has:

- **`command`** (required): the shell command to run.
- **`description`** (optional): shown in `fa list`.
- **`aliases`** (optional): short names to invoke it with.
- **`platform`** (optional): restrict to `debian` or `termux`.

### Example — add a deploy alias

Put this in `~/.config/fa/recipes.toml`:

```toml
[aliases.deploy]
deploy = { command = "node --run build && rsync -av dist/ server:/srv/www", description = "Build and deploy", aliases = ["dep"] }
```

Now, from inside any project:

```bash
fa alias deploy   # run by canonical name
fa alias dep      # run by alias
fa -a dep         # run by alias (shorthand)
```

Aliases are grouped by category, so you can organize your workflow in sections:

```toml
[aliases.git]
status = { command = "git status", description = "Ver estado del repo" }
l = { command = "git log --oneline -10", description = "Últimos commits" }

[aliases.sistema]
free = { command = "free -h", description = "Memoria disponible" }
```

```bash
fa alias status   # git status
fa alias free     # free -h
```

Categories are ordinary TOML sections, so they can live in separate modular files too — e.g. a `git.toml` under `recipes.d/` containing only `[aliases.git]`.

### Rules to remember

- Aliases must be **explicitly defined** — `fa` never invents or infers them.
- Names and aliases are matched case-insensitively (`fa -a DEP` works).
- `fa list` shows scaffold recipes under **Recipes** and general-purpose commands under **Aliases** (ordered by category); `fa show <alias>` prints the command that would run.
- The same `[TRUST]` confirmation that guards recipe steps also guards command aliases from your config files.
