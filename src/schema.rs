//! JSON Schema generation for fast-alias recipe and alias configuration files.

use std::fs;
use std::path::{Path, PathBuf};

/// JSON Schema (Draft-07) for fast-alias configuration files.
/// This enables real-time autocomplete, hover documentation, and validation
/// in editors supporting TOML LSP (Taplo, VS Code Even Better TOML, Neovim, Helix, Zed).
pub const RECIPE_SCHEMA_JSON: &str = r##"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "FastAliasRecipeConfig",
  "description": "Configuration schema for fast-alias (fa) recipe and alias files.",
  "type": "object",
  "properties": {
    "vars": {
      "type": "object",
      "description": "Global template variables substituted via {{KEY}} across command strings, descriptions, and environment variables.",
      "additionalProperties": { "type": "string" }
    },
    "recipes": {
      "type": "object",
      "description": "Map of recipe definitions, keyed by recipe name.",
      "additionalProperties": {
        "$ref": "#/definitions/Recipe"
      }
    },
    "aliases": {
      "type": "object",
      "description": "Map of alias sections containing named command aliases.",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "_vars": {
            "type": "object",
            "description": "Namespace template variables substituted via {{KEY}} within this section.",
            "additionalProperties": { "type": "string" }
          },
          "_env": {
            "oneOf": [
              { "type": "object", "additionalProperties": { "type": "string" } },
              { "type": "string" }
            ],
            "description": "Shared fallback environment variables for all commands in this namespace."
          },
          "_env_force": {
            "oneOf": [
              { "type": "object", "additionalProperties": { "type": "string" } },
              { "type": "string" }
            ],
            "description": "Shared forced environment variables for all commands in this namespace."
          }
        },
        "additionalProperties": {
          "$ref": "#/definitions/Alias"
        }
      }
    }
  },
  "definitions": {
    "Recipe": {
      "type": "object",
      "required": ["name", "description"],
      "properties": {
        "name": {
          "type": "string",
          "description": "Display name of the recipe."
        },
        "description": {
          "type": "string",
          "description": "A concise description of what this recipe scaffolds."
        },
        "language": {
          "type": "string",
          "description": "Primary programming language or ecosystem (e.g. rust, typescript, python)."
        },
        "variants": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Optional list of variants supported by this recipe."
        },
        "templates_dir": {
          "type": "string",
          "description": "Path to template files under ~/.config/fa/ (e.g. 'templates/my-recipe')."
        },
        "template_base": {
          "type": "string",
          "description": "Base directory prepended to relative 'from' and 'template' paths."
        },
        "packs_dir": {
          "type": "string",
          "description": "Directory where pack definition files live (e.g. 'packs/my-recipe')."
        },
        "default_pack": {
          "type": "string",
          "description": "Default pack bundle name when default_behavior is 'default'."
        },
        "packs": {
          "type": "object",
          "description": "Inline pack bundles declared directly in the recipe file.",
          "additionalProperties": {
            "$ref": "#/definitions/PackDefinition"
          }
        },
        "create": {
          "$ref": "#/definitions/Create",
          "description": "Strategy used to initialize the project directory."
        },
        "files": {
          "type": "object",
          "description": "Map of destination file paths to file creation specifications.",
          "additionalProperties": {
            "$ref": "#/definitions/FileSpec"
          }
        },
        "pm": {
          "$ref": "#/definitions/Pm",
          "description": "Package manager dependencies to install."
        },
        "tools": {
          "type": "array",
          "items": { "$ref": "#/definitions/Tool" },
          "description": "Tooling scripts (linter, formatter, typechecker)."
        },
        "steps": {
          "type": "array",
          "items": { "$ref": "#/definitions/Step" },
          "description": "Ordered post-scaffold shell commands or create steps."
        },
        "variables": {
          "type": "object",
          "description": "Interactive prompt variables with optional validation.",
          "additionalProperties": {
            "$ref": "#/definitions/Variable"
          }
        },
        "final_message": {
          "type": "string",
          "description": "Custom final message displayed on successful scaffold."
        },
        "pin_versions": {
          "type": "boolean",
          "description": "Whether to strip ^ and ~ from dependencies in package.json."
        }
      },
      "additionalProperties": false
    },
    "Alias": {
      "type": "object",
      "required": ["command"],
      "properties": {
        "command": { "type": "string", "description": "Shell command to execute." },
        "description": { "type": "string", "description": "Description of the alias." },
        "env": {
          "oneOf": [
            { "type": "object", "additionalProperties": { "type": "string" } },
            { "type": "string" }
          ],
          "description": "Fallback environment variables (applied only if not set in system)."
        },
        "env_force": {
          "oneOf": [
            { "type": "object", "additionalProperties": { "type": "string" } },
            { "type": "string" }
          ],
          "description": "Forced environment variables (always overrides system environment)."
        },
        "aliases": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Optional short alias names for this command."
        }
      },
      "additionalProperties": false
    },
    "PackDefinition": {
      "type": "object",
      "required": ["components"],
      "properties": {
        "description": { "type": "string", "description": "Description of the pack bundle." },
        "components": {
          "type": "array",
          "items": { "type": "string" },
          "description": "List of component folder names included in this pack."
        }
      },
      "additionalProperties": false
    },
    "Create": {
      "type": "object",
      "properties": {
        "command": { "type": "string", "description": "External command to scaffold the project (e.g. 'cargo new {{name}}')." },
        "template_dir": { "type": "string", "description": "Template directory to copy recursively." }
      },
      "additionalProperties": false
    },
    "FileSpec": {
      "type": "object",
      "properties": {
        "from": { "type": "string", "description": "Path to a static source file under templates/." },
        "inline": { "type": "string", "description": "Verbatim file contents (max 40 lines)." },
        "template": { "type": "string", "description": "Path to a template file with {{placeholder}} substitution." },
        "skip_if_exists": { "type": "boolean", "description": "Whether to skip if target file already exists." }
      },
      "additionalProperties": false
    },
    "Pm": {
      "type": "object",
      "properties": {
        "install": { "type": "object", "additionalProperties": { "type": "string" } },
        "dev_install": { "type": "object", "additionalProperties": { "type": "string" } }
      },
      "additionalProperties": false
    },
    "Tool": {
      "type": "object",
      "required": ["tool"],
      "properties": {
        "tool": { "type": "string", "description": "Tool command name (e.g. eslint, prettier, cargo-clippy)." },
        "script": { "type": "string", "description": "Custom script to run for this tool." }
      },
      "additionalProperties": false
    },
    "Step": {
      "type": "object",
      "properties": {
        "command": { "type": "string", "description": "Shell command to run." },
        "description": { "type": "string", "description": "Description of this step." },
        "dir": { "type": "string", "description": "Working directory relative to project root." },
        "create": {
          "type": "object",
          "required": ["from", "to"],
          "properties": {
            "from": { "type": "string" },
            "to": { "type": "string" }
          },
          "additionalProperties": false
        },
        "platform": { "type": "string", "description": "Platform gate (e.g. 'debian' or 'termux')." },
        "script": { "type": "string", "description": "Optional inline script." },
        "install": { "type": "boolean", "description": "Whether this step installs dependencies (skipped with --no-install and triggers version pinning)." }
      },
      "additionalProperties": false
    },
    "Variable": {
      "type": "object",
      "properties": {
        "description": { "type": "string", "description": "Prompt message shown to user." },
        "default": { "type": "string", "description": "Default value if user presses Enter." },
        "type": {
          "type": "string",
          "enum": ["string", "integer", "boolean"],
          "description": "Validation type for input."
        },
        "choices": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Allowed choice values."
        },
        "pattern": { "type": "string", "description": "Regex pattern the input must match." },
        "required": { "type": "boolean", "description": "Whether non-empty input is mandatory." }
      },
      "additionalProperties": false
    }
  }
}"##;

/// Canonical raw GitHub URL for the fast-alias recipe schema.
pub const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/fusoras/fast-alias/main/schema/recipe.schema.json";

/// Standard filename for local cached/fallback schema.
pub const SCHEMA_FILE_NAME: &str = "recipe.schema.json";

/// Ensures that the local fallback schema file exists under state_dir (`~/.local/state/fa/recipe.schema.json`) and is up to date.
pub fn ensure_schema_file(state_dir: &Path) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(state_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create state directory {}: {e}", state_dir.display()))?;

    let schema_path = state_dir.join(SCHEMA_FILE_NAME);
    let needs_write = match fs::read_to_string(&schema_path) {
        Ok(existing) => existing != RECIPE_SCHEMA_JSON,
        Err(_) => true,
    };

    if needs_write {
        fs::write(&schema_path, RECIPE_SCHEMA_JSON)
            .map_err(|e| anyhow::anyhow!("Failed to write schema file {}: {e}", schema_path.display()))?;
    }

    Ok(schema_path)
}
