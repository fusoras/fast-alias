# Implementation Plan — Recipe: `astro` (First Recipe)

This document specifies the detailed implementation of the first recipe for `fa`: the **Astro** flow that reproduces the user's current shell aliases as a single short command.

## 1. Source of Truth (current shell aliases)

The alias *names* are discarded — only the commands they execute matter:

```bash
# Scaffold base project (create-astro performs install + git init itself)
in-astro() { pnpm create astro@latest $1 --template basics --install --git }

# Initial dev-dependencies
in-devdep-astro(){ pnpm i -D lightningcss stylelint stylelint-config-standard stylelint-config-html vite-plugin-supersvg }
```

`fa` replaces both aliases with one command:

```bash
fa new astro myapp
fa new astro myapp -n   # --dry-run: preview everything
```

## 2. Expanded dev-dependencies (user adopts prettier + oxlint)

```bash
pnpm i -D \
  lightningcss \
  stylelint stylelint-config-standard stylelint-config-html \
  vite-plugin-supersvg \
  prettier prettier-plugin-astro \
  oxlint
```

## 3. Recipe schema (`recipes.toml`)

```toml
[recipes.astro]
name        = "Astro"
description = "Astro site with lightningcss, stylelint, supersvg, prettier and oxlint"
language    = "web · typescript"
variants    = ["pnpm"]

[create]
command = "pnpm create astro@latest {{name}} --template basics --install --git"

[pm]
install     = { pnpm = "pnpm add" }
dev_install = { pnpm = "pnpm add -D" }

[tooling]
linter    = { tool = "oxlint", script = "lint" }
formatter = { tool = "prettier", script = "format" }
check     = { tool = "@astrojs/check", script = "check" }

[[steps]]
command     = "pnpm i -D lightningcss stylelint stylelint-config-standard stylelint-config-html vite-plugin-supersvg prettier prettier-plugin-astro oxlint"
description = "Install initial dev-dependencies"
```

## 4. Execution flow of `fa new astro myapp`

1. **Create**: run `pnpm create astro@latest myapp --template basics --install --git` (scaffolds base, installs base deps, inits git).
2. **Files**: write config files on top of the scaffolded project.
3. **Steps**: run the dev-dependencies install step.
4. **Report**: print success message and next steps (`cd myapp && pnpm dev`).

## 5. Files & Configurations (user-provided)

Each config is stored as a template under `templates/astro/` and declared in `[files]` (or merged for `package.json`).

### 5.1 `.prettierrc`
```json
{
  "semi": false,
  "singleQuote": true,
  "jsxSingleQuote": false,
  "tabWidth": 2,
  "printWidth": 80,
  "trailingComma": "all",
  "useTabs": false,
  "arrowParens": "avoid",
  "bracketSpacing": true,
  "endOfLine": "lf",
  "plugins": ["prettier-plugin-astro"],
  "overrides": [
    {
      "files": "*.astro",
      "options": {
        "parser": "astro",
        "astroAllowShorthand": true
      }
    }
  ]
}
```

### 5.2 `.prettierignore`
```gitignore
# build output
dist/
.output/

# dependencies
node_modules/

# logs
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*

# environment variables
.env
.env.production

# macOS-specific files
.DS_Store

# Astro generated files
.astro/

# Lock files
package-lock.json
yarn.lock
pnpm-lock.yaml

# Agents
/.agents/

# VScode & Cursor settings
/.vscode/
/.cursor/
```

### 5.3 `.stylelintrc.json`
```json
{
  "extends": [
    "stylelint-config-standard",
    "stylelint-config-html",
    "stylelint-config-html/astro"
  ],
  "overrides": [{ "files": ["**/*.css", "**/.astro"] }],
  "ignorePath": ".gitignore",
  "rules": {
    "selector-nested-pattern": "^&",
    "color-named": "never",
    "color-function-notation": "modern",
    "function-disallowed-list": ["rgba", "hsla"],
    "function-name-case": "lower",
    "function-url-quotes": "always",
    "value-keyword-case": "lower",
    "declaration-no-important": true,
    "selector-pseudo-class-no-unknown": [
      true,
      {
        "ignorePseudoClasses": ["global"]
      }
    ]
  }
}
```

### 5.4 `.gitignore`
```gitignore
# build output
dist/

# generated types
.astro/

# dependencies
node_modules/

# logs
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*

# environment variables
.env
.env.production

# macOS-specific files
.DS_Store

# jetbrains setting folder
.idea/

.vscode/*
!extensions.json
!settings.json

# codegraph
.codegraph
```

### 5.5 `tsconfig.json`
```json
{
  "extends": "astro/tsconfigs/strict",
  "include": [".astro/types.d.ts", "**/*"],
  "exclude": ["dist"],
  "compilerOptions": {
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}
```

### 5.6 `pnpm-workspace.yaml`
```yaml
allowBuilds:
  esbuild: true
  sharp: true
# ***** Minimum age config *****
# only install package versions published at least 2 days ago
minimumReleaseAge: 2880 # minutes
# exclude trusted packages from the age gate
minimumReleaseAgeExclude:
  - typescript
  - svelte
  - react
  - react-dom
  - stylelint
```

### 5.7 `astro.config.mjs` (i18n removed by user)
```js
// @ts-check
import { defineConfig } from 'astro/config'
import supersvgPlugin from 'vite-plugin-supersvg'

// https://astro.build/config
export default defineConfig({
  vite: {
    css: {
      transformer: 'lightningcss',
      lightningcss: {
        drafts: {
          customMedia: true,
        },
      },
    },
    plugins: [supersvgPlugin()],
  },
})
```

### 5.8 `package.json` scripts (MERGED on top of create-astro output)
```json
{
  "scripts": {
    "dev": "astro dev",
    "build": "astro build",
    "preview": "astro preview",
    "astro": "astro",
    "lint": "oxlint",
    "lint:fix": "oxlint --fix",
    "prettier": "prettier --write .",
    "stylelint": "stylelint \"src/**/*.{css,scss,astro}\""
  }
}
```

### 5.9 `.oxlintrc.json`
```json
{
  "$schema": "./node_modules/oxlint/configuration_schema.json",
  "plugins": ["typescript", "unicorn", "oxc", "node"],
  "categories": {
    "correctness": "error"
  },
  "rules": {
    "unicorn/prefer-query-selector": "error",
    "unicorn/prefer-dom-node-text-content": "error",
    "unicorn/prefer-dom-node-append": "error",
    "unicorn/prefer-dom-node-remove": "error",
    "unicorn/prefer-dom-node-dataset": "error",
    "unicorn/prefer-classlist-toggle": "error",
    "unicorn/prefer-single-call": "error",
    "unicorn/prefer-add-event-listener": "error",
    "prefer-const": "error",
    "no-var": "error",
    "prefer-template": "error",
    "object-shorthand": "error",
    "prefer-destructuring": "error",
    "prefer-exponentiation-operator": "error",
    "promise/catch-or-return": "error",
    "unicorn/prefer-module": "error",
    "typescript/no-var-requires": "error",
    "unicorn/prefer-ternary": "error",
    "unicorn/no-static-only-class": "error",
    "unicorn/prefer-global-this": "error",
    "eqeqeq": ["error", "always", { "null": "ignore" }],
    "new-cap": [{ "newIsCap": true, "capIsNew": false, "properties": true }],
    "no-array-constructor": "error",
    "no-caller": "error",
    "no-case-declarations": "error",
    "no-cond-assign": "error",
    "no-debugger": "error",
    "no-extend-native": "error",
    "no-fallthrough": "error",
    "no-global-assign": "error",
    "no-iterator": "error",
    "no-labels": ["error", { "allowLoop": false, "allowSwitch": false }],
    "no-lone-blocks": "error",
    "no-loss-of-precision": "error",
    "no-misleading-character-class": "error",
    "no-prototype-builtins": "error",
    "no-useless-catch": "error",
    "no-new": "error",
    "no-new-func": "error",
    "no-object-constructor": "error",
    "no-new-wrappers": "error",
    "no-proto": "error",
    "no-redeclare": "error",
    "no-self-assign": ["error", { "props": true }],
    "no-self-compare": "error",
    "no-sequences": "error",
    "no-shadow-restricted-names": "error",
    "no-template-curly-in-string": "error",
    "no-unmodified-loop-condition": "error",
    "no-unneeded-ternary": ["error", { "defaultAssignment": false }],
    "no-unsafe-finally": "error",
    "no-unsafe-negation": "error",
    "no-unused-vars": [
      "error",
      { "args": "none", "caughtErrors": "none", "ignoreRestSiblings": true, "vars": "all" }
    ],
    "no-use-before-define": ["error", { "functions": false, "classes": false, "variables": false }],
    "no-useless-call": "error",
    "no-useless-computed-key": "error",
    "no-useless-constructor": "error",
    "no-useless-rename": "error",
    "no-useless-return": "error",
    "no-void": "error",
    "no-with": "error",
    "symbol-description": "error",
    "unicode-bom": ["error", "never"],
    "use-isnan": ["error", { "enforceForSwitchCase": true, "enforceForIndexOf": true }],
    "valid-typeof": ["error", { "requireStringLiterals": true }],
    "yoda": ["error", "never"],
    "promise/param-names": "error",
    "node/handle-callback-err": ["error", "^(err|error)$"],
    "node/no-exports-assign": "error",
    "node/no-new-require": "error",
    "node/no-path-concat": "error"
  },
  "env": {
    "builtin": true
  }
}
```

## 6. File-to-source mapping

| Destination file | Source | Mode |
|---|---|---|
| `.prettierrc` | `templates/astro/.prettierrc` | `from` |
| `.prettierignore` | `templates/astro/.prettierignore` | `from` |
| `.stylelintrc.json` | `templates/astro/.stylelintrc.json` | `from` |
| `.gitignore` | `templates/astro/.gitignore` | `from` |
| `tsconfig.json` | `templates/astro/tsconfig.json` | `from` |
| `pnpm-workspace.yaml` | `templates/astro/pnpm-workspace.yaml` | `from` |
| `astro.config.mjs` | `templates/astro/astro.config.mjs` | `from` |
| `package.json` | create-astro output | **merge scripts** |
| `.oxlintrc.json` | `templates/astro/.oxlintrc.json` | `from` |

## 7. Testing plan

- Parse recipe TOML + alias/variant resolution (`astro` resolves to recipe).
- Verify `create` command construction with `{{name}}`.
- Verify steps execution and platform filtering.
- Verify file writing (`from` / `inline`) and dry-run.
- Verify `package.json` scripts merge behavior.
- Verify `list` / `show` output formatting.

## 8. Out of scope (this phase)

- No binary / Rust code yet — only the markdown plan + config collection.
- Variants `bun`/`npm` and other recipes (`ts-lib`, `rust-cli`, `python`) in later phases.
