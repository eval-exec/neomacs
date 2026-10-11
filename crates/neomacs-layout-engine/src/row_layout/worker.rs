//! One active program batch, one replaceable pending batch, one completed
//! result. Evaluator-owned layout identities never enter this mailbox.

use super::program::{ComputedRow, RowProgram, RowProgramError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

const MAX_ROWS: usize = 64;
pub(crate) const MAX_PROGRAMS: usize = 64;
const MAX_BYTES: usize = 64 * 1024;
const MAX_GLYPHS: usize = 16 * 1024;
const MAX_ITEMS: usize = 4096;
pub(crate) const MAX_FONT_BYTES: usize = 64 * 1024;

/// An opaque receipt. A caller must still validate its full layout key before
/// admitting the corresponding result; this number says nothing about reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowJobTicket(u64);

pub(crate) struct RowJobResult {
    pub ticket: RowJobTicket,
    pub rows: Result<Vec<ComputedRow>, RowProgramError>,
}

struct Job {
    ticket: RowJobTicket,
    rows: Vec<RowProgram>,
}

#[derive(Default)]
struct Mailbox {
    pending: Option<Job>,
    completed: Option<RowJobResult>,
    stopping: bool,
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    wake: Condvar,
    revision: AtomicU64,
}

pub(crate) struct RowWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Default for RowWorker {
    fn default() -> Self {
        Self {
            shared: Arc::default(),
            thread: None,
        }
    }
}

impl RowWorker {
    pub(crate) fn submit(
        &mut self,
        rows: Vec<RowProgram>,
    ) -> Result<RowJobTicket, RowProgramError> {
        if rows.is_empty() || rows.len() > MAX_PROGRAMS {
            return Err(RowProgramError::Budget);
        }
        // Reserve worst-case output, not merely the number of glyphs capture
        // happened to see. This bounds active, pending and completed payloads.
        let mut bytes = 0usize;
        let mut glyphs = 0usize;
        let mut items = 0usize;
        let mut font_bytes = 0usize;
        let mut snapshots = rustc_hash::FxHashSet::default();
        for row in &rows {
            let limits = row.limits();
            bytes = bytes.saturating_add(limits.text_bytes);
            glyphs = glyphs.saturating_add(limits.glyphs);
            items = items.saturating_add(limits.items);
            if row
                .font_snapshot_identity()
                .is_some_and(|identity| snapshots.insert(identity))
            {
                font_bytes = font_bytes.saturating_add(row.font_bytes());
            }
        }
        if bytes > MAX_BYTES
            || glyphs > MAX_GLYPHS
            || items > MAX_ITEMS
            || font_bytes > MAX_FONT_BYTES
        {
            return Err(RowProgramError::Budget);
        }
        if self.thread.is_none() {
            let shared = self.shared.clone();
            self.thread = Some(
                std::thread::Builder::new()
                    .name("neomacs-row-layout".into())
                    .spawn(move || run(shared))
                    .map_err(|_| RowProgramError::Unsupported)?,
            );
        }
        let mut mailbox = self.shared.mailbox.lock().unwrap();
        let next = self
            .shared
            .revision
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or(RowProgramError::Budget)?;
        let ticket = RowJobTicket(next);
        self.shared.revision.store(next, Ordering::Release);
        mailbox.pending = Some(Job { ticket, rows });
        mailbox.completed = None;
        tracing::debug!(target: "neomacs_layout_engine::row_worker",
            ticket = ticket.0, "row job submitted");
        self.shared.wake.notify_one();
        Ok(ticket)
    }

    /// Invalidate active work as well as pending and already completed work.
    /// No join or wait is performed on the input/redisplay path.
    pub(crate) fn cancel(&mut self) {
        let mut mailbox = self.shared.mailbox.lock().unwrap();
        self.shared.revision.fetch_add(1, Ordering::Release);
        mailbox.pending = None;
        mailbox.completed = None;
    }

    pub(crate) fn take_completed(&mut self) -> Option<RowJobResult> {
        let result = self.shared.mailbox.lock().unwrap().completed.take();
        if let Some(result) = &result {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = result.ticket.0, "row result taken");
        }
        result
    }
}

fn run(shared: Arc<Shared>) {
    let mut fonts = super::font_measurement::WorkerFontMeasurements::default();
    loop {
        let job = {
            let mut mailbox = shared.mailbox.lock().unwrap();
            while mailbox.pending.is_none() && !mailbox.stopping {
                mailbox = shared.wake.wait(mailbox).unwrap();
            }
            if mailbox.stopping {
                return;
            }
            mailbox.pending.take().unwrap()
        };
        let cancelled = || shared.revision.load(Ordering::Acquire) != job.ticket.0;
        tracing::debug!(target: "neomacs_layout_engine::row_worker",
            ticket = job.ticket.0, programs = job.rows.len(), "row computation started");
        let rows = compute_rows(job.rows, &mut fonts, cancelled);
        let mut mailbox = shared.mailbox.lock().unwrap();
        if !mailbox.stopping && !cancelled() {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = job.ticket.0, rows = rows.as_ref().map_or(0, Vec::len),
                succeeded = rows.is_ok(), "row result ready");
            mailbox.completed = Some(RowJobResult {
                ticket: job.ticket,
                rows,
            });
        } else {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = job.ticket.0, "row result cancelled");
        }
    }
}

// A failed row is never published. Earlier complete rows remain useful for
// a partial scroll; cancellation invalidates the entire batch.
fn compute_rows(
    programs: Vec<RowProgram>,
    fonts: &mut super::font_measurement::WorkerFontMeasurements,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<ComputedRow>, RowProgramError> {
    let mut rows = Vec::with_capacity(programs.len());
    let mut programs = programs.into_iter();
    while let Some(mut program) = programs.next() {
        if rows.len() == MAX_ROWS {
            break;
        }
        program.measure_on_worker(fonts, &cancelled)?;
        while !program.is_complete() {
            let Some(mut fragment) = programs.next() else {
                break;
            };
            fragment.measure_on_worker(fonts, &cancelled)?;
            program.append_measured_fragment(
                fragment,
                super::program::RowProgramLimits {
                    items: MAX_ITEMS,
                    text_bytes: MAX_BYTES,
                    glyphs: MAX_GLYPHS,
                },
            )?;
        }
        match program.compute_visual_rows(MAX_ROWS - rows.len(), &cancelled) {
            Ok(computed) => rows.extend(computed),
            Err(
                RowProgramError::Overflow | RowProgramError::Budget | RowProgramError::Incomplete,
            ) if !rows.is_empty() => break,
            Err(error) => return Err(error),
        }
    }
    if cancelled() {
        Err(RowProgramError::Cancelled)
    } else {
        Ok(rows)
    }
}

impl Drop for RowWorker {
    fn drop(&mut self) {
        {
            let mut mailbox = self.shared.mailbox.lock().unwrap();
            mailbox.stopping = true;
            mailbox.pending = None;
            mailbox.completed = None;
            self.shared.revision.fetch_add(1, Ordering::Release);
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "tests/worker_test.rs"]
mod tests;
