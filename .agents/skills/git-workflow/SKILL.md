---
name: git-workflow
description: Run git commands using the user's aliases for token savings and follow this project's branch strategy and modern git conventions. Use when asked to "check status", "switch branch", "create branch", "list branches", "commit", "push", "merge", "crear rama", "cambiar de rama", "revisar estado", or for any git operation in this repo. Load BEFORE running git commands.
---

# Git Workflow

This skill governs how git commands are run in this project. Use it together
with `git-commit` (commit message rules) — `git-workflow` for commands and
flow, `git-commit` for writing the commit message itself.

## Git Aliases (prefer over full commands)

The user's global git aliases **must be used before the full command** to save
tokens. When an alias exists, write the alias form, not the long form.

| Alias   | Full command      | Prefer              |
| ------- | ----------------- | ------------------- |
| `git b` | `git branch`      | list / create branches |
| `git s` | `git switch`      | change branches     |
| `git st`| `git status`      | check repo state    |

Examples:

```bash
git st                  # instead of: git status
git s develop           # instead of: git switch develop
git b                   # instead of: git branch
git b feature/foo       # instead of: git branch feature/foo
```

## Modern Commands

- **Never** use `git checkout`. Use modern equivalents:
  - `git s <branch>` (alias of `git switch`) — switch branches.
  - `git restore <file>` — discard changes to a working-tree file.
  - `git restore --staged <file>` — unstage a file.

## Branch Strategy

- All development edits happen on a **development branch**: `develop` or a
  feature branch (`feature/<name>`).
- **Never** commit directly to `main`.
- Merge into `main` **only** after the code is fully tested and verified to
  work cleanly.
- To start work: `git s develop` then create a feature branch with
  `git b feature/<name>` and switch to it with `git s feature/<name>`.

## Procedure

1. Check repo state first: `git st` and `git b` to see current branch and
   existing branches.
2. Run `git diff` (staged + unstaged) before committing to see exact changes.
3. Stage only intended files; never commit secrets or build artifacts
   (`target/`, `.codegraph/`).
4. Switch branches with `git s`, never `git checkout`.
5. For the commit message itself, load the `git-commit` skill.

## Anti-patterns

- Do NOT use `git checkout` for switching branches or discarding changes.
- Do NOT commit or merge to `main` without prior testing.
- Do NOT force-push (`git push --force`) or amend commits unless explicitly
  requested.
- Do NOT run git hooks with `--no-verify`.
- Do NOT modify the user's git config.
- Do NOT run `git add .` blindly — stage only the intended files.
