# Cybersecurity & Secret Governance — fa

Rules and constraints to prevent credential leaks, arbitrary code execution (RCE) via untrusted configuration files, and unverified binary downloads.

- **No Secrets or Credentials in `~/.config/fa/templates/`**:
  - Never commit or copy API keys, tokens, private SSH keys (`id_*`), certificates (`*.pem`, `*.key`), `.env` files, or history files into `~/.config/fa/templates/<recipe-name>/` subdirectories.
  - Config and templates live on disk in the user's config directory; any secret placed there persists on disk.
- **Environment Token Protection**:
  - Never log, display, or persist runtime tokens (e.g. `GITHUB_TOKEN`, `NPM_TOKEN`, API keys) in stdout, stderr, debug logs, or state files.
- **Untrusted Configuration Trust Gate**:
  - Commands in recipes execute arbitrary shell. Ask for an explicit one-time `[TRUST]` confirmation before running recipe `[[steps]]`/`create` commands (`fa new`) and command aliases (`fa alias`). Persist the decision by config path in `~/.local/state/fa/state.toml` and never re-prompt for the same path; trusting the primary `recipes.toml` covers all `recipes.d/*.toml` modular files. Auto-trust the provisioned example config.
- **Non-Interactive Prompt Handling**:
  - When a subcommand runs without an interactive terminal, auto-feed `y` to stdin so approval prompts (e.g. pnpm `minimumReleaseAge` continuation) do not abort the install.
- **Integrity & Checksum Governance**:
  - Release binaries, self-update assets, and any downloaded template must be fetched strictly over HTTPS from verified GitHub Release assets or pinned URLs.
- **Code Execution from Recipes**:
  - Recipe `[[steps]]` commands execute arbitrary shell on the host. Never run a recipe from an untrusted source without displaying its steps first.
