# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.1.0-beta.4] - Unreleased

### Added

- `fa list`: group aliases by `[aliases.<section>]` section with `  <section>:` header and blank line between groups.
- `fa list`: fallback to `general` section label for empty section names (`Config::FALLBACK_ALIAS_SECTION`, `display_section`).
- `fa list`: deterministic grouped output via `format_section_header` / `format_alias_groups`; recipes untouched, no new flags.
- `pin_versions = true` recipe toggle: after the last install step, strip `^`/`~` prefixes from `package.json` versions (Node only) so the initial commit records exact versions (default: false).
- Native Node pinning (`src/pinning.rs`): byte-preserving rewrite of `dependencies`/`devDependencies`/`optionalDependencies`; reports Pinned/Unchanged/NotFound/Error; dry-run preview; other ecosystems plug into `pin_project`.
