# Cybersecurity & Secret Governance — fa

Rules and constraints to prevent credential leaks, arbitrary code execution (RCE) via untrusted configuration files, and unverified binary downloads.

- **No Secrets or Credentials in `templates/`**:
  - Never commit API keys, tokens, private SSH keys (`id_*`), certificates (`*.pem`, `*.key`), `.env` files, or history files inside `templates/<recipe-name>/` subdirectories.
  - Recipe template files embedded via `include_str!` or `include_bytes!` are compiled directly into the release executable. Any committed secret is permanently baked into public binary releases.
- **Environment Token Protection**:
  - Never log, display, or persist runtime tokens (e.g. `GITHUB_TOKEN`, `NPM_TOKEN`, API keys) in stdout, stderr, debug logs, or state files.
- **Untrusted Configuration Warning**:
  - When loading `recipes.toml` (or `recipes.d/*.toml`) from a local path (`./recipes.toml` or `~/.config/fa/recipes.toml`) instead of the embedded default configuration, print a clear `[WARNING]` alert before processing custom installers or executing recipe `[[steps]]` commands.
- **Integrity & Checksum Governance**:
  - Release binaries, self-update assets, and any downloaded template must be fetched strictly over HTTPS from verified GitHub Release assets or pinned URLs.
- **Code Execution from Recipes**:
  - Recipe `[[steps]]` commands execute arbitrary shell on the host. Never run a recipe from an untrusted source without displaying its steps first.
