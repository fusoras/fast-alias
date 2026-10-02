# Static Template Variables Guide (`_vars` and `[vars]`)

## 1. Core Concept: What are Template Variables?

Template variables are **static string substitutions** processed at configuration load time:

- **When they expand**: When fast-alias loads your TOML configuration into memory, before any command executes.
- **Where they expand**: Across `command`, `description`, `env`, and `env_force` fields.
- **Difference from Environment Variables**:
  - `{{KEY}}` = **Template variable** (static text replacement by fast-alias).
  - `$VAR` or `${VAR}` = **Environment variable** (dynamic runtime value injected into the subshell).

---

## 2. Scopes and Declaration

You can declare template variables at two distinct levels:

### A. Namespace Level (`_vars`)
Declared within a specific namespace section (`[aliases.":<namespace>"]`). These variables are scoped to aliases inside that namespace:

```toml
[aliases.":ai"]
# Single-line inline table:
_vars = {
  SD = "sd-cli --steps 25 --cfg-scale 7.0 -m $AI_IMAGE_MODEL",
  SD_DESC = "Generate image with SD"
}

create-img-anima = {
  description = "{{SD_DESC}} anime style: <prompt> <output>",
  command = "{{SD}}",
  env_force = 'export AI_IMAGE_MODEL="$AI_MODELS_DIR/vision/anima-aesthetic-v3.0.safetensors"'
}
```

> [!NOTE]
> In TOML v0.8, inline tables `{ ... }` must not contain unescaped newlines. You can write them on a single line or declare them as a standard TOML sub-table:
> ```toml
> [aliases.":ai"._vars]
> SD = "sd-cli --steps 25 --cfg-scale 7.0 -m $AI_IMAGE_MODEL"
> SD_DESC = "Generate image with SD"
> ```

### B. File Level (`[vars]`)
Declared at the top level of any recipe or alias TOML file. They provide shared defaults across all alias sections in that file:

```toml
# ~/.config/fa/recipes.d/ai.toml

# File-wide global variables
[vars]
GLOBAL_BIN = "sd-cli"
DEFAULT_STEPS = "25"

[aliases.":ai"]
_vars = {
  SD = "{{GLOBAL_BIN}} --steps {{DEFAULT_STEPS}} -m $AI_IMAGE_MODEL"
}
```

---

## 3. Scope Hierarchy and Precedence

When expanding `{{KEY}}` in an alias command, description, or environment variable, fast-alias resolves values in order:

1. **Namespace `_vars`**: Highest priority. Overrides file-level variables for commands in that section.
2. **File-level `[vars]`**: Base fallback for all aliases within the file.

---

## 4. Transitive / Nested Substitutions

Template variables can reference other template variables. Fast-alias performs multi-pass resolution (up to 5 passes) with cycle protection:

```toml
[vars]
BIN = "llama-cli"
COMMON_FLAGS = "--ctx-size 4096 --threads 8"

[aliases.":ai"]
_vars = {
  BASE_CMD = "{{BIN}} {{COMMON_FLAGS}} -m $MODEL_PATH"
}

coder = {
  command = "{{BASE_CMD}}",
  env_force = { MODEL_PATH = "/models/coder.gguf" }
}
```

Resolution chain:
1. `{{BIN}}` $\rightarrow$ `llama-cli`
2. `{{COMMON_FLAGS}}` $\rightarrow$ `--ctx-size 4096 --threads 8`
3. `{{BASE_CMD}}` $\rightarrow$ `llama-cli --ctx-size 4096 --threads 8 -m $MODEL_PATH`
4. Resulting command: `llama-cli --ctx-size 4096 --threads 8 -m $MODEL_PATH`

---

## 5. Combining Template Variables with Environment Overrides

The most powerful pattern combines `_vars` for common command templates with `env_force` for target-specific configuration:

```toml
[aliases.":ai"]
_vars = {
  SD = "sd-cli --steps 25 --cfg-scale 7.0 -m $AI_IMAGE_MODEL",
  SD_DESC = "Generate image with SD"
}

create-img-anima = {
  description = "{{SD_DESC}} anime style: <prompt> <output>",
  command = "{{SD}}",
  env_force = 'export AI_IMAGE_MODEL="$AI_MODELS_DIR/vision/anima-aesthetic-v3.0.safetensors"'
}

create-img-standard = {
  description = "{{SD_DESC}} standard: <prompt> <output>",
  command = "{{SD}}",
  env_force = 'export AI_IMAGE_MODEL="$AI_MODELS_DIR/vision/qwen-image-2.1/qwen-image-2.1-Q5_K_M.gguf"'
}
```

---

## Related Documentation
- For runtime environment variables (`env`, `env_force`, `_env`, `_env_force`) and Bash export syntax, see [docs/environment.md](environment.md).
- For complete alias catalogs and recipes, see [docs/recipes.md](recipes.md).
- For general CLI commands and flags, see [docs/commands.md](commands.md).
