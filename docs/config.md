# Configuration

This document covers user-level configuration for `fa` that lives outside the recipe files.

## Git-Style Aliases (`~/.config/fa/config.toml`)

Configure custom command aliases directly under `[alias]` in `~/.config/fa/config.toml`. Supports native command shortcuts as well as external shell commands prefixed with `!`:

```toml
[alias]
# Native command shortcuts:
n  = "--new"
l  = "--list"
r  = "--recipe"
rn = "--recipe new"
rv = "--recipe validate"
re = "--recipe edit"
rm = "--recipe rm"

# External shell commands (prefixed with '!'):
rec = "!EDITOR=code fa --recipe edit"
b   = "!git branch"
s   = "!git switch"
ac  = "!git add -A && git commit -m"
```

Invoking `fa rn my-app` expands to `fa --recipe new my-app`. Invoking `fa ac "feat: init"` executes the shell command with appended arguments.

## UI Display (`[ui]`)

Controls how search-only keyword hints appear in `--search` and `--list`. Values are case-insensitive and unknown values fall back to the field default instead of breaking config loading.

```toml
# ~/.config/fa/config.toml
[ui]
# Icon style: "unicode" (default) or "nerd-font"
icons = "unicode"
# Boolean alias for icons = "nerd-font"
nerd_fonts = false
# `fa --search` suffix: "off" | "matched" (default) | "all"
search_keywords = "matched"
# `fa --list` suffix: "off" | "all" (default)
list_keywords = "all"
```

| Key | Default | `off` | `matched` | `all` |
| --- | ------- | ----- | --------- | ----- |
| `search_keywords` | `"matched"` | No keyword suffix | Only keywords that matched the query | Every declared keyword |
| `list_keywords` | `"all"` | No keyword suffix | Treated as `off` (no query) | Every declared keyword |

Accepted synonyms:

- `on`, `true`, `yes`, `1`, `all` → `all`
- `off`, `false`, `no`, `0` → `off`
- `matched` → `matched`

Displayed hints use `‹a, b, c›` and show at most three keywords; hidden ones collapse into `+N` (for example `‹a, b, c, +2›`).

With `icons = "nerd-font"` (or the legacy boolean `nerd_fonts = true`), the `‹ ›` markers become a single tags glyph (`nf-fa-tags`, U+F02B) before the list, keeping the same green tone and `+N` truncation. Unicode mode is unchanged. `KEYWORD_ICON_NERD` and `KEYWORD_COLOR` in `src/engine.rs` are the single places to change the glyph and the color.

## Running `fa --new` Without Arguments (Global Pack Configuration)

You can configure what happens when you run `fa --new <recipe>` without specifying a pack or component in `~/.config/fa/config.toml`:

```toml
# ~/.config/fa/config.toml
[packs]
# Available modes: "list" (default), "default", "error"
default_behavior = "list"
```

1. **`default_behavior = "list"` (Default mode)**:
   Running `fa --new wc-lib` displays a formatted list of all available packs in `packs_dir` and components in `templates_dir`:
   ```bash
   fa --new wc-lib
   ```
   *Note: If a recipe specifies `default_pack` while `default_behavior` is set to `list` or `error`, `fa` reports a validation error prompting you to set `default_behavior = "default"` in `~/.config/fa/config.toml`.*

2. **`default_behavior = "default"`**:
   Automatically runs the pack specified by `default_pack = "<pack-name>"` in your recipe.

3. **`default_behavior = "error"`**:
   Immediately halts with an error requiring an explicit pack or component argument.

## Auto-Increment File Fallback Overflow (`[autoincrement]`)

Configure the default global overflow behavior when positional auto-increment placeholders (e.g. `${2:-output-{+##}.png}`) exhaust their numerical or character slots:

```toml
# ~/.config/fa/config.toml
[autoincrement]
# Overflow behavior when limit is reached:
# "overwrite" (default) -> caps at last file and overwrites it with warning
# "grow" -> expands number of digits or characters (e.g. 99 -> 100)
# "rotate" -> wraps back to beginning (e.g. 99 -> 01)
# "error" -> halts with an error
overflow = "overwrite"
```

### Precedence Hierarchy
1. **Inline Token Strategy**: `${2:-output-{+##:grow}.png}` (highest precedence).
2. **Alias-Level Setting**: `overflow = "rotate"` declared inside the command table in TOML.
3. **Global Setting**: `[autoincrement] overflow = "..."` in `~/.config/fa/config.toml`.
4. **Builtin Default**: `"overwrite"`.

## Search Keywords (`keywords`, `_keywords`)

Recipes and aliases can declare `keywords` as search-only metadata for `fa --search` / `fa -se`. Keywords never change execution; they only improve discoverability. Both forms are accepted and normalized at load time (trimmed, lowercased, deduplicated):

```toml
# Array form
[aliases.git]
gco = { command = "git switch", description = "Switch branch", keywords = ["vcs", "branch"] }

# Comma/space-separated string form
[aliases.docker]
up = { command = "docker compose up -d", description = "Start containers", keywords = "compose, containers" }

# Section-level keywords: merged into every command in the section
[aliases.":skills"]
_keywords = ["agent", "tooling"]
ls = { command = "bunx tabernaculo list", description = "List installed skills" }

# Top-level `_keywords` applies to flat aliases under [aliases] / [alias]
[aliases]
_keywords = "flat, root"
single = { command = "echo single" }
```

Section-level `_keywords` accepts the same array or string forms as per-command `keywords`. They are merged with command keywords preserving the first occurrence. Keywords also support `{{var}}` substitution from `[vars]` and the section's `_vars`.

## Related Documentation

- For the full CLI reference and command flags, see [commands.md](commands.md).
- For packs and components, see [packs.md](packs.md).
- For variable interpolation and template variables, see [variables.md](variables.md).
