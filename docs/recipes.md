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
pin_versions = true                                  # strip ^/~ from package.json after installs (default: false)

[recipes.my-recipe.variables]                        # prompts with defaults
name   = { prompt = "Project name", default = "app" }
author = { prompt = "Author", default = "" }

[recipes.my-recipe.create]                           # hybrid: official CLI OR template dir
command = "pnpm create app@latest {{name}} -- --template basics --no-install"
# OR:
# template_dir = "templates/my-recipe/"

[recipes.my-recipe.pm]                               # package-manager commands per variant
install     = { pnpm = "pnpm add", bun = "bun add", npm = "npm install" }
dev_install = { pnpm = "pnpm add -D", bun = "bun add -d", npm = "npm install -D" }

[recipes.my-recipe.tooling]                          # linter / formatter / typechecker
linter    = { tool = "oxlint", script = "lint" }
formatter = { tool = "prettier", script = "format" }
check     = { tool = "tsc", script = "check" }

[recipes.my-recipe.files]                            # files: inline, from template or templated
"tsconfig.json"      = { from = "templates/tsconfig.strict.json" }
".editorconfig"      = { inline = "root = true ..." }
"src/pages/{{name}}.ts" = { template = "templates/page.ts.tpl" }

## Template Base (`template_base`)

Recipes with many template files can declare an optional base directory once instead of repeating it in every spec:

```toml
[recipes.rust-stack]
name         = "Rust Stack"
description  = "Rust CLI bootstrap"
template_base = "rust-stack"                       # base under ~/.config/fa/templates/ (default: unset)

[recipes.rust-stack.files]
".gitignore"    = { from = ".gitignore" }          # → templates/rust-stack/.gitignore
"deny.toml"     = { from = "config/deny.toml" }    # → templates/rust-stack/config/deny.toml
"welcome.txt"   = { template = "greet.txt.tpl" }   # → templates/rust-stack/greet.txt.tpl (+ {{var}} substitution)
"legacy.cfg"    = { from = "templates/other/legacy.cfg" }  # explicit `templates/` prefix ignores the base
"snippet.toml"  = { inline = "..." }               # inline always ignores the base
```

Rules:
- `template_base` lives at recipe level (`[recipes.<name>]`), never inside `files`.
- Prefixless `from`/`template` resolve as `base/spec` (both sides normalized).
- A spec with an explicit `templates/` prefix ignores the base (legacy escape hatch).
- Absolute paths, `..` escapes and empty paths fail fast in `Config::load` with `must stay inside ~/.config/fa/templates/` naming `recipe`/`file`.
- **Compatibility**: when `template_base` is unset (`None`, the default), every spec resolves exactly as before — `from = "templates/X"` and `from = "X"` keep resolving to `~/.config/fa/templates/X` byte-identically.

[[recipes.my-recipe.steps]]                          # post-install commands
command = "git init"
description = "Initialize git repository"
```

## Variables

Each entry prompts once during `fa new` (`prompt` label, `default` on empty
input). All validation keys are optional — a plain `prompt` keeps the legacy
free-string behavior with zero changes:

```toml
[recipes.demo.variables]
port = { prompt = "Port", default = "3000", type = "integer", choices = ["3000", "8080"] }
slug = { prompt = "Slug", type = "string", pattern = "^[a-z0-9-]+$", required = true }
```

- **`type`** (`string` | `integer` | `float` | `boolean`; default: free string).
- **`choices`**: input must equal one of the listed strings (must be non-empty).
- **`pattern`**: regex the value must match (use `^…$` to anchor; string only).
- **`required`**: `true` rejects empty input.
- Unknown keys, unknown types, empty `choices`, `pattern` on non-string types
  and invalid regexes fail at config load naming recipe + variable.
- Interactive `fa new` re-prompts (max 3 retries); non-interactive mode
  (`FA_VAR_<NAME>` env override or default) fails fast instead of hanging.

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

## Version Pinning (`pin_versions`)

When `pin_versions = true`, the engine strips floating range prefixes (`^`/`~`) from `package.json` dependency versions after all installs complete, right before `git init` so the initial commit includes exact versions. Toggle by adding or removing the line in `[recipes.<name>]`:

```toml
[recipes.astro]
pin_versions = true    # strip ^ and ~ → exact versions
# omit or set false to keep defaults
```

## Alias Governance

Recipes may declare short aliases via the `aliases` array. Users can invoke `fa new <alias>` interchangeably with the canonical recipe name. Aliases must be explicitly defined in the recipe — never auto-generated.

## General-Purpose Aliases (`[aliases]`)

Beyond scaffolding, `fa` acts as a section-grouped alternative to Bash aliases. Commands live in top-level `[aliases]` sections — one per section — and run with `fa alias <name>`:

```toml
[aliases.git]                                     # section
status = { command = "git status", description = "Repo state" }
gco    = { command = "git checkout {{branch}}", description = "Switch branch", aliases = ["co"] }
rm     = { command = "git rm", description = "Remove files" }

[aliases.sistema]
free = { command = "free -h", description = "Free memory" }
du   = { command = "du -sh */", description = "Size per folder" }
```

- **Section**: the section name (`git`, `sistema`). `fa list` and `fa search` group aliases by section.
- **Command**: shell command executed via `sh -c`. Supports positional parameters (`$1`, `$2`, `${1}`, `{{1}}`, `{{2}}`, `$@`, `$*`) and template variables (`{{var}}`). All substituted values are shell-quoted (CWE-78 safe).
- **Arguments**:
  - Positional variables (`$1`, `{{1}}`, etc.) are replaced with corresponding CLI argument index.
  - If no positional placeholders are present, arguments are automatically appended shell-quoted at the end as passthrough (e.g. `fa alias rm a.txt b.txt` → `git rm 'a.txt' 'b.txt'`).
- **Aliases**: the `aliases` array declares alternative names (`fa alias co` or `fa co` also works).
- **Platform** (optional): restrict to `debian`/`termux`.
- **Dry-run**: `fa alias <name> [args] --dry-run` prints the resolved command without executing it.
- **Modularity**: sections can live in separate files, e.g. `recipes.d/git.toml` containing only `[aliases.git]`.
