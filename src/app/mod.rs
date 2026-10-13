mod desktop;
mod editor;
mod preview;
use crate::{
    instance::Instance,
    model::Config,
    storage::Store,
    worker::{Command, Worker, WorkerStatus},
};
use desktop::Desktop;
use eframe::egui::{self, Color32};
use global_hotkey::HotKeyState;
use preview::Preview;
use std::time::{Duration, Instant};

pub struct App {
    config: Config,
    store: Store,
    worker: Worker,
    bridge: crate::events::Bridge,
    _instance: Instance,
    desktop: Desktop,
    hotkey_input: String,
    error: String,
    status: WorkerStatus,
    name: String,
    preview: Preview,
    dirty_since: Option<Instant>,
    exiting: bool,
    locked: bool,
    hidden: bool,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, store: Store, instance: Instance) -> Self {
        Self::new_with_worker(cc, store, instance, Worker::start)
    }
    fn new_with_worker(
        cc: &eframe::CreationContext<'_>,
        store: Store,
        instance: Instance,
        start_worker: fn(std::path::PathBuf, egui::Context) -> Worker,
    ) -> Self {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window = match cc.window_handle().expect("Windows native handle").as_raw() {
            RawWindowHandle::Win32(handle) => handle.hwnd.get() as usize,
            _ => unreachable!("Windows-only application"),
        };
        let bridge = crate::events::Bridge::new(&cc.egui_ctx, window, instance.event_handle());
        let mut fonts = egui::FontDefinitions::default();
        for path in [
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
        ] {
            if let Ok(data) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("Chinese".into(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, "Chinese".into());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let (config, error) = match store.load() {
            Ok(c) => (c, String::new()),
            Err(e) => (Config::default(), e.to_string()),
        };
        let (desktop, desktop_error) = Desktop::new();
        let error = desktop_error.map_or(error.clone(), |e| format!("{error} {e}"));
        let worker = start_worker(store.root().to_path_buf(), cc.egui_ctx.clone());
        let _ = worker
            .tx
            .send(Command::Update(config.current.clone(), config.visible));
        #[cfg(feature = "ui-capture")]
        if std::env::var_os("EFRAME_SCREENSHOT_TO").is_some() {
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                if worker
                    .status
                    .recv_timeout(Duration::from_millis(50))
                    .is_ok_and(|s| s == WorkerStatus::Submitted)
                {
                    break;
                }
            }
        }
        let hotkey_input = config.hotkey.clone();
        let mut app = Self {
            config,
            store,
            worker,
            bridge,
            _instance: instance,
            desktop,
            hotkey_input,
            error,
            status: WorkerStatus::Waiting,
            name: String::new(),
            preview: Preview::default(),
            dirty_since: None,
            exiting: false,
            locked: false,
            hidden: false,
        };
        app.register_hotkey();
        app
    }
    fn register_hotkey(&mut self) {
        match self.desktop.register_hotkey(&self.hotkey_input) {
            Ok(()) => self.config.hotkey = self.hotkey_input.clone(),
            Err(e) => self.error = e.to_string(),
        }
    }
    fn open(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    fn changed(&mut self) {
        self.dirty_since = Some(Instant::now());
        if self.locked {
            return;
        }
        self.submit();
    }
    fn submit(&self) {
        let _ = self.worker.tx.send(Command::Update(
            self.config.current.clone(),
            self.config.visible,
        ));
    }
    fn save(&mut self) {
        match self.store.save(&self.config) {
            Ok(()) => self.dirty_since = None,
            Err(e) => {
                self.error = format!("保存失败：{e}");
                self.dirty_since = None;
            }
        }
    }
    fn events(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Q)) {
            self.request_exit(ctx, false);
        }
        while let Ok(event) = self.bridge.rx.try_recv() {
            match event {
                crate::events::Event::Open => self.open(ctx),
                crate::events::Event::Menu(event) => {
                    if event.id == self.desktop.open_item.id() {
                        self.open(ctx);
                    } else if event.id == self.desktop.toggle_item.id() && !self.locked {
                        self.config.visible = !self.config.visible;
                        self.changed();
                        self.save();
                    } else if event.id == self.desktop.exit_item.id() {
                        self.request_exit(ctx, false);
                    } else if event.id == self.desktop.keep_exit_item.id() {
                        self.request_exit(ctx, true);
                    } else if event.id == self.desktop.lock_item.id() {
                        self.set_locked(!self.locked);
                    }
                }
                crate::events::Event::Hotkey(event) => {
                    if event.state == HotKeyState::Pressed
                        && !self.locked
                        && self.desktop.hotkey.is_some_and(|h| h.id() == event.id)
                    {
                        self.config.visible = !self.config.visible;
                        self.changed();
                        self.save();
                    }
                }
            }
        }
        while let Ok(s) = self.worker.status.try_recv() {
            self.status = s;
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.exiting
            && self.desktop._tray.is_some()
        {
            self.hide_to_tray(ctx);
        }
    }
    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.save();
    }
    fn set_locked(&mut self, locked: bool) {
        self.save();
        self.locked = locked;
        self.desktop.lock_item.set_text(if locked {
            "解锁准星"
        } else {
            "锁定准星"
        });
        self.desktop.toggle_item.set_enabled(!locked);
        self.status = if locked {
            WorkerStatus::LockWaiting
        } else {
            WorkerStatus::Rechecking
        };
        let _ = self.worker.tx.send(Command::Lock(locked));
    }
    fn request_exit(&mut self, ctx: &egui::Context, keep: bool) {
        if self.exiting {
            return;
        }
        if keep && !self.locked {
            self.changed();
        }
        self.save();
        if let Err(e) = self.worker.finish(keep) {
            self.error = format!("退出失败：{e}");
            self.open(ctx);
            return;
        }
        self.exiting = true;
        self.bridge.close(ctx);
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.bridge.stop();
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.events(ctx);
        if self.hidden || self.exiting {
            return;
        }
        let before = self.config.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RivaCross · 独立准星");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.locked, |ui| {
                    ui.checkbox(&mut self.config.visible, "显示准星");
                });
                if ui
                    .button(if self.locked {
                        "解锁准星"
                    } else {
                        "锁定准星"
                    })
                    .clicked()
                {
                    // Queue the latest UI state before entering the worker's idle wait.
                    if !self.locked {
                        self.changed();
                    }
                    self.set_locked(!self.locked);
                }
            });
            ui.label(status_text(self.status.clone()));
            ui.horizontal(|ui| {
                if ui.button("保留准星并退出").clicked() {
                    self.request_exit(ctx, true);
                }
                if ui.button("清除准星并退出").clicked() {
                    self.request_exit(ctx, false);
                }
            });
            if self.locked {
                ui.small("已锁定：暂停后台检查和显隐热键；RTSS 重启或倍率变化后请解锁。");
            }
            if !self.error.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(Color32::LIGHT_RED, &self.error);
                    if ui.button("关闭提示").clicked() {
                        self.error.clear();
                    }
                });
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_enabled_ui(!self.locked, |ui| {
                    editor::controls(ui, &mut self.config, &self.store, &mut self.error);
                    ui.separator();
                    if let Some(error) = self.preview.draw(ui, &self.config.current, &self.store) {
                        self.error = error;
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("显隐热键");
                        ui.text_edit_singleline(&mut self.hotkey_input);
                        if ui.button("应用热键").clicked() {
                            self.register_hotkey();
                        }
                    });
                    ui.small("格式示例：Ctrl+Alt+F9；关闭窗口收起到托盘。");
                    if editor::schemes(ui, &mut self.config, &mut self.name, &mut self.error) {
                        self.save();
                    }
                });
            });
        });
        let edit = editor::EditResult::between(&before, &self.config);
        if edit.save {
            self.dirty_since = Some(Instant::now());
        }
        if edit.output && !self.locked {
            self.submit();
        }
        if self
            .dirty_since
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(500))
        {
            self.save();
        }
        if let Some(since) = self.dirty_since {
            ctx.request_repaint_after(Duration::from_millis(500).saturating_sub(since.elapsed()));
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
    }
}

fn status_text(status: WorkerStatus) -> String {
    match status {
        WorkerStatus::Waiting => "等待 RTSS".into(),
        WorkerStatus::Rechecking => "已解锁，正在重新检查 RTSS".into(),
        WorkerStatus::Connected => "RTSS 已连接".into(),
        WorkerStatus::Submitted => "RTSS 已连接 · 当前内容已提交".into(),
        WorkerStatus::Busy => "RTSS 锁忙，自动重试中".into(),
        WorkerStatus::LockWaiting => "正在锁定，等待提交完成".into(),
        WorkerStatus::Paused => "已锁定 · 后台检查已暂停".into(),
        WorkerStatus::ContentError(e) => format!("内容错误：{e}"),
        WorkerStatus::Error(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Crosshair, Shape},
        render,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        thread,
    };
    use windows::Win32::{
        Foundation::{HANDLE, HWND},
        System::Threading::SetEvent,
        UI::WindowsAndMessaging::{IsWindowVisible, PostMessageW, WM_CLOSE},
    };

    #[test]
    #[ignore = "manual screenshot QA; opens the editor with isolated data and RTSS mappings"]
    fn capture_preset_controls() {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        const PRESETS: [(Shape, &str, f32); 6] = [
            (Shape::Cross, "cross", 10.),
            (Shape::Dot, "dot", 10.),
            (Shape::Square, "square", 10.),
            (Shape::Quadrant, "quadrant", 14.),
            (Shape::Quadrant, "quadrant-short", 6.),
            (Shape::Ring, "ring", 10.),
        ];
        struct CaptureApp {
            app: App,
            output: std::path::PathBuf,
            index: usize,
            frames: u8,
            requested: bool,
            deadline: Instant,
        }
        impl eframe::App for CaptureApp {
            fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
                assert!(
                    Instant::now() < self.deadline,
                    "screenshot capture timed out"
                );
                self.app.update(ctx, frame);
                let screenshot = ctx.input(|input| {
                    input.events.iter().find_map(|event| {
                        if let egui::Event::Screenshot { image, .. } = event {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                });
                if let Some(image) = screenshot {
                    render::preset(&self.app.config.current)
                        .unwrap()
                        .save(
                            self.output
                                .join(format!("{}-raster.png", PRESETS[self.index].1)),
                        )
                        .unwrap();
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, rgba)
                        .unwrap()
                        .save(self.output.join(format!("{}.png", PRESETS[self.index].1)))
                        .unwrap();
                    self.index += 1;
                    if self.index == PRESETS.len() {
                        self.app.request_exit(ctx, false);
                        return;
                    }
                    self.app.config.switch_shape(PRESETS[self.index].0);
                    self.app.config.current.length = PRESETS[self.index].2;
                    self.app.config.current.thickness = 4.;
                    self.app.config.current.gap = 12.;
                    self.app.config.current.outline = 2.;
                    self.app.config.current.outline_color = [255, 0, 255];
                    self.app.config.current.center_dot = PRESETS[self.index].0 != Shape::Quadrant;
                    self.app.preview.zoom = if PRESETS[self.index].0 == Shape::Quadrant {
                        4.
                    } else {
                        1.
                    };
                    self.frames = 0;
                    self.requested = false;
                }
                if !self.app.exiting && !self.requested {
                    self.frames += 1;
                    if self.frames >= 3 {
                        ctx.send_viewport_cmd(
                            egui::ViewportCommand::Screenshot(Default::default()),
                        );
                        self.requested = true;
                    }
                }
                ctx.request_repaint();
            }
        }
        let root = tempfile::tempdir().unwrap();
        let store = Store::at(root.path().into()).unwrap();
        let config = Config {
            current: Crosshair {
                gap: 12.,
                thickness: 4.,
                center_dot: true,
                t_shape: true,
                outline: 2.,
                outline_color: [255, 0, 255],
                rtss_zoom: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        store.save(&config).unwrap();
        let output = std::env::current_dir().unwrap().join("target/preset-ui");
        std::fs::create_dir_all(&output).unwrap();
        let instance = Instance::acquire()
            .unwrap()
            .expect("close the editor before screenshot QA");
        eframe::run_native(
            "RivaCross preset QA",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([720., 800.]),
                event_loop_builder: Some(Box::new(|builder| {
                    builder.with_any_thread(true);
                })),
                ..Default::default()
            },
            Box::new(move |cc| {
                Ok(Box::new(CaptureApp {
                    app: App::new_with_worker(cc, store, instance, Worker::start_mock),
                    output,
                    index: 0,
                    frames: 0,
                    requested: false,
                    deadline: Instant::now() + Duration::from_secs(30),
                }))
            }),
        )
        .unwrap();
    }

    #[test]
    fn locked_app_stays_idle_after_close_to_tray_and_reopen() {
        const CHILD: &str = "RIVACROSS_LOCKED_TRAY_TEST";
        if std::env::var_os(CHILD).is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "app::tests::locked_app_stays_idle_after_close_to_tray_and_reopen",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "locked tray regression failed");
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("locked tray test timed out");
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use winit::platform::windows::EventLoopBuilderExtWindows;
        struct TestApp {
            app: App,
            phase: u8,
            window: usize,
            cpu: Arc<AtomicU64>,
            expected: Crosshair,
        }
        impl eframe::App for TestApp {
            fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
                // CI has no Explorer tray. Exercise the same close handler without an icon.
                if !self.app.exiting
                    && self.app.desktop._tray.is_none()
                    && ctx.input(|i| i.viewport().close_requested())
                {
                    self.app.hide_to_tray(ctx);
                }
                self.app.update(ctx, frame);
                if self.phase == 0 {
                    self.app.set_locked(true);
                    self.phase = 1;
                } else if self.phase == 1 && self.app.status == WorkerStatus::Paused {
                    self.phase = 2;
                    let window = self.window;
                    let event = self.app._instance.event_handle();
                    let cpu = self.cpu.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(300));
                        unsafe {
                            PostMessageW(
                                Some(HWND(window as *mut _)),
                                WM_CLOSE,
                                Default::default(),
                                Default::default(),
                            )
                            .unwrap();
                        }
                        thread::sleep(Duration::from_millis(600));
                        assert!(!unsafe { IsWindowVisible(HWND(window as *mut _)) }.as_bool());
                        let before = crate::events::tests::process_cpu_ticks();
                        thread::sleep(Duration::from_secs(1));
                        cpu.store(
                            crate::events::tests::process_cpu_ticks() - before,
                            Ordering::Release,
                        );
                        unsafe {
                            SetEvent(HANDLE(event as *mut _)).unwrap();
                        }
                    });
                } else if self.phase == 2 && self.cpu.load(Ordering::Acquire) != u64::MAX {
                    assert!(self.app.locked, "closing to tray unlocked the crosshair");
                    assert!(self.app.status == WorkerStatus::Paused);
                    assert!(!self.app.hidden, "reopening did not restore the window");
                    assert_eq!(
                        self.app.config.current, self.expected,
                        "closing to tray changed the crosshair"
                    );
                    assert_eq!(
                        self.app.store.load().unwrap().current,
                        self.expected,
                        "closing to tray overwrote saved settings"
                    );
                    self.phase = 3;
                    self.app.request_exit(ctx, false);
                }
            }
        }
        let root = tempfile::tempdir().unwrap();
        let store = Store::at(root.path().into()).unwrap();
        let mut config = Config::default();
        config.current.rtss_zoom = 1;
        config.current.shape = Shape::Ring;
        config.current.color = [255, 40, 160];
        config.current.size = 27.;
        config.current.x = 11;
        let expected = config.current.clone();
        store.save(&config).unwrap();
        let instance = Instance::acquire()
            .unwrap()
            .expect("close the running editor before native regression tests");
        let cpu = Arc::new(AtomicU64::new(u64::MAX));
        eframe::run_native(
            "RivaCross locked tray regression",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([720., 800.]),
                event_loop_builder: Some(Box::new(|builder| {
                    builder.with_any_thread(true);
                })),
                ..Default::default()
            },
            Box::new({
                let cpu = cpu.clone();
                move |cc| {
                    let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw()
                    else {
                        unreachable!()
                    };
                    let window = handle.hwnd.get() as usize;
                    let app = App::new_with_worker(cc, store, instance, Worker::start_mock);
                    Ok(Box::new(TestApp {
                        app,
                        phase: 0,
                        window,
                        cpu,
                        expected,
                    }))
                }
            }),
        )
        .unwrap();
        let ticks = cpu.load(Ordering::Acquire);
        println!(
            "Locked tray idle CPU: {:.3} ms over one second",
            ticks as f64 / 10_000.
        );
        assert!(
            ticks < 2_000_000,
            "locked tray window consumed more than 200 ms CPU in one second"
        );
    }
}
