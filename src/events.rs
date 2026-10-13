//! Hidden windows do not receive ordinary WM_PAINT requests on Windows. Forward
//! tray/hotkey/instance events explicitly, then post WM_PAINT to run egui logic
//! once without showing the window. No rendering timer is needed while hidden.
use eframe::egui;
use global_hotkey::GlobalHotKeyEvent;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, TrayIconEvent};
use windows::Win32::{
    Foundation::{HANDLE, HWND, WAIT_OBJECT_0},
    System::Threading::{INFINITE, SetEvent, WaitForSingleObject},
    UI::WindowsAndMessaging::{PostMessageW, WM_PAINT},
};
pub enum Event {
    Open,
    Menu(MenuEvent),
    Hotkey(GlobalHotKeyEvent),
}
pub struct Bridge {
    pub rx: Receiver<Event>,
    window: usize,
    instance_event: usize,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
fn send(tx: &Sender<Event>, event: Event, ctx: &egui::Context, window: usize, alive: &AtomicBool) {
    if !alive.load(Ordering::Acquire) {
        return;
    }
    if tx.send(event).is_ok() {
        ctx.request_repaint();
        wake(window);
    }
}
fn wake(window: usize) {
    // SAFETY: App stops and joins Bridge before dropping its native window/instance.
    // PostMessage is asynchronous, and HWND is an opaque handle, not dereferenced.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(window as *mut _)),
            WM_PAINT,
            Default::default(),
            Default::default(),
        );
    }
}
impl Bridge {
    pub fn new(ctx: &egui::Context, window: usize, instance_event: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let tray_tx = tx.clone();
        let tray_ctx = ctx.clone();
        let tray_alive = alive.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                send(&tray_tx, Event::Open, &tray_ctx, window, &tray_alive);
            }
        }));
        let menu_tx = tx.clone();
        let menu_ctx = ctx.clone();
        let menu_alive = alive.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            send(&menu_tx, Event::Menu(event), &menu_ctx, window, &menu_alive)
        }));
        let key_tx = tx.clone();
        let key_ctx = ctx.clone();
        let key_alive = alive.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event| {
            send(&key_tx, Event::Hotkey(event), &key_ctx, window, &key_alive)
        }));
        let thread_alive = alive.clone();
        let thread_ctx = ctx.clone();
        let thread = thread::spawn(move || {
            while thread_alive.load(Ordering::Acquire)
                && unsafe { WaitForSingleObject(HANDLE(instance_event as *mut _), INFINITE) }
                    == WAIT_OBJECT_0
            {
                send(&tx, Event::Open, &thread_ctx, window, &thread_alive);
            }
        });
        Self {
            rx,
            window,
            instance_event,
            alive,
            thread: Some(thread),
        }
    }
    pub fn close(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        // eframe consumes Close on the following frame. A hidden window cannot
        // rely on request_repaint alone to produce that frame on Windows.
        wake(self.window);
    }
    pub fn stop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            unsafe {
                let _ = SetEvent(HANDLE(self.instance_event as *mut _));
            }
            let _ = thread.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU32, AtomicU64},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Foundation::{CloseHandle, FILETIME, HINSTANCE, LPARAM, LRESULT, WPARAM},
            System::{
                LibraryLoader::GetModuleHandleW,
                Threading::{CreateEventW, GetCurrentProcess, GetProcessTimes, SetEvent},
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG, PM_REMOVE,
                PeekMessageW, RegisterClassW, TranslateMessage, UnregisterClassW, WINDOW_EX_STYLE,
                WNDCLASSW, WS_OVERLAPPED,
            },
        },
        core::w,
    };
    static PAINTS: AtomicU32 = AtomicU32::new(0);
    pub(crate) fn process_cpu_ticks() -> u64 {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
            .unwrap();
        }
        let ticks =
            |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
        ticks(kernel) + ticks(user)
    }
    #[test]
    fn hidden_window_exits_after_one_request() {
        const CHILD: &str = "RIVACROSS_HIDDEN_EXIT_TEST";
        if std::env::var_os(CHILD).is_none() {
            // Isolate the native event loop and bound hangs caused by this regression.
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "events::tests::hidden_window_exits_after_one_request",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "hidden-window exit test failed");
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("one exit request did not terminate the hidden window");
                }
                thread::sleep(Duration::from_millis(20));
            }
        }

        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        use winit::platform::windows::EventLoopBuilderExtWindows;

        struct ExitApp {
            bridge: Bridge,
            window: usize,
            event: usize,
            phase: u8,
            exited: Arc<AtomicBool>,
            idle_cpu: Arc<AtomicU64>,
        }
        impl eframe::App for ExitApp {
            fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
                match self.phase {
                    0 => {
                        self.phase = 1;
                        ctx.request_repaint();
                    }
                    1 => {
                        self.phase = 2;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        let event = self.event;
                        let idle_cpu = self.idle_cpu.clone();
                        let repaint_ctx = ctx.clone();
                        thread::spawn(move || {
                            // Let startup repaints settle before the single external request.
                            thread::sleep(Duration::from_millis(300));
                            // A worker can finish submitting after the window was hidden.
                            repaint_ctx.request_repaint();
                            thread::sleep(Duration::from_millis(200));
                            let cpu = process_cpu_ticks();
                            thread::sleep(Duration::from_secs(1));
                            idle_cpu.store(process_cpu_ticks() - cpu, Ordering::Release);
                            unsafe {
                                SetEvent(HANDLE(event as *mut _)).unwrap();
                            }
                        });
                    }
                    _ => {}
                }
                if matches!(self.bridge.rx.try_recv(), Ok(Event::Open)) {
                    assert!(!unsafe { IsWindowVisible(HWND(self.window as *mut _)) }.as_bool());
                    self.bridge.close(ctx);
                }
            }
            fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
                self.exited.store(true, Ordering::Release);
            }
        }

        let event = unsafe { CreateEventW(None, false, false, None).unwrap() };
        let exited = Arc::new(AtomicBool::new(false));
        let idle_cpu = Arc::new(AtomicU64::new(u64::MAX));
        let result = eframe::run_native(
            "RivaCross hidden exit regression",
            eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([100., 100.]),
                event_loop_builder: Some(Box::new(|builder| {
                    builder.with_any_thread(true);
                })),
                ..Default::default()
            },
            Box::new({
                let exited = exited.clone();
                let idle_cpu = idle_cpu.clone();
                let event = event.0 as usize;
                move |cc| {
                    let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw()
                    else {
                        unreachable!();
                    };
                    let window = handle.hwnd.get() as usize;
                    Ok(Box::new(ExitApp {
                        bridge: Bridge::new(&cc.egui_ctx, window, event),
                        window,
                        event,
                        phase: 0,
                        exited,
                        idle_cpu: idle_cpu.clone(),
                    }))
                }
            }),
        );
        unsafe {
            CloseHandle(event).unwrap();
        }
        result.unwrap();
        assert!(
            exited.load(Ordering::Acquire),
            "normal shutdown hook was skipped"
        );
        assert!(
            idle_cpu.load(Ordering::Acquire) < 2_000_000,
            "hidden idle window consumed more than 200 ms CPU in one second: {} ticks",
            idle_cpu.load(Ordering::Acquire)
        );
    }
    unsafe extern "system" fn procedure(window: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_PAINT {
            PAINTS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { DefWindowProcW(window, msg, wp, lp) }
    }
    #[test]
    fn instance_event_wakes_a_hidden_native_window_without_showing_it() {
        unsafe {
            let module = HINSTANCE(GetModuleHandleW(None).unwrap().0);
            let name = w!("RivaCrossWakeRegressionTest");
            let class = WNDCLASSW {
                hInstance: module,
                lpszClassName: name,
                lpfnWndProc: Some(procedure),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                name,
                name,
                WS_OVERLAPPED,
                0,
                0,
                100,
                100,
                None,
                None,
                Some(module),
                None,
            )
            .unwrap();
            let event = CreateEventW(None, false, false, None).unwrap();
            let mut bridge = Bridge::new(
                &egui::Context::default(),
                window.0 as usize,
                event.0 as usize,
            );
            SetEvent(event).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut received = false;
            while Instant::now() < deadline {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, Some(window), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                received |= matches!(bridge.rx.try_recv(), Ok(Event::Open));
                if received && PAINTS.load(Ordering::Relaxed) > 0 {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            bridge.stop();
            let paints = PAINTS.load(Ordering::Relaxed);
            let visible =
                windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(window).as_bool();
            DestroyWindow(window).unwrap();
            CloseHandle(event).unwrap();
            UnregisterClassW(name, Some(module)).unwrap();
            assert!(received && paints > 0);
            assert!(!visible);
        }
    }
}
