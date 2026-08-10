---
name: docs-instructions
description: Create or edit additional instruction files (docs/ folder, @file.md references) and the instructions field of opencode.json. Use when asked to "add instructions", "create docs rules", "reference docs", "split AGENTS.md", "add instruction files", or when detailed guides should live outside AGENTS.md.
---

# Additional Instructions (docs/ + opencode.json)

Create detailed instruction files referenced from `AGENTS.md` with `@file.md`,
and register instruction paths in `opencode.json`. This keeps `AGENTS.md`
concise (10–200 lines) while letting each area have its own maintainable guide.

## Related Skills

Load these skills alongside this one for complete coverage:

| Skill               | What it provides                                                          |
| ------------------- | ------------------------------------------------------------------------- |
| `create-skills`     | When instructions should be a skill vs a `docs/` file; skill authoring    |
| `agents-md`         | The `AGENTS.md` file these guides get referenced from, and its 10–200 limit|
| `customize-opencode`| Exact `opencode.json` schema for `instructions` and `skills`              |

Load them in order: `create-skills` → `agents-md` → `docs-instructions`.

## When to Use

- `AGENTS.md` is getting long and needs to split by area
- A repo needs per-area guides (architecture, testing, styles, deployment)
- Sharing instruction files across projects or loading them from remote URLs
- Referencing extra markdown files so the agent reads them on demand

## Approach A: `docs/` folder referenced with `@`

Structure:

```
AGENTS.md                    Main project rules
docs/
  arquitectura.md            Structure, patterns, technical decisions
  testing.md                 Testing rules and info
  estilos.md                 Code conventions, naming, formatting
  despliegue.md              CI/CD, env vars, commands
```

In `AGENTS.md`, reference each guide on demand:

```markdown
## Adicional
For more context on each area, consult:
- Arquitectura: @docs/arquitectura.md
- Testing: @docs/testing.md
```

Benefits:

- `AGENTS.md` stays concise and ordered (10–200 lines)
- Each guide is an independent, versionable file
- The agent only loads the files relevant to the current task
- Ideal for teams: one responsible file per area

## Approach B: `instructions` field in opencode.json

Load markdown files automatically without `@` references — from multiple paths
or even a remote URL:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "instructions": [
    "CONTRIBUTING.md",
    "docs/style-guide.md",
    "docs/design/*.md",
    "https://raw.githubusercontent.com/org/shared-rules/main/style.md"
  ]
}
```

- Use to reuse existing rules without duplicating them, or share conventions.
- Remote URLs have a 5-second timeout — beware slow pages.
- Validate the shape against `https://opencode.ai/config.json` before writing
  (see `customize-opencode`).

## Procedure

1. Read the current `AGENTS.md` and identify which areas are detailed enough
   to split out.
2. Choose the approach: `docs/` + `@` references (local, on-demand) or
   `instructions` (always loaded, can be remote).
3. Create one guide file per area; keep each focused.
4. Reference the guides from `AGENTS.md` (Approach A) or register them in
   `opencode.json` (Approach B).
5. Keep `AGENTS.md` within 10–200 lines after splitting.

## Anti-patterns

- Do NOT dump full guide contents into `AGENTS.md` — reference them instead.
- Do NOT use remote URLs for slow or unverified sources (5s timeout).
- Do NOT duplicate the same rules in both `docs/` and `opencode.json`.
- Do NOT add a guide as a skill — use `docs/` for content, `skills/` for
  reusable behavior (see `create-skills`).
