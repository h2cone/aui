# Repository Guidelines

## Project Structure & Module Organization
- Root binary: `src/main.rs` (thin launcher that calls `aui_gui::run()`).
- Workspace crates live in `crates/`:
  - `aui-gui`: desktop GUI, UI composition, app wiring.
  - `aui-agent-core`: runtime/domain logic (functional core).
  - `aui-ai`: provider gateway and model/provider registry.
  - `aui-tui`, `aui-web-ui`: adapter surfaces around `aui-agent-core`.
- GUI internals are organized under `crates/aui-gui/src/` (`ui/`, `session/`, `app.rs`, etc.).
- Assets: `screenshots/`. Build artifacts: `target/` (do not commit).

## Architecture Overview
- Follow **functional core, imperative shell**:
  - Keep pure transformations and mapping logic deterministic and testable.
  - Keep side effects (windowing, IO, async event loops, provider calls) at boundaries.
- Put reusable runtime behavior in `aui-agent-core`, not GUI modules.

## Build, Test, and Development Commands
- `cargo run` - build and launch the desktop app.
- `cargo check --workspace` - fast compile validation across all crates.
- `cargo test --workspace` - run all unit/integration tests.
- `cargo fmt --all` - format Rust code.
- `cargo clippy --workspace --all-targets` - lint binaries, libs, and tests.
- `cargo build --release` - produce optimized binaries in `target/release/`.

## Coding Style & Naming Conventions
- Rust edition: **2024**. Prefer small, focused changes.
- Naming: `snake_case` (functions/modules), `CamelCase` (types), `SCREAMING_SNAKE_CASE` (constants).
- Keep UI-only code in `crates/aui-gui/src/ui/`.
- Avoid unnecessary dependencies; favor existing workspace crates/utilities.

## Testing Guidelines
- Add focused unit tests next to changed modules (`mod tests { ... }`).
- Keep tests deterministic; avoid real network calls.
- Mock provider/store ports for runtime behavior.
- Example targeted run: `cargo test -p aui-gui`.

## Commit & Pull Request Guidelines
- Use Conventional Commits (`feat:`, `fix:`, `refactor:`, `perf:`, `chore:`).
- PRs should include: what changed, why, risk/impact, and screenshots for UI updates.
- Link related issues and call out any config/env changes.

## Security & Configuration Tips
- Read credentials from environment variables, e.g. `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`/`GOOGLE_API_KEY`.
- Store local config/sessions/logs under the OS config directory in `aui/`.
