# Versioning & Release Workflow Guide — fa

This document specifies the Semantic Versioning (SemVer) convention, version bump checklist, and automated release pipeline for `fa`. This workflow is identical to `project-dots`.

---

## 1. Versioning Convention (SemVer 2.0.0)

`fa` follows standard **Semantic Versioning** with pre-release identifiers during initial development:

Format: `v<MAJOR>.<MINOR>.<PATCH>-<PRERELEASE>` (e.g. `v0.1.0-beta.3`)

- **MAJOR**: Incompatible API or structural breaking changes.
- **MINOR**: Backward-compatible new subcommands or features (e.g. `0.2.0`).
- **PATCH**: Backward-compatible bug fixes and internal refactoring (e.g. `0.1.1`).
- **PRERELEASE**: Pre-release testing phase indicator (e.g. `-beta.1`, `-beta.2`).

### Version Bumping Rules & User Authority
- **Exclusive User Authority**: The user exclusively determines, authorizes, and defines version numbers (e.g., `v0.1.0-beta.35`) and release triggers. The AI assistant MAY ONLY suggest version numbers when asked, and MUST NEVER increment or assign version numbers autonomously.
- **No Automatic Bumps for Intermediate Local Edits**: Do NOT increment the version number for every small local commit or feature/fix edit.
- **Main Branch Strict Prohibition**: The AI assistant must NEVER switch to, merge into, or operate on the `main` branch under any circumstances unless explicitly instructed by the user. All development occurs on `develop`.
- **Bumping Executed Only on Explicit User Instruction**: Version updates across files (`Cargo.toml`, `Cargo.lock`, `install.sh`, `AGENTS.md`) are executed ONLY when the user explicitly instructs to update the version and specifies the exact version string.

---

## 2. Version Bump Checklist (Files to Update)

When preparing a new release, update the version string across all 4 authoritative files:

## Version Locations
The version number appears in **`Cargo.toml`** (`version = "0.1.0-beta.11"`).
`src/main.rs` uses `env!("CARGO_PKG_VERSION")` to prevent version drift at compile time.
`install.sh` and `AGENTS.md` should also be updated during version bumps.

---

## 3. Release Execution Protocol (Step-by-Step)

```bash
# 1. Run full unit test suite verification
cargo test -- --nocapture

# 2. Stage and commit version bump
git add Cargo.toml src/main.rs install.sh AGENTS.md Cargo.lock
git commit -m "update: bump version to v0.1.0-beta.3"

# 3. Push develop branch to remote (Requires explicit user confirmation)
git push origin develop

# 4. Create and push release Git tag
git tag -a v0.1.0-beta.3 -m "Release v0.1.0-beta.3"
git push origin v0.1.0-beta.3
```

---

## 4. Automated CI Release Pipeline

Pushing a tag starting with `v*` (e.g. `v0.1.0-beta.3`) automatically triggers GitHub Actions (`.github/workflows/release.yml`):

1. **x86_64-unknown-linux-gnu**: Compiles native Linux 64-bit binary and packages `fa-x86_64-unknown-linux-gnu.tar.gz`.
2. **aarch64-unknown-linux-musl**: Cross-compiles static ARM64 binary via `musl-tools` + `gcc-aarch64-linux-gnu` and packages `fa-aarch64-unknown-linux-musl.tar.gz`.
3. **GitHub Release Creation**: Uploads both tarball assets to the new GitHub Release tag automatically using `softprops/action-gh-release@v2`.

---

## 5. Changelog Strategy (`CHANGELOG.md` Policy)

- **Current Beta Phase (`develop` branch)**:
  During the active pre-release / beta iteration phase (`v0.1.0-beta.*`), rapid architecture adjustments and feature tests occur without maintaining a manual `CHANGELOG.md` file. Release details are tracked via Conventional Commit messages and GitHub Release tags.

- **Stable Release Phase (`main` branch TODO)**:
  When `fa` exits beta, undergoes final verification, and merges into the `main` production branch for stable releases (`v1.0.0+` / `v0.1.0` stable):
  1. A formal **`CHANGELOG.md`** file will be established following the [Keep a Changelog](https://keepachangelog.com/) standard.
  2. Every release merged into `main` must record an explicit changelog entry detailing Added, Changed, Deprecated, Removed, Fixed, and Security changes.
