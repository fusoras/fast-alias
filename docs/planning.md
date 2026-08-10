# Project Planning & Roadmap — fa

`fa` (fast-alias) is a modular, recipe-based project scaffolder CLI designed to automate project bootstrapping: scaffolding base structure, installing dependencies, and applying tooling (linter, formatter, typechecker) and configuration files — across **Debian** and **Termux** devices.

`fa` is a **recipe player**: recipes are declared in TOML files (one per stack/toolchain). Astro is just the first recipe. Adding a language or toolchain never requires touching code — only adding a `.toml`.

## Core Features & Commands

- **`fa new <recipe> <name>`**: Scaffolds a new project from the given recipe into a directory named `<name>` (or the current directory). Variant selection via `-v`/`--variant` (e.g. `fa new astro app -v bun`).
- **`fa list`**: Displays available recipes grouped by language/category in a concise single-line format (routed through system `$PAGER` / `less` when on TTY).
- **`fa search <query>`**: Searches recipes by name, alias, language, or variant, printing matches in the same single-line format as `list`.
- **`fa show <recipe>`**: Displays full recipe details: description, language, variants, create strategy, tooling, files to generate, and steps.
- **`fa doctor`**: Detects the current device environment: platform, architecture, installed package managers (pnpm/bun/npm), and prerequisites.
- **`fa sync`**: Fetches the latest recipe catalog from the repository without recompiling the binary.
- **`fa self-update [--dry-run / -n]`**: Checks GitHub Releases for new versions and updates the binary in-place.
- **`fa self-uninstall [--yes / -y] [--no / -n] [--dry-run / -d]`**: Safely removes the binary executable and state/config directories.

## System Installation & Self-Update Architecture

1. **One-Line Bootstrap Script (`install.sh`)**:
   - POSIX shell script: `curl -sSL https://raw.githubusercontent.com/<user>/fast-alias/develop/install.sh | sh`
   - Detects architecture (Debian x86_64 vs Termux aarch64), fetches pre-compiled GitHub Release binary asset, places it in `~/.local/bin` (or `$PREFIX/bin` on Termux), and displays shell `$PATH` tips.
2. **Self-Update & Self-Uninstall Engine (`src/update.rs`)**:
   - Queries GitHub Releases API for `latest`.
   - Compares release tag with current binary crate version.
   - Replaces current executable atomically (`std::env::current_exe()`).
   - Self-uninstalls binary and prompts interactively to purge `~/.config/fa` and `~/.local/state/fa`.

## Recipe Engine Architecture

1. **Recipe Catalog (`recipes.toml` + `recipes.d/*.toml`)**:
   - Declarative TOML catalog of all recipes. Load order mirrors `project-dots`: local `./recipes.toml`, XDG `~/.config/fa/recipes.toml`, then embedded default.
   - Modular `recipes.d/*.toml` files allow domain-specific catalogs (e.g. `web.toml`, `cli.toml`).
2. **Recipe Schema**: Each recipe declares metadata, variables/prompts, create strategy (official CLI command OR bundled template directory), package-manager commands, tooling (linter/formatter/typechecker), files to generate (inline / from template / templated), and ordered `steps` (post-install commands).
3. **Templating Engine**: `{{var}}` placeholders substituted in file content, paths, and commands; variables resolved from prompts with defaults.
4. **Hybrid Create Strategy**: Use the official scaffold CLI when available (`create-astro`, `cargo new`); fall back to an embedded template directory otherwise.

## Completed & Roadmap Tasks

### Completed Documentation & Project Setup Tasks ✅
- [x] **Task 1: Project Setup, Documentation, Agent Rules & Git Strategy**
- [x] **Task 2: Agent Skills Deployment (rust-best-practices, bash-defensive-patterns, git-workflow, git-commit, agents-md, docs-instructions)**

### Core Engine Tasks 🔲
- [ ] **Task 3: Cargo Project Setup, SemVer Packaging & Versioning**
- [ ] **Task 4: Recipe Catalog & Schema (`recipes.toml`)**
- [ ] **Task 5: Platform Detection & Pre-flight Checks (`src/platform.rs`)**
- [ ] **Task 6: Configuration & Atomic State Engine (`src/config.rs`, `src/state.rs`)**
- [ ] **Task 7: Recipe Player Core (`src/engine.rs`): create → files → install → steps**
- [ ] **Task 8: Variable Prompts & Templating Engine (`src/templating.rs`)**
- [ ] **Task 9: CLI Interface & Verification (`src/main.rs`)**
- [ ] **Task 10: Unit Test Suite & TDD Verification (`cargo test -- --nocapture`)**

### Recipe Catalog Tasks 🔲
- [ ] **Task 11: `astro-pnpm` Recipe (default Astro flow: oxlint + prettier + stylelint + astro check)**
- [ ] **Task 12: `astro-bun` / `astro-npm` Variants**
- [ ] **Task 13: `ts-lib` Recipe (TypeScript library: oxlint + prettier + tsc)**
- [ ] **Task 14: `rust-cli` Recipe (cargo new + clippy + rustfmt)**
- [ ] **Task 15: `python` Recipe**

### Distribution Tasks 🔲
- [ ] **Task 16: Bootstrap Installation Script (`install.sh`)**
- [ ] **Task 17: GitHub Actions Release CI Pipeline (`.github/workflows/release.yml`)**
- [ ] **Task 18: Self-Update Engine (`src/update.rs`)**
- [ ] **Task 19: Self-Uninstall Engine & `--yes` Interactive Confirmation**
- [ ] **Task 20: `fa list`, `fa search`, `fa show`, `fa doctor` Commands**
- [ ] **Task 21: `fa sync` Recipe Catalog Refresh**
