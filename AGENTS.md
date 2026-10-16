# Repository Guidelines

Use repository-root-relative paths in these guidelines. Keep machine-specific absolute paths and local agent configuration out of this file.

## Project Structure & Module Organization

RivaCross Rust is a lightweight Windows x64 crosshair editor using Rust 2024, egui/eframe, and RTSS. The idea of displaying a customizable crosshair through RTSS was inspired by [irseny/RivaCross](https://github.com/irseny/RivaCross) ([releases](https://github.com/irseny/RivaCross/releases)). Keep this inspiration credit in both public READMEs. Use “inspired by” / “灵感来源” wording rather than describing that repository as the original project or upstream of this repository.

`src/main.rs` initializes the application and diagnostic commands. `src/app/mod.rs` coordinates the editor lifecycle; `editor.rs` edits parameters and schemes, `preview.rs` owns preview textures, `desktop.rs` manages tray, hotkeys and opening the data directory, and `hunt_help.rs` displays weapon instructions and asset credits. `src/model.rs` defines four modes (Preset, Image, Text, Hunt), five preset settings and three independent Hunt weapon settings. `image_input.rs` validates and decodes images; `render.rs` calculates pixels, centered cropping and textures; `rtss_tags.rs` generates tiled image tags from explicit inputs. `output.rs` assembles resource loading, rendering, caching and tags through `storage.rs`. `rtss.rs` implements native RTSS access; `worker/mod.rs` performs threaded operations while `worker/state.rs` owns deterministic scheduling and state transitions. `events.rs`/`instance.rs` manage native events and single-instance behavior. All these modules are under `src/`.

Tests live inside source modules. `docs/` contains implementation and verification notes; `scripts/package.ps1` packages releases. `vendor/eframe/` contains a documented event-loop patch. Generated builds belong in `target/`, packaged artifacts in `dist/`; imported assets are stored in the user's application-data directory.

`assets/` contains the three Hunt SVGs embedded by `include_bytes!`, source notes and a reference sniper-crossbow SVG used by tests. Preserve asset credits, editable vector paths and the original canvas center. Ignore rules allow the three embedded SVGs and `assets/reference/sniper-crossbow-original.svg`; other local artwork remains ignored. `README.md` is the English GitHub entry point; `README_zh.md` is its Chinese counterpart; `learn-docs/README.md` is the Chinese source-reading guide. Keep both public READMEs consistent. Before publishing, check that required SVG inputs are tracked. Local `scripts/`, `docs/` and `learn-docs/` remain ignored; public README instructions and links must work with the published checkout.

## Build, Test, and Development Commands

Public README screenshots live in `assets/screenshots/` with explicit ignore-rule exceptions. Keep English and Chinese captions consistent, and describe editor previews separately from game captures.

Use Windows with the Rust MSVC x64 toolchain, MSVC linker, and Windows SDK. Keep dependency resolution consistent with `Cargo.lock`.

- `cargo run --locked`: launch the editor locally.
- `cargo fmt --all -- --check`: check standard Rust formatting.
- `cargo test --locked`: run unit and Windows platform tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: reject lint warnings.
- `cargo build --release --locked`: produce `target/release/rivacross.exe`.
- `./scripts/package.ps1`: build and package the executable, documentation, and SHA256 checksum.

With RTSS running, `cargo run --locked -- --inspect` reports slot state and `--query-zoom` reads global scaling. Exit the editor before running `--probe`, which submits sample overlays and releases its slot.

For live centering QA, first exit the editor and clear any retained RivaCross overlay, then run `cargo test --locked --no-run` and `python scripts/verify-hunt-live.py --zoom 2 --full`. Python requires Pillow and NumPy. The script opens a visible test window; `--zoom` accepts 1–4 and omitting `--full` tests the production crop path. It uses a test-process RTSS profile override without saving RTSS settings. Keep ignored live/UI tests opt-in.

## Coding Style & Naming Conventions

Use rustfmt defaults, four-space indentation, `snake_case` functions/modules, and `PascalCase` types. Follow existing `anyhow::Result` error handling and Chinese user-facing messages. Keep Win32 and shared-memory operations within their dedicated modules. Document changes to the vendored patch in `vendor/README.md`.

## Testing Guidelines

Use Rust's `#[test]` framework in `#[cfg(test)] mod tests`, with descriptive `snake_case` names. Add regression tests for changed behavior; use temporary directories and isolated RTSS mappings. No numeric coverage threshold is configured. Record manual UI/RTSS checks and remaining limitations in `docs/verification.md`.

## Commit & Pull Request Guidelines

Use concise, imperative commit subjects such as `Fix hidden-window event handling`. PRs should describe the behavior change, link relevant issues, list validation results, and include screenshots for visible UI changes. Describe lightweight behavior through implementation or measured evidence; do not promise zero resource usage, a fixed executable size or universal game compatibility.

## Configuration & Integration Boundaries

The current application version is 0.1.1, which fixes semi-transparent RTSS output and preview consistency. Configuration format version 3 is an internal schema identifier, not a release number. Public documentation should describe first-run defaults and reset behavior without presenting development schema changes as released upgrade history.

User data lives under `%LOCALAPPDATA%\RivaCross`. Preserve atomic configuration saves, image-import validation, and ownership checks for RTSS slots. Do not modify RTSS configuration or other clients' overlays. Keep the development-only `ui-capture` feature disabled in release packages.

Configuration requires complete version 3 data. Reject version 2 and earlier, missing required fields and invalid weapon values; do not silently migrate or replace malformed files. Only missing `hunt_settings.sniper_crossbow` defaults in an otherwise complete version 3 file. Switching, applying schemes and resetting must preserve independent mode, shape and weapon settings.

Preserve event-driven idle behavior and the vendored hidden-window fix. Locking drains pending output before blocking and disables periodic checks; unlocking reconnects. Keep both exit paths: retaining confirms the latest output and preserves its slot/cache, while clearing releases owned slots. Retain failures keep the editor running. For image/Hunt output, crop symmetrically around the original canvas center before applying opacity; keep source-pixel scale, RTSS zoom compensation and bounded tiled layers so oversized canvases cannot shift the aim point.
