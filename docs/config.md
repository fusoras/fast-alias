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
b  = "!git branch"
s  = "!git switch"
st = "!git status"
ac = "!git add -A && git commit -m"
```

Invoking `fa rn my-app` expands to `fa --recipe new my-app`. Invoking `fa ac "feat: init"` executes the shell command with appended arguments.

## Running `fa new` Without Arguments (Global Pack Configuration)

You can configure what happens when you run `fa new <recipe>` without specifying a pack or component in `~/.config/fa/config.toml`:

```toml
# ~/.config/fa/config.toml
[packs]
# Available modes: "list" (default), "default", "error"
default_behavior = "list"
```

1. **`default_behavior = "list"` (Default mode)**:
   Running `fa new wc-lib` displays a formatted list of all available packs in `packs_dir` and components in `templates_dir`:
   ```bash
   fa new wc-lib
   ```
   *Note: If a recipe specifies `default_pack` while `default_behavior` is set to `list` or `error`, `fa` reports a validation error prompting you to set `default_behavior = "default"` in `~/.config/fa/config.toml`.*

2. **`default_behavior = "default"`**:
   Automatically runs the pack specified by `default_pack = "<pack-name>"` in your recipe.

3. **`default_behavior = "error"`**:
   Immediately halts with an error requiring an explicit pack or component argument.

## Related Documentation

- For the full CLI reference and command flags, see [commands.md](commands.md).
- For packs and components, see [packs.md](packs.md).
