# Packs — Modular Component & Asset Bundling

The **Packs** system in `fa` allows you to bundle and install reusable components and assets (such as web components, UI elements, or config files) directly into your current project without creating a separate recipe for every single component.

It decouples **where files are stored** (`templates/`) from **how they are grouped** (`packs/`).

---

## 1. Creating the Recipe File

Every pack workflow starts with a recipe configuration file in `~/.config/fa/recipes.d/`.

### 1.1 Scaffold the Recipe

Use the built-in `fa recipe new` command:

```bash
fa recipe new wc-lib
```

This creates a new file at `~/.config/fa/recipes.d/wc-lib.toml` and opens it in your default `$EDITOR`.

### 1.2 Write the Recipe Configuration

Replace the file content with the following configuration:

```toml
[recipes.wc-lib]
name          = "Web Components Library"
description   = "Scaffold and install modular web components"
packs_dir     = "packs/wc-lib"
templates_dir = "templates/wc-lib"
default       = "default"

[[recipes.wc-lib.steps]]
create = { from = "{{templates_dir}}/{{component}}", to = "src/components/{{component}}" }
description = "Install component files into src/components/"
```

#### What Each Setting Does:
- **`packs_dir`**: The folder where your pack definition files will live (under `~/.config/fa/`).
- **`templates_dir`**: The folder where your actual component files and folders are stored (under `~/.config/fa/`).
- **`default`**: The pack that gets installed when you run `fa new wc-lib` without any arguments.
- **`create`**: Tells `fa` to copy the files from `{{templates_dir}}/{{component}}` into your project's `src/components/{{component}}`.

### 1.3 Validate the Recipe

Check that your recipe syntax is valid:

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

Any files you put inside that folder (e.g. `.astro`, `.js`, `.css`, `.ts`) will be copied as-is into your project.

### 2.2 What is a Pack?

A pack is a small `.toml` file located inside your `packs_dir` (`~/.config/fa/packs/wc-lib/`).

It defines a named bundle that lists which component folders belong together:

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

You can create additional packs to group different sets of components. For example, a larger UI pack (`~/.config/fa/packs/wc-lib/wc-ui.toml`):

```toml
# ~/.config/fa/packs/wc-lib/wc-ui.toml
name = "wc-ui"
components = [
    "toggle-theme",
    "btn-ally",
    "wc-modal",
]
```

### 2.3 Installing Components into Your Project

Navigate to your target project folder and run `fa new`:

#### Install the Default Pack
Runs the pack specified by `default = "default"` in your recipe:
```bash
fa new wc-lib
```
*Copies `toggle-theme/` and `btn-ally/` directly into your project's `src/components/` directory.*

#### Install a Specific Pack
To choose a different pack, set `FA_PACK`:
```bash
FA_PACK=wc-ui fa new wc-lib
```
*Copies all three components (`toggle-theme`, `btn-ally`, `wc-modal`) into your project.*

#### Install a Single Component
To install only one specific component folder without installing an entire pack, set `FA_COMPONENT`:
```bash
FA_COMPONENT=toggle-theme fa new wc-lib
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

---

## 4. Recommendations & Best Practices

1. **How to Add New Components (Zero-Maintenance)**:
   Whenever you build a new component, just create a new folder under `~/.config/fa/templates/wc-lib/<component-name>/` and put its files inside. You **never** need to touch Rust code or modify your recipe file.
2. **Add to Packs When Needed**:
   To include your new component in an existing pack, simply add its folder name to the `components = [...]` array in that pack's `.toml` file.
3. **Keep Component Folders Atomic**:
   Each component folder should contain only the files necessary for that component. Avoid relative imports pointing outside the component folder so each component can be installed independently.
4. **Namespace Packs Per Recipe**:
   Always place pack files inside a subfolder matching the recipe name (e.g. `packs/wc-lib/` and `packs/rust-lib/`). This ensures two recipes can both have a `default.toml` without conflicts.
5. **Distinctive Naming**:
   Use clear, descriptive names for recipes and packs (such as `wc-lib`, `wc-ui`, `ds-buttons`). Avoid generic names like `app`, `pack`, or `components`.
