use super::WorkerStatus;
use crate::model::Crosshair;
use std::time::{Duration, Instant};

pub(super) struct WorkerState {
    pub pending: Option<Vec<u8>>,
    needs_write: bool,
    content_error: Option<String>,
    request: Option<(Crosshair, bool)>,
    current: Option<(Crosshair, bool)>,
    submitted: Option<Vec<u8>>,
    locked: bool,
    health_check: Instant,
    cache_check: Instant,
    zoom_check: Instant,
    previous_zoom: u32,
    pub retry_at: Instant,
    last_submit: Instant,
}
impl WorkerState {
    pub fn new(now: Instant) -> Self {
        Self {
            pending: None,
            needs_write: false,
            content_error: None,
            request: None,
            current: None,
            submitted: None,
            locked: false,
            health_check: now,
            cache_check: now,
            zoom_check: now,
            previous_zoom: 0,
            retry_at: now,
            last_submit: now - Duration::from_secs(1),
        }
    }
    pub fn update(&mut self, c: Crosshair, visible: bool) {
        if !self.locked {
            self.request = Some((c, visible));
        }
    }
    pub fn set_locked(&mut self, locked: bool, now: Instant) {
        self.locked = locked;
        if !locked {
            self.retry_at = now;
            if self.request.is_none() {
                self.request = self.current.clone();
            }
        }
    }
    pub fn paused(&self) -> bool {
        self.locked && !self.needs_write && self.request.is_none()
    }
    pub fn pause_status(&self) -> WorkerStatus {
        self.content_error
            .clone()
            .map(WorkerStatus::ContentError)
            .unwrap_or(WorkerStatus::Paused)
    }
    pub fn wait_timeout(&self) -> Duration {
        Duration::from_millis(if self.needs_write || self.request.is_some() {
            10
        } else {
            250
        })
    }
    pub fn submit_due(&self, now: Instant) -> bool {
        now.duration_since(self.last_submit) >= Duration::from_millis(34)
    }
    pub fn take_request(&mut self) -> Option<(Crosshair, bool)> {
        let request = self.request.take()?;
        self.current = Some(request.clone());
        Some(request)
    }
    pub fn latest(&self) -> Option<&(Crosshair, bool)> {
        self.request.as_ref().or(self.current.as_ref())
    }
    pub fn prepared(&self) -> Option<&[u8]> {
        if self.request.is_none() && self.content_error.is_none() {
            self.pending.as_deref()
        } else {
            None
        }
    }
    pub fn preparation_result(&mut self, result: Result<Vec<u8>, String>, now: Instant) {
        match result {
            Ok(bytes) => {
                self.pending = Some(bytes);
                self.content_error = None;
            }
            Err(error) => {
                self.pending = Some(vec![]);
                self.content_error = Some(error);
            }
        }
        self.needs_write = true;
        self.last_submit = now;
    }
    pub fn connected(&mut self) {
        self.needs_write = true;
        self.request = self.current.clone();
    }
    pub fn retry_due(&self, now: Instant) -> bool {
        self.retry_at <= now
    }
    pub fn connection_failed(&mut self, error: String, now: Instant) -> WorkerStatus {
        self.retry_at = now + Duration::from_secs(1);
        WorkerStatus::Error(error)
    }
    pub fn write_due(&self) -> bool {
        self.needs_write && self.request.is_none()
    }
    pub fn write_result(&mut self, result: Result<bool, String>, now: Instant) -> WorkerStatus {
        self.last_submit = now;
        match result {
            Ok(true) => {
                self.needs_write = false;
                self.submitted = self.pending.clone();
                self.content_error
                    .clone()
                    .map(WorkerStatus::ContentError)
                    .unwrap_or(WorkerStatus::Submitted)
            }
            Ok(false) => WorkerStatus::Busy,
            Err(error) => self.connection_failed(error, now),
        }
    }
    pub fn health_due(&mut self, now: Instant) -> bool {
        if !self.paused() && now.duration_since(self.health_check) >= Duration::from_secs(1) {
            self.health_check = now;
            true
        } else {
            false
        }
    }
    pub fn cache_due(&mut self, now: Instant) -> bool {
        if !self.locked && now.duration_since(self.cache_check) >= Duration::from_secs(10) {
            self.cache_check = now;
            true
        } else {
            false
        }
    }
    pub fn zoom_due(&mut self, now: Instant, connected: bool) -> bool {
        if !self.locked
            && connected
            && now.duration_since(self.zoom_check) >= Duration::from_secs(2)
            && self.current.as_ref().is_some_and(|(c, _)| c.rtss_zoom == 0)
        {
            self.zoom_check = now;
            true
        } else {
            false
        }
    }
    pub fn zoom_result(&mut self, zoom: u32) {
        if self.previous_zoom != zoom {
            self.previous_zoom = zoom;
            self.request = self.current.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(text: &str) -> Crosshair {
        Crosshair {
            kind: crate::model::Kind::Text,
            text: text.into(),
            ..Default::default()
        }
    }
    #[test]
    fn burst_update_lock_finish_and_unlock_preserve_latest() {
        let now = Instant::now();
        let mut s = WorkerState::new(now);
        s.update(c("first"), true);
        s.update(c("latest"), true);
        s.set_locked(true, now);
        s.update(c("late"), false);
        assert_eq!(s.latest().unwrap().0.text, "latest");
        assert!(s.prepared().is_none());
        assert!(!s.paused());
        assert_eq!(s.take_request().unwrap().0.text, "latest");
        s.preparation_result(Ok(b"latest".to_vec()), now);
        assert_eq!(s.prepared(), Some(b"latest".as_slice()));
        assert!(!s.paused());
        assert_eq!(s.write_result(Ok(true), now), WorkerStatus::Submitted);
        assert!(s.paused());
        let later = now + Duration::from_secs(60);
        assert!(!s.health_due(later));
        assert!(!s.cache_due(later));
        assert!(!s.zoom_due(later, true));
        s.set_locked(false, later);
        assert!(!s.paused());
        assert!(s.retry_due(later));
        assert_eq!(s.latest().unwrap().0.text, "latest");
    }
    #[test]
    fn busy_and_connection_failure_preserve_pending_until_success() {
        let now = Instant::now();
        let mut s = WorkerState::new(now);
        s.update(c("new"), true);
        s.take_request();
        s.preparation_result(Ok(b"new".to_vec()), now);
        assert_eq!(s.write_result(Ok(false), now), WorkerStatus::Busy);
        assert!(s.write_due());
        assert!(s.submitted.is_none());
        assert!(!s.submit_due(now + Duration::from_millis(33)));
        assert!(s.submit_due(now + Duration::from_millis(34)));
        assert_eq!(
            s.write_result(Err("disconnected".into()), now),
            WorkerStatus::Error("disconnected".into())
        );
        assert!(!s.retry_due(now + Duration::from_millis(999)));
        assert!(s.retry_due(now + Duration::from_secs(1)));
        s.connected();
        assert!(!s.write_due());
        assert_eq!(s.take_request().unwrap().0.text, "new");
        s.preparation_result(Ok(b"new".to_vec()), now);
        s.write_result(Ok(true), now);
        assert_eq!(s.submitted.as_deref(), Some(b"new".as_slice()));
        assert!(!s.write_due());
    }
    #[test]
    fn invalid_content_cannot_reuse_prepared_output_for_exit() {
        let now = Instant::now();
        let mut s = WorkerState::new(now);
        s.update(c("valid"), true);
        s.take_request();
        s.preparation_result(Ok(b"valid".to_vec()), now);
        s.write_result(Ok(true), now);
        s.update(c("<invalid>"), true);
        assert!(s.prepared().is_none());
        s.take_request();
        s.preparation_result(Err("invalid".into()), now);
        assert!(s.prepared().is_none());
        assert_eq!(
            s.write_result(Ok(true), now),
            WorkerStatus::ContentError("invalid".into())
        );
        s.set_locked(true, now);
        assert_eq!(
            s.pause_status(),
            WorkerStatus::ContentError("invalid".into())
        );
        s.set_locked(false, now);
        s.update(c("valid"), false);
        assert!(!s.take_request().unwrap().1);
        s.preparation_result(Ok(vec![]), now);
        assert_eq!(s.prepared(), Some([].as_slice()));
    }
    #[test]
    fn health_cache_and_auto_zoom_schedule_only_when_due() {
        let now = Instant::now();
        let mut s = WorkerState::new(now);
        s.update(c("auto"), true);
        s.take_request();
        assert!(!s.health_due(now));
        assert!(s.health_due(now + Duration::from_secs(1)));
        assert!(!s.health_due(now + Duration::from_secs(1)));
        assert!(!s.cache_due(now + Duration::from_secs(9)));
        assert!(s.cache_due(now + Duration::from_secs(10)));
        assert!(!s.zoom_due(now + Duration::from_secs(2), false));
        assert!(s.zoom_due(now + Duration::from_secs(2), true));
        s.zoom_result(2);
        assert!(s.take_request().is_some());
        s.zoom_result(2);
        assert!(s.take_request().is_none());
        s.zoom_result(3);
        assert!(s.take_request().is_some());
        s.current.as_mut().unwrap().0.rtss_zoom = 4;
        assert!(!s.zoom_due(now + Duration::from_secs(20), true));
    }
}
