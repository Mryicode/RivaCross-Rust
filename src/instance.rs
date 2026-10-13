use anyhow::Result;
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::{CreateEventW, CreateMutexW, SetEvent},
    },
    core::w,
};
pub struct Instance {
    mutex: HANDLE,
    event: HANDLE,
}
impl Instance {
    pub fn acquire() -> Result<Option<Self>> {
        unsafe {
            let mutex = CreateMutexW(None, false, w!("Local\\RivaCrossRust.Instance"))?;
            let exists = GetLastError() == ERROR_ALREADY_EXISTS;
            let event = match CreateEventW(None, false, false, w!("Local\\RivaCrossRust.Open")) {
                Ok(h) => h,
                Err(e) => {
                    let _ = CloseHandle(mutex);
                    return Err(e.into());
                }
            };
            if exists {
                SetEvent(event)?;
                let _ = CloseHandle(event);
                let _ = CloseHandle(mutex);
                return Ok(None);
            }
            Ok(Some(Self { mutex, event }))
        }
    }
    pub fn event_handle(&self) -> usize {
        self.event.0 as usize
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.event);
            let _ = CloseHandle(self.mutex);
        }
    }
}
