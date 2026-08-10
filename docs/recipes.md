# Recipe Structure & Configuration

`recipes.toml` is the central **declarative catalog** of `fa`. It defines the recipes for scaffolding projects: their human-readable descriptions, language/category, variants, create strategy, package-manager commands, tooling, files to generate, and post-install steps.

## Configuration File Resolution Order

1. **Base Catalog**:
   - `./recipes.toml` (Current working directory)
   - `~/.config/fa/recipes.toml` (XDG User Config)
   - Embedded default in binary (`include_str!("../recipes.toml")`)

2. **Modular Directories (`recipes.d/`)**:
   - `./recipes.d/*.toml` (Local modular configurations, loaded alphabetically)
   - `~/.config/fa/recipes.d/*.toml` (XDG User modular configurations)

Modular `.toml` files allow breaking down large configurations into clean, domain-specific files (e.g. `web.toml`, `cli.toml`). Recipes declared in `recipes.d/*.toml` are merged into the main recipe catalog.

## State Management (`~/.local/state/fa/state.toml`)

`fa` tracks created projects for safe removal and `doctor` reporting:

```toml
[projects.myapp]
recipe = "astro-pnpm"
variant = "pnpm"
created_at = "2026-08-10T12:00:00Z"
path = "/home/user/projects/myapp"
installed = true
```

## Recipe Schema (`recipes.toml`)

```toml
[recipes.astro]
name        = "Astro"
description = "Astro site with oxlint, prettier, stylelint and astro check"
aliases     = ["astro"]
language    = "web · typescript"
variants    = ["pnpm", "bun", "npm"]

[variables]                                  # prompts with defaults
name   = { prompt = "Project name", default = "app" }
author = { prompt = "Author", default = "" }

[create]                                     # hybrid: official CLI OR template dir
command = "pnpm create astro@latest -- --template basics --no-install"
# OR:
# template_dir = "templates/astro/"

[pm]                                         # package-manager commands per variant
install     = { pnpm = "pnpm add", bun = "bun add", npm = "npm install" }
dev_install = { pnpm = "pnpm add -D", bun = "bun add -d", npm = "npm install -D" }

[tooling]                                    # linter / formatter / typechecker
linter    = { tool = "oxlint", files = ["oxlint.json"], script = "lint" }
formatter = { tool = "prettier", files = [".prettierrc"], script = "format" }
check     = { tool = "@astrojs/check", script = "check" }

[files]                                      # files: inline, from template or templated
"tsconfig.json"      = { from = "templates/tsconfig.strict.json" }
".editorconfig"      = { inline = "root = true ..." }
"src/pages/{{name}}.astro" = { template = "templates/page.astro.tpl" }

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
- **`from`**: Copy a static file from the bundled template directory (`templates/`).
- **`template`**: Copy a file AND apply `{{var}}` substitution to its content.
- **`skip_if_exists`** (Optional): Do not overwrite an existing destination.

## Variants

Each recipe can declare `variants` (e.g. `pnpm`, `bun`, `npm`). Variants select the matching `[pm]` command set and may override `[tooling]` and `[files]`. The user selects a variant with `-v`/`--variant`, defaulting to the first declared variant. Variants are NOT aliases: aliases are short names for the recipe itself.

## Alias Governance

Recipes may declare short aliases via the `aliases` array. Users can invoke `fa new <alias>` interchangeably with the canonical recipe name. Aliases must be explicitly defined in the recipe — never auto-generated.

## Planned Active Recipes

### 1. `astro` (Astro Web Project)
- **Description**: Astro site with oxlint, prettier (+ prettier-plugin-astro), stylelint and @astrojs/check.
- **Variants**: `pnpm`, `bun`, `npm`.
- **Create**: Official `create-astro` CLI (`--no-install`).
- **Tooling**: `oxlint` (lint), `prettier` (format), `@astrojs/check` (check).
- **Files**: strict `tsconfig.json`, `.prettierrc`, `.stylelintrc`, `oxlint.json`, `.editorconfig`, `.gitignore`.

### 2. `ts-lib` (TypeScript Library)
- **Description**: Pure TypeScript library with oxlint, prettier and `tsc` typecheck.
- **Variants**: `pnpm`, `bun`.
- **Create**: Template directory (`templates/ts-lib/`).
- **Files**: `package.json`, `tsconfig.json`, `src/index.ts`, `.prettierrc`, `oxlint.json`.

### 3. `rust-cli` (Rust CLI)
- **Description**: Rust command-line binary with clippy and rustfmt.
- **Create**: `cargo new`.
- **Tooling**: `clippy` (lint), `rustfmt` (format).
- **Files**: `rustfmt.toml`, `.editorconfig`, `.gitignore`.

### 4. `python` (Python Project)
- **Description**: Python project with ruff and mypy.
- **Create**: Template directory (`templates/python/`).
- **Tooling**: `ruff` (lint/format), `mypy` (typecheck).
- **Files**: `pyproject.toml`, `.editorconfig`, `.gitignore`.
