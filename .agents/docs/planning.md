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
- TOML UX enhancements: JSON Schema generation (`schema/recipe.schema.json` & fallback `~/.local/state/fa/recipe.schema.json`), clean minimal recipe scaffolding (`fa -r new <name>`), and `alias` scaffold preset (`fa -r new <name> alias`)
- Git-style `[alias]` in `~/.config/fa/config.toml`: native multi-token and single-token expansions alongside `!` shell commands
- Namespaced command aliases (`[aliases.":<namespace>"]`): isolated command namespaces with `fa <namespace> <command>` syntax, root command execution (`fa <namespace>`), automatic help fallback, and strict isolation preventing leakage into global aliases
- Interactive recipe picker on bare `fa -n` (TTY only): interactive numbered recipe selection menu and project directory prompt when invoked without arguments in an interactive terminal
- Namespace argument signatures in help (`fa <namespace>`): automatic extraction and display of positional argument signatures (`<arg1> <arg2>`, `<arg1> [arg2]`, `[args...]`, `<input> <output>`) beside command names in namespace listings and command help
- Variable typo suggestions in `fa -r validate`: automatic "Did you mean?" suggestions using Levenshtein distance for undeclared variable placeholders in `[[steps]]` and `[create]` that closely match declared `[variables]` or builtins

## Next 🔲

- Pinning beyond Node: `pin_versions` only handles `package.json`; decide TOML-declared per-ecosystem rules vs keep Node-only
- **Interactive Visual Configurator (`fa --config` / `fa -co`)**: Interactive visual menu for managing `~/.config/fa/config.toml` with arrow keys (`↑`/`↓`) and `Enter` selection. Focus is on maximum ease of use.
  - **Menu Navigation & Features (All UI in English)**:
    - **Packs Default Behavior**: Select between `"list"`, `"default"`, or `"error"`.
    - **Visual Alias Manager**:
      - Shows an aligned table of all currently defined aliases (`alias -> original command`).
      - Top action button: `[+ Add new alias]`.
      - Interactive prompt for alias name followed by a filterable / arrow-selectable picker of native `fa` commands (or custom `!` shell commands).
    - **Open in Editor**: Launch `$EDITOR` (or fallback to `nano`/`vim`) directly from the menu.
    - **Reset to Defaults**: Restore initial clean configuration state.
    - **Cancel / Save Confirmation**: Clear exit confirmation; `Esc`/`q` exits without altering disk.
  - **Sparse Configuration & Comment Preservation**:
    - **Sparse persistence (deltas only)**: Default values are omitted from disk; `config.toml` only reflects explicit user modifications to keep the file clean.
    - **Comment preservation**: Uses AST-preserving TOML manipulation (`toml_edit`) so user comments, whitespace, and formatting are strictly preserved upon save.
  - **Architecture Decision (Pending Selection)**:
    - **Option A (Lightweight interactive library, e.g., `inquire` or `dialoguer`)**: ~120–180 lines in `fa`, +1 crate. Recommended given the need for text input prompts (`[+ Add new alias]`) and filterable command list selection.
    - **Option B (Native zero-dependency / Raw Mode ANSI basic)**: ~400–600 lines in `fa` (handling raw mode, cursor movement, backspace, string buffers, and list filtering manually), 0 additional crates.
  - **Safety & Platform Compatibility**:
    - Strictly non-blocking: must inspect `std::io::stdin().is_terminal()` to gracefully exit or bypass in non-interactive/CI environments (Zero-Hang Policy).
    - Tested for flawless operation on both Debian and Termux (Android).

## Proposed features (research-backed, medium-term roadmap)

Cross-referenced against similar tools (copier, chezmoi, cargo-generate, mise, just, topgrade, nix templates, hygen, cookiecutter). Filtered through project principles: no new command flags (TOML toggles), dumb engine, lightweight deps, Debian + Termux.

### TOML Authoring UX & Assistance (S/M — planned)

- **JSON Schema for TOML editor integration**: generate/export JSON Schema (`~/.config/fa/schema/recipe.json`) with `#:schema` header support for in-editor autocomplete, hover documentation, and real-time linting in VS Code / Taplo / Neovim / Helix.
- **Enhanced CLI diagnostics in `fa --recipe validate`**: "Did you mean?" suggestions using Levenshtein distance for typos on keys/properties, plus semantic consistency checks (unresolved template paths, orphan variables in templates, `default_pack` vs `default_behavior` mismatches).
- **Contextual recipe scaffolds (`fa --recipe new`)**: support specialized recipe scaffolding (e.g. standard project vs component/pack library preset) with comprehensive inline guidance and comments.

### Differentiators (M — core of medium-term roadmap)

- Diff preview in dry-run (`-d`): show file diffs, not just "would run" (chezmoi `diff`, copier) — pure read path
- Recipe inheritance: `extends` in TOML for base recipes (mise templates, copier) — continues `template_base` / `template_rel` line
- Declarative conditionals: `when = "variant == 'bun'"` on files/steps (cargo-generate, nx) — stack branching without Rust if-logic
- Interactive recipe picker when no args, gated on TTY only (just `--choose`)
- `modify` / `inject` action in `[files]` to append into existing files (hygen, plop)

### Vision (L — only if product grows, explicit user request)

- Update-in-place / re-sync of scaffolded projects (copier `update`, cruft) — `state.toml` already tracks projects; needs 3-way merge

### Rejected (violates principles or discarded per user decision)

- `fa sync` & remote catalog syncing → discarded per user decision; keep configurations local in `~/.config/fa/` or personal dotfiles
- `fa alias --dry-run` → discarded per user decision; command inspection is handled cleanly via `fa --show` / `fa -sh`
- Automatic pager (`$PAGER` / `less`) → discarded per user decision; output is printed directly to `stdout` for fast shell interaction and Termux ergonomics
- `recipes.local.toml` machine override → discarded per user decision; keep configurations in `recipes.d/` or dotfiles
- Shell completions (dynamic or static scripts) → unnecessary maintenance overhead; commands are short and direct
- Per-invocation long flags (`--ts --eslint…`) → breaks Short Commands; use TOML toggles
- JS plugin ecosystems (yeoman/nx/plop custom actions) → heavy deps, not lightweight
- AST transforms, project graphs, remote cache, watch daemons → engine stops being dumb
- Secrets/password-manager integration (chezmoi) → token-leak surface
- Per-stack hardcoded branches in Rust → absolute Recipe Player violation

