# Repository Guidelines

## Project Structure & Module Organization
- Root crate (`src/`) contains the desktop GUI entrypoint and integration logic.
  - `src/main.rs`: app bootstrap and keybindings.
  - `src/app.rs`: UI orchestration, runtime wiring, provider/session flow.
  - `src/ui/`: visual components (`conversation`, `sidebar`, `settings_panel`, widgets).
  - `src/session/`, `src/model_catalog.rs`: local persistence and model metadata.
- Workspace crates in `crates/`:
  - `aui-ai`: unified multi-provider LLM gateway and provider registry.
  - `aui-agent-core`: agent runtime, state reducer, bridge protocol, persistence ports.
  - `aui-ui-tui`, `aui-ui-web`: adapter surfaces around `aui-agent-core`.
- Other paths: `screenshots/` for assets, `target/` for build output (do not commit).

## Build, Test, and Development Commands
- `cargo run` — build and launch the desktop GUI.
- `cargo build --release` — optimized binaries under `target/release/`.
- `cargo check --workspace` — fast compile verification for all crates.
- `cargo test --workspace` — run unit/integration tests across workspace.
- `cargo fmt` — format all Rust code.
- `cargo clippy --all-targets` — lint binaries, tests, and libs.

## Coding Style & Naming Conventions
- Rust edition: **2024**; prefer small, focused changes.
- Naming: `snake_case` (functions/modules), `CamelCase` (types), `SCREAMING_SNAKE_CASE` (constants).
- Keep UI-specific code inside `src/ui/`; keep reusable runtime/agent logic in `crates/aui-agent-core`.
- Avoid adding dependencies unless clearly needed.

## Testing Guidelines
- Place focused unit tests near changed modules (`mod tests { ... }`).
- Keep tests deterministic; avoid real network calls.
- Mock provider/store ports when testing runtime behavior.
- Run `cargo test --workspace` before opening a PR.

## Commit & Pull Request Guidelines
- Use Conventional Commits (`feat:`, `refactor:`, `perf:`, `chore:`).
- PRs should include: what changed, why, risk/impact, and screenshots for UI updates.
- Link related issues and note any new config keys or env vars.

## Security & Configuration Tips
- Read credentials from env vars (e.g., `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY` or `GOOGLE_API_KEY`).
- Store local config/sessions/logs under the OS config directory in `aui/`.
