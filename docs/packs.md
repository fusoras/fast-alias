# Packs — Modular Component & Asset Bundling

The **Packs** system in `fa` allows you to bundle and install reusable components and assets (such as web components, UI elements, or config files) directly into your current project without creating a separate recipe for every single component.

It decouples **where files are stored** (`templates/`) from **how they are grouped** (`packs/`).

---

## Quickstart: Minimal Packs Recipe (Inline)

The fastest and most minimalist way to use packs without creating extra directories or files is defining packs **inline** inside your recipe.

Scaffold a new recipe file with:

```bash
fa recipe new wc-lib
```

This creates `~/.config/fa/recipes.d/wc-lib.toml` and opens it in your default `$EDITOR`.

Configure it with inline packs:

```toml
[recipes.wc-lib]
name          = "Web Components"
description   = "Scaffold and install modular web components"
templates_dir = "templates/wc-lib"

[recipes.wc-lib.packs.default]
description   = "Default component bundle"
components    = ["toggle-theme", "btn-ally"]

[[recipes.wc-lib.steps]]
create        = { from = "{{templates_dir}}/{{component}}", to = "src/components/{{component}}" }
description   = "Install component files into src/components/"
```

Your component folders live inside `~/.config/fa/templates/wc-lib/<component-name>/`.

Install them into your project:

```bash
fa new wc-lib          # Lists available packs and components (or runs default if configured)
fa new wc-lib default  # Installs the 'default' pack bundle
fa new wc-lib btn-ally # Installs only the 'btn-ally' component
```

> [!TIP]
> **Recommended Organization**:
> For larger component libraries, we recommend decoupling packs into a dedicated folder (`~/.config/fa/packs/<recipe>/`). See [Option A: One Pack per File](#option-a-one-pack-per-file-default) and [Recommended Directory Layout](#31-recommended-layout-separated-packs--templates).

---

## Detailed Features & Reference

- [Recipe File Configuration with External Packs](#1-creating-the-recipe-file-with-external-packs)
- [What is a Component? (Atomic folder structure)](#21-what-is-a-component)
- [What is a Pack? (Organization options)](#22-what-is-a-pack)
  - [Option A: One Pack per File (Default)](#option-a-one-pack-per-file-default)
  - [Option B: Multiple Packs in a Single TOML File](#option-b-multiple-packs-in-a-single-toml-file)
  - [Option C: Inline Packs in Recipe Files](#option-c-inline-packs-in-recipe-files)
- [Global Behavior Configuration (`default_behavior`)](#23-running-fa-new-without-arguments-global-configuration)
- [Installing Components into Your Project](#24-installing-components-into-your-project)
- [Directory Layouts (Separated vs Inside Templates)](#3-directory-layouts)

---

## 1. Creating the Recipe File with External Packs

When you prefer separate pack files, configure `packs_dir` in your recipe file (`~/.config/fa/recipes.d/wc-lib.toml`):

```toml
[recipes.wc-lib]
name          = "Web Components Library"
description   = "Scaffold and install modular web components"
packs_dir     = "packs/wc-lib"
templates_dir = "templates/wc-lib"
# default_pack = "default" # Requires default_behavior = "default" in config.toml

[[recipes.wc-lib.steps]]
create = { from = "{{templates_dir}}/{{component}}", to = "src/components/{{component}}" }
description = "Install component files into src/components/"
```

- **`packs_dir`**: The folder where your pack definition files live (under `~/.config/fa/`). Always namespace this per recipe (e.g., `packs/wc-lib/`) so different recipes can each have a `default.toml` without naming collisions.
- **`templates_dir`**: The folder where your component source folders are stored (under `~/.config/fa/`).
- **`default_pack`**: Optional default pack name used when `default_behavior = "default"` is enabled in `~/.config/fa/config.toml`.
- **`create`**: Copies component files from `{{templates_dir}}/{{component}}` into your project's `src/components/{{component}}`.

Validate the recipe syntax anytime with:

```bash
fa recipe validate wc-lib
```

---

## 2. Where & How to Create Packs

### 2.1 What is a Component?

A component is simply a **folder** inside your `templates_dir` containing the files for that component.

**The component name is literally the folder name.**

For example, inside `~/.config/fa/templates/wc-lib/`:

```text
~/.config/fa/templates/wc-lib/
├── toggle-theme/               <-- Component name is "toggle-theme"
│   ├── toggle-theme.astro
│   └── toggle-theme.js
├── btn-ally/                   <-- Component name is "btn-ally"
│   ├── btn-ally.astro
│   └── btn-ally.js
└── wc-modal/                   <-- Component name is "wc-modal"
    ├── wc-modal.astro
    └── wc-modal.js
```

Any files you put inside that folder will be copied as-is into your project.

> [!TIP]
> **Zero-Maintenance & Atomic Components**:
> - Whenever you build a new component, simply create a new folder under `~/.config/fa/templates/wc-lib/<component-name>/`. You **never** need to touch Rust code or edit your recipe.
> - Keep component folders atomic: include only the files necessary for that component, and avoid relative imports pointing outside the folder so each component can be installed independently.

### 2.2 What is a Pack?

A pack defines a named bundle that lists which component folders belong together. `fa` offers complete flexibility for organizing packs:

#### Option A: One Pack per File (Default)
Create small `.toml` files inside `packs_dir` (`~/.config/fa/packs/wc-lib/`):

```bash
mkdir -p ~/.config/fa/packs/wc-lib
```

Create your default pack (`~/.config/fa/packs/wc-lib/default.toml`):

```toml
# ~/.config/fa/packs/wc-lib/default.toml
name = "default"
components = [
    "toggle-theme",    # Matches folder: templates/wc-lib/toggle-theme/
    "btn-ally",        # Matches folder: templates/wc-lib/btn-ally/
]
```

You can create additional packs to group different sets of components (e.g., `~/.config/fa/packs/wc-lib/wc-ui.toml`):

```toml
# ~/.config/fa/packs/wc-lib/wc-ui.toml
name = "wc-ui"
components = [
    "toggle-theme",
    "btn-ally",
    "wc-modal",
]
```

#### Option B: Multiple Packs in a Single TOML File
Instead of creating multiple `.toml` files, you can define multiple packs inside a single file (such as `~/.config/fa/packs/wc-lib/packs.toml` or any `.toml` file in `packs_dir`). All three styles are supported:

**1. Table of packs (`[packs.<name>]`)**:
```toml
# ~/.config/fa/packs/wc-lib/packs.toml
[packs.default]
description = "Default starter components"
components = ["toggle-theme", "btn-ally"]

[packs.wc-ui]
description = "Complete UI components pack"
components = ["toggle-theme", "btn-ally", "wc-modal"]
```

**2. Array of tables (`[[packs]]`)**:
```toml
[[packs]]
name = "default"
components = ["toggle-theme", "btn-ally"]

[[packs]]
name = "wc-ui"
components = ["toggle-theme", "btn-ally", "wc-modal"]
```

**3. Top-level tables (`[<pack_name>]`)**:
```toml
[default]
components = ["toggle-theme", "btn-ally"]

[wc-ui]
components = ["toggle-theme", "btn-ally", "wc-modal"]
```

#### Option C: Inline Packs in Recipe Files
You don't even need a separate `packs_dir` folder if you want to declare packs directly inside your recipe file (`~/.config/fa/recipes.d/wc-lib.toml`):

```toml
[recipes.wc-lib]
name = "Web Components Library"
description = "Modular components"
templates_dir = "templates/wc-lib"

[recipes.wc-lib.packs.default]
description = "Default bundle"
components = ["toggle-theme", "btn-ally"]

[recipes.wc-lib.packs.wc-ui]
description = "Full UI bundle"
components = ["toggle-theme", "btn-ally", "wc-modal"]

[[recipes.wc-lib.steps]]
create = { from = "{{templates_dir}}/{{component}}", to = "src/components/{{component}}" }
```

To include a new or existing component in any pack, simply add its folder name to the `components = [...]` array in the corresponding pack definition.

### 2.3 Running `fa new` Without Arguments (Global Configuration)

The `default_behavior` setting in `~/.config/fa/config.toml` controls what `fa new <recipe>` does without a pack or component: `list` (default), `default`, or `error`.

For full configuration details, see [Global Pack Configuration](config.md#running-fa-new-without-arguments-global-pack-configuration).

### 2.4 Installing Components into Your Project

Navigate to your target project folder and run `fa new`:

#### Install a Specific Pack
Pass the pack name as the second argument:
```bash
fa new wc-lib wc-ui
```
*Copies all components declared in `wc-ui.toml` directly into your project.*

#### Install a Single Component
Pass the component folder name directly:
```bash
fa new wc-lib toggle-theme
```
*Copies only `toggle-theme/` into `src/components/toggle-theme/`.*

---

## 3. Directory Layouts

### 3.1 Recommended Layout (Separated Packs & Templates)

The recommended structure keeps your pack lists in `packs/` and your component source files in `templates/`:

```text
~/.config/fa/
│
├── recipes.d/
│   └── wc-lib.toml                  # 1. Recipe configuration (`fa recipe new wc-lib`)
│
├── packs/
│   └── wc-lib/                      # 2. Pack lists (grouping folders together)
│       ├── default.toml             #    components = ["toggle-theme", "btn-ally"]
│       └── wc-ui.toml               #    components = ["toggle-theme", "btn-ally", "wc-modal"]
│
└── templates/
    └── wc-lib/                      # 3. Component folders & source files
        ├── toggle-theme/            #    -> Folder name matches string in components array
        │   ├── toggle-theme.astro
        │   └── toggle-theme.js
        ├── btn-ally/                #    -> Folder name matches string in components array
        │   ├── btn-ally.astro
        │   └── btn-ally.js
        └── wc-modal/
            ├── wc-modal.astro
            └── wc-modal.js
```

When installed, the files land directly inside your project:
```text
my-project/
└── src/
    └── components/
        ├── toggle-theme/
        │   ├── toggle-theme.astro
        │   └── toggle-theme.js
        └── btn-ally/
            ├── btn-ally.astro
            └── btn-ally.js
```

### 3.2 Alternative Layout (Packs Inside Templates)

If you prefer to keep everything inside `templates/`, change `packs_dir` in your recipe to `"templates/packs/wc-lib"`:

```text
~/.config/fa/
├── recipes.d/
│   └── wc-lib.toml                  # packs_dir = "templates/packs/wc-lib"
└── templates/
    ├── wc-lib/                      # Component folders
    │   ├── toggle-theme/
    │   └── btn-ally/
    └── packs/
        └── wc-lib/                  # Pack TOML files
            └── default.toml
```
