---
name: agents-md
description: Create or edit AGENTS.md rule files for projects and dotfiles. Use when asked to "create AGENTS.md", "add rules", "add instructions", "set up project rules", "configurar el agente", or when a repo needs agent behavior defined. Covers locations, content, and the 10-200 line limit.
---

# Agent Rules (AGENTS.md)

Write `AGENTS.md` files that instruct the agent on project behavior. These are
the primary entry point for agent instructions — loaded automatically at
session start, not on demand.

## Related Skills

Load these skills alongside this one for complete coverage:

| Skill                    | What it provides                                                              |
| ------------------------ | ----------------------------------------------------------------------------- |
| `create-skills`          | When and how to create reusable skills vs plain rules; skill authoring rules  |
| `docs-instructions`      | Splitting detailed guides into `docs/` files referenced with `@file.md`       |
| `customize-opencode`     | opencode config schema (`instructions`, `skills`, permissions)                |

Load them in order: `create-skills` → `agents-md` → `docs-instructions`.

## When to Use

- A project, vault, or repo needs persistent agent behavior rules
- Setting up a new repo's conventions and commands
- Adding project structure, code style, or workflow rules
- Keeping `AGENTS.md` concise while delegating detail to `docs/` files

## Locations

| Scope                | Path                                          | Applies to        |
| -------------------- | --------------------------------------------- | ----------------- |
| Project              | `<repo-root>/AGENTS.md`                       | That project      |
| Global (opencode)    | `~/.config/opencode/AGENTS.md`                | Every opencode use|

- Project-level files are the default; only use the global file for rules that
  apply everywhere.
- Read any existing `AGENTS.md` before editing — never overwrite rules blindly.

## What to Include

- Commands for build, lint, test, and other important tasks
- Project structure and code conventions
- Team or personal style guides
- References to additional instruction files via `@docs/file.md`
- Language and response conventions (e.g. "always answer in Spanish")

Keep the file between **10 and 200 lines**. Beyond that, split detail into
`docs/` files and reference them with `@` (see `docs-instructions`).

## Procedure

1. Read existing `AGENTS.md` and the repo's conventions.
2. Identify the rule areas: commands, structure, style, workflow, language.
3. Draft rules as concise bullet points, not prose.
4. Keep to 10–200 lines; move detail to `docs/` files with `@` references.
5. Follow the project's own `AGENTS.md` rules if one exists.

## Example

```markdown
# My Project — Agent Rules

- Never commit unless the user explicitly asks.
- Run `npm run lint` and `npm test` before finishing a task.
- Use `@docs/arquitectura.md` for structure and patterns.

## Adicional
For details, consult:
- Arquitectura: @docs/arquitectura.md
- Testing: @docs/testing.md
```

## Anti-patterns

- Do NOT exceed 200 lines — split into `docs/` files instead.
- Do NOT duplicate rules already present in another `AGENTS.md` scope.
- Do NOT overwrite existing project rules without reading them first.
- Do NOT add skills to `AGENTS.md` — skills live in `skills/` folders and load
  on demand (see `create-skills`).
