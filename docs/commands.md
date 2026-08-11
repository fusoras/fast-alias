# CLI Command Reference & Manual — fa

`fa` provides a simple, safety-first command-line interface to scaffold projects, apply tooling, and manage recipe catalogs across **Debian** and **Termux**.

## Overview of Commands

| Command | Purpose | Options |
| ------- | ------- | ------- |
| `fa new <recipe> <name>` | Scaffolds a new project from the recipe into directory `<name>` | `-v` / `--variant` (e.g. `pnpm`, `bun`, `npm`), `-d` / `--dry-run` to preview, `--no-install` to skip dependency installation |
| `fa list` | Displays available recipes in a concise single-line format (routed through system `$PAGER` / `less` when on TTY) | `-sh` / `--show-hidden` to display unsupported recipes |
| `fa search <query>` | Searches recipes by name, alias, language, or variant and prints matches in the same format as `list` | Query is matched case-insensitively |
| `fa show <recipe>` | Displays full recipe details: description, language, variants, tooling, files, and steps | Accepts recipe name or alias |
| `fa doctor` | Detects platform, architecture, and installed package managers (pnpm/bun/npm) | — |
| `fa sync` | Fetches the latest recipe catalog from the repository without recompiling | `-d` / `--dry-run` to preview |
| `fa self-update` | Checks GitHub Releases and updates the application binary in-place | `-d` / `--dry-run` to preview version update without downloading |
| `fa self-uninstall` | Safely removes `fa` binary executable and state/config directories | `--yes` / `-y` to confirm deletion, `--no` / `-n` to keep state/config, `-d` / `--dry-run` to preview |

> [!NOTE]
> **Short flags:** `fa -n <recipe> <name>` is shorthand for `fa new <recipe> <name>`, and `fa -a <command>` for `fa alias <command>`. Inside the `new` subcommand, dry-run is `-d` / `--dry-run` (not `-n`).
| `fa --version` | Displays the current application version | `-v` |
| `fa --help` | Displays the command-line help summary | `-h` |

---

## Command Specifications & Exact Terminal Outputs

### 1. New Project (Simplified Scaffolding)
**Command**:
```bash
fa new my-recipe myapp
# Select a variant explicitly:
fa new my-recipe myapp -v bun
```

**Exact Output** (default variant, dependency install):
```text
[Recipe] my-recipe · A stack bootstrap with tooling
[Variant] pnpm

Scaffolding base via: pnpm create app@latest myapp --template basics --no-install
✓ Base scaffolded at ./myapp

Writing configuration files:
  ✓ tsconfig.json
  ✓ .prettierrc
  ✓ .stylelintrc
  ✓ oxlint.json
  ✓ .editorconfig

Installing dependencies...
  ✓ pnpm add -D oxlint prettier stylelint tsc

Steps:
  ✓ git init

Project 'myapp' created successfully.
Run `cd myapp && pnpm dev` to start developing.
```

### 2. New Project (Dry-Run Preview)
**Command**:
```bash
fa new my-recipe myapp --dry-run
```

**Exact Output**:
```text
=== DRY-RUN MODE ACTIVE: No changes will be made ===
[Recipe] my-recipe · A stack bootstrap with tooling
[Variant] pnpm
[Dry-Run] Would scaffold base via: pnpm create app@latest myapp --template basics --no-install
[Dry-Run] Would write file: tsconfig.json
[Dry-Run] Would write file: .prettierrc
[Dry-Run] Would run: pnpm add -D oxlint prettier stylelint tsc
[Dry-Run] Would run: git init
```

### 3. List Recipes
**Command**:
```bash
fa list
```

**Exact Output**:
```text
my-recipe (pnpm / bun / npm) · web · typescript [apply]
ts-lib (pnpm / bun) · typescript [apply]
rust-cli · systems · rust [apply]
python · scripting · python [apply]
```

### 4. Search Recipes
**Command**:
```bash
fa search my-recipe
fa search rust
```

**Exact Output** (same format as `list`):
```text
my-recipe (pnpm / bun / npm) · web · typescript [apply]
```

### 5. Show Recipe Details
**Command**:
```bash
fa show my-recipe
```

**Exact Output**:
```text
Recipe: my-recipe
Description: A stack bootstrap with tooling
Language: web · typescript
Aliases: my-recipe
Variants: pnpm (default), bun, npm

Create: pnpm create app@latest myapp --template basics --no-install

Tooling:
  - linter: oxlint (script: lint)
  - formatter: prettier (script: format)
  - check: tsc (script: check)

Files:
  - tsconfig.json (from template)
  - .prettierrc (from template)
  - .stylelintrc (from template)
  - oxlint.json (from template)
  - .editorconfig (inline)

Steps:
  - git init (Initialize git repository)
```

### 6. Doctor (Environment Detection)
**Command**:
```bash
fa doctor
```

**Exact Output**:
```text
=== fa Environment Diagnosis ===
Platform: Debian (x86_64)
Package Managers:
  ✓ pnpm 10.0.0
  ✓ bun 1.2.0
  ✗ npm (not found)
Prerequisites:
  ✓ git
  ✓ curl
  ✓ tar
```

### 7. Self-Update Engine
**Command**:
```bash
fa self-update --dry-run
```

**Exact Output (Up-to-Date)**:
```text
Checking GitHub Releases for updates...
Current version: <current-version>
Latest release tag: v<current-version>

[Up-to-Date] fa is already running the latest version.
```

**Exact Output (Update Available - Dry Run)**:
```text
Checking GitHub Releases for updates...
Current version: <current-version>
Latest release tag: v<newer-version>

=== DRY-RUN MODE ACTIVE: No binary changes will be made ===
[Dry-Run] Would download pre-compiled release binary asset: fa-x86_64-unknown-linux-gnu.tar.gz
[Dry-Run] Would extract and replace executable at: /home/user/.local/bin/fa
```

### 8. Self-Uninstall Engine
**Command**:
```bash
fa self-uninstall --dry-run
# Or non-interactive confirmation:
fa self-uninstall --yes
# Or skip removing config/state directories:
fa self-uninstall --no
```

**Exact Output (Dry-Run Preview)**:
```text
=== DRY-RUN MODE ACTIVE: No files will be deleted ===
=== fa Self-Uninstall Engine ===
Target Binary Path: /home/user/.local/bin/fa
Target State Directory: /home/user/.local/state/fa
Target Config Directory: /home/user/.config/fa

[Dry-Run] Would remove executable: /home/user/.local/bin/fa
[Dry-Run] Would remove state directory: /home/user/.local/state/fa
```

### 9. System Bootstrap Installation Script
**Command**:
```bash
curl -sSL https://raw.githubusercontent.com/<user>/fast-alias/develop/install.sh | sh
```

**Purpose**:
Installs `fa` on a fresh machine (Debian or Termux) in one command without requiring Rust or Cargo.

---

## Environment Variables & Token Security

`fa` respects environment variables such as `FAST_ALIAS_REPO` and `GITHUB_TOKEN`. For full specification and token security guidelines, see [`docs/environment.md`](environment.md).
