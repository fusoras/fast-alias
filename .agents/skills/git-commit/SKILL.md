---
name: git-commit
description: Write git commit messages that follow the Conventional Commits spec and this user's personal style. Use when asked to "commit", "create a commit", "make a commit", "generar commit", or when writing a commit message. Apply BEFORE running git commit.
---

# Git Commit Messages

Commit messages follow the **Conventional Commits** specification
(`https://www.conventionalcommits.org/`) adapted to the user's personal style.

## Message structure

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

- Header is mandatory, at most **72 characters** total.
- `type` and `scope` are lowercase; `description` starts with a lowercase verb
  in the **imperative mood** ("add", "fix", "update", "refactor") and has
  **no trailing period**.
- Scope goes in parentheses right after the type, only when the change is
  confined to a specific component/area:
  - `feat(ColorPicker): add data-custom-property`
  - `fix(seo): shorten descriptions and add meta assets`
- A body paragraph is optional; use it only when the summary alone cannot
  convey the change. When present, wrap at 72 characters and describe **what**
  and **why**, not the mechanics.
- No footer (BREAKING CHANGE, references) unless the change genuinely breaks
  the API or the user asks for it.

## Type selection

| Type      | When to use                                                |
| --------- | ---------------------------------------------------------- |
| `feat`    | A new feature, component, or user-facing capability        |
| `fix`     | A bug fix or correction of broken behavior                 |
| `refactor`| Code restructuring with no behavior change                 |
| `update`  | Bumping dependencies, versions, or config files            |
| `docs`    | Documentation-only changes                                 |
| `style`   | Formatting, whitespace, lint fixes (no logic change)       |
| `test`    | Adding or modifying tests                                  |
| `perf`    | Performance improvements                                  |
| `build`   | Build system / tooling changes                             |
| `ci`      | CI configuration and files                                 |
| `chore`   | Miscellaneous maintenance                                  |

Match the type to the *dominant* intent of the change. When in doubt, prefer
the most specific type.

## User's preferred types (from history)

This user most frequently uses `feat`, `fix`, `refactor`, and `update`.
Prefer those four over rarer ones unless the change clearly matches another
type.

## Procedure

1. Run `git status` and `git diff` (staged + unstaged) to see exactly what is
   being committed.
2. Stage only the intended files; never commit secrets or build artifacts.
3. Write the header as `<type>: <description>` (add a scope when the change is
   isolated to one area).
4. Add a body only when extra context is needed.
5. Commit with the single `-m` form for the header; use a second `-m` for the
   body.

## Examples

```bash
# new feature
git commit -m "feat: add Web Component ColorPicker"

# scoped fix
git commit -m "fix(seo): shorten descriptions and add meta assets"

# dependency bump
git commit -m "update: Astro 6.4"

# refactor with body
git commit -m "refactor: update deploy.yml" \
  -m "Simplify the workflow and pin the Node version used in CI."
```

## Anti-patterns

- Do NOT use past tense ("added", "fixed") — use imperative mood.
- Do NOT capitalize the first word of the description.
- Do NOT end the description with a period.
- Do NOT commit unless the user explicitly asks to commit.
- Do NOT amend a commit or force-push unless explicitly requested.
- Do NOT run git hooks with `--no-verify` or touch git config.
