//! Opt-in per-presentation stage evidence, not a GPU/display fence.
//!
//! `NEOMACS_PRESENT_TRACE=/absolute/path.jsonl` enables an append-only Linux
//! trace. Local stages use CLOCK_MONOTONIC; publish also samples CLOCK_REALTIME
//! immediately afterwards for mapping Lisp float-time offline. Wayland native
//! feedback retains its advertised clock ID; consumers must reject mismatched
//! domains. X11/non-Wayland captures contain local stages only.
//! Repeated draws of one sealed revision intentionally repeat its identity.
//! `submit` marks the root glyph pass's queue submission, or the retained
//! static composition's submission, not the frame's final GPU submission; a
//! rebuilt retained static scene records it twice.
//!
//! Enabling the trace also requests Wayland presentation feedback for every
//! frame, which keeps the render thread waking while feedback is pending. A
//! traced run therefore measures a slightly busier renderer than an untraced one.
//!
//! Hot threads only sample clocks and try_send fixed-size records; nobody
//! waits on the queue, so a send never wakes another thread. One writer owns
//! the file and drains the queue every 10 ms into buffered JSONL (no fsync).
//! The queue is bounded; dropped records are reported separately. Abrupt
//! process exit can lose a tail, and so can a normal exit in two cases:
//! a daemon is not drained at teardown, and the detached preparation worker
//! may still record Prepared/Discarded after the final drain. Call [`init`]
//! early so the writer thread is not started under a hot-path lock, and
//! [`shutdown`] after GUI/evaluator teardown for a normal drain.

use crate::PresentationId;
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

const CAPACITY: usize = 4096;
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Publish,
    Prepared,
    RenderStart,
    Submit,
    Present,
    Superseded,
    Discarded,
    Presented,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct Record {
    stage: Stage,
    frame: u64,
    presentation: u64,
    clock_id: u32,
    ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    realtime_ns: Option<u64>,
}

struct Recorder {
    events: Sender<Record>,
    dropped: Arc<AtomicU64>,
    stop: Sender<()>,
    writer: Mutex<Option<JoinHandle<()>>>,
}

static RECORDER: OnceLock<Option<Recorder>> = OnceLock::new();

fn parse_path(value: Option<OsString>) -> Result<Option<PathBuf>, &'static str> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("NEOMACS_PRESENT_TRACE requires an absolute path");
    }
    Ok(Some(path))
}

fn recorder() -> Option<&'static Recorder> {
    RECORDER
        .get_or_init(|| {
            let path = match parse_path(std::env::var_os("NEOMACS_PRESENT_TRACE")) {
                Ok(path) => path?,
                Err(error) => {
                    tracing::warn!(error, "presentation trace disabled");
                    return None;
                }
            };
            if !cfg!(target_os = "linux") {
                tracing::warn!("presentation trace requires Linux clocks");
                return None;
            }
            let (events, incoming) = bounded(CAPACITY);
            let (stop, stopped) = bounded(1);
            let dropped = Arc::new(AtomicU64::new(0));
            let writer_dropped = dropped.clone();
            let writer = match std::thread::Builder::new()
                .name("neomacs-present-trace".into())
                .spawn(move || {
                    let result = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                        .and_then(|file| {
                            write_records(BufWriter::new(file), incoming, stopped, &writer_dropped)
                        });
                    if let Err(error) = result {
                        tracing::warn!(%error, "cannot write presentation trace");
                    }
                }) {
                Ok(writer) => writer,
                Err(error) => {
                    tracing::warn!(%error, "cannot start presentation trace writer");
                    return None;
                }
            };
            Some(Recorder {
                events,
                dropped,
                stop,
                writer: Mutex::new(Some(writer)),
            })
        })
        .as_ref()
}

/// Environment configuration is cached once. Unset: no clocks or allocation.
#[inline]
pub fn enabled() -> bool {
    recorder().is_some()
}

/// Read the configuration and start the writer, if enabled, before any hot
/// path records a stage.
pub fn init() {
    let _ = recorder();
}

#[cfg(target_os = "linux")]
fn clock_ns(clock: libc::clockid_t) -> Option<u64> {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes to this valid, initialized timespec.
    if unsafe { libc::clock_gettime(clock, &mut time) } != 0 {
        return None;
    }
    u64::try_from(time.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u64::try_from(time.tv_nsec).ok()?)
}

fn sample(stage: Stage) -> Option<(u32, u64, Option<u64>)> {
    #[cfg(target_os = "linux")]
    {
        let ns = clock_ns(libc::CLOCK_MONOTONIC)?;
        let realtime = if stage == Stage::Publish {
            Some(clock_ns(libc::CLOCK_REALTIME)?)
        } else {
            None
        };
        Some((libc::CLOCK_MONOTONIC as u32, ns, realtime))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = stage;
        None
    }
}

impl Recorder {
    fn send(&self, record: Record) {
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
            self.events.try_send(record)
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn stamp_with(
    recorder: &Recorder,
    stage: Stage,
    frame: u64,
    presentation: PresentationId,
    clock: impl FnOnce(Stage) -> Option<(u32, u64, Option<u64>)>,
) -> Option<Record> {
    let Some((clock_id, ns, realtime_ns)) = clock(stage) else {
        recorder.dropped.fetch_add(1, Ordering::Relaxed);
        return None;
    };
    Some(Record {
        stage,
        frame,
        presentation: presentation.get(),
        clock_id,
        ns,
        realtime_ns,
    })
}

fn record_to(
    recorder: Option<&Recorder>,
    stage: Stage,
    frame: u64,
    presentation: PresentationId,
    clock: impl FnOnce(Stage) -> Option<(u32, u64, Option<u64>)>,
) {
    let Some(recorder) = recorder else { return };
    if let Some(record) = stamp_with(recorder, stage, frame, presentation, clock) {
        recorder.send(record);
    }
}

#[inline]
pub fn record(stage: Stage, frame: u64, presentation: PresentationId) {
    record_to(recorder(), stage, frame, presentation, sample);
}

/// A stage timestamp taken now and queued later by [`Stamp::emit`], so a
/// caller can sample inside a critical section and queue after leaving it.
#[must_use]
pub struct Stamp(Record);

/// Sample a stage without queueing it. `None` when the trace is disabled.
#[inline]
pub fn stamp(stage: Stage, frame: u64, presentation: PresentationId) -> Option<Stamp> {
    stamp_with(recorder()?, stage, frame, presentation, sample).map(Stamp)
}

impl Stamp {
    pub fn emit(self) {
        if let Some(recorder) = recorder() {
            recorder.send(self.0);
        }
    }
}

/// Preserve the compositor's native timestamp/domain, not callback receive time.
#[inline]
pub fn presented(frame: u64, presentation: PresentationId, clock_id: u32, ns: u64) {
    let Some(recorder) = recorder() else { return };
    recorder.send(Record {
        stage: Stage::Presented,
        frame,
        presentation: presentation.get(),
        clock_id,
        ns,
        realtime_ns: None,
    });
}

fn write_record(writer: &mut impl Write, record: &Record) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, record)?;
    writer.write_all(b"\n")
}

fn flush_records(writer: &mut impl Write, dropped: &AtomicU64) -> io::Result<()> {
    let count = dropped.swap(0, Ordering::Relaxed);
    if count != 0 {
        writeln!(writer, "{{\"stage\":\"dropped\",\"count\":{count}}}")?;
    }
    writer.flush()
}

fn write_records(
    mut writer: impl Write,
    incoming: Receiver<Record>,
    stopped: Receiver<()>,
    dropped: &AtomicU64,
) -> io::Result<()> {
    loop {
        // Sleep on the stop channel only: producers never have a waiting
        // receiver to wake, so their sends stay free of syscalls.
        let stop = !matches!(
            stopped.recv_timeout(DRAIN_INTERVAL),
            Err(crossbeam_channel::RecvTimeoutError::Timeout)
        );
        // A bounded drain per pass, even if a producer is still exiting.
        for _ in 0..CAPACITY {
            match incoming.try_recv() {
                Ok(record) => write_record(&mut writer, &record)?,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return flush_records(&mut writer, dropped);
                }
            }
        }
        flush_records(&mut writer, dropped)?;
        if stop {
            return Ok(());
        }
    }
}

/// Only after GUI/evaluator teardown; never wait for file I/O on a hot thread.
/// Does not initialize an otherwise unused recorder.
pub fn shutdown() {
    if let Some(Some(recorder)) = RECORDER.get() {
        let _ = recorder.stop.try_send(());
        if let Some(writer) = recorder.writer.lock().unwrap().take() {
            let _ = writer.join();
        }
    }
}

#[cfg(test)]
mod tests;
