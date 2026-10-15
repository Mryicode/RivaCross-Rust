//! All access to the external mapping is isolated here. Layout offsets follow
//! RTSS 7.3.7 SDK/Include/RTSSSharedMemory.h; never cast a complete SDK struct.
use anyhow::{Context, Result, bail, ensure};
use std::{
    ptr,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Globalization::{CP_ACP, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte},
        System::Memory::{
            FILE_MAP_ALL_ACCESS, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS,
            MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, VirtualQuery,
        },
    },
    core::w,
};

const OWNER: &[u8] = b"RivaCross_Rust\0";
/// Read the installed SDK profile API; never save or update RTSS profiles.
/// Each query uses a separate process (the probe CLI) to avoid changing the
/// profile cache used by an injected RTSS renderer in the UI process.
pub fn installed_directory() -> Result<std::path::PathBuf> {
    unsafe {
        use windows::Win32::System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        };
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let result = (|| -> Result<std::path::PathBuf> {
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            Process32FirstW(snapshot, &mut entry)?;
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                if name.eq_ignore_ascii_case("RTSS.exe") {
                    let process = OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION,
                        false,
                        entry.th32ProcessID,
                    )?;
                    let mut path = vec![0u16; 32768];
                    let mut size = path.len() as u32;
                    let query = QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        windows::core::PWSTR(path.as_mut_ptr()),
                        &mut size,
                    );
                    let _ = CloseHandle(process);
                    query?;
                    return std::path::PathBuf::from(String::from_utf16_lossy(
                        &path[..size as usize],
                    ))
                    .parent()
                    .map(|p| p.to_owned())
                    .context("RTSS 安装路径无效");
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    bail!("RTSS 未启动");
                }
            }
        })();
        let _ = CloseHandle(snapshot);
        result
    }
}
pub fn query_global_zoom() -> Result<u32> {
    unsafe {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::{
                Foundation::FreeLibrary,
                System::LibraryLoader::{GetProcAddress, LoadLibraryW},
            },
            core::{PCWSTR, s},
        };
        let path = installed_directory()?.join("RTSSHooks64.dll");
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let dll = LoadLibraryW(PCWSTR(wide.as_ptr()))?;
        let result = (|| -> Result<u32> {
            let load = GetProcAddress(dll, s!("LoadProfile")).context("SDK 缺少 LoadProfile")?;
            let get = GetProcAddress(dll, s!("GetProfileProperty"))
                .context("SDK 缺少 GetProfileProperty")?;
            let load: unsafe extern "C" fn(*const u8) = std::mem::transmute(load);
            let get: unsafe extern "C" fn(*const u8, *mut u8, u32) -> windows::core::BOOL =
                std::mem::transmute(get);
            load(c"".as_ptr().cast());
            let mut zoom = 0u32;
            ensure!(
                get(
                    c"ZoomRatio".as_ptr().cast(),
                    (&mut zoom as *mut u32).cast(),
                    4
                )
                .as_bool(),
                "SDK 无法读取 ZoomRatio"
            );
            ensure!((1..=16).contains(&zoom), "RTSS 缩放倍率无效");
            Ok(zoom)
        })();
        let _ = FreeLibrary(dll);
        result
    }
}
pub fn global_zoom() -> u32 {
    static CACHE: std::sync::Mutex<Option<(Instant, u32)>> = std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((time, zoom)) = *cache
        && time.elapsed() < Duration::from_secs(2)
    {
        return zoom;
    }
    let result = (|| -> Result<u32> {
        let output = std::process::Command::new(std::env::current_exe()?)
            .arg("--query-zoom")
            .creation_flags(0x08000000)
            .output()?;
        ensure!(output.status.success(), "读取倍率失败");
        let zoom: u32 = std::str::from_utf8(&output.stdout)?.trim().parse()?;
        ensure!((1..=16).contains(&zoom), "倍率无效");
        Ok(zoom)
    })();
    let zoom = result.unwrap_or(1);
    *cache = Some((Instant::now(), zoom));
    zoom
}
use std::os::windows::process::CommandExt;
#[derive(Debug, Clone, Copy)]
struct Layout {
    version: u32,
    offset: usize,
    stride: usize,
    count: usize,
    text: usize,
    capacity: usize,
}
impl Layout {
    fn parse(h: &[u32; 10], len: usize) -> Result<Self> {
        ensure!(h[0] == 0x52545353, "RTSS 未就绪或已经退出");
        ensure!(h[1] >> 16 == 2, "不支持此 RTSS 共享内存版本");
        let (text, capacity) = if h[1] >= 0x20014 && h[5] as usize >= 512 + 4096 + 262144 + 32768 {
            (512 + 4096 + 262144, 32768)
        } else if h[1] >= 0x20007 {
            (512, 4096)
        } else {
            (0, 256)
        };
        let l = Self {
            version: h[1],
            offset: h[6] as usize,
            stride: h[5] as usize,
            count: h[7] as usize,
            text,
            capacity,
        };
        ensure!(
            l.count > 1
                && l.count <= 1024
                && l.offset >= 40
                && l.stride >= 512
                && l.stride >= text + capacity,
            "RTSS 槽位布局无效"
        );
        let end = l
            .count
            .checked_mul(l.stride)
            .and_then(|n| l.offset.checked_add(n))
            .context("RTSS 数组溢出")?;
        ensure!(end <= len, "RTSS 数组超出映射边界");
        Ok(l)
    }
}
struct BusyGuard(*const AtomicU32);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: guard cannot outlive the mapping in write(), and bit 0 is ours.
            unsafe {
                (*self.0).fetch_and(!1, Ordering::Release);
            }
        }
    }
}
pub struct Mapping {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
}
impl Mapping {
    pub fn open() -> Result<Self> {
        unsafe {
            let handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, w!("RTSSSharedMemoryV2"))
                .context("等待 RTSS：请启动 RTSS 7.3.7")?;
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                bail!("RTSS 映射失败");
            }
            let mut info = MEMORY_BASIC_INFORMATION::default();
            if VirtualQuery(Some(view.Value), &mut info, std::mem::size_of_val(&info)) == 0 {
                let _ = UnmapViewOfFile(view);
                let _ = CloseHandle(handle);
                bail!("无法检查共享内存边界");
            }
            let m = Self {
                handle,
                view,
                len: info.RegionSize,
            };
            m.layout()?;
            Ok(m)
        }
    }
    fn base(&self) -> *mut u8 {
        self.view.Value.cast()
    }
    pub fn health(&self) -> Result<()> {
        self.layout()?;
        installed_directory()?;
        self.own_text()?
            .context("RTSS 自有槽位已重置，重新提交中")
            .map(|_| ())
    }
    pub fn own_text(&self) -> Result<Option<Vec<u8>>> {
        let l = self.layout()?;
        for i in 1..l.count {
            let offset = l.offset + i * l.stride;
            let matches = OWNER.iter().enumerate().all(|(j, b)| unsafe {
                ptr::read_volatile(self.base().add(offset + 256 + j)) == *b
            });
            if matches {
                let mut text = vec![];
                for j in 0..l.capacity {
                    let b = unsafe { ptr::read_volatile(self.base().add(offset + l.text + j)) };
                    if b == 0 {
                        break;
                    }
                    text.push(b);
                }
                return Ok(Some(text));
            }
        }
        Ok(None)
    }
    fn layout(&self) -> Result<Layout> {
        ensure!(self.len >= 40, "RTSS 映射过短");
        let mut h = [0; 10];
        for (i, v) in h.iter_mut().enumerate() {
            // SAFETY: mapping is live, header fits and Windows mappings are page aligned.
            *v = unsafe { ptr::read_volatile(self.base().add(i * 4).cast::<u32>()) };
        }
        Layout::parse(&h, self.len)
    }
    pub fn write(&mut self, bytes: &[u8], release: bool) -> Result<bool> {
        let l = self.layout()?;
        ensure!(
            bytes.len() < l.capacity && !bytes.contains(&0),
            "RTSS 内容超过槽位容量或含空字符"
        );
        let lock = if l.version >= 0x2000e {
            unsafe { self.base().add(36).cast::<AtomicU32>() }
        } else {
            ptr::null_mut()
        };
        if !lock.is_null() && unsafe { (*lock).fetch_or(1, Ordering::Acquire) } & 1 != 0 {
            return Ok(false);
        }
        let _guard = BusyGuard(lock);
        // Recheck signature after acquiring the RTSS protocol lock.
        self.layout()?;
        let mut free = None;
        let mut owned = None;
        for i in 1..l.count {
            let offset = l.offset + i * l.stride;
            let mut name = [0u8; 256];
            for (j, v) in name.iter_mut().enumerate() {
                *v = unsafe { ptr::read_volatile(self.base().add(offset + 256 + j)) };
            }
            if name.starts_with(OWNER) {
                owned = Some(offset);
                break;
            }
            if name[0] == 0 && free.is_none() {
                free = Some(offset);
            }
        }
        let offset = if release {
            match owned {
                Some(o) => o,
                None => return Ok(true),
            }
        } else {
            owned
                .or(free)
                .context("RTSS 槽位耗尽：请关闭其他 OSD 客户端后重试")?
        };
        unsafe {
            if release {
                for j in 0..l.stride {
                    ptr::write_volatile(self.base().add(offset + j), 0);
                }
            } else {
                if owned.is_none() {
                    for (j, b) in OWNER.iter().enumerate() {
                        ptr::write_volatile(self.base().add(offset + 256 + j), *b);
                    }
                }
                // Keep legacy text empty when the extended buffer is in use.
                if l.text != 0 {
                    ptr::write_volatile(self.base().add(offset), 0);
                }
                if l.text > 512 {
                    ptr::write_volatile(self.base().add(offset + 512), 0);
                }
                // v2.20's additional buffer must not retain stale text.
                if l.text != 512 + 4096 + 262144
                    && l.version >= 0x20014
                    && l.stride >= 512 + 4096 + 262144 + 32768
                {
                    ptr::write_volatile(self.base().add(offset + 512 + 4096 + 262144), 0);
                }
                for (j, b) in bytes.iter().chain(std::iter::once(&0)).enumerate() {
                    ptr::write_volatile(self.base().add(offset + l.text + j), *b);
                }
            }
            let frame = self.base().add(32).cast::<AtomicU32>();
            (*frame).fetch_add(1, Ordering::Release);
        }
        Ok(true)
    }
    pub fn release(&mut self) -> Result<()> {
        let start = Instant::now();
        loop {
            if self.write(&[], true)? {
                return Ok(());
            }
            if start.elapsed() > Duration::from_secs(2) {
                bail!("RTSS 锁持续忙，未能释放槽位");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}
/// RTSS takes ANSI, not UTF-8. Never silently substitute path characters.
pub fn encode(text: &str) -> Result<Vec<u8>> {
    if unsafe { windows::Win32::Globalization::GetACP() } == 65001 {
        return Ok(text.as_bytes().to_vec());
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut replaced = windows::core::BOOL(0);
    let len = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            WC_NO_BEST_FIT_CHARS,
            &wide,
            None,
            None,
            Some(&mut replaced),
        )
    };
    ensure!(len > 0, "RTSS 内容编码失败");
    let mut bytes = vec![0; len as usize];
    let count = unsafe {
        WideCharToMultiByte(
            CP_ACP,
            WC_NO_BEST_FIT_CHARS,
            &wide,
            Some(&mut bytes),
            None,
            Some(&mut replaced),
        )
    };
    ensure!(
        count == len && !replaced.as_bool(),
        "缓存路径含当前 Windows ANSI 代码页不支持的字符，请使用可编码的 LOCALAPPDATA 路径"
    );
    Ok(bytes)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn additional_text_buffer_roundtrips_large_tags_and_preserves_other_owners() {
        let stride = 512 + 4096 + 262144 + 32768;
        let mut m = mock_with_stride(stride);
        unsafe {
            ptr::write_volatile(m.base().add(40 + stride + 256), b'A');
            ptr::write_volatile(m.base().add(40 + stride), b'X');
            let ours = 40 + 2 * stride;
            ptr::write_volatile(m.base().add(ours), b'Y');
            ptr::write_volatile(m.base().add(ours + 512), b'Z');
            let data = vec![b'a'; 24000];
            assert!(m.write(&data, false).unwrap());
            assert_eq!(m.own_text().unwrap(), Some(data));
            assert_eq!(ptr::read_volatile(m.base().add(ours)), 0);
            assert_eq!(ptr::read_volatile(m.base().add(ours + 512)), 0);
            assert!(m.write(&vec![b'b'; 32768], false).is_err());
            assert_eq!(m.own_text().unwrap().unwrap().len(), 24000);
            m.release().unwrap();
            assert!(m.own_text().unwrap().is_none());
            assert_eq!(ptr::read_volatile(m.base().add(40 + stride)), b'X');
            assert_eq!(ptr::read_volatile(m.base().add(40 + stride + 256)), b'A');
        }
    }
    #[test]
    fn dropping_client_mapping_preserves_shared_content() {
        let mut client = mock();
        client.write(b"retained", false).unwrap();
        // A separate view represents RTSS retaining the shared memory object.
        let observer = Mapping {
            handle: HANDLE::default(),
            view: unsafe { MapViewOfFile(client.handle, FILE_MAP_ALL_ACCESS, 0, 0, client.len) },
            len: client.len,
        };
        assert!(!observer.view.Value.is_null());
        drop(client);
        assert_eq!(observer.own_text().unwrap(), Some(b"retained".to_vec()));
    }
    pub(crate) fn mock() -> Mapping {
        mock_with_stride(4608)
    }
    fn mock_with_stride(stride: usize) -> Mapping {
        unsafe {
            use windows::{
                Win32::{
                    Foundation::INVALID_HANDLE_VALUE,
                    System::Memory::{CreateFileMappingW, PAGE_READWRITE},
                },
                core::PCWSTR,
            };
            let len = 40 + stride * 3;
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                len as u32,
                PCWSTR::null(),
            )
            .unwrap();
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, len);
            assert!(!view.Value.is_null());
            let m = Mapping { handle, view, len };
            let mut h = header();
            h[5] = stride as u32;
            for (i, v) in h.iter().enumerate() {
                ptr::write_volatile(m.base().add(i * 4).cast::<u32>(), *v);
            }
            m
        }
    }
    #[test]
    fn busy_preserves_frame_and_retries_without_touching_other_slots() {
        unsafe {
            let mut m = mock();
            let first = 40;
            let other = 40 + 4608;
            ptr::write_volatile(m.base().add(first), b'X');
            ptr::write_volatile(m.base().add(first + 256), b'A');
            ptr::write_volatile(m.base().add(other + 256), b'B');
            ptr::write_volatile(m.base().add(36).cast::<u32>(), 1);
            assert!(!m.write(b"crosshair", false).unwrap());
            assert_eq!(ptr::read_volatile(m.base().add(32).cast::<u32>()), 0);
            ptr::write_volatile(m.base().add(36).cast::<u32>(), 0);
            assert!(m.write(b"crosshair", false).unwrap());
            assert_eq!(ptr::read_volatile(m.base().add(32).cast::<u32>()), 1);
            assert_eq!(ptr::read_volatile(m.base().add(first)), b'X');
            assert_eq!(ptr::read_volatile(m.base().add(other + 256)), b'B');
            assert_eq!(ptr::read_volatile(m.base().add(40 + 4608 * 2 + 512)), b'c');
            m.release().unwrap();
            assert_eq!(ptr::read_volatile(m.base().add(40 + 4608 * 2 + 256)), 0);
            assert_eq!(ptr::read_volatile(m.base().add(other + 256)), b'B');
            assert_eq!(ptr::read_volatile(m.base().add(36).cast::<u32>()), 0);
        }
    }
    #[test]
    fn exhaustion_and_oversize_do_not_write() {
        unsafe {
            let mut m = mock();
            for i in 1..3 {
                ptr::write_volatile(m.base().add(40 + 4608 * i + 256), b'A');
            }
            assert!(m.write(b"hi", false).is_err());
            assert!(m.write(&vec![b'a'; 4096], false).is_err());
            assert_eq!(ptr::read_volatile(m.base().add(32).cast::<u32>()), 0);
            assert_eq!(ptr::read_volatile(m.base().add(36).cast::<u32>()), 0);
        }
    }
    fn header() -> [u32; 10] {
        [0x52545353, 0x20014, 0, 0, 0, 4608, 40, 3, 0, 0]
    }
    #[test]
    fn validates_version_stride_and_bounds() {
        let h = header();
        assert!(Layout::parse(&h, 40 + 4608 * 3).is_ok());
        assert!(Layout::parse(&h, 100).is_err());
        for i in [0, 1, 5, 6, 7] {
            let mut broken = h;
            broken[i] = 0;
            assert!(Layout::parse(&broken, 20000).is_err());
        }
    }
    #[test]
    fn legacy_capacity() {
        let mut h = header();
        h[1] = 0x20006;
        h[5] = 512;
        let l = Layout::parse(&h, 20000).unwrap();
        assert_eq!((l.text, l.capacity), (0, 256));
    }
    #[test]
    fn detects_slot_reset_after_server_reinitialization() {
        let mut m = mock();
        assert!(m.own_text().unwrap().is_none());
        m.write(b"hello", false).unwrap();
        assert_eq!(m.own_text().unwrap(), Some(b"hello".to_vec()));
        unsafe {
            ptr::write_volatile(m.base().add(40 + 4608 + 256), 0);
        }
        assert!(m.own_text().unwrap().is_none());
    }
}
