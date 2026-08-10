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
3. **No Embedded Tokens**: Secrets or tokens must NEVER be placed inside `templates/<recipe-name>/`, as `include_str!` / `include_bytes!` embeds them directly into the compiled executable binary.
4. **Recipe Source Trust**: Recipes loaded from `./recipes.toml` or `~/.config/fa/recipes.toml` (non-embedded) execute arbitrary `[[steps]]` commands. Always print a `[WARNING]` before running recipes from untrusted local sources.
