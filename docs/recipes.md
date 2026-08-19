# Recipe Structure & Configuration

The **declarative catalog** of `fa` lives in your personal config directory. It defines the recipes for scaffolding projects: their human-readable descriptions, language/category, variants, create strategy, package-manager commands, tooling, files to generate, and post-install steps.

## Configuration File Resolution

`fa` loads the catalog exclusively from `~/.config/fa/`:

1. **Primary Catalog**: `~/.config/fa/recipes.toml`
2. **Modular Directory**: `~/.config/fa/recipes.d/*.toml` (loaded alphabetically, merged into the catalog)

On first run (no `~/.config/fa/` directory yet), `fa` provisions a small example configuration with an `example` alias. Deleting it leaves an empty catalog: no recipes, no aliases.

Template files referenced as `{ from = "templates/<path>" }` or `{ template = "templates/<path>" }` are resolved from `~/.config/fa/templates/<path>` at runtime.

Modular `.toml` files allow breaking down large configurations into clean, domain-specific files (e.g. `web.toml`, `cli.toml`). Recipes declared in `recipes.d/*.toml` are merged into the main recipe catalog.

## State Management (`~/.local/state/fa/state.toml`)

`fa` tracks created projects for safe removal:

```toml
[projects.myapp]
recipe = "my-recipe"
variant = "pnpm"
created_at = "2026-08-10T12:00:00Z"
path = "/home/user/projects/myapp"
installed = true
```

## Recipe Schema (`recipes.toml`)

```toml
[recipes.my-recipe]
name        = "My Recipe"
description = "A stack bootstrap with tooling"
aliases     = ["mr"]
language    = "web · typescript"
variants    = ["pnpm", "bun", "npm"]

[variables]                                  # prompts with defaults
name   = { prompt = "Project name", default = "app" }
author = { prompt = "Author", default = "" }

[create]                                     # hybrid: official CLI OR template dir
command = "pnpm create app@latest -- --template basics --no-install"
# OR:
# template_dir = "templates/my-recipe/"

[pm]                                         # package-manager commands per variant
install     = { pnpm = "pnpm add", bun = "bun add", npm = "npm install" }
dev_install = { pnpm = "pnpm add -D", bun = "bun add -d", npm = "npm install -D" }

[tooling]                                    # linter / formatter / typechecker
linter    = { tool = "oxlint", files = ["oxlint.json"], script = "lint" }
formatter = { tool = "prettier", files = [".prettierrc"], script = "format" }
check     = { tool = "tsc", script = "check" }

[files]                                      # files: inline, from template or templated
"tsconfig.json"      = { from = "templates/tsconfig.strict.json" }
".editorconfig"      = { inline = "root = true ..." }
"src/pages/{{name}}.ts" = { template = "templates/page.ts.tpl" }

[[steps]]                                    # post-install commands (like dotss post_install_commands)
command = "git init"
description = "Initialize git repository"
```

## Step Execution (`steps`)

Recipes can declare ordered shell commands executed after files are written and dependencies installed.
- **Command**: Shell command to run (with `{{var}}` templating).
- **Description**: Human-readable label shown before execution.
- **Platform** (Optional): Restrict a step to a specific platform (`debian`, `termux`).
- **Prompt/Confirm** (Optional): Interactive confirmation before running.
- **Dry-Run Safety**: In `--dry-run` mode, `fa` prints `[Dry-Run] Would run: <command>` without executing.

## File Generation (`files`)

Each key is the destination path (templatable with `{{var}}`). Value is one of:
- **`inline`**: File content written verbatim.
- **`from`**: Copy a static file from `~/.config/fa/templates/`.
- **`template`**: Copy a file AND apply `{{var}}` substitution to its content.
- **`skip_if_exists`** (Optional): Do not overwrite an existing destination.

### Inline Content Size Limit

`inline` file content MUST NOT exceed **40 lines**:

- Any file longer than 40 lines must be moved to `~/.config/fa/templates/<recipe-name>/` and referenced with `{ from = "templates/<recipe-name>/<file>" }`. Template files are read from disk at runtime (no Rust code changes needed).
- Short files (`.gitkeep`, minimal configs, small components) may stay inline.
- Special cases that must stay inline beyond 40 lines require explicit user approval before proceeding.

## Variants

Each recipe can declare `variants` (e.g. `pnpm`, `bun`, `npm`). Variants select the matching `[pm]` command set and may override `[tooling]` and `[files]`. The user selects a variant with `-v`/`--variant`, defaulting to the first declared variant. Variants are NOT aliases: aliases are short names for the recipe itself.

## Alias Governance

Recipes may declare short aliases via the `aliases` array. Users can invoke `fa new <alias>` interchangeably with the canonical recipe name. Aliases must be explicitly defined in the recipe — never auto-generated.

## General-Purpose Aliases (`[aliases]`)

Beyond scaffolding, `fa` acts as a categorized alternative to Bash aliases. Commands live in top-level `[aliases]` sections — one per category — and run with `fa alias <name>`:

```toml
[aliases.git]                                     # category = section
status = { command = "git status", description = "Repo state" }
gco    = { command = "git checkout {{branch}}", description = "Switch branch", aliases = ["co"] }
rm     = { command = "git rm", description = "Remove files" }

[aliases.sistema]
free = { command = "free -h", description = "Free memory" }
du   = { command = "du -sh */", description = "Size per folder" }
```

- **Category**: the section name (`git`, `sistema`). `fa list` and `fa search` group aliases by category.
- **Command**: shell command executed via `sh -c`. Supports positional parameters (`$1`, `$2`, `${1}`, `{{1}}`, `{{2}}`, `$@`, `$*`) and template variables (`{{var}}`). All substituted values are shell-quoted (CWE-78 safe).
- **Arguments**:
  - Positional variables (`$1`, `{{1}}`, etc.) are replaced with corresponding CLI argument index.
  - If no positional placeholders are present, arguments are automatically appended shell-quoted at the end as passthrough (e.g. `fa alias rm a.txt b.txt` → `git rm 'a.txt' 'b.txt'`).
- **Aliases**: the `aliases` array declares alternative names (`fa alias co` or `fa co` also works).
- **Platform** (optional): restrict to `debian`/`termux`.
- **Dry-run**: `fa alias <name> [args] --dry-run` prints the resolved command without executing it.
- **Modularity**: categories can live in separate files, e.g. `recipes.d/git.toml` containing only `[aliases.git]`.
