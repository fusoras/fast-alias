# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Strict prefixed native commands: all built-in commands now strictly require long (`--new`, `--list`, `--search`, `--show`, `--recipe`, `--template`, `--alias`, `--self-update`, `--self-uninstall`) or short flags (`-n`, `-l`, `-se`, `-sh`, `-r`, `-t`, `-a`), reserving all unprefixed words exclusively for user aliases to completely prevent name collisions. Subcommands (`--recipe new`, `--template add`) remain clean without prefixes.
- `fa --list` hybrid filtering: filter catalog output by category using dedicated flags (`-r` / `--recipes`, `-a` / `--aliases`, `-p` / `--packs`) or positional arguments (`recipes`, `aliases`, `packs`), with support for combining flags (e.g. `fa -l -r -p`).
- Packs catalog display (Option 2): `fa --list -p` formats packs showing the bundle name and description (`• <name> · <description>`) with indented components (`components: <comp1>, ...`).
- `fa --template add <recipe> <path>...` (`fa -t add`): copy files or directories recursively into a recipe's template directory under `~/.config/fa/templates/` (respecting `template_base` and `templates_dir`), with interactive confirmation on overwrite and `-f` / `--force` to skip prompts.
- Packs global behavior & default pack: added `~/.config/fa/config.toml` support with `packs.default_behavior` (`list`, `default`, `error`); renamed recipe field to `default_pack` (with backwards-compatible `default` alias); validated that recipes declaring `default_pack` require `default_behavior = "default"` in global config.
- Packs system: install modular components and asset bundles into the current project via `fa new <recipe> [pack/component]` using `packs_dir`, `templates_dir`, and `[[steps]]` `create` declarations with dynamic `{{component}}` substitution; includes `docs/packs.md`.
- `final_message` recipe option: customize the post-scaffold success message in `recipes.toml` / `recipes.d/*.toml` with automatic variable substitution (`{{name}}`, `{name}`).
- Neutral post-scaffold navigation hint: `Run 'cd <project-name>' to go to project` replaces the hardcoded `node --run dev` fallback.
- `fa show`: displays `Final Message:` for recipes, showing the configured custom message or the default navigation hint.
- `fa recipe` (`new` / `edit` / `validate`): create or edit recipe files under `~/.config/fa/recipes.d/` opened in `$VISUAL`/`$EDITOR`; `validate` checks all config files or a single recipe by name and reports parse errors plus duplicate recipe keys. No flags, no trust prompt; new unit tests in `src/recipe.rs`.

### Fixed

- CLI help column alignment: dynamically calculates maximum command name width in `format_help_with_inline_aliases` with guaranteed 2-space padding, fixing an issue where command flags and descriptions were rendered without spacing.
- CLI help: `fa <command> --help` / `fa help <command>` now render that command's own help instead of always falling back to the top-level help (`render_cli_help` preserves subcommand context, e.g. `fa recipe --help`, `fa help recipe new`); top-level help keeps inline aliases.

## [v0.1.0-beta.4] - 2026-09-21

### Added

- `fa list`: group aliases by `[aliases.<section>]` section with `  <section>:` header and blank line between groups.
- `fa list`: fallback to `general` section label for empty section names (`Config::FALLBACK_ALIAS_SECTION`, `display_section`).
- `fa list`: deterministic grouped output via `format_section_header` / `format_alias_groups`; recipes untouched, no new flags.
- `pin_versions = true` recipe toggle: after the last install step, strip `^`/`~` prefixes from `package.json` versions (Node only) so the initial commit records exact versions (default: false).
- Native Node pinning (`src/pinning.rs`): byte-preserving rewrite of `dependencies`/`devDependencies`/`optionalDependencies`; reports Pinned/Unchanged/NotFound/Error; dry-run preview; other ecosystems plug into `pin_project`.
- `files` root shortcut: `"" = { from = "..." }` in `recipes.toml` writes the file to the project root, inferring the filename from the `from`/`template` basename.
- `template_base = "..."` recipe option: set a base directory under `~/.config/fa/templates/` so `from`/`template` paths shorten (e.g. with `template_base = "rust-stack"`, `".gitignore" = { from = ".gitignore" }` resolves to `rust-stack/.gitignore`).
- Compat note: when absent (default None) behavior is exactly legacy (`templates/...` and bare paths resolve identically); `templates/`-prefixed `from` ignores base; `..`/absolute rejected with clear `recipe/file` error.
