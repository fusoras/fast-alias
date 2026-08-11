# Rule: Recipe and Template Naming Convention

## Principles

1. **No Generic Names**:
   - Never use generic terms like `app`, `web`, `cli`, `template`, `starter`, or `config` as canonical recipe names or folder names in `~/.config/fa/templates/`.

2. **Distinctive & Specific Naming**:
   - Combine the tool/stack type with its specific variant, toolchain, or flavor (e.g., `ts-lib`, `rust-cli`, `next-ts`).
   - Keep names simple, clear, and hyphenated (`kebab-case`).
   - Variants within a recipe must reflect the toolchain difference explicitly (e.g., `pnpm`, `bun`, `npm`), never a generic `default` or `alt`.

3. **Collision Prevention**:
   - Multiple configurations for the same tool/stack will exist (e.g., different package managers or linters for the same stack). Specific naming ensures each recipe and template lives in a dedicated, collision-free path under `recipes/` and `~/.config/fa/templates/<recipe-name>/`.

4. **Explicit Alias Governance**:
   - Aliases (recipe aliases, command aliases, variant shorthands) must ONLY be created when explicitly requested and defined by the user.
   - Agents must NEVER generate, infer, or automatically add aliases without explicit user instruction.
