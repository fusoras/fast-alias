# Implementation Notes — Personal Config Model

This document describes how `fa` resolves configuration, templates, and aliases entirely from the user's personal config directory instead of embedding them in the binary.

## 1. Configuration Model

`fa` ships as a **recipe player** with zero bundled recipes. All catalog data lives in the user's config directory:

| Data | Location |
|---|---|
| Primary catalog | `~/.config/fa/recipes.toml` |
| Modular catalogs | `~/.config/fa/recipes.d/*.toml` |
| Template files | `~/.config/fa/templates/<path>` |
| Example config | `~/.config/fa/recipes.toml` (provisioned on first run) |

### 1.1 First-run provisioning

When `~/.config/fa/` does not exist, `fa` creates it and writes a minimal example catalog (one `example` alias). This gives new users a starting point. Deleting the example leaves an empty catalog — `fa list` then shows no recipes and no aliases.

## 2. Template Resolution

File specs use `{ from = "templates/<path>" }` or `{ template = "templates/<path>" }`. At runtime the engine:

1. Strips the `templates/` prefix (`templates/my-recipe/Layout.tsx` → `my-recipe/Layout.tsx`).
2. Resolves it against `~/.config/fa/templates/` on disk (`read_user_template` in `src/engine.rs`).

Templates are read from disk on every invocation, so editing a template file takes effect immediately — no recompilation and no `build.rs`.

## 3. Why not embedded templates

- **No secret leakage in binaries**: `include_str!`/`include_bytes!` bake committed files into the release executable. Runtime reads keep the binary free of user config.
- **Recipe Player Principle**: adding a language/toolchain is config only — never Rust code.
- **User ownership**: users edit their own `~/.config/fa` without touching the project.

## 4. Security

- A one-time `[TRUST]` confirmation is requested before executing recipe `[[steps]]`/`create` commands (`fa new`) and command aliases (`fa alias`), since these run arbitrary shell. The decision is persisted by config path in `~/.local/state/fa/state.toml`; the same path is never re-prompted. The provisioned example config is trusted automatically.
- Never store secrets in `~/.config/fa/templates/` that you do not want on disk.
