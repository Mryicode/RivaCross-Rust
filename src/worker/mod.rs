mod state;
use crate::{model::Crosshair, rtss, storage::Store};
use state::WorkerState;
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerStatus {
    Waiting,
    Rechecking,
    Connected,
    Submitted,
    Busy,
    LockWaiting,
    Paused,
    ContentError(String),
    Error(String),
}
pub enum Command {
    Update(Crosshair, bool),
    Lock(bool),
    Finish(bool, Sender<Result<(), String>>),
    Stop,
}
pub struct Worker {
    pub tx: Sender<Command>,
    pub status: Receiver<WorkerStatus>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(root: std::path::PathBuf, ctx: eframe::egui::Context) -> Self {
        Self::start_with_open(root, ctx, rtss::Mapping::open)
    }
    fn start_with_open(
        root: std::path::PathBuf,
        ctx: eframe::egui::Context,
        open: fn() -> anyhow::Result<rtss::Mapping>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let (status_tx, status) = mpsc::channel();
        let join = thread::spawn(move || {
            let store = match Store::at(root) {
                Ok(s) => s,
                Err(e) => {
                    let _ = status_tx.send(WorkerStatus::Error(e.to_string()));
                    return;
                }
            };
            let mut mapping = None;
            let mut state = WorkerState::new(Instant::now());
            let mut last_status = None;
            let mut report = |s: WorkerStatus| {
                if Some(&s) != last_status.as_ref() {
                    let _ = status_tx.send(s.clone());
                    last_status = Some(s);
                    ctx.request_repaint();
                }
            };
            loop {
                let mut stop = false;
                if state.paused() {
                    report(state.pause_status());
                }
                let received = if state.paused() {
                    rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    rx.recv_timeout(state.wait_timeout())
                };
                if matches!(received, Err(mpsc::RecvTimeoutError::Disconnected)) {
                    stop = true;
                }
                let mut commands = received.ok().into_iter().collect::<Vec<_>>();
                commands.extend(rx.try_iter());
                for cmd in commands {
                    match cmd {
                        Command::Update(c, v) => state.update(c, v),
                        Command::Lock(value) => {
                            state.set_locked(value, Instant::now());
                            if !value {
                                mapping = None;
                            }
                        }
                        Command::Finish(keep, reply) => {
                            let result =
                                finish(keep, state.latest(), state.prepared(), &store, open)
                                    .map_err(|e| e.to_string());
                            let success = result.is_ok();
                            let _ = reply.send(result);
                            if success {
                                return;
                            }
                        }
                        Command::Stop => stop = true,
                    }
                }
                if stop {
                    if let Some(mut m) = mapping
                        && let Err(e) = rtss::Mapping::release(&mut m)
                    {
                        let _ = store.record_exit_error(&e.to_string());
                    }
                    break;
                }
                if state.paused() {
                    report(state.pause_status());
                    continue;
                }
                if !state.submit_due(Instant::now()) {
                    continue;
                }
                if let Some((c, v)) = state.take_request() {
                    let result = if v {
                        crate::output::tags(&c, &store, effective_zoom(&c))
                            .and_then(|(t, _)| rtss::encode(&t))
                    } else {
                        Ok(vec![])
                    };
                    state.preparation_result(result.map_err(|e| e.to_string()), Instant::now());
                }
                if state.health_due(Instant::now())
                    && let Some(m) = &mapping
                    && rtss::Mapping::health(m).is_err()
                {
                    mapping = None;
                    state.retry_at = Instant::now();
                }
                if mapping.is_none() && state.retry_due(Instant::now()) {
                    match open() {
                        Ok(m) => {
                            mapping = Some(m);
                            state.connected();
                            report(WorkerStatus::Connected);
                        }
                        Err(e) => {
                            report(state.connection_failed(e.to_string(), Instant::now()));
                        }
                    }
                }
                if state.write_due()
                    && let (Some(m), Some(bytes)) = (&mut mapping, &state.pending)
                {
                    let result = m.write(bytes, false).map_err(|e| e.to_string());
                    if result.is_err() {
                        mapping = None;
                    }
                    report(state.write_result(result, Instant::now()));
                }
                if state.cache_due(Instant::now()) {
                    let _ = store.prune_cache(Duration::from_secs(120));
                }
                if state.zoom_due(Instant::now(), mapping.is_some()) {
                    state.zoom_result(rtss::global_zoom());
                }
            }
        });
        Self {
            tx,
            status,
            join: Some(join),
        }
    }
    pub fn finish(&mut self, keep: bool) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Command::Finish(keep, tx))
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn start_mock(root: std::path::PathBuf, ctx: eframe::egui::Context) -> Self {
        Self::start_with_open(root, ctx, || Ok(rtss::tests::mock()))
    }
}
fn finish(
    keep: bool,
    state: Option<&(Crosshair, bool)>,
    prepared: Option<&[u8]>,
    store: &Store,
    open: fn() -> anyhow::Result<rtss::Mapping>,
) -> anyhow::Result<()> {
    let mut mapping = match open() {
        Ok(mapping) => mapping,
        Err(_) if !keep && rtss::installed_directory().is_err() => return Ok(()),
        Err(e) => return Err(e),
    };
    finish_mapping(&mut mapping, keep, state, prepared, store)
}
fn finish_mapping(
    mapping: &mut rtss::Mapping,
    keep: bool,
    state: Option<&(Crosshair, bool)>,
    prepared: Option<&[u8]>,
    store: &Store,
) -> anyhow::Result<()> {
    if !keep {
        return mapping.release();
    }
    let (crosshair, visible) = state.ok_or_else(|| anyhow::anyhow!("尚无可保留的准星"))?;
    let bytes = if let Some(bytes) = prepared {
        bytes.to_vec()
    } else if *visible {
        rtss::encode(&crate::output::tags(crosshair, store, effective_zoom(crosshair))?.0)?
    } else {
        Vec::new()
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !mapping.write(&bytes, false)? {
        anyhow::ensure!(Instant::now() < deadline, "RTSS 锁持续忙，无法保留最新准星");
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn effective_zoom(c: &crate::model::Crosshair) -> u32 {
    if c.rtss_zoom == 0 {
        crate::rtss::global_zoom()
    } else {
        c.rtss_zoom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static OPENS: AtomicUsize = AtomicUsize::new(0);
    fn open_mock() -> anyhow::Result<rtss::Mapping> {
        OPENS.fetch_add(1, Ordering::Relaxed);
        Ok(rtss::tests::mock())
    }
    fn text_state() -> (Crosshair, bool) {
        (
            Crosshair {
                kind: Kind::Text,
                text: "latest".into(),
                rtss_zoom: 1,
                ..Default::default()
            },
            true,
        )
    }
    fn wait_status(worker: &Worker, expected: WorkerStatus) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let status = worker
                .status
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if status == expected {
                return;
            }
        }
    }
    #[test]
    fn locked_worker_stops_checks_and_can_unlock_and_exit() {
        let root = tempfile::tempdir().unwrap();
        OPENS.store(0, Ordering::Relaxed);
        let mut worker = Worker::start_with_open(root.path().into(), Default::default(), open_mock);
        let (c, visible) = text_state();
        worker.tx.send(Command::Update(c, visible)).unwrap();
        worker.tx.send(Command::Lock(true)).unwrap();
        wait_status(&worker, WorkerStatus::Paused);
        let opens = OPENS.load(Ordering::Relaxed);
        // Late UI notifications must not resume submission or RTSS checks while locked.
        let (mut late, visible) = text_state();
        late.text = "unexpected late update".into();
        worker.tx.send(Command::Update(late, visible)).unwrap();
        assert!(matches!(
            worker.status.recv_timeout(Duration::from_millis(1250)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(OPENS.load(Ordering::Relaxed), opens);
        worker.tx.send(Command::Lock(false)).unwrap();
        wait_status(&worker, WorkerStatus::Submitted);
        assert!(OPENS.load(Ordering::Relaxed) > opens);
        worker.tx.send(Command::Lock(true)).unwrap();
        wait_status(&worker, WorkerStatus::Paused);
        worker.finish(true).unwrap();
        assert!(worker.join.is_none());
    }
    #[test]
    fn preserve_submits_latest_and_clear_releases_it() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::at(root.path().into()).unwrap();
        let mut mapping = rtss::tests::mock();
        mapping.write(b"old", false).unwrap();
        let state = text_state();
        finish_mapping(&mut mapping, true, Some(&state), None, &store).unwrap();
        let expected = rtss::encode(
            &crate::output::tags(&state.0, &store, effective_zoom(&state.0))
                .unwrap()
                .0,
        )
        .unwrap();
        assert_eq!(mapping.own_text().unwrap(), Some(expected));
        finish_mapping(&mut mapping, true, Some(&state), Some(b"frozen"), &store).unwrap();
        assert_eq!(mapping.own_text().unwrap(), Some(b"frozen".to_vec()));
        finish_mapping(&mut mapping, false, None, None, &store).unwrap();
        assert_eq!(mapping.own_text().unwrap(), None);
    }
    #[test]
    fn invalid_preserve_keeps_existing_content_and_hidden_stays_hidden() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::at(root.path().into()).unwrap();
        let mut mapping = rtss::tests::mock();
        mapping.write(b"old", false).unwrap();
        let mut state = text_state();
        state.0.text = "<invalid>".into();
        assert!(finish_mapping(&mut mapping, true, Some(&state), None, &store).is_err());
        assert_eq!(mapping.own_text().unwrap(), Some(b"old".to_vec()));
        state.1 = false;
        finish_mapping(&mut mapping, true, Some(&state), None, &store).unwrap();
        assert_eq!(mapping.own_text().unwrap(), Some(vec![]));
    }
}
