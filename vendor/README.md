# Local eframe patch

`eframe/` is eframe 0.31.1 from the crates.io package, licensed MIT OR Apache-2.0.
Upstream: https://github.com/emilk/egui/tree/0.31.1/crates/eframe
This copy is redistributed under MIT; the upstream MIT copyright and license
are retained in `eframe/LICENSE-MIT`. Package include paths point at that local
license, instead of the upstream workspace's parent directory.

The only functional patch is in `src/native/run.rs::check_redraw_requests`:
skip normal redraw requests for hidden windows and return to `ControlFlow::Wait`
when no repaint is pending. Windows does not ordinarily paint hidden windows;
leaving the loop in `Poll` after hiding otherwise consumes approximately one core.
Explicit WM_PAINT messages used by this app still wake its tray actions.

Regression coverage: `app::tests::locked_app_stays_idle_after_close_to_tray_and_reopen`
uses the real editor and native event loop, isolated RTSS memory, a nondefault
configuration, a native close request, CPU sampling, and reopening the same window.
