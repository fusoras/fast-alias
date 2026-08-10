# Platform Support — Debian vs Termux

`fa` adapts its operations dynamically based on the detected operating system. Platform detection and binary installation follow the same architecture as `project-dots`.

## Pre-flight Check Strategy

Before initiating scaffold or installation steps, `fa` runs pre-flight checks:
- **Prerequisite Binaries**: Verifies presence of `git`, `curl`, and `tar` by inspecting `$PATH` directories directly via `std::env::split_paths`.
- **Package Manager Detection**: Detects available JS package managers by checking `$PATH`:
  - **pnpm**: `pnpm -v`
  - **bun**: `bun --version`
  - **npm**: `npm -v`
- **Platform-Specific Package Managers** (for recipes that install system tooling):
  - **Debian**: `dpkg-query -W -f='${db:Status-Status}' <package>`
  - **Termux**: `dpkg-query -W -f='${db:Status-Status}' <package>`

## Platform Command Matrix

| Feature | Debian | Termux |
| ------- | ------ | ------ |
| **Detection Method** | `/etc/debian_version` or `/etc/os-release` | `$TERMUX_VERSION` env var or `/data/data/com.termux/` |
| **Binary Install Dir** | `~/.local/bin/` | `$PREFIX/bin/` (`/data/data/com.termux/files/usr/bin`) |
| **JS Package Managers** | `pnpm`, `bun`, `npm` (via corepack/pnpm) | `pnpm`, `bun`, `npm` (via pkg) |
| **Node Bootstrap (Debian)** | NVM → Node LTS → corepack pnpm | `pkg install nodejs-lts` → corepack pnpm |
| **System Package Install** | `sudo apt install -y <packages>` | `pkg install -y <packages>` |

## Bootstrap Script POSIX Compliance (`install.sh`)

When users run `curl -fsSL .../install.sh | sh`, the script is interpreted directly by the default shell (`/bin/sh`), which resolves to **Dash** on Debian and standard `/bin/sh` on Termux:
- **Strict POSIX (`/bin/sh`)**: The script must never contain Bash-isms such as `set -o pipefail`, `set -E`, `trap ... ERR`, arrays (`()`), or `[[ ]]` tests.
- **Trap Handling**: Use POSIX-standard signals (`trap 'rm -rf "$TMP_DIR"' EXIT INT TERM`).
- **Binary & Command Checks**: Use POSIX `command -v >/dev/null 2>&1` instead of `&>/dev/null` or `which`.

## Release Asset Resolution

| Platform | Architecture | Target Asset |
| -------- | ------------ | ------------ |
| Debian | x86_64 | `fa-x86_64-unknown-linux-gnu.tar.gz` |
| Termux | aarch64 | `fa-aarch64-unknown-linux-musl.tar.gz` |
