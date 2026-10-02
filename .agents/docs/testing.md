# Unit Testing Guide & Specifications — fa

`fa` uses Rust's built-in unit testing framework (`#[cfg(test)]`) coupled with **Test-Driven Development (TDD Red-Green)** to ensure reliability across critical system components.

## Running Unit Tests

To run the unit test suite with human-readable explanatory descriptions printed directly in the console:

```bash
cargo test -- --nocapture
```

---

## Mandatory Test Failure & Sensitivity Protocol (Red-Before-Green Rule)

> [!IMPORTANT]
> **NEVER accept a test that passes on the first try without having seen it fail first.**

To avoid tautological, useless, or false-positive tests (tests that pass even when production code is broken), every new or modified test MUST strictly comply with the following protocol:

1. **Phase 1 — Mandatory Failure Test (RED / Mutation State):**
   - Before considering a test valid, the production logic being evaluated must be deliberately broken (syntactic mutation, inverted condition, erroneous return value, or incomplete state).
   - Execute `cargo test` and verify empirically that the test **FAILS** with an explanatory assertion or panic.
   - If the test passes on the first attempt (`ok`) against mutated or incomplete code, the test is deemed **invalid/tautological** and must be rewritten to tie its assertions to the actual domain logic.

2. **Phase 2 — Implementation & Acceptance (GREEN State):**
   - Once failure sensitivity is verified, restore or implement the correct production logic.
   - Re-run `cargo test` to confirm that the test legitimately passes.

3. **Mandatory Transparent Evidence in Chat:**
   - To allow auditing of this rule whenever the AI assistant writes tests, the response MUST include a dedicated section titled `## Test Failure Verification (RED State)` displaying the actual terminal failure trace before presenting the final passing implementation.

4. **Verification Tooling:**
   - The test suite can be audited by creating an isolated git worktree (`git worktree add .worktrees/test-check develop`) to run mutation tests without dirtying the primary working tree.

---

## Non-Blocking Tests & Zero-Hang Policy

> [!CAUTION]
> **Strict prohibition of interactive blocking in the test suite.**

1. **No `stdin` waits or interactive calls:**
   - No unit test may request input via standard input (`std::io::stdin()`) or block waiting for responses in `prompt_yes_no`.
   - Any function accepting interactive confirmation (`perform_self_uninstall`, etc.) MUST be tested using automated flags (`auto_confirm: true` or `auto_reject: true`).

2. **Detection, immediate resolution, and reporting:**
   - If a `cargo test` run exceeds the expected duration or hangs, the assistant MUST terminate the process immediately, diagnose the root cause, fix the code/test to ensure it is 100% non-blocking, or report it immediately to the user rather than leaving background commands unresolved.

---

## Planned Unit Test Inventory & Scope

| Test Name | Module | Primary Purpose & Verification |
| --------- | ------ | ----------------------------- |
| `test_example_config_parsing` | `src/config.rs` | Verifies the example TOML catalog deserialization and presence of the `example` alias (first-run provisioning). |
| `test_recipe_alias_resolution` | `src/config.rs` | Verifies resolution of recipe aliases (e.g., `demo` -> `demo` canonical). |
| `test_recipe_uniqueness` | `src/config.rs` | Verifies that recipe names, variants, and aliases are strictly unique with no duplicate names or collisions. |
| `test_variant_resolution` | `src/config.rs` | Verifies variant selection (`-v bun`) and default fallback to first declared variant. |
| `test_templating_placeholder_substitution` | `src/templating.rs` | Verifies `{{var}}` substitution in file content, paths, and commands. |
| `test_templating_missing_variable` | `src/templating.rs` | Verifies missing variables produce a clear error or remain unresolved with a warning. |
| `test_expand_home_utility` | `src/engine.rs` | Verifies path expansion from tilde paths to absolute user paths. |
| `test_process_files_dry_run` | `src/engine.rs` | Verifies dry-run preview execution for `files` actions without modifying filesystem. |
| `test_process_steps_dry_run` | `src/engine.rs` | Verifies dry-run execution of `steps` actions. |
| `test_process_steps_platform_filter` | `src/engine.rs` | Verifies platform-filtered `steps` (Debian vs Termux) and platform-agnostic steps. |
| `test_inline_vs_template_files` | `src/engine.rs` | Verifies `inline`, `from`, and `template` file generation modes. |
| `test_skip_if_exists_file` | `src/engine.rs` | Verifies `skip_if_exists` prevents overwriting existing destinations. |
| `test_list_recipes_output` | `src/engine.rs` | Verifies simplified single-line `list` output formatting. |
| `test_show_recipe_resolution` | `src/engine.rs` | Verifies recipe inspection (`show`) via canonical key and explicit alias. |
| `test_command_exists_utility` | `src/platform.rs` | Verifies PATH directory inspection for command presence without relying on external `which`. |
| `test_pm_detection` | `src/platform.rs` | Verifies detection of pnpm/bun/npm availability. |
| `test_is_newer_version_logic` | `src/update.rs` | Verifies SemVer version comparison logic for `self-update`. |
| `test_platform_asset_resolution` | `src/update.rs` | Verifies target release asset resolution for Debian (`x86_64`) vs Termux (`aarch64-musl`). |
| `test_self_uninstall_dry_run` | `src/update.rs` | Verifies dry-run preview for `self-uninstall` executable and state/config removal. |
| `test_new_subcommand` | `src/main.rs` | Verifies parsing of the `new` subcommand and variant flag. |
| `test_dependency_preflight` | `src/engine.rs` | Verifies preflight: shell command extraction (first token per `&&`/`;`/`|` segment), shell-builtin/assignment exclusion, missing-application detection, and passing when all applications exist. |
| `test_version_comparison` | `src/update.rs` | Verifies release-tag comparison: higher `-beta.N` is newer, same tag is not, older version is not. |
| `test_cached_version_check` | `src/update.rs` | Verifies `check_version_update` reads the cached latest release from state.toml and returns `None` for a future running version. |

---

## Exact Terminal Output (`cargo test -- --nocapture`)

```text
running 12 tests

🔍 [TEST] Example Config TOML Parsing
   Explanation: Verifies that the example catalog parses successfully and contains 'example'.

🔍 [TEST] Templating Placeholder Substitution
   Explanation: Verifies that {{var}} placeholders are substituted in content and paths.

🔍 [TEST] Variant Resolution
   Explanation: Verifies that variant selection falls back to the first declared variant.

🔍 [TEST] Recipe Alias Resolution
   Explanation: Verifies that 'demo' resolves to the canonical demo recipe.

🔍 [TEST] Platform Release Asset Resolution
   Explanation: Verifies that Debian resolves to the x86_64 tarball asset and Termux to aarch64.
   ✓ Debian target asset resolved correctly: fa-x86_64-unknown-linux-gnu.tar.gz
   ✓ Termux target asset resolved correctly: fa-aarch64-unknown-linux-musl.tar.gz

🔍 [TEST] SemVer Version Comparison for Self-Update
   Explanation: Verifies that release tag versions (e.g. v0.1.0-beta.2) are correctly identified as newer than v0.1.0-beta.1.

🔍 [TEST] Self-Uninstall Engine (Dry-Run Simulation)
   Explanation: Verifies that perform_self_uninstall(true) previews executable and state directory removal cleanly without altering disk state.
=== fa Self-Uninstall Engine ===
Target Binary Path: /path/to/target/debug/deps/fa
Target State Directory: /home/user/.local/state/fa
Target Config Directory: /home/user/.config/fa

=== DRY-RUN MODE ACTIVE: No files will be deleted ===
[Dry-Run] Would remove executable: /path/to/target/debug/deps/fa
[Dry-Run] Would remove state directory: /home/user/.local/state/fa
   ✓ Self-uninstall dry-run completed successfully.

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
