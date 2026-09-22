# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `final_message` recipe option: customize the post-scaffold success message in `recipes.toml` / `recipes.d/*.toml` with automatic variable substitution (`{{name}}`, `{name}`).
- Neutral post-scaffold navigation hint: `Run 'cd <project-name>' to go to project` replaces the hardcoded `node --run dev` fallback.
- `fa show`: displays `Final Message:` for recipes, showing the configured custom message or the default navigation hint.

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
