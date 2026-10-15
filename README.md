# RivaCross Rust

[English](README.md) | [简体中文](README_zh.md)

A lightweight crosshair editor for Windows x64, built with Rust and egui/eframe. It displays custom crosshairs through RivaTuner Statistics Server (RTSS).

The initial release is **0.1.0**.

Inspired by [irseny/RivaCross](https://github.com/irseny/RivaCross) ([releases](https://github.com/irseny/RivaCross/releases)) and its use of RTSS to display a crosshair. This project brings that idea to a graphical Rust editor with a focus on low background resource use.

## Built to stay lightweight

- Run a single EXE without an installer or OverlayEditor plugin. Hunt SVG assets are embedded in the executable. RTSS runs separately.
- The native Rust and egui/eframe UI uses the Glow renderer and enables only selected dependency features.
- The UI repaints in response to events. A local eframe patch prevents continuous polling while the window is hidden.
- Locking waits for pending output, then suspends connection checks, zoom queries and cache cleanup. The worker waits for commands.
- You can keep the crosshair displayed and exit the editor. RTSS must stay running, and the image cache must remain available.
- Release builds use thin LTO and strip symbols. Executable size and resource use depend on the build and workload.

## Features

| Mode | What you can customize |
| --- | --- |
| Preset | Cross, dot, square, corner brackets and ring; supported size controls, center dot, cross-only T shape, colors and independent outline opacity |
| Image | Static PNG/SVG, transparency and proportional scaling; SVG is rerendered at the target size |
| Text | Printable ASCII rendered directly by RTSS, with color, opacity and size controls |
| Hunt | Built-in crossbow, bow and sniper-crossbow artwork, with weapon instructions and source credits |

The four modes, five preset shapes and three Hunt: Showdown 1896 weapons each remember their settings. You can save up to 100 named schemes. Applying a scheme or resetting defaults affects only the corresponding mode, shape or weapon. Visibility and the configurable global hotkey apply across all modes.

Image and Hunt crosshairs keep the original canvas center when transparent borders are removed. RTSS output is split into tiles so large canvases stay centered when they extend beyond the target window. The preview shows actual size or 4× magnification.

## Screenshots

The editor UI is currently in Chinese. These screenshots show editor previews with RTSS disconnected.

| Cross preset | Ring preset |
| --- | --- |
| [![Cross preset editor](assets/screenshots/preset-cross.png)](assets/screenshots/preset-cross.png) | [![Ring preset editor](assets/screenshots/preset-ring.png)](assets/screenshots/preset-ring.png) |

| Imported image | Hunt crossbow |
| --- | --- |
| [![Image import and preview](assets/screenshots/image-mode.png)](assets/screenshots/image-mode.png) | [![Hunt crossbow preview and instructions](assets/screenshots/hunt-crossbow.png)](assets/screenshots/hunt-crossbow.png) |

## Quick start

1. Start RTSS, enable **On-Screen Display**, and select **Raster3D** rendering. This project was developed with RTSS 7.3.7.
2. Run `RivaCross.exe` from a release package, or build it using the instructions below. The current editor UI is in Chinese.
3. Select a mode and adjust the crosshair. Changes are saved automatically about 0.5 seconds after editing stops.
4. Press **Ctrl+Alt+F9** to toggle visibility. Change the hotkey in the editor if it conflicts with another application.
5. Close the window to minimize to the tray. Double-click the tray icon to reopen; right-click for visibility, lock and exit commands.

Use **Lock crosshair / 锁定准星** when you have finished adjusting the crosshair. It waits for pending output, then disables editing and the visibility hotkey. If RTSS restarts, resets the overlay slot or changes zoom while locked, unlock to reconnect. The editor starts unlocked on each launch.

| Exit command | Result |
| --- | --- |
| Keep crosshair and exit / 保留准星并退出 | Saves configuration and confirms the latest output before ending the editor. Keeps the owned slot and image cache; failed submission keeps the editor open. Hidden output stays hidden. |
| Clear crosshair and exit / 清除准星并退出 | Releases the owned RTSS slot and exits. Also available through Ctrl+Q in the editor. |

The hotkey works only while the editor is running. After RTSS or Windows restarts, launch RivaCross again to restore a retained crosshair. Launching a second instance opens the existing editor.

## Scaling and limits

- Set **RTSS zoom compensation = 0** to read the global OSD zoom automatically, with a fallback of 1× if the query fails. If the target application uses a separate RTSS profile, enter its zoom manually. RivaCross does not change RTSS configuration.
- PNG enlargement uses nearest-neighbor sampling. SVG scales from its source geometry. Text preview may differ from RTSS; large text remains limited by RTSS font texture quality.
- Import static, self-contained PNG/SVG files up to 32 MiB. Decoded images can be at most 4096 pixels per edge and 4,194,304 pixels in total. Animated files and SVGs with external resources are rejected. Transparent SVG margins may be cropped before allocating pixels; visible content must still fit these limits.
- Hunt scale ranges from 0.01 to 16.00, within the image limits. The artwork keeps its colors and alpha; the opacity control multiplies the source alpha. The sniper-crossbow artwork is designed for **16:10**. **16:9 has not been calibrated**.
- Text accepts 1 to 128 printable ASCII characters, excluding `<` and `>`. Chinese, emoji and line breaks are unsupported in text crosshairs.
- Display support depends on RTSS and the target application. Compatibility with specific games and resolutions still needs testing. RivaCross has no automatic per-game switching or startup registration.

## Configuration

RivaCross stores its data in `%LOCALAPPDATA%\RivaCross`. You can open this folder from the editor or tray menu, even while the crosshair is locked.

| Path | Purpose |
| --- | --- |
| `config.json` | Version 3 settings, independent mode/shape/weapon parameters, visibility, hotkey and schemes; saved using atomic replacement |
| `resources/` | Imported source images, retained even if the original is moved |
| `cache/` | Generated PNG textures; cleanup preserves the latest image for reconnection and retained output |
| `last-exit-error.txt` | Diagnostic written when RTSS slot cleanup repeatedly fails during exit |

If no configuration file exists, RivaCross starts with default settings. The `version: 3` value in `config.json` identifies the internal configuration format, separate from the application version. The editor reports invalid configuration. To reset settings, exit the editor, back up and move `config.json` aside, then restart.

## Build and validate

Building requires Windows, a Rust MSVC x64 toolchain, MSVC Build Tools and the Windows SDK. Use the dependency versions in `Cargo.lock`.

```powershell
cargo run --locked
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --release --locked
```

The release executable is `target/release/rivacross.exe`; it can be renamed to `RivaCross.exe`. Keep the development-only `ui-capture` feature disabled in release builds.

With RTSS running, use `cargo run --locked -- --inspect` to inspect RivaCross's overlay slot or `cargo run --locked -- --query-zoom` to read global scaling. Exit the editor before running `cargo run --locked -- --probe`, which submits sample overlays and releases its slot. Ignored UI and live RTSS tests need a separate test setup.

See the [vendored eframe patch notes](vendor/README.md) for details of the hidden-window event-loop fix.

## Inspiration and credits

- [irseny/RivaCross](https://github.com/irseny/RivaCross) ([releases](https://github.com/irseny/RivaCross/releases)) inspired the idea of displaying a customizable crosshair through RTSS. Thanks to Irseny.
- RivaTuner Statistics Server and its SDK provide the overlay platform.
- The crossbow and bow artwork comes from bilibili @Zuko819470293, credited in the editor.
- The sniper-crossbow artwork is based on [Hunt:showdown_crossbow_sniper_steel&slug_16:10](https://steamcommunity.com/sharedfiles/filedetails/?id=3316676875) from the Steam Crosshair Workshop. The embedded version is a vector redraw with 19 editable paths.
- The UI uses egui/eframe with the documented local event-loop patch.

Dependencies and artwork retain their respective copyright and license terms.
