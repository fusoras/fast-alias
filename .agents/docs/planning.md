# Planning & Roadmap — fa

`fa` (fast-alias) `v0.1.0-beta.3`: recipe-player project scaffolder — bootstraps projects, installs dependencies, applies tooling (linter, formatter, typechecker) and config files on **Debian** and **Termux**. All stack knowledge lives in user TOML (`~/.config/fa/recipes.toml` + `recipes.d/*.toml` + `templates/`); the engine stays dumb. Docs split: `docs/` is user-only (`commands.md`, `recipes.md`); `.agents/docs/` is agent-internal (5 files).

## Implemented commands

- `fa new <recipe> <name>` (`-v` variant, `-d` dry-run, `--no-install`) · `fa list` (`-s` show-hidden) · `fa search <query>` · `fa show <recipe|command>` · `fa alias <name> [args]` (plus `fa -n` / `fa -a` shorthands, direct `fa <alias>` invocation) · `fa self-update` / `fa self-uninstall` (`-d`, `-y`/`-n`) · `fa --version` (offline cache + background `update-check`)
- Pipeline: create → files (`inline` / `from` / `template`) → pm install → `pin_versions` (Node-only) → steps → `final_message` + generic `node --run dev` fallback hint
- Safety: dependency preflight on `PATH`, one-time `[TRUST]` per config path in `state.toml`, auto-`y` on non-TTY, POSIX `install.sh` (musl `x86_64`/`aarch64`, `~/.local/bin` / `$PREFIX/bin`)

## Done ✅

- Scaffolder core: create strategies, files from/inline/template, platform-gated steps/install, variants, prompts + `{{var}}` templating
- pm install + tooling (linter/formatter/checker) + `pin_versions` for Node (`src/pinning.rs`, byte-preserving, dry-run preview)
- Generic dev fallback hint, per-recipe `final_message` printed after `fa new`
- `[TRUST]` per config path (one covers `recipes.d/`), example config auto-trust + first-run provisioning
- `install.sh`, self-update/self-uninstall, async `--version` check
- `list` / `search` / `show` / `alias` (section-grouped, case-insensitive, arg passthrough), dependency preflight
- Docs split (`docs/` vs `.agents/docs/`), 40-line inline limit with `templates/` overflow
- `template_rel` normalization: `from`/`template` with or without `templates/` prefix resolve equally; traversal/absolute rejected with a friendly error
- Release asset names single-sourced from `resolve_asset_name` (`src/update.rs`), kept in sync with `install.sh` / `release.yml`
- Branch workflow: `feature/*` for complex, direct to `develop` for tiny

## Next 🔲

- Pinning beyond Node: `pin_versions` only handles `package.json`; decide TOML-declared per-ecosystem rules vs keep Node-only
- Commands drift: `docs/commands.md` + `README.md` document `fa sync`, `fa alias --dry-run`, and `$PAGER` paging — none exist in `Commands` (`src/main.rs`); implement or remove from docs
- `fa show` gaps: doesn't display `final_message`; add per-recipe `dev_hint` to replace the hardcoded `node --run dev` fallback

## Future (explicitly not implemented)

- `fa sync` catalog refresh, pager/`less` output, `[apply]` — referenced nowhere in code; only build on explicit user request
