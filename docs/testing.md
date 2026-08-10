# Unit Testing Guide & Specifications — fa

`fa` uses Rust's built-in unit testing framework (`#[cfg(test)]`) coupled with **Test-Driven Development (TDD Red-Green)** to ensure reliability across critical system components.

## Running Unit Tests

To run the unit test suite with human-readable explanatory descriptions printed directly in the console:

```bash
cargo test -- --nocapture
```

---

## Planned Unit Test Inventory & Scope

| Test Name | Module | Primary Purpose & Verification |
| --------- | ------ | ----------------------------- |
| `test_embedded_recipe_config_parsing` | `src/config.rs` | Verifies TOML recipe catalog deserialization, embedded fallback loader (`include_str!`), and presence of default recipes (`astro`, `ts-lib`, `rust-cli`, `python`). |
| `test_recipe_alias_resolution` | `src/config.rs` | Verifies resolution of recipe aliases (e.g., `astro` -> `astro-pnpm` default). |
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
| `test_doctor_output` | `src/main.rs` | Verifies platform and package-manager detection output formatting. |

---

## Exact Terminal Output (`cargo test -- --nocapture`)

```text
running 12 tests

🔍 [TEST] Embedded Default TOML Recipe Catalog Parsing
   Explanation: Verifies that the recipe catalog parses successfully and contains 'astro'.

🔍 [TEST] Templating Placeholder Substitution
   Explanation: Verifies that {{var}} placeholders are substituted in content and paths.

🔍 [TEST] Variant Resolution
   Explanation: Verifies that variant selection falls back to the first declared variant.

🔍 [TEST] Recipe Alias Resolution
   Explanation: Verifies that 'astro' resolves to the canonical astro recipe.

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
