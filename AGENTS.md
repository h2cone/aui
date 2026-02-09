# Repository Guidelines

## Project Structure & Module Organization
- Root crate (`src/`) hosts the desktop GUI entrypoint and integration code:
  - `src/main.rs`: app bootstrap and keybindings.
  - `src/app.rs`: GUI orchestration and runtime dispatch.
  - `src/ui/`: visual components (`conversation`, `sidebar`, `settings_panel`, widgets).
  - `src/providers/`, `src/session/`, `src/model_catalog.rs`: adapters and persistence used by the GUI path.
- Core architecture lives in workspace crates under `crates/`:
  - `aui-core-domain`, `aui-core-ports`, `aui-core-engine`, `aui-runtime-native`, `aui-bridge`, `aui-ui-tui`, `aui-ui-web`.
- Docs/assets: `README.md`, `screenshots/`. Build output: `target/` (do not commit).

## Build, Test, and Development Commands
- `cargo run`: build and launch the GUI app.
- `cargo build --release`: optimized build in `target/release/`.
- `cargo test --workspace`: run all crate and integration tests.
- `cargo fmt`: format all Rust code.
- `cargo clippy --all-targets`: lint binaries, tests, and library targets.

## Coding Style & Naming Conventions
- Rust edition: 2024. Keep code idiomatic and dependency-light.
- Naming: `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- Keep UI concerns inside `src/ui/`; place cross-UI business logic in core crates.
- Prefer small, focused changes over broad refactors.

## Testing Guidelines
- Add focused unit tests adjacent to changed modules (e.g., `mod tests { ... }`).
- Favor deterministic tests; avoid real network calls (mock or inject ports).
- For cross-surface consistency, prefer workspace-level tests (see `crates/aui-ui-web/tests/`).
- Run `cargo test --workspace` before opening a PR.

## Commit & Pull Request Guidelines
- Use Conventional Commits style seen in history (`feat:`, `refactor:`, `perf:`, `chore:`).
- PRs should include: what changed, why, risk/impact, and screenshots for UI changes.
- Link related issues and list any new env vars or config keys.

## Configuration & Secrets
- Credentials come from environment variables (e.g., `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`).
- Local config is stored under the OS config directory in `aui/` (config, sessions, logs).
