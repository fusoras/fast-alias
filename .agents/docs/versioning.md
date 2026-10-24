# Versioning & Release Workflow Guide — fa

This document specifies the Semantic Versioning (SemVer) convention, version bump checklist, and automated release pipeline for `fa`. This workflow is identical to `project-dots`.

---

## 1. Versioning Convention (SemVer 2.0.0)

`fa` follows standard **Semantic Versioning**. Stable releases use the plain `MAJOR.MINOR.PATCH` form; pre-release identifiers are optional for testing builds:

Format: `v<MAJOR>.<MINOR>.<PATCH>[-<PRERELEASE>]` (e.g. `v0.1.0`, `v0.1.1`, `v0.2.0-beta.1`)

- **MAJOR**: Incompatible API or structural breaking changes.
- **MINOR**: Backward-compatible new subcommands or features (e.g. `0.2.0`).
- **PATCH**: Backward-compatible bug fixes and internal refactoring (e.g. `0.1.1`).
- **PRERELEASE**: Optional testing phase indicator (e.g. `-beta.1`, `-rc.1`).

### Version Bumping Rules & User Authority
- **Exclusive User Authority**: The user exclusively determines, authorizes, and defines version numbers (e.g., `v0.1.0-beta.35`) and release triggers. The AI assistant MAY ONLY suggest version numbers when asked, and MUST NEVER increment or assign version numbers autonomously.
- **No Automatic Bumps for Intermediate Local Edits**: Do NOT increment the version number for every small local commit or feature/fix edit.
- **Main Branch Strict Prohibition**: The AI assistant must NEVER switch to, merge into, or operate on the `main` branch under any circumstances unless explicitly instructed by the user. All development occurs on `develop`.
- **Bumping Executed Only on Explicit User Instruction**: Version updates across files (`Cargo.toml`, `Cargo.lock`, `install.sh`, `AGENTS.md`, `.agents/docs/planning.md`, `CHANGELOG.md`) are executed ONLY when the user explicitly instructs to update the version and specifies the exact version string.

---

## 2. Version Bump Checklist (Files to Update)

When preparing a new release, update the version string across the canonical files:
1. `Cargo.toml` (`version = "X.Y.Z"`)
2. `Cargo.lock` (updated automatically via `cargo check`)
3. `install.sh` (`VERSION="${RELEASE_TAG:-vX.Y.Z}"`)
4. `AGENTS.md` (project version references)
5. `.agents/docs/planning.md` (project version reference)
6. `CHANGELOG.md` (`## [Unreleased]` becomes `## [X.Y.Z] - YYYY-MM-DD`)

*(Note: `src/main.rs` uses `env!("CARGO_PKG_VERSION")` and must never be edited during a bump).*

---

## 3. Release Execution Protocol (Step-by-Step)

```bash
# 1. Run full unit test suite verification on the development branch
cargo test -- --nocapture

# 2. Stage and commit version bump on develop
git add Cargo.toml Cargo.lock install.sh AGENTS.md .agents/docs/planning.md CHANGELOG.md
git commit -m "update: bump version to vX.Y.Z"

# 3. Push develop branch to remote (Requires explicit user confirmation)
git push origin develop

# 4. Merge develop into main (fast-forward) and push it
git switch main
git merge --ff-only develop
git push origin main

# 5. Create and push the release tag from main
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z

# 6. Return to the development branch
git switch develop
```

---

## 4. Automated CI Release Pipeline

Pushing a tag starting with `v*` (e.g. `v0.1.0`) automatically triggers GitHub Actions (`.github/workflows/release.yml`):

1. **x86_64-unknown-linux-gnu**: Compiles native Linux 64-bit binary and packages `fa-x86_64-unknown-linux-gnu.tar.gz`.
2. **aarch64-unknown-linux-musl**: Cross-compiles static ARM64 binary via `musl-tools` + `gcc-aarch64-linux-gnu` and packages `fa-aarch64-unknown-linux-musl.tar.gz`.
3. **GitHub Release Creation**: Uploads both tarball assets to the new GitHub Release tag automatically using `softprops/action-gh-release@v2`.

---

## 5. Changelog Strategy (`CHANGELOG.md` Policy)

- **Stable Release Phase (`main` branch — active since `v0.1.0`)**:
  `main` is the default and release branch; `develop` remains the integration branch.
  1. **`CHANGELOG.md`** follows the [Keep a Changelog](https://keepachangelog.com/) standard.
  2. Every release merged into `main` must record an explicit changelog entry detailing Added, Changed, Deprecated, Removed, Fixed, and Security changes.
  3. Pre-release identifiers (`-beta.N`, `-rc.N`) are optional and only used for testing builds.
