use crate::{model::Crosshair, render};
use global_hotkey::{GlobalHotKeyManager, hotkey::HotKey};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuItem},
};
pub(super) struct Desktop {
    pub(super) _tray: Option<TrayIcon>,
    pub(super) open_item: MenuItem,
    pub(super) config_directory_item: MenuItem,
    pub(super) toggle_item: MenuItem,
    pub(super) exit_item: MenuItem,
    pub(super) keep_exit_item: MenuItem,
    pub(super) lock_item: MenuItem,
    manager: Option<GlobalHotKeyManager>,
    pub(super) hotkey: Option<HotKey>,
}
impl Desktop {
    pub fn new() -> (Self, Option<String>) {
        let menu = Menu::new();
        let open_item = MenuItem::new("打开界面", true, None);
        let config_directory_item = MenuItem::new("打开配置文件目录", true, None);
        let toggle_item = MenuItem::new("显示／隐藏准星", true, None);
        let exit_item = MenuItem::new("清除准星并退出", true, None);
        let keep_exit_item = MenuItem::new("保留准星并退出", true, None);
        let lock_item = MenuItem::new("锁定准星", true, None);
        let tray_result = (|| -> anyhow::Result<TrayIcon> {
            menu.append_items(&[
                &open_item,
                &config_directory_item,
                &toggle_item,
                &lock_item,
                &keep_exit_item,
                &exit_item,
            ])?;
            let icon = render::preset(&Crosshair::default())?;
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_tooltip("RivaCross 准星")
                .with_icon(Icon::from_rgba(
                    icon.clone().into_raw(),
                    icon.width(),
                    icon.height(),
                )?)
                .build()?;
            Ok(tray)
        })();
        let (tray, error) = match tray_result {
            Ok(t) => (Some(t), None),
            Err(e) => (None, Some(format!("托盘不可用：{e}"))),
        };
        (
            Self {
                _tray: tray,
                open_item,
                config_directory_item,
                toggle_item,
                exit_item,
                keep_exit_item,
                lock_item,
                manager: GlobalHotKeyManager::new().ok(),
                hotkey: None,
            },
            error,
        )
    }
    pub fn register_hotkey(&mut self, input: &str) -> anyhow::Result<()> {
        let key: HotKey = input
            .parse()
            .map_err(|e| anyhow::anyhow!("热键格式无效：{e}"))?;
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("全局热键管理器不可用"))?;
        if self.hotkey == Some(key) {
            return Ok(());
        }
        manager
            .register(key)
            .map_err(|e| anyhow::anyhow!("热键冲突或无法注册，请重新设置：{e}"))?;
        if let Some(old) = self.hotkey {
            let _ = manager.unregister(old);
        }
        self.hotkey = Some(key);
        Ok(())
    }
}

pub(super) fn open_directory(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{PCWSTR, w},
    };

    let directory: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // ShellExecuteW opens the directory using Explorer and reports failures as values <= 32.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(directory.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    anyhow::ensure!(
        result.0 as isize > 32,
        "无法打开配置文件目录 {}（系统错误代码：{}）",
        path.display(),
        result.0 as isize
    );
    Ok(())
}
