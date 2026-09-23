# Planning & Roadmap — fa

`fa` (fast-alias) `v0.1.0-beta.4`: recipe-player project scaffolder — bootstraps projects, installs dependencies, applies tooling (linter, formatter, typechecker) and config files on **Debian** and **Termux**. All stack knowledge lives in user TOML (`~/.config/fa/recipes.toml` + `recipes.d/*.toml` + `templates/`); the engine stays dumb. Docs split: `docs/` is user-only (`commands.md`, `recipes.md`); `.agents/docs/` is agent-internal (5 files).

## Implemented commands

- `fa new <recipe> <name>` (`-v` variant, `-d` dry-run, `--no-install`) · `fa list` (`-s` show-hidden) · `fa search <query>` · `fa show <recipe|command>` · `fa alias <name> [args]` (plus `fa -n` / `fa -a` shorthands, direct `fa <alias>` invocation) · `fa self-update` / `fa self-uninstall` (`-d`, `-y`/`-n`) · `fa --version` (offline cache + background `update-check`)
- Pipeline: create → files (`inline` / `from` / `template`) → pm install → `pin_versions` (Node-only) → steps → `final_message` (with fallback to neutral `Run 'cd <project-name>' to go to project`)
- Safety: dependency preflight on `PATH`, one-time `[TRUST]` per config path in `state.toml`, auto-`y` on non-TTY, POSIX `install.sh` (musl `x86_64`/`aarch64`, `~/.local/bin` / `$PREFIX/bin`)

## Done ✅

- Scaffolder core: create strategies, files from/inline/template, platform-gated steps/install, variants, prompts + `{{var}}` templating
- pm install + tooling (linter/formatter/checker) + `pin_versions` for Node (`src/pinning.rs`, byte-preserving, dry-run preview)
- Neutral post-scaffold navigation hint (`Run 'cd <name>' to go to project`) replacing hardcoded Node fallback; custom `final_message` support with variable substitution and `fa show` display
- Typed prompt validation in `[variables]`: `type` (string/integer/boolean), `choices`, `pattern` (regex), and `required` (fail-fast schema check at load time + runtime validation)
- Non-interactive answer injection via `FA_VAR_<KEY>` environment variables (CI/script friendly without adding CLI flags)
- `[TRUST]` per config path (one covers `recipes.d/`), example config auto-trust + first-run provisioning
- `install.sh`, self-update/self-uninstall, async `--version` check
- `list` / `search` / `show` / `alias` (section-grouped, case-insensitive, arg passthrough), dependency preflight
- Docs split (`docs/` vs `.agents/docs/`), 40-line inline limit with `templates/` overflow
- `template_rel` normalization: `from`/`template` with or without `templates/` prefix resolve equally; traversal/absolute rejected with a friendly error
- Release asset names single-sourced from `resolve_asset_name` (`src/update.rs`), kept in sync with `install.sh` / `release.yml`
- Branch workflow: `feature/*` for complex, direct to `develop` for tiny

## Next 🔲

- `recipes.local.toml` machine override (gitignored): optional machine-specific configuration loaded after `recipes.d/` with highest priority (topgrade `.local.toml` pattern)
- Commands drift: `docs/commands.md` + `README.md` document `fa sync`, `fa alias --dry-run`, and `$PAGER` paging — none exist in `Commands` (`src/main.rs`); implement or remove from docs
- Pinning beyond Node: `pin_versions` only handles `package.json`; decide TOML-declared per-ecosystem rules vs keep Node-only

## Proposed features (research-backed, medium-term roadmap)

Cross-referenced against similar tools (copier, chezmoi, cargo-generate, mise, just, topgrade, nix templates, hygen, cookiecutter). Filtered through project principles: no new command flags (TOML toggles), dumb engine, lightweight deps, Debian + Termux.

### Quick wins (S — candidate next betas)

- `recipes.local.toml` machine override (gitignored), same load pattern as `recipes.d/` (topgrade `.local.toml`)

### Differentiators (M — core of medium-term roadmap)

- Diff preview in dry-run (`-d`): show file diffs, not just "would run" (chezmoi `diff`, copier) — pure read path
- Recipe inheritance: `extends` in TOML for base recipes (mise templates, copier) — continues `template_base` / `template_rel` line
- Declarative conditionals: `when = "variant == 'bun'"` on files/steps (cargo-generate, nx) — stack branching without Rust if-logic
- Interactive recipe picker when no args, gated on TTY only (just `--choose`)
- `modify` / `inject` action in `[files]` to append into existing files (hygen, plop)

### Vision (L — only if product grows, explicit user request)

- Update-in-place / re-sync of scaffolded projects (copier `update`, cruft) — `state.toml` already tracks projects; needs 3-way merge
- Shared recipe catalog / gallery (cookiecutter catalog) — only if social ecosystem wanted; related to deferred `fa sync`

### Rejected (violates principles)

- Shell completions (dynamic or static scripts) → unnecessary maintenance overhead; commands are short and direct
- Per-invocation long flags (`--ts --eslint…`) → breaks Short Commands; use TOML toggles
- JS plugin ecosystems (yeoman/nx/plop custom actions) → heavy deps, not lightweight
- AST transforms, project graphs, remote cache, watch daemons → engine stops being dumb
- Secrets/password-manager integration (chezmoi) → token-leak surface
- Per-stack hardcoded branches in Rust → absolute Recipe Player violation

## Future (explicitly not implemented)

- `fa sync` catalog refresh, pager/`less` output, `[apply]` — referenced nowhere in code; only build on explicit user request
