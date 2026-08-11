# Environment Variables Reference & Governance — fa

`fa` respects specific environment variables for runtime configuration, platform detection, and GitHub Release API operations.

## Supported Environment Variables

| Variable | Scope / Usage | Default Value | Security & Handling |
| -------- | ------------- | ------------- | ------------------ |
| `FAST_ALIAS_REPO` | Overrides the target GitHub repository for release checks and self-updates | `<user>/fast-alias` | Used during development or custom forks. Must match `owner/repository` format. |
| `GITHUB_TOKEN` | Authenticates GitHub API requests during `self-update` to increase rate limit | Empty (Unauthenticated: 60 req/h) | Optional. **SECURITY**: Never log, display, or store token values in persistent state or terminal logs. |
| `TERMUX_VERSION` | Indicates execution environment is Android Termux | Set by Termux environment | Inspected for platform detection (`Platform::Termux`). |
| `HOME` | Resolves user home directory for path expansions (`~/.config`, `~/.local`) | OS environment | Required for atomic state storage and config file actions. |
| `PATH` | System executable lookup directories | OS environment | Checked directly to verify binary/package-manager presence without relying on external `which`. |

---

## Security Governance & Token Protection

1. **Token Masking**: `GITHUB_TOKEN` or any authentication secrets MUST NEVER be printed in stdout, stderr, debug logs, or state files.
2. **Repository Override Verification**: `FAST_ALIAS_REPO` must only point to trusted GitHub repositories.
3. **Personal Config**: Templates and recipes live in `~/.config/fa/`. Never store secrets in `~/.config/fa/templates/` that you do not want on disk.
4. **Recipe Source Trust**: Recipes loaded from `~/.config/fa/recipes.toml` (and `recipes.d/*.toml`) execute arbitrary `[[steps]]` commands. `fa` asks for an explicit one-time confirmation (`[TRUST]`) before running recipe steps or command aliases; the answer is persisted by path in `~/.local/state/fa/state.toml` and never re-prompted for the same config path. The auto-generated example config is trusted automatically. When a subcommand runs without an interactive terminal, `fa` auto-feeds `y` to stdin so approval prompts (e.g. pnpm `minimumReleaseAge` continuation) do not abort the install.
