# AGENTS.md — fast-alias (`fa`)

Recipe-based project scaffolder CLI that bootstraps projects, installs dependencies, and applies tooling (linter, formatter, typechecker) and configuration files on Debian and Termux (temporary name: `project-fast-alias`, version: `0.1.0-beta.1`).

## Project Facts
- Binary crate `fa` v0.1.0-beta.1, edition 2024 (`Cargo.toml`). Written in Rust with no heavy external dependencies.
- **Recipe Player Model**: `fa` is a "recipe player" — recipes are declared in TOML files (`~/.config/fa/recipes.toml` + `~/.config/fa/recipes.d/*.toml`) in the user's personal config directory. Adding a language/toolchain = adding a `.toml`, never touching code.
- **Command Renaming Note**: Executable binary command is `fa` (from `fast-alias`) for CLI user convenience.
- Target platforms: **Debian** and **Termux** (via the `project-dots` release pattern: `install.sh` + GitHub Releases + cross-compiled musl).
- Entrypoint: `src/main.rs` (future).
- `.gitignore` ignores `/target` and `.codegraph`.

## Commands
- Build / Run (Dev): `cargo build` / `cargo run`
- Release Build: `cargo build --release` (binary at `./target/release/fa`)
- Local Installation: `cargo install --path .` (installs `fa` executable to `~/.cargo/bin` or `$PATH`)
- Code linting: `cargo clippy` (warnings)
- Type checking: `cargo check`
- Unit Testing: `cargo test -- --nocapture`

## CodeGraph (Fast file & symbol lookup)
CodeGraph v1.5.0 is installed globally (`~/.local/bin/codegraph`) with an active MCP server.
- Find files / symbols: `codegraph files`, `codegraph query <symbol>`
- Explore area: `codegraph explore "<topic>"`
- Sync index: `codegraph sync`

## Additional Documentation
For detailed architecture, roadmap, CLI reference, recipe schema, unit testing, and release specifications, consult:
- Planning and Roadmap: @docs/planning.md
- Recipes and Configuration: @docs/recipes.md
- Debian & Termux Support: @docs/platforms.md
- CLI Reference & Commands: @docs/commands.md
- Unit Testing Guide: @docs/testing.md
- Versioning & Release Guide: @docs/versioning.md
- Environment Variables & Token Security: @docs/environment.md

## Rules and Conventions
- Keep the codebase lightweight and modular in Rust.
- Use concise bullet points for agent rules and documentation.
- Present user-facing commands using the compiled binary (`fa <command>`) rather than `cargo run --`.
- **Recipe Player Principle**: The engine must stay "dumb" — all stack/toolchain complexity lives in the TOML recipes, never hardcoded in the binary. No recipe-specific logic in code.
- **Security Governance & Secret Leak Prevention**:
  - NEVER commit API keys, private keys (`id_*`), certificates (`*.key`, `*.pem`), or `.env` files within project directories. Templates and recipes live in `~/.config/fa/` on disk — no config is embedded in the release binary.
  - Never log runtime tokens (e.g. `GITHUB_TOKEN`) in stdout, stderr, or `state.toml`.
  - Ask for a one-time `[TRUST]` confirmation before running recipe `[[steps]]`/`create` commands (`fa new`) or command aliases (`fa alias`), since they execute arbitrary shell. Persist the decision by config path in `~/.local/state/fa/state.toml`; never re-prompt for the same path ("one covers all": trusting the primary `recipes.toml` covers `recipes.d/*.toml`). Auto-trust the provisioned example config.
  - When a subcommand runs without an interactive terminal, auto-feed `y` to stdin so approval prompts (e.g. pnpm `minimumReleaseAge` continuation) do not abort the install.
- **Recipe & Template Naming Convention**:
  - Recipe names and template subdirectories under `~/.config/fa/templates/` must NEVER be generic (e.g., avoid `app`, `web`, `cli`, `template`, `starter`).
  - Names must be simple but distinctive, combining the stack/tool type with its specific variant, toolchain, or flavor (e.g., `next-ts`, `ts-lib`, `rust-cli`).
  - This prevents naming collisions when multiple distinct configurations exist for the same tool or stack.
- **Explicit Alias Governance**:
  - Aliases for recipes or commands must ONLY be created when explicitly defined by the user.
  - Never generate, infer, or automatically append unrequested aliases. Always consult or ask the user before defining aliases.
- **Recipe Inline Content Size Limit**:
  - `inline` file content in `recipes.toml`/`recipes.d/*.toml` MUST NOT exceed 40 lines.
  - Longer files MUST move to `~/.config/fa/templates/<recipe-name>/` via `{ from = ... }` (read from disk at runtime, no Rust edits).
  - Special cases beyond 40 lines inline require explicit user approval — ask before proceeding. See `.agents/rules/recipes-content.md` and @docs/recipes.md.
- **Git Strategy & Branch Restrictions**:
  - Never use `git checkout`. Use modern git commands (`git switch`, `git restore`).
  - User Git Aliases: `git s` -> `git switch`, `git b` -> `git branch`.
  - All development edits must be conducted on the development branch (`develop` or feature branches).
  - **Main Branch Strict Prohibition**: The assistant must NEVER switch to, merge into, or touch the `main` branch under any circumstances unless explicitly requested by the user in their message.
  - **Explicit Merge Restriction**: Never execute a branch merge (`git merge`) unless the user explicitly instructs to merge in their message.
  - **Explicit Push Restriction**: Never execute a remote push command (`git push`) unless the user explicitly instructs to push in their message. Before pushing, clearly explain what commits, branches, or tags will be pushed and request confirmation.
- **User Version Control & Version Bumping Policy**:
  - **User Defines Versions Exclusively**: The user exclusively determines, authorizes, and defines version numbers (e.g. `v0.1.0-beta.1`) and release triggers. The assistant MAY ONLY suggest version numbers when asked, and MUST NEVER increment or change version numbers independently.
  - Do NOT increment or bump the version number for intermediate local commits or small feature/bugfix edits.
  - **Bump Trigger**: Version updates (`Cargo.toml`, `Cargo.lock`, `install.sh`, `AGENTS.md`) are executed ONLY when the user explicitly defines the target version and instructs to bump/publish.
