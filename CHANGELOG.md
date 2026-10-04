# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Custom argument signatures and documentation (`args`): explicitly declare positional argument names and documentation for command aliases using structured token arrays (`args = ["<input>", "[output]"]`), object arrays (`args = [{ name = "input", description = "..." }]`), or table syntax (`[alias.sec.cmd.args]`). Displays custom signatures in `fa list` and namespace listings, and renders usage signatures and aligned argument documentation in `fa --show <command>`. Rejects ambiguous single strings.
- `[alias]` table synonym: support singular `[alias.<section>]` and `[alias.<section>.<command>]` interchangeably with plural `[aliases]`, allowing cleaner single-command definitions while merging coexisting tables seamlessly; full editor schema and source tracking support.

- Alias environment variable injection (`env`, `env_force`, `_env`, `_env_force`): declare custom environment variables scoped to aliases or namespaces directly in TOML. Supports fallback mode (`env`, `_env` applied only if unset in system) and forced mode (`env_force`, `_env_force` overriding terminal environment). Values dynamically expand tildes (`~`), existing variables (`$VAR`, `${VAR}`), and fallback syntax (`${VAR:-default}`).
- Bash-style export syntax for alias environments: supply environment variables as strings using standard Bash syntax (`env_force = 'export MODEL="qwen.gguf" && export THREADS="8"'`), with automatic parsing of assignments separated by `&&`, `;`, newlines, or spaces.
- Static template variables (`_vars` and `[vars]`): declare reusable template placeholders in section tables (`_vars`) or at root (`[vars]`) substituted automatically via `{{KEY}}` across `command`, `description`, `env`, and `env_force`, with recursive/transitive resolution.
- Git-style typo suggestions ("Did you mean?"): suggest closest valid matches using Jaro-Winkler distance on typo'd top-level subcommands, namespaced subcommands, and recipe names.
- Dedicated environment variables guide: added `docs/environment.md` detailing fallback vs. force resolution modes, scopes, priority hierarchy, and real-world examples.
- Namespaced command aliases (`[aliases.":<namespace>"]`): group commands under isolated `:`-prefixed namespace sections called with multi-word syntax (`fa <namespace> <command>` or `fa :<namespace> <command>`). Invoking `fa <namespace>` directly executes the root command if one with the same name exists (`skills = { ... }`), or prints the namespace's available subcommands and usage help if not. Commands inside a namespace are strictly isolated and never collide with global root aliases.
- Git-style `[alias]` in `~/.config/fa/config.toml`: configure custom native command shortcuts (`rn = "--recipe new"`, `st = "--list"`) and external shell commands prefixed with `!` (`ac = "!git add -A && git commit -m"`).
- Clean minimal recipe scaffolds & alias preset: `fa --recipe new <name>` now generates an ultra-clean 8-line scaffold by default; preserved `standard` and `pack` templates and added the `alias` preset (`fa -r new <name> alias`) for fast alias catalog scaffolding.
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

- Resilient recipe editing and diagnostics: `fa -r edit` and `fa -r validate` now continue gracefully when config files have syntax or parse errors instead of aborting before opening the editor; added support for `fa -r edit recipes` to edit primary config, and raw text searching for malformed files.
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
