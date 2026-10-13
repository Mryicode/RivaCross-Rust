# Repository Guidelines

## Project Structure & Module Organization

RivaCross is a Windows x64 crosshair editor using Rust 2024, egui/eframe, and RTSS. `src/main.rs` initializes the application and diagnostic commands. `app/mod.rs` coordinates the editor lifecycle, `app/editor.rs` edits parameters and schemes, `app/preview.rs` owns preview textures, and `app/desktop.rs` manages tray and hotkey resources. `model.rs` defines settings; `image_input.rs` validates and decodes images; `render.rs` calculates pixels and textures; `rtss_tags.rs` generates tags from explicit inputs. `output.rs` assembles resource loading, rendering, caching and tags through `storage.rs`. `rtss.rs` implements native RTSS access; `worker/mod.rs` performs threaded operations while `worker/state.rs` owns deterministic scheduling and state transitions. `events.rs`/`instance.rs` manage native events and single-instance behavior.

Tests live inside source modules. `docs/` contains implementation and verification notes; `scripts/package.ps1` packages releases. `vendor/eframe/` contains a documented event-loop patch. Generated builds belong in `target/`, packaged artifacts in `dist/`; imported assets are stored in the user's application-data directory.

## Build, Test, and Development Commands

Use Windows with the Rust MSVC x64 toolchain, MSVC linker, and Windows SDK. Keep dependency resolution consistent with `Cargo.lock`.

- `cargo run --locked`: launch the editor locally.
- `cargo fmt --all -- --check`: check standard Rust formatting.
- `cargo test --locked`: run unit and Windows platform tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: reject lint warnings.
- `cargo build --release --locked`: produce `target/release/rivacross.exe`.
- `./scripts/package.ps1`: build and package the executable, documentation, and SHA256 checksum.

With RTSS running, `cargo run --locked -- --inspect` reports slot state and `--query-zoom` reads global scaling. Exit the editor before running `--probe`, which submits sample overlays and releases its slot.

## Coding Style & Naming Conventions

Use rustfmt defaults, four-space indentation, `snake_case` functions/modules, and `PascalCase` types. Follow existing `anyhow::Result` error handling and Chinese user-facing messages. Keep Win32 and shared-memory operations within their dedicated modules. Document changes to the vendored patch in `vendor/README.md`.

## Testing Guidelines

Use Rust's `#[test]` framework in `#[cfg(test)] mod tests`, with descriptive `snake_case` names. Add regression tests for changed behavior; use temporary directories and isolated RTSS mappings. No numeric coverage threshold is configured. Record manual UI/RTSS checks and remaining limitations in `docs/verification.md`.

## Commit & Pull Request Guidelines

There is no Git history yet. Use concise, imperative commit subjects such as `Fix hidden-window event handling`. PRs should describe the behavior change, link relevant issues, list validation results, and include screenshots for visible UI changes.

## Configuration & Integration Boundaries

User data lives under `%LOCALAPPDATA%\RivaCross`. Preserve atomic configuration saves, image-import validation, and ownership checks for RTSS slots. Do not modify RTSS configuration or other clients' overlays. Keep the development-only `ui-capture` feature disabled in release packages.
