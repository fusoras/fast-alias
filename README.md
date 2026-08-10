# fast-alias (`fa`)

> Recipe-based project scaffolder that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on **Debian** and **Termux**.

`fa` is a small CLI tool written in Rust that scaffolds a project with a single short command, replicating your full Astro workflow (and any future stack) across all your devices. It follows the `project-dots` release pattern (`install.sh` + GitHub Releases + cross-compiled musl).

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
| `fa new <recipe> <name>` | Scaffolds a new project from the recipe into directory `<name>` | `-v` / `--variant` to pick a toolchain variant, `--dry-run` / `-n` to preview, `--no-install` to skip dependencies |
| `fa list` | Displays available recipes for current platform | `-sh` / `--show-hidden` to display unsupported recipes |
| `fa search <query>` | Searches recipes by name, alias, language, or variant | Same single-line format as `list` |
| `fa show <recipe>` | Displays full recipe description, tooling, files, and steps | Accepts recipe name or alias |
| `fa doctor` | Detects platform, architecture, and installed package managers | — |
| `fa sync` | Fetches the latest recipe catalog from the repository | `--dry-run` / `-n` to preview |
| `fa self-update` | Checks GitHub Releases and updates `fa` binary in-place | `--dry-run` / `-n` to preview update check |
| `fa self-uninstall` | Safely removes `fa` binary executable, state, and config directories | `--yes` / `-y` to confirm deletion, `--no` / `-n` to keep state/config |
