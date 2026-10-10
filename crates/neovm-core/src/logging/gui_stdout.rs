//! Bounded, lossy diagnostics only. Never use this writer for Lisp/data output.
//!
//! `tracing_appender::WorkerGuard` cannot guard this sink: its full-queue
//! shutdown timeout prints synchronously to stdout, which may itself be blocked.
//! Keep the existing appender for files; the GUI sink needs a separate stop
//! channel and a non-waiting guard. A blocked OS write cannot be cancelled
//! portably: its one worker is detached at exit, not joined on the evaluator.

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;

use crossbeam_channel::{Sender, bounded};
use tracing_subscriber::fmt::MakeWriter;

// At most 16 MiB queued, plus one in-flight record. Bound bytes as well as count.
const QUEUED_RECORDS: usize = 256;
const RECORD_BYTES: usize = 64 * 1024;

#[cfg(test)]
#[path = "gui_stdout/tests.rs"]
mod tests;

#[derive(Default)]
struct Counters {
    rejected: AtomicUsize,
    write_failures: AtomicUsize,
    closed: AtomicBool,
}

fn increment(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(n.saturating_add(1))
    });
}

#[derive(Clone)]
pub(super) struct GuiWriter {
    sender: Sender<Vec<u8>>,
    counters: Arc<Counters>,
}

pub(super) struct GuiGuard {
    stop: Sender<()>,
    counters: Arc<Counters>,
    // Owned until drop; dropping a JoinHandle detaches, never waits for a sink.
    _worker: JoinHandle<()>,
}

impl GuiGuard {
    pub(super) fn rejected_records(&self) -> usize {
        self.counters.rejected.load(Ordering::Relaxed)
    }

    pub(super) fn write_failures(&self) -> usize {
        self.counters.write_failures.load(Ordering::Relaxed)
    }
}

impl Drop for GuiGuard {
    fn drop(&mut self) {
        self.counters.closed.store(true, Ordering::Release);
        // This control channel is independent of the possibly full data queue.
        let _ = self.stop.try_send(());
    }
}

pub(super) fn stdout() -> io::Result<(GuiWriter, GuiGuard)> {
    start(std::io::stdout(), QUEUED_RECORDS)
}

fn start<W: Write + Send + 'static>(
    mut sink: W,
    capacity: usize,
) -> io::Result<(GuiWriter, GuiGuard)> {
    let (sender, receiver) = bounded::<Vec<u8>>(capacity);
    let (stop, stopped) = bounded(1);
    let counters = Arc::new(Counters::default());
    let worker_counters = counters.clone();
    let worker = std::thread::Builder::new()
        .name("neomacs-gui-log".into())
        .spawn(move || {
            let mut reported = 0;
            let mut failed = false;
            loop {
                crossbeam_channel::select_biased! {
                    recv(stopped) -> _ => {
                        // Best-effort drain off-thread, bounded by queue capacity.
                        for record in receiver.try_iter() {
                            if write_record(&mut sink, &worker_counters, &mut reported, &record)
                                .is_err()
                            {
                                increment(&worker_counters.write_failures);
                                failed = true;
                                break;
                            }
                        }
                        break;
                    }
                    recv(receiver) -> record => match record {
                        Ok(record) => {
                            if write_record(&mut sink, &worker_counters, &mut reported, &record)
                                .is_err()
                            {
                                increment(&worker_counters.write_failures);
                                failed = true;
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            worker_counters.closed.store(true, Ordering::Release);
            // Rejections after the last written record have no later record
            // to carry their report; admission is closed, so report once more.
            if !failed && report_rejections(&mut sink, &worker_counters, &mut reported).is_err() {
                increment(&worker_counters.write_failures);
            }
            if sink.flush().is_err() {
                increment(&worker_counters.write_failures);
            }
        })?;
    Ok((
        GuiWriter {
            sender,
            counters: counters.clone(),
        },
        GuiGuard {
            stop,
            counters,
            _worker: worker,
        },
    ))
}

fn write_record(
    sink: &mut impl Write,
    counters: &Counters,
    reported: &mut usize,
    record: &[u8],
) -> io::Result<()> {
    sink.write_all(record)?;
    report_rejections(sink, counters, reported)
}

fn report_rejections(
    sink: &mut impl Write,
    counters: &Counters,
    reported: &mut usize,
) -> io::Result<()> {
    let rejected = counters.rejected.load(Ordering::Relaxed);
    if rejected != *reported {
        writeln!(
            sink,
            "warning: GUI logging rejected {} records (queue full, oversized, or closed)",
            rejected
        )?;
        *reported = rejected;
    }
    Ok(())
}

// One MakeWriter lifetime is one formatted tracing record. Reject oversized
// records whole: no truncation, broken UTF-8, or partial diagnostic lines.
pub(super) struct RecordWriter {
    writer: GuiWriter,
    bytes: Vec<u8>,
    oversized: bool,
}

impl<'a> MakeWriter<'a> for GuiWriter {
    type Writer = RecordWriter;

    fn make_writer(&'a self) -> Self::Writer {
        RecordWriter {
            writer: self.clone(),
            bytes: Vec::new(),
            oversized: false,
        }
    }
}

impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.oversized {
            if bytes.len() > RECORD_BYTES - self.bytes.len() {
                self.oversized = true;
                self.bytes.clear();
            } else {
                self.bytes.extend_from_slice(bytes);
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RecordWriter {
    fn drop(&mut self) {
        if self.oversized || self.writer.counters.closed.load(Ordering::Acquire) {
            increment(&self.writer.counters.rejected);
        } else if !self.bytes.is_empty()
            && self
                .writer
                .sender
                .try_send(std::mem::take(&mut self.bytes))
                .is_err()
        {
            increment(&self.writer.counters.rejected);
        }
    }
}
