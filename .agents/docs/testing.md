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
> **NUNCA aceptar un test que pase a la primera sin haberlo visto fallar primero.**

Para evitar tests tautológicos, inútiles o falsos positivos (tests que aprueban incluso cuando el código de producción está roto), todo test nuevo o modificado DEBE cumplir estrictamente con el siguiente protocolo:

1. **Fase 1 — Prueba de Fallo Obligatoria (RED / Mutation State):**
   - Antes de dar por válido un test, la lógica del código de producción a evaluar debe alterarse deliberadamente (mutación sintáctica, cambio de condición, retorno de valores erróneos o estado incompleto).
   - Se debe ejecutar `cargo test` y verificar empíricamente que el test **FALLA** con una aserción o pánico explicativo.
   - Si el test pasa a la primera (`ok`) sobre código mutado o incompleto, el test se considera **inválido/tautológico** y debe reescribirse para ajustar sus aserciones a la lógica real del dominio.

2. **Fase 2 — Implementación y Aprobación (GREEN State):**
   - Una vez comprobada la sensibilidad al fallo del test, se restaura o implementa la lógica correcta del código de producción.
   - Se vuelve a ejecutar `cargo test` para confirmar que el test pasa en verde de forma legítima.

3. **Evidencia Transparente Obligatoria en el Chat:**
   - Para que el usuario pueda auditar el cumplimiento de esta regla cuando la IA cree tests espontáneamente, la respuesta DEBE incluir una sección dedicada llamada `## Test Failure Verification (RED State)` mostrando la traza real del fallo en terminal antes de presentar el pase final en verde.

4. **Herramienta de Verificación:**
   - La suite puede auditarse creando un git worktree aislado (`git worktree add .worktrees/test-check develop`) para ejecutar pruebas de mutación sin alterar el árbol de trabajo principal.

---

## Non-Blocking Tests & Zero-Hang Policy

> [!CAUTION]
> **Prohibición estricta de bloqueos interactivos en la suite de pruebas.**

1. **Sin esperas en `stdin` ni llamadas interactivas:**
   - Ningún test unitario puede solicitar datos por entrada estándar (`std::io::stdin()`) ni quedarse esperando respuestas en `prompt_yes_no`.
   - Cualquier función que acepte confirmación interactiva (`perform_self_uninstall`, etc.) DEBE ser testeada con sus banderas automatizadas (`auto_confirm: true` o `auto_reject: true`).

2. **Detección, resolución inmediata y reporte:**
   - Si una ejecución de `cargo test` excede el tiempo esperado o queda colgada, el asistente DEBE cancelar el proceso inmediatamente, diagnosticar la causa raíz, corregir el código/test para garantizar que sea 100% no-bloqueante o informarlo de inmediato al usuario en lugar de dejar el comando en segundo plano sin resolver.

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
