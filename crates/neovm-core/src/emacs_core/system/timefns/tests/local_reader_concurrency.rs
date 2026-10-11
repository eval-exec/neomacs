use super::*;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug)]
enum WriterExit {
    Released,
    Watchdog,
    ControllerGone,
}

/// Release the writer, prove worker completion under a deadline, then join.
/// The writer also has a finite channel watchdog. All assertions follow explicit
/// finish(); Drop only releases/detaches and never waits.
#[derive(Debug)]
struct Workers {
    release: Option<Sender<()>>,
    writer: Option<JoinHandle<WriterExit>>,
    reader: Option<JoinHandle<()>>,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(Workers: Send, Sync);

impl Workers {
    fn release_writer(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        self.release_writer();
        let deadline = Instant::now() + Duration::from_secs(10);
        // Never join a live worker. A recursive-lock regression becomes a
        // bounded failure; the isolated nextest process contains detached
        // leftovers when it exits after reporting that failure.
        while self
            .writer
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
            || self
                .reader
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
        {
            if Instant::now() >= deadline {
                self.writer.take();
                self.reader.take();
                return Err("worker cleanup deadline expired; detached, never passed".to_owned());
            }
            thread::sleep(Duration::from_millis(1));
        }
        // Both handles proved completion under the finite deadline above.
        let writer = self.writer.take().map(|handle| handle.join());
        let reader = self.reader.take().map(|handle| handle.join());
        if matches!(reader, Some(Err(_))) {
            return Err("Local reader panicked".to_owned());
        }
        match writer {
            Some(Ok(WriterExit::Released)) | None => Ok(()),
            Some(Ok(WriterExit::Watchdog)) => Err("writer watchdog expired; no pass".to_owned()),
            Some(Ok(WriterExit::ControllerGone)) => {
                Err("writer lost controller; no pass".to_owned())
            }
            Some(Err(_)) => Err("temporary timezone writer panicked".to_owned()),
        }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        // No waits or joins in Drop. Unwinding releases the writer and detaches
        // handles; successful paths use explicit bounded finish() first.
        self.release_writer();
        self.writer.take();
        self.reader.take();
    }
}

#[derive(Debug)]
enum ReaderWhileWriterHeld {
    ReturnedEarly(i64),
    ParkedOnFutex { tid: u32, wchan: String },
}

fn wait_for_reader(
    tid: &AtomicU32,
    completed: &Receiver<i64>,
) -> Result<ReaderWhileWriterHeld, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_wchan = String::new();
    loop {
        // A timeout is never accepted as proof of serialization. Green needs
        // positive kernel wait evidence AND a correct result after restoration.
        match completed.recv_timeout(Duration::from_millis(1)) {
            Ok(offset) => return Ok(ReaderWhileWriterHeld::ReturnedEarly(offset)),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("reader disconnected without a result".to_owned());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let tid = tid.load(Ordering::Acquire);
        if tid != 0 {
            match std::fs::read_to_string(format!("/proc/self/task/{tid}/wchan")) {
                Ok(wchan) => {
                    last_wchan = wchan.trim().to_owned();
                    if last_wchan.contains("futex") {
                        return Ok(ReaderWhileWriterHeld::ParkedOnFutex {
                            tid,
                            wchan: last_wchan,
                        });
                    }
                }
                Err(error) => {
                    // A finished reader may have exited between the channel
                    // poll and procfs read. Consume its already published result.
                    return completed
                        .try_recv()
                        .map(ReaderWhileWriterHeld::ReturnedEarly)
                        .map_err(|_| format!("reader wchan unavailable: {error}"));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "no positive blocked-reader evidence before deadline; last wchan={last_wchan:?}"
            ));
        }
    }
}

#[test]
fn local_zone_reader_waits_for_temporary_zone_restoration() {
    let _test_lock = tz_test_lock();
    // Read the persistent host zone using the raw helper while owning the
    // mutex. Calling a Local dispatch here would recursively lock after the fix.
    let expected = {
        let _lock = tz_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        local_offset_name_at_epoch(0).0
    };
    let (temporary_spec, temporary_offset) = if expected == 0 {
        ("TSB-9", 32_400)
    } else {
        ("TSB0", 0)
    };
    let (release_tx, release_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (completed_tx, completed_rx) = mpsc::channel();
    let tid = Arc::new(AtomicU32::new(0));
    let mut workers = Workers {
        release: Some(release_tx),
        writer: None,
        reader: None,
        _thread: PhantomData,
    };
    let protocol = (|| -> Result<ReaderWhileWriterHeld, String> {
        workers.writer = Some(
            thread::Builder::new()
                .name("temporary-tz-writer".to_owned())
                .spawn(move || {
                    let _guard = ScopedTzEnv::new(Some(TimeZoneSpec::from(temporary_spec)));
                    // Positive readiness: ScopedTzEnv already owns the mutex, has
                    // installed TZ, and completed tzset before the reader is spawned.
                    if ready_tx.send(local_offset_name_at_epoch(0).0).is_err() {
                        return WriterExit::ControllerGone;
                    }
                    match release_rx.recv_timeout(Duration::from_secs(30)) {
                        Ok(()) => WriterExit::Released,
                        Err(mpsc::RecvTimeoutError::Timeout) => WriterExit::Watchdog,
                        Err(mpsc::RecvTimeoutError::Disconnected) => WriterExit::ControllerGone,
                    }
                    // The guard restores TZ and unlocks on every return or unwind.
                })
                .map_err(|error| format!("writer spawn failed: {error}"))?,
        );
        let installed = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| format!("writer not ready: {error}"))?;
        if installed != temporary_offset || installed == expected {
            return Err(format!(
                "temporary zone did not differ: host={expected}, installed={installed}"
            ));
        }
        let reader_tid = Arc::clone(&tid);
        workers.reader = Some(
            thread::Builder::new()
                .name("local-tz-reader".to_owned())
                .spawn(move || {
                    // SAFETY: gettid takes no pointer arguments and only reads this
                    // Linux worker's kernel thread ID. It creates no wait or signal.
                    let native_tid = unsafe { libc::syscall(libc::SYS_gettid) };
                    let native_tid = u32::try_from(native_tid).expect("positive Linux thread ID");
                    // No channel waits or barriers follow publication: the only
                    // reachable blocking conversion path is the actual Local call.
                    reader_tid.store(native_tid, Ordering::Release);
                    let offset = zone_rule_to_offset_name(&ZoneRule::Local, 0).0;
                    let _ = completed_tx.send(offset);
                })
                .map_err(|error| format!("reader spawn failed: {error}"))?,
        );
        wait_for_reader(&tid, &completed_rx)
    })();
    // Release and join before ANY assertion, including protocol/worker failures.
    let cleanup = workers.finish();
    assert!(cleanup.is_ok(), "timezone worker cleanup: {cleanup:?}");
    match protocol.expect("finite Local-reader synchronization protocol") {
        ReaderWhileWriterHeld::ReturnedEarly(offset) => {
            assert_ne!(
                offset, temporary_offset,
                "Local reader observed temporary writer TZ"
            );
            panic!("Local reader returned while the writer still owned TZ: offset={offset}");
        }
        ReaderWhileWriterHeld::ParkedOnFutex { tid, wchan } => {
            let observed = completed_rx
                .try_recv()
                .expect("joined reader published its offset");
            assert_eq!(
                observed, expected,
                "Local reader {tid} ({wchan}) observed temporary TZ"
            );
        }
    }
}
